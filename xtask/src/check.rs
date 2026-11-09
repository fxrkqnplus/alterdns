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
const SVC_MANIFEST: &str = "apps/svc/Cargo.toml";
const SVC_DENY_CONFIG: &str = "deny-svc.toml";

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
    eprintln!("==> deny-svc");
    match svc_deny_args(&root) {
        Some(args) => {
            let status = Command::new(&cargo)
                .args(&args)
                .current_dir(&root)
                .status()
                .context("deny-svc adımı başlatılamadı")?;
            if !status.success() {
                bail!(
                    "deny-svc adımı başarısız ({status}): svc ağacında izin dışı lisans veya yasaklı crate"
                );
            }
        }
        None => eprintln!("{SVC_MANIFEST} yok; svc ağacı denetimi atlandı (Faz 1'de etkinleşir)"),
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

/// `alterdns-svc` bağımlılık ağacı için cargo-deny argümanları; svc crate'i yoksa `None`.
///
/// `--manifest-path` bir workspace üyesini gösterdiğinde cargo-deny grafiği yalnızca o
/// crate'ten başlatır; böylece `deny-svc.toml` kuralı yalnızca svc ağacına uygulanır.
pub fn svc_deny_args(root: &Path) -> Option<Vec<String>> {
    let manifest = root.join(SVC_MANIFEST);
    if !manifest.is_file() {
        return None;
    }
    let config = root.join(SVC_DENY_CONFIG);
    Some(
        [
            "deny",
            "--manifest-path",
            &manifest.display().to_string(),
            "--config",
            &config.display().to_string(),
            "check",
            "licenses",
            "bans",
        ]
        .iter()
        .map(ToString::to_string)
        .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alterdns-xtask-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn repo_file(name: &str) -> toml::Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(name);
        std::fs::read_to_string(path).unwrap().parse().unwrap()
    }

    fn allowed_licenses(cfg: &toml::Table) -> Vec<String> {
        cfg["licenses"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn svc_deny_args_absent_without_svc_manifest() {
        assert_eq!(svc_deny_args(&temp_root("nosvc")), None);
    }

    #[test]
    fn svc_deny_args_target_svc_manifest_with_svc_config() {
        let root = temp_root("svc");
        std::fs::create_dir_all(root.join("apps/svc")).unwrap();
        std::fs::write(root.join("apps/svc/Cargo.toml"), "").unwrap();
        let manifest = root.join("apps/svc/Cargo.toml").display().to_string();
        let config = root.join("deny-svc.toml").display().to_string();
        let expected: Vec<String> = [
            "deny",
            "--manifest-path",
            &manifest,
            "--config",
            &config,
            "check",
            "licenses",
            "bans",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(svc_deny_args(&root), Some(expected));
    }

    #[test]
    fn main_policy_allows_0bsd() {
        assert!(allowed_licenses(&repo_file("deny.toml")).contains(&"0BSD".to_string()));
    }

    #[test]
    fn deny_svc_config_forbids_gpl3_only_and_bans_slint() {
        let cfg = repo_file("deny-svc.toml");
        let allow = allowed_licenses(&cfg);
        assert!(allow.contains(&"GPL-3.0-or-later".to_string()));
        assert!(!allow.contains(&"GPL-3.0-only".to_string()));
        let banned: Vec<&str> = cfg["bans"]["deny"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["crate"].as_str().unwrap())
            .collect();
        assert!(banned.contains(&"slint"));
    }

    #[test]
    fn deny_svc_policy_is_main_policy_minus_gpl3_only() {
        let expected: Vec<String> = allowed_licenses(&repo_file("deny.toml"))
            .into_iter()
            .filter(|l| l != "GPL-3.0-only")
            .collect();
        assert_eq!(allowed_licenses(&repo_file("deny-svc.toml")), expected);
    }

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
