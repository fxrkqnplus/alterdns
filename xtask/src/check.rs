// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask check`: fmt, clippy, test, deny ve kişisel veri taraması.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

use crate::scan::{self, ScanContext};

/// Taranmayan ikili dosya uzantıları.
const BINARY_EXTS: [&str; 8] = [
    ".png", ".ico", ".sys", ".dll", ".exe", ".zip", ".pdf", ".jpg",
];
const ALLOWLIST_FILE: &str = "xtask/private-allowlist.txt";
const LOCAL_TERMS_FILE: &str = "lab/local/private-terms.txt";

/// `xtask check` adımlarını sırayla çalıştırır; ilk hatada durur.
pub fn run() -> anyhow::Result<()> {
    let root = repo_root()?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let steps: [(&str, &[&str]); 4] = [
        ("fmt", &["fmt", "--all", "--check"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("test", &["test", "--workspace", "--locked"]),
        ("deny", &["deny", "check"]),
    ];
    for (name, args) in steps {
        eprintln!("==> {name}");
        let status = Command::new(&cargo)
            .args(args)
            .current_dir(&root)
            .status()
            .with_context(|| format!("{name} adımı başlatılamadı"))?;
        if !status.success() {
            bail!("{name} adımı başarısız ({status})");
        }
    }
    eprintln!("==> scan");
    scan_repo(&root)
}

fn repo_root() -> anyhow::Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("git çalıştırılamadı")?;
    if !out.status.success() {
        bail!("git deposu bulunamadı");
    }
    Ok(PathBuf::from(String::from_utf8(out.stdout)?.trim()))
}

fn read_optional(path: &Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("{} okunamadı", path.display())),
    }
}

/// İzlenen ve yok sayılmayan izlenmeyen dosyaları tarar (yeni dosyalar `git add` öncesinde de yakalanır).
fn scan_repo(root: &Path) -> anyhow::Result<()> {
    let ctx = ScanContext {
        allowlist: parse_allowlist(&read_optional(&root.join(ALLOWLIST_FILE))?),
        local_terms: parse_terms(&read_optional(&root.join(LOCAL_TERMS_FILE))?),
    };
    let out = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()
        .context("git ls-files çalıştırılamadı")?;
    if !out.status.success() {
        bail!("git ls-files başarısız");
    }
    let list = String::from_utf8(out.stdout)?;
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    for rel in list.split('\0').filter(|p| !p.is_empty()) {
        let lower = rel.to_ascii_lowercase();
        if BINARY_EXTS.iter().any(|e| lower.ends_with(e)) {
            continue;
        }
        let bytes = match std::fs::read(root.join(rel)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("{rel} okunamadı")),
        };
        scanned += 1;
        findings.extend(scan::scan_bytes_header(rel, &bytes));
        match std::str::from_utf8(&bytes) {
            Ok(text) => findings.extend(scan::scan_text(rel, text, &ctx)),
            Err(_) => eprintln!("uyarı: {rel} UTF-8 değil; metin taraması atlandı"),
        }
    }
    eprintln!(
        "scan: {scanned} dosya, {} izinli değer, {} yerel terim",
        ctx.allowlist.len(),
        ctx.local_terms.len()
    );
    if findings.is_empty() {
        return Ok(());
    }
    for f in &findings {
        eprintln!("{}:{}: {:?} {}", f.path, f.line, f.rule, f.excerpt);
    }
    bail!("{} kişisel veri / politika bulgusu", findings.len())
}

fn meaningful_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// `yol<TAB>değer` satırlarını okur; boş ve `#` ile başlayan satırlar atlanır.
pub fn parse_allowlist(text: &str) -> Vec<(String, String)> {
    meaningful_lines(text)
        .filter_map(|l| l.split_once('\t'))
        .map(|(p, v)| (p.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Satır başına bir terim; boş ve `#` ile başlayan satırlar atlanır.
pub fn parse_terms(text: &str) -> Vec<String> {
    meaningful_lines(text).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_parses_tab_separated_pairs() {
        let text = "# açıklama\n\ndocs/UI_SPEC.md\tdeger-1\r\nbozuk satır\n";
        assert_eq!(
            parse_allowlist(text),
            vec![("docs/UI_SPEC.md".to_string(), "deger-1".to_string())]
        );
    }

    #[test]
    fn terms_skip_comments_and_blanks() {
        let text = "# yorum\n  terim-a  \n\nterim-b\r\n";
        assert_eq!(
            parse_terms(text),
            vec!["terim-a".to_string(), "terim-b".to_string()]
        );
    }
}
