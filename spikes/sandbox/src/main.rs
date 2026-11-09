// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Faz 0 sandbox spike'ı (T-SPK-003): AppContainer (yeteneksiz) + Low IL + Job Object +
//! devralınmış salt okunur handle. Aynı exe `broker` ve `worker` kiplerinde çalışır.
//! Spike kodudur; ürün kuralları (ör. `unsafe` yasağı) uygulanmaz, her `unsafe` blok
//! `// SAFETY:` açıklaması taşır.

mod broker;
mod worker;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let result = match args.first().map(String::as_str) {
        Some("broker") => broker::main(rest),
        Some("worker") => worker::main(rest),
        Some("cleanup") => broker::cleanup(),
        _ => Err(anyhow::anyhow!(
            "kullanım: sandbox-spike broker --file <yol> --probe <ad> | cleanup"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

/// `--ad değer` biçimindeki argümanı döndürür.
pub fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}
