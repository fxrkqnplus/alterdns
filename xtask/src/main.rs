// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme görevleri. Kullanım: `cargo xtask <komut>`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::{check, docs_errors, hooks, lab};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => match match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("--staged"), _) => check::run_scan_only(&check::Source::Index),
            (Some("--tree"), Some(rev)) => check::run_scan_only(&check::Source::Tree(rev.clone())),
            (None, _) => check::run(),
            _ => Err(anyhow::anyhow!(
                "kullanım: cargo xtask check [--staged | --tree <rev>]"
            )),
        } {
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
        Some("hooks") if args.get(1).map(String::as_str) == Some("install") => {
            match check::repo_root().and_then(|r| hooks::install(&r)) {
                Ok(p) => {
                    eprintln!("kanca yazıldı: {}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("xtask hooks install başarısız: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("lab") => match lab::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask lab başarısız: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "kullanım: cargo xtask check [--staged | --tree <rev>] | docs-errors | hooks install | lab <komut>"
            );
            ExitCode::from(2)
        }
    }
}
