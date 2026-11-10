// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bağlantı araştırma düzeneği (`cargo xtask lab`; CONNECTIVITY_RESEARCH §4–§5).

mod cli;
pub mod dns;
pub mod journal;
pub mod ops;
pub mod probe;
pub mod report;
pub mod scenario;
pub mod sockettech;
pub mod watchdog;

/// `cargo xtask lab <komut>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let rest = args.get(1..).unwrap_or_default();
    cli::dispatch(args.first().map(String::as_str), rest)
}
