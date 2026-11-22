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

/// Tek bir journal dosyasının kurtarma sonucu.
#[derive(Debug)]
pub(crate) enum FileRecovery {
    Reverted(RevertReport),
    /// Commit edilmiş `Persistent` txn; dokunulmadı.
    Kept,
    /// Canlı bir işlem dosyayı tutuyor; dokunulmadı.
    InUse,
    Error(AdnsError),
}

/// Açılış kurtarması ve bekçi aynı kuralı kullanır.
pub(crate) fn recover_file(path: &Path, ops: &dyn SystemOps, targets: &Targets) -> FileRecovery {
    let mut file = match JournalFile::open(path) {
        Ok(Some(f)) => f,
        Ok(None) => return FileRecovery::InUse,
        Err(e) => return FileRecovery::Error(e),
    };
    if file.committed() && file.lifetime() == Some(Lifetime::Persistent) {
        return FileRecovery::Kept;
    }
    match revert_file(&mut file, ops, targets) {
        Ok(r) => FileRecovery::Reverted(r),
        Err(e) => FileRecovery::Error(e),
    }
}

/// Txn kimliğindeki (`<bileşen>-<unix_ms>-<pid>`) başlangıç zamanı; okunamazsa 0.
fn started_ms(path: &Path) -> u64 {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit('-').nth(1))
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(0)
}

/// Txn'ler en yeniden en eskiye geri alınır: aynı değeri art arda değiştiren iki txn'de yeni olan
/// önce geri alınmazsa eskisi çakışma görür ve AlterDNS'in yazdığı değer yerinde kalır.
pub fn recover_at_startup(dir: &Path, ops: &dyn SystemOps, targets: &Targets) -> RecoveryReport {
    let mut report = RecoveryReport::default();
    let mut paths = match list(dir) {
        Ok(p) => p,
        Err(e) => {
            report.errors.push((dir.display().to_string(), e));
            return report;
        }
    };
    paths.sort_by(|a, b| started_ms(b).cmp(&started_ms(a)).then_with(|| b.cmp(a)));
    for path in paths {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match recover_file(&path, ops, targets) {
            FileRecovery::Reverted(r) => report.txns.push((name, r)),
            FileRecovery::Kept => report.kept.push(name),
            FileRecovery::InUse => report.in_use.push(name),
            FileRecovery::Error(e) => report.errors.push((name, e)),
        }
    }
    report
}
