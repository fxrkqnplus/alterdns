// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-ui.exe`: `--tray [--logon]` tepsi kipi, argümansız ya da `--page <ad>` ana pencere.
//! Tanınmayan argümanla 2, başlatma hatasında (ADNS-UI-001 ve benzeri) ileti kutusuyla 1 döner.

#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

use std::process::ExitCode;

use alterdns_core::Lang;
use alterdns_ui_app::args::{Mode, parse_args};
use alterdns_ui_app::{tray, window};

fn main() -> ExitCode {
    // İlk iş: DLL'ler yalnızca System32'den yüklenir (K18); arayüz kendi dizininden DLL yüklemez.
    if alterdns_winsys::harden_dll_search(false).is_err() {
        return ExitCode::from(1);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Ok(mode) = parse_args(&args) else {
        return ExitCode::from(2);
    };
    let result = match mode {
        Mode::Tray { logon } => tray::run(logon),
        Mode::Window { page } => window::run(page),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(code = e.code().as_str(), "arayüz başlatılamadı");
            let r = e.render(Lang::default());
            alterdns_winsys::error_box(&r.title, &format!("{} ({})", r.message, e.code().as_str()));
            ExitCode::FAILURE
        }
    }
}
