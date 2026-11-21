// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-svc`: yalnızca SCM başlatır. Argümanla çalıştırılırsa 2, SCM dışından
//! çalıştırılırsa 1 ile çıkar.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::process::ExitCode;

use alterdns_journal::SERVICE_NAME;
use alterdns_svc::{SvcConfig, exit_code, start};
use alterdns_winsys::{dispatch, register_stop_handler};

fn service_main(_args: Vec<OsString>) {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let Ok(reporter) = register_stop_handler(SERVICE_NAME, move || {
        let _ = tx.send(());
    }) else {
        return;
    };
    match SvcConfig::product().and_then(start) {
        Ok(running) => {
            let _ = reporter.running();
            let _ = rx.recv();
            let _ = reporter.stop_pending();
            running.stop();
            let _ = reporter.stopped(None);
        }
        Err(e) => {
            tracing::error!(code = e.code().as_str(), "servis başlatılamadı");
            let _ = reporter.stopped(Some(exit_code(&e)));
        }
    }
}

fn main() -> ExitCode {
    // İlk iş: DLL'ler System32'den ve servisin kendi dizininden yüklenir (K18).
    if alterdns_winsys::harden_dll_search(true).is_err() {
        return ExitCode::from(1);
    }
    if std::env::args_os().len() > 1 {
        return ExitCode::from(2);
    }
    match dispatch(SERVICE_NAME, service_main) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(1),
    }
}
