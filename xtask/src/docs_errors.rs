// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `docs/ERROR_CATALOG.md` üretimi ve katalog denetimleri.
//!
//! Belge = elle yazılan giriş (`errors/catalog-head.md`, §1–2) + katalogdan üretilen §3.
//! `check` iki adım ekler: belgenin güncelliği ve T-ERR-003 kapsamı.

use std::path::Path;

use alterdns_errors::{AdnsErrorCode, Lang, Severity};
use anyhow::{Context, bail};

/// Geçerli faz: T-ERR-003 yalnızca `faz` değeri buna eşit ya da küçük kodlara uygulanır.
pub const CURRENT_FAZ: u8 = 1;
/// Kapsam denetimi 1A'da rapor kipindeydi; 1E'de katı kipe geçildi (eksik kod check hatasıdır).
pub const CATALOG_COVERAGE_STRICT: bool = true;

pub const NOTICE: &str = "> Bu dosya `errors/catalog.toml`'dan üretilir; elle düzenlemeyin. Yeniden üretmek için: `cargo xtask docs-errors`.";
const HEAD: &str = "errors/catalog-head.md";
const DOC: &str = "docs/ERROR_CATALOG.md";

fn severity_tr(s: Severity) -> &'static str {
    match s {
        Severity::Info => "Bilgi",
        Severity::Warning => "Uyarı",
        Severity::Error => "Hata",
        Severity::Critical => "Kritik",
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

fn area(code: AdnsErrorCode) -> &'static str {
    code.as_str()
        .strip_prefix("ADNS-")
        .and_then(|r| r.split('-').next())
        .unwrap_or("?")
}

/// Başlık satırı, üretim notu, giriş metni ve alan başına tablolar.
pub fn render_catalog_doc(head: &str) -> String {
    let (title, rest) = head.split_once('\n').unwrap_or((head, ""));
    let mut out = format!("{title}\n\n{NOTICE}\n{rest}");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n## 3. Katalog\n");
    let mut current = "";
    for &code in AdnsErrorCode::ALL {
        let a = area(code);
        if a != current {
            current = a;
            out.push_str(&format!(
                "\n### {a}\n\n| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |\n|---|---|---|---|---|---|---|\n"
            ));
        }
        let t = code.text(Lang::Tr);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            code.as_str(),
            severity_tr(code.severity()),
            code.faz(),
            cell(t.title),
            cell(t.message),
            cell(&t.causes.join("; ")),
            cell(&t.solutions.join("; ")),
        ));
    }
    out
}

fn read_head(root: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(root.join(HEAD)).with_context(|| format!("{HEAD} okunamadı"))
}

/// `cargo xtask docs-errors`: belgeyi yeniden yazar.
pub fn write(root: &Path) -> anyhow::Result<()> {
    let doc = render_catalog_doc(&read_head(root)?);
    std::fs::write(root.join(DOC), doc).with_context(|| format!("{DOC} yazılamadı"))?;
    eprintln!("{DOC} üretildi ({} kod)", AdnsErrorCode::ALL.len());
    Ok(())
}

/// `check` adımı: diskteki belge üretilenle aynı olmalı.
pub fn check_doc(root: &Path) -> anyhow::Result<()> {
    let expected = render_catalog_doc(&read_head(root)?);
    let actual = std::fs::read_to_string(root.join(DOC)).unwrap_or_default();
    if actual.replace("\r\n", "\n") != expected {
        bail!("{DOC} güncel değil; `cargo xtask docs-errors` çalıştırın");
    }
    Ok(())
}

/// Test kodundaki metin: `tests/` altındaki dosyanın tamamı ya da `#[cfg(test)]` sonrası.
fn test_text<'a>(path: &str, text: &'a str) -> &'a str {
    let p = path.replace('\\', "/");
    if p.contains("/tests/") || p.starts_with("tests/") {
        text
    } else {
        text.find("#[cfg(test)]").map_or("", |i| &text[i..])
    }
}

/// T-ERR-003: `faz ≤ CURRENT_FAZ` olup hiçbir test kodunda `AdnsErrorCode::<Ad>` biçiminde
/// geçmeyen kodlar. `files`: (yol, içerik).
pub fn uncovered(files: &[(String, String)], faz: u8) -> Vec<AdnsErrorCode> {
    AdnsErrorCode::ALL
        .iter()
        .copied()
        .filter(|c| c.faz() <= faz)
        .filter(|c| {
            let needle = format!("AdnsErrorCode::{c:?}");
            !files.iter().any(|(p, t)| test_text(p, t).contains(&needle))
        })
        .collect()
}

/// Kapsam yalnızca ürün kodundan sayılır: `crates/` (katalog crate'i hariç) ve `apps/`.
/// Katalog crate'inin ve xtask'ın kendi testleri kodları üretmez, yalnızca adlarını anar.
pub fn counts_for_coverage(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p.ends_with(".rs")
        && ((p.starts_with("crates/") && !p.starts_with("crates/errors/"))
            || p.starts_with("apps/"))
}

/// `check` adımı: kapsam raporu (1A'da yalnızca yazılır, başarısız olmaz).
pub fn coverage(root: &Path, rs_files: &[String]) -> anyhow::Result<()> {
    let files: Vec<(String, String)> = rs_files
        .iter()
        .filter(|p| counts_for_coverage(p))
        .filter_map(|p| {
            std::fs::read_to_string(root.join(p))
                .ok()
                .map(|t| (p.clone(), t))
        })
        .collect();
    let missing = uncovered(&files, CURRENT_FAZ);
    if missing.is_empty() {
        eprintln!("katalog kapsamı: faz ≤ {CURRENT_FAZ} kodlarının hepsi testte üretiliyor");
        return Ok(());
    }
    let list: Vec<&str> = missing.iter().map(|c| c.as_str()).collect();
    if CATALOG_COVERAGE_STRICT {
        bail!("testi olmayan kodlar: {}", list.join(", "));
    }
    eprintln!(
        "katalog kapsamı (rapor): testi henüz olmayan {} kod: {}",
        list.len(),
        list.join(", ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD_SAMPLE: &str = "# AlterDNS — Hata Kataloğu\n\nGiriş.\n";

    #[test]
    fn doc_starts_with_generated_notice() {
        let doc = render_catalog_doc(HEAD_SAMPLE);
        let first: Vec<&str> = doc.lines().take(3).collect();
        assert_eq!(first[0], "# AlterDNS — Hata Kataloğu");
        assert_eq!(first[2], NOTICE);
    }

    #[test]
    fn doc_contains_every_code_once() {
        let doc = render_catalog_doc(HEAD_SAMPLE);
        for &c in AdnsErrorCode::ALL {
            let row = format!("| {} |", c.as_str());
            assert_eq!(doc.matches(&row).count(), 1, "{c}");
        }
    }

    #[test]
    fn coverage_lists_codes_without_tests() {
        let files = vec![
            (
                "crates/ipc/src/lib.rs".to_string(),
                "fn f() { AdnsErrorCode::IpcConnect; }\n#[cfg(test)]\nmod t { AdnsErrorCode::IpcBadRequest }"
                    .to_string(),
            ),
            (
                "crates/ipc/tests/x.rs".to_string(),
                "AdnsErrorCode::IpcUnauthorized".to_string(),
            ),
        ];
        let missing = uncovered(&files, 1);
        // Test dışı kodda geçmek sayılmaz; test modülü ve tests/ dizini sayılır.
        assert!(missing.contains(&AdnsErrorCode::IpcConnect));
        assert!(!missing.contains(&AdnsErrorCode::IpcBadRequest));
        assert!(!missing.contains(&AdnsErrorCode::IpcUnauthorized));
        // Sonraki fazın kodları faz 1 raporunda yer almaz.
        assert!(missing.iter().all(|c| c.faz() <= 1));
    }

    #[test]
    fn coverage_counts_only_product_code() {
        assert!(counts_for_coverage("crates/ipc/tests/pipe.rs"));
        assert!(counts_for_coverage("apps/svc/src/main.rs"));
        assert!(!counts_for_coverage("crates/errors/tests/catalog.rs"));
        assert!(!counts_for_coverage("xtask/src/docs_errors.rs"));
        assert!(!counts_for_coverage("crates/ipc/README.md"));
    }
}
