// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme görevleri. Kullanım: `cargo xtask <komut>`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::{assets, check, devinstall, docs_errors, hooks, lab, perf};

fn report(name: &str, r: anyhow::Result<()>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask {name} başarısız: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => match match (args.get(1).map(String::as_str), args.get(2)) {
            (Some("--staged"), _) => check::run_scan_only(&check::Source::Index),
            (Some("--pre-push"), _) => check::run_pre_push(std::io::stdin()),
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
        Some("assets") => report(
            "assets",
            check::repo_root().and_then(|r| {
                if args.get(1).map(String::as_str) == Some("--check") {
                    assets::check(&r)
                } else {
                    assets::generate(&r).map(|files| {
                        for f in files {
                            eprintln!("yazıldı: {}", f.display());
                        }
                    })
                }
            }),
        ),
        // `svc` yönetici terminalinde, `ui` normal terminalde; ikisi de dev-install sonrası.
        Some("perf") => report("perf", perf::run(&args[1..])),
        Some("test") => match args.get(1).map(String::as_str) {
            None => report("test", check::run_tests(false)),
            // Kurulu servise karşı salt okunur testler: yönetici terminali, dev-install sonrası.
            Some("--live") => report("test", check::run_tests(true)),
            _ => {
                eprintln!("kullanım: cargo xtask test [--live]");
                ExitCode::from(2)
            }
        },
        // Sistem değiştirir: yalnızca kullanıcının yönetici terminalinde, onayıyla.
        Some("dev-install") => {
            let profile = if args.get(1).map(String::as_str) == Some("--debug") {
                "debug"
            } else {
                "release"
            };
            report("dev-install", devinstall::dev_install(profile))
        }
        Some("dev-uninstall") => report("dev-uninstall", devinstall::dev_uninstall()),
        Some("lab") => match lab::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask lab başarısız: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "kullanım: cargo xtask check [--staged | --tree <rev>] | test [--live] | perf svc [--minutes N] | perf ui [--opens N] | perf size | docs-errors | assets [--check] | hooks install | dev-install [--debug] | dev-uninstall | lab <komut>"
            );
            ExitCode::from(2)
        }
    }
}
