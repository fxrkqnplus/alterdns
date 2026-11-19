// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `errors/catalog.toml` → doğrulama → `OUT_DIR/codes.rs` (`AdnsErrorCode` ve metin tabloları).
//! Doğrulama hatasında derleme durur. Clippy'nin `panic` yasağı bu betiğe de uygulanır; hatalar
//! yazılıp `exit(1)` ile çıkılır.

#[path = "src/catalog.rs"]
mod catalog;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn fail(problems: &[String]) -> ! {
    for p in problems {
        eprintln!("errors/catalog.toml: {p}");
    }
    std::process::exit(1)
}

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../errors/catalog.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    println!("cargo:rerun-if-changed=src/catalog.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| fail(&[format!("{} okunamadı: {e}", path.display())]));
    let entries = catalog::parse(&text).unwrap_or_else(|e| fail(&[e]));
    let problems = catalog::validate(&entries);
    if !problems.is_empty() {
        fail(&problems);
    }
    let out = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| fail(&["OUT_DIR tanımlı değil".to_string()]));
    if let Err(e) = std::fs::write(out.join("codes.rs"), generate(&entries)) {
        fail(&[format!("codes.rs yazılamadı: {e}")]);
    }
}

fn severity(s: &str) -> &'static str {
    match s {
        "info" => "Severity::Info",
        "warning" => "Severity::Warning",
        "critical" => "Severity::Critical",
        _ => "Severity::Error",
    }
}

fn text(t: &catalog::Texts) -> String {
    format!(
        "Text {{ title: {:?}, message: {:?}, causes: &{:?}, solutions: &{:?} }}",
        t.title, t.message, t.causes, t.solutions
    )
}

/// Üretilen kod: `str` için `{:?}` geçerli bir Rust dize sabiti verir.
fn generate(entries: &[catalog::Entry]) -> String {
    let mut s = String::from("// errors/catalog.toml'dan üretildi; elle düzenlenmez.\n\n");
    s.push_str("/// Katalogdaki bütün hata kodları (`errors/catalog.toml`).\n");
    s.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]\npub enum AdnsErrorCode {\n");
    for e in entries {
        let _ = writeln!(s, "    /// `{}`: {}", e.code, e.tr.title);
        let _ = writeln!(s, "    {},", e.name);
    }
    s.push_str("}\n\nimpl AdnsErrorCode {\n    /// Katalog sırasıyla bütün kodlar.\n    pub const ALL: &'static [AdnsErrorCode] = &[\n");
    for e in entries {
        let _ = writeln!(s, "        AdnsErrorCode::{},", e.name);
    }
    s.push_str("    ];\n}\n\n");
    let _ = writeln!(s, "static META: [Meta; {}] = [", entries.len());
    for e in entries {
        let _ = writeln!(
            s,
            "    Meta {{ code: {:?}, severity: {}, faz: {}, retryable: {}, user_action_required: {}, params: &{:?}, tr: {}, en: {} }},",
            e.code,
            severity(&e.severity),
            e.faz,
            e.retryable,
            e.user_action_required,
            e.params,
            text(&e.tr),
            text(&e.en)
        );
    }
    s.push_str("];\n");
    s
}
