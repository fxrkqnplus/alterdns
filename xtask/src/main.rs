// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme görevleri. Kullanım: `cargo xtask <komut>`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::{check, docs_errors, lab};

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
        Some("docs-errors") => match check::repo_root().and_then(|r| docs_errors::write(&r)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask docs-errors başarısız: {e:#}");
                ExitCode::FAILURE
            }
        },
        Some("lab") => match lab::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask lab başarısız: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "kullanım: cargo xtask check | cargo xtask docs-errors | cargo xtask lab <komut>"
            );
            ExitCode::from(2)
        }
    }
}
