// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bağlantı araştırma düzeneği (`cargo xtask lab`; CONNECTIVITY_RESEARCH §4–§5).

pub mod journal;
pub mod ops;
pub mod watchdog;

/// `cargo xtask lab <komut>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("watchdog") => watchdog::main(rest),
        other => anyhow::bail!("bilinmeyen lab komutu: {other:?}"),
    }
}
