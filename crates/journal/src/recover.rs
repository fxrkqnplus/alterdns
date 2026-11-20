// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Açılışta kurtarma (`docs/ARCHITECTURE.md` §6): commit edilmemiş txn'ler ve `Session` txn'leri
//! geri alınır; commit edilmiş `Persistent` txn'lere (kurulum, güncelleme) dokunulmaz. Canlı bir
//! işlemin tuttuğu (kilitli) journal atlanır. Bir dosyanın hatası diğerlerini durdurmaz.

use std::path::Path;

use alterdns_errors::AdnsError;

use crate::file::{JournalFile, list};
use crate::model::Lifetime;
use crate::ops::{SystemOps, Targets};
use crate::txn::{RevertReport, revert_file};

#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Geri alınan txn'ler ve sonuçları.
    pub txns: Vec<(String, RevertReport)>,
    /// Commit edilmiş kalıcı txn'ler (dokunulmadı).
    pub kept: Vec<String>,
    /// Canlı bir işlemin tuttuğu txn'ler (dokunulmadı).
    pub in_use: Vec<String>,
    pub errors: Vec<(String, AdnsError)>,
}

impl RecoveryReport {
    /// Çakışma, hata ya da açık kalan kayıt yok mu?
    pub fn clean(&self) -> bool {
        self.errors.is_empty() && self.txns.iter().all(|(_, r)| r.clean())
    }
}

pub fn recover_at_startup(dir: &Path, ops: &dyn SystemOps, targets: &Targets) -> RecoveryReport {
    let mut report = RecoveryReport::default();
    let paths = match list(dir) {
        Ok(p) => p,
        Err(e) => {
            report.errors.push((dir.display().to_string(), e));
            return report;
        }
    };
    for path in paths {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut file = match JournalFile::open(&path) {
            Ok(Some(f)) => f,
            Ok(None) => {
                report.in_use.push(name);
                continue;
            }
            Err(e) => {
                report.errors.push((name, e));
                continue;
            }
        };
        if file.committed() && file.lifetime() == Some(Lifetime::Persistent) {
            report.kept.push(name);
            continue;
        }
        match revert_file(&mut file, ops, targets) {
            Ok(r) => report.txns.push((name, r)),
            Err(e) => report.errors.push((name, e)),
        }
    }
    report
}
