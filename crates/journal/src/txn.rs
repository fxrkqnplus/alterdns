// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Txn: önce kaydet, sonra uygula; commit ya da geri al (`docs/ARCHITECTURE.md` §6).

use std::path::{Path, PathBuf};

use alterdns_errors::{AdnsError, AdnsErrorCode};

use crate::file::JournalFile;
use crate::model::{
    CREATED_BY, ChangeRecord, Kind, Lifetime, Line, Snapshot, State, new_txn_id, now_ms,
};
use crate::ops::{SystemOps, Targets, is_own_object, target_allowed};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevertReport {
    pub reverted: Vec<u32>,
    pub conflicts: Vec<u32>,
    pub failed: Vec<(u32, String)>,
}

impl RevertReport {
    pub fn clean(&self) -> bool {
        self.conflicts.is_empty() && self.failed.is_empty()
    }
}

fn apply_err(msg: impl Into<String>) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SysApplyFailed).with_source(std::io::Error::other(msg.into()))
}

pub struct Txn {
    file: JournalFile,
    targets: Targets,
    component: String,
    next_id: u32,
}

impl Txn {
    pub fn begin(
        dir: &Path,
        component: &str,
        lifetime: Lifetime,
        targets: Targets,
    ) -> Result<Txn, AdnsError> {
        let id = new_txn_id(component);
        let file = JournalFile::create(dir, &id, component, lifetime)?;
        Ok(Txn {
            file,
            targets,
            component: component.to_string(),
            next_id: 1,
        })
    }

    pub fn id(&self) -> &str {
        self.file.txn_id()
    }

    pub fn journal_path(&self) -> PathBuf {
        self.file.path().to_path_buf()
    }

    /// İzin denetimi → önceki değer → `Pending` (diske) → yazma → `Applied`. Journal satırı
    /// yazılamazsa sistem değiştirilmez.
    pub fn apply(
        &mut self,
        ops: &dyn SystemOps,
        kind: Kind,
        target: &str,
        after: Snapshot,
    ) -> Result<u32, AdnsError> {
        if !target_allowed(kind, target, &self.targets) {
            return Err(apply_err(format!("izin dışı hedef: {kind:?} {target}")));
        }
        let before = ops.read(kind, target).map_err(apply_err)?;
        let id = self.next_id;
        self.next_id += 1;
        let mut rec = ChangeRecord {
            id,
            txn_id: self.id().to_string(),
            component: self.component.clone(),
            kind,
            target: target.to_string(),
            before: before.clone(),
            after: after.clone(),
            state: State::Pending,
            created_by: CREATED_BY.into(),
            ts: now_ms(),
        };
        self.file.append(&Line::Record(rec.clone()))?;
        match ops.write(kind, target, &after) {
            Ok(()) => {
                rec.state = State::Applied;
                rec.ts = now_ms();
                self.file.append(&Line::Record(rec))?;
                Ok(id)
            }
            Err(e) => {
                // Değer değişmediyse kayıt kapanır; değiştiyse açık kalır ve geri alınır.
                if ops.read(kind, target).ok().as_ref() == Some(&before) {
                    rec.state = State::Failed;
                    rec.ts = now_ms();
                    let _ = self.file.append(&Line::Record(rec));
                }
                Err(apply_err(e))
            }
        }
    }

    pub fn commit(mut self) -> Result<(), AdnsError> {
        let id = self.id().to_string();
        self.file.append(&Line::TxnCommit {
            txn_id: id,
            ts: now_ms(),
        })
    }

    pub fn rollback(mut self, ops: &dyn SystemOps) -> Result<RevertReport, AdnsError> {
        let targets = self.targets.clone();
        revert_file(&mut self.file, ops, &targets)
    }
}

fn set_state(file: &mut JournalFile, rec: &ChangeRecord, state: State) -> Result<(), AdnsError> {
    let mut r = rec.clone();
    r.state = state;
    r.ts = now_ms();
    file.append(&Line::Record(r))
}

/// Açık kayıtları ters sırayla geri alır:
/// - İzin dışı ya da bilinmeyen tür → dokunulmaz, açık kalır.
/// - Değer zaten `before` → `Reverted`.
/// - Kendi nesnemiz önceden vardıysa → `Conflict` (yeniden eklenmez).
/// - Değer `after` → `before` yazılır.
/// - Başka bir değer → `Conflict` (kullanıcının değeri ezilmez).
pub fn revert_file(
    file: &mut JournalFile,
    ops: &dyn SystemOps,
    targets: &Targets,
) -> Result<RevertReport, AdnsError> {
    let mut report = RevertReport::default();
    let mut records = file.records();
    records.reverse();
    for rec in records.into_iter().filter(|r| r.state.is_open()) {
        if !target_allowed(rec.kind, &rec.target, targets) {
            report
                .failed
                .push((rec.id, "izin dışı ya da bilinmeyen tür; dokunulmadı".into()));
            continue;
        }
        let current = match ops.read(rec.kind, &rec.target) {
            Ok(v) => v,
            Err(e) => {
                report.failed.push((rec.id, e));
                continue;
            }
        };
        if current == rec.before {
            set_state(file, &rec, State::Reverted)?;
            report.reverted.push(rec.id);
        } else if is_own_object(rec.kind) && rec.before != Snapshot::Absent {
            if rec.state != State::Conflict {
                set_state(file, &rec, State::Conflict)?;
            }
            report.conflicts.push(rec.id);
        } else if current == rec.after {
            match ops.write(rec.kind, &rec.target, &rec.before) {
                Ok(()) => {
                    set_state(file, &rec, State::Reverted)?;
                    report.reverted.push(rec.id);
                }
                Err(e) => report.failed.push((rec.id, e)),
            }
        } else {
            if rec.state != State::Conflict {
                set_state(file, &rec, State::Conflict)?;
            }
            report.conflicts.push(rec.id);
        }
    }
    Ok(report)
}
