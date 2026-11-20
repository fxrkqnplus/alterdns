// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-maint`: bakım ikilisi. Şimdilik tek alt komut vardır:
//!
//! `alterdns-maint watchdog --txn <id>`: geri alma bekçisi (`alterdns_journal::run_watchdog`).
//! Journal dizini `%ProgramData%\AlterDNS\journal`'dır ve korumalı olmalıdır (K20); komut
//! satırından yol alınmaz.
//!
//! Çıkış kodları: 0 tamam, 1 hata, 2 geçersiz komut.

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use alterdns_core::Paths;
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_journal::{RealOps, Targets, run_watchdog, valid_txn_id};

fn main() -> ExitCode {
    // İlk iş: DLL'ler yalnızca System32'den yüklenir (uygulama dizini dahil değil).
    if alterdns_winsys::harden_dll_search(false).is_err() {
        return ExitCode::from(1);
    }
    let args: Option<Vec<String>> = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string().ok())
        .collect();
    let args = args.unwrap_or_default();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["watchdog", "--txn", id] if valid_txn_id(id) => watchdog(id),
        _ => ExitCode::from(2),
    }
}

fn watchdog(txn_id: &str) -> ExitCode {
    let run = || -> Result<(), AdnsError> {
        let paths = Paths::system()?;
        let dir = journal_dir(&paths)?;
        let targets = Targets::product(paths.program_files.clone());
        run_watchdog(&dir, txn_id, &RealOps, &targets, std::io::stdin())?;
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(1),
    }
}

/// Ürün: `%ProgramData%\AlterDNS\journal`; `AlterDNS` ve `journal` korumalı olmalı (K20).
/// Kullanıcının yazabildiği bir dizindeki journal'a göre SYSTEM yetkisiyle geri alma yapılmaz.
fn journal_dir(paths: &Paths) -> Result<PathBuf, AdnsError> {
    #[cfg(feature = "test-ops")]
    if let Some(d) = std::env::var_os("ALTERDNS_TEST_JOURNAL_DIR") {
        return Ok(PathBuf::from(d));
    }
    // `%ProgramData%\AlterDNS`'ten başlayarak her halka: üst dizin bir kullanıcınınsa yaprak
    // denetimden sonra bir bağlantıyla değiştirilebilirdi.
    let dir = paths.journal_dir();
    alterdns_winsys::verify_protected_chain(&paths.machine, &dir).map_err(|p| {
        AdnsError::new(AdnsErrorCode::SysRevertFailed).with_source(std::io::Error::other(format!(
            "journal dizini korumalı değil: {p:?}"
        )))
    })?;
    Ok(dir)
}
