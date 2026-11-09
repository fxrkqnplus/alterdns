// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask lab report <koşu dizini>`: JSONL sonuçlarından Markdown özet (`rapor.md`).
//! Rapor koşu dizininde kalır (git dışı); public belgelere yalnızca özet bulgu taşınır.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;

use super::probe::{Outcome, ProbeResult};

fn label(o: &Outcome) -> String {
    match o {
        Outcome::Ok { .. } => "Ok".into(),
        other => format!("{other:?}"),
    }
}

#[derive(Default)]
struct Group {
    ok: usize,
    total: usize,
    /// Hedef → (başarılı, toplam); ilk görülme sırasıyla.
    targets: Vec<(String, usize, usize)>,
    failures: BTreeMap<String, usize>,
}

/// Teknik × DNS grupları: başarı oranı, hedef başına sonuç ve hata dağılımı.
pub fn summarize(rows: &[ProbeResult]) -> String {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for r in rows {
        let key = (format!("{:?}", r.tech), format!("{:?}", r.dns_mode));
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        let g = groups.entry(key).or_default();
        let ok = r.outcome.is_ok();
        g.total += 1;
        g.ok += usize::from(ok);
        match g.targets.iter_mut().find(|(n, _, _)| *n == r.target) {
            Some(t) => {
                t.1 += usize::from(ok);
                t.2 += 1;
            }
            None => g.targets.push((r.target.clone(), usize::from(ok), 1)),
        }
        if !ok {
            *g.failures.entry(label(&r.outcome)).or_default() += 1;
        }
    }
    let mut out =
        String::from("| Teknik | DNS | Başarı | Hedefler | Hatalar |\n|---|---|---|---|---|\n");
    for key in &order {
        let Some(g) = groups.get(key) else { continue };
        let targets = g
            .targets
            .iter()
            .map(|(n, ok, total)| format!("{n} {ok}/{total}"))
            .collect::<Vec<_>>()
            .join(", ");
        let failures = g
            .failures
            .iter()
            .map(|(k, v)| format!("{k}×{v}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "| {} | {} | {}/{} | {targets} | {failures} |\n",
            key.0, key.1, g.ok, g.total
        ));
    }
    out
}

/// Dizindeki bütün `*.jsonl` sonuç dosyalarını okur (journal hariç).
pub fn read_rows(dir: &Path) -> anyhow::Result<Vec<ProbeResult>> {
    let mut rows = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("{} okunamadı", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    for f in files {
        for (i, line) in std::fs::read_to_string(&f)?.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            rows.push(
                serde_json::from_str(line)
                    .with_context(|| format!("{}:{} ayrıştırılamadı", f.display(), i + 1))?,
            );
        }
    }
    Ok(rows)
}

/// `cargo xtask lab report <koşu dizini>`.
pub fn main(args: &[String]) -> anyhow::Result<()> {
    let dir = args.first().map(std::path::PathBuf::from).ok_or_else(|| {
        anyhow::anyhow!("kullanım: cargo xtask lab report <lab/results/koşu-dizini>")
    })?;
    let rows = read_rows(&dir)?;
    let md = summarize(&rows);
    std::fs::write(dir.join("rapor.md"), &md)?;
    println!("{md}");
    println!("{} satır → {}", rows.len(), dir.join("rapor.md").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::dns::DnsMode;
    use crate::lab::sockettech::SockTech;

    fn row(target: &str, outcome: Outcome) -> ProbeResult {
        ProbeResult {
            target: target.into(),
            domain: format!("{target}.example"),
            dns_mode: DnsMode::Doh,
            tech: SockTech::None,
            addrs: Vec::new(),
            outcome,
            dns_ms: None,
            tcp_ms: None,
            tls_ms: None,
            total_ms: 0,
            bytes_in: 0,
            detail: None,
        }
    }

    #[test]
    fn summarize_counts_success_per_group_and_target() {
        let rows = [
            row("a", Outcome::Ok { http_status: 200 }),
            row("a", Outcome::TlsReset),
            row("b", Outcome::TlsReset),
        ];
        let md = summarize(&rows);
        assert!(md.contains("| None | Doh | 1/3 |"), "{md}");
        assert!(md.contains("a 1/2, b 0/1"), "{md}");
        assert!(md.contains("TlsReset×2"), "{md}");
    }

    #[test]
    fn rows_roundtrip_through_jsonl() {
        let dir = std::env::temp_dir().join(format!("alterdns-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = row("a", Outcome::Ok { http_status: 204 });
        std::fs::write(
            dir.join("s1.jsonl"),
            format!("{}\n", serde_json::to_string(&r).unwrap()),
        )
        .unwrap();
        let back = read_rows(&dir).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].outcome, Outcome::Ok { http_status: 204 });
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
