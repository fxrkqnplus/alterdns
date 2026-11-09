// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme görevleri. Kullanım: `cargo xtask <komut>`.

#![forbid(unsafe_code)]

mod check;
mod scan;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => match check::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask check başarısız: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("kullanım: cargo xtask check");
            ExitCode::from(2)
        }
    }
}
