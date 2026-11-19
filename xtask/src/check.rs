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
    eprintln!("==> catalog-doc");
    crate::docs_errors::check_doc(&root)?;
    eprintln!("==> catalog-coverage");
    crate::docs_errors::coverage(&root, &repo_files(&root)?)?;
    eprintln!("==> scan");
    scan_repo(&root, &Source::Worktree)
}

/// Depoya girebilecek dosyalar (izlenenler + yok sayılmayan izlenmeyenler), göreli yollar.
pub fn repo_files(root: &Path) -> anyhow::Result<Vec<String>> {
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
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn repo_root() -> anyhow::Result<PathBuf> {
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

/// Taranacak içeriğin kaynağı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Çalışma ağacı: izlenen ve yok sayılmayan izlenmeyen dosyalar (yeni dosyalar `git add`
    /// öncesinde de yakalanır).
    Worktree,
    /// Git indeksi (`--staged`): commit'e hazırlanan içerik.
    Index,
    /// Bir commit'in ağacı (`--tree <rev>`): push edilecek içerik.
    Tree(String),
}

fn git(root: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("git {} çalıştırılamadı", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} başarısız: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// Git'e seçenek olarak yorumlanabilecek revizyonları reddeder (`-` ile başlayan).
fn safe_rev(rev: &str) -> anyhow::Result<&str> {
    if rev.is_empty() || rev.starts_with('-') {
        bail!("geçersiz revizyon: {rev:?}");
    }
    Ok(rev)
}

fn list(root: &Path, source: &Source) -> anyhow::Result<Vec<String>> {
    if let Source::Tree(rev) = source {
        safe_rev(rev)?;
    }
    let raw = match source {
        Source::Worktree => return repo_files(root),
        Source::Index => git(root, &["ls-files", "--cached", "-z"])?,
        Source::Tree(rev) => git(root, &["ls-tree", "-r", "-z", "--name-only", rev])?,
    };
    Ok(String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

fn read(root: &Path, source: &Source, rel: &str) -> anyhow::Result<Option<Vec<u8>>> {
    match source {
        Source::Worktree => match std::fs::read(root.join(rel)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("{rel} okunamadı")),
        },
        Source::Index => git(root, &["show", &format!(":{rel}")]).map(Some),
        Source::Tree(rev) => git(root, &["show", &format!("{rev}:{rel}")]).map(Some),
    }
}

fn finding(rel: &str, rule: scan::Rule) -> scan::Finding {
    scan::Finding {
        path: rel.to_string(),
        line: 0,
        rule,
        excerpt: String::new(),
    }
}

/// Kaynaktaki dosyaları tarar: (taranan dosya sayısı, bulgular).
pub fn scan_files(root: &Path, source: &Source) -> anyhow::Result<(usize, Vec<scan::Finding>)> {
    let entries = list(root, source)?
        .into_iter()
        .map(|rel| (source.clone(), rel))
        .collect::<Vec<_>>();
    scan_entries(root, &entries)
}

/// (kaynak, yol) çiftlerini tarar.
fn scan_entries(
    root: &Path,
    entries: &[(Source, String)],
) -> anyhow::Result<(usize, Vec<scan::Finding>)> {
    let ctx = ScanContext {
        allowlist: parse_allowlist(&read_optional(&root.join(ALLOWLIST_FILE))?),
        local_terms: parse_terms(&read_optional(&root.join(LOCAL_TERMS_FILE))?),
    };
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    for (source, rel) in entries {
        let rel = rel.as_str();
        if scan::forbidden_file(rel) {
            findings.push(finding(rel, scan::Rule::ForbiddenFile));
            continue;
        }
        let lower = rel.to_ascii_lowercase();
        if BINARY_EXTS.iter().any(|e| lower.ends_with(e)) {
            continue;
        }
        let Some(bytes) = read(root, source, rel)? else {
            continue;
        };
        scanned += 1;
        findings.extend(scan::scan_bytes_header(rel, &bytes));
        match scan::decode_text(&bytes) {
            Some(text) => findings.extend(scan::scan_text(rel, &text, &ctx)),
            None => findings.push(finding(rel, scan::Rule::UndecodableText)),
        }
    }
    Ok((scanned, findings))
}

fn report(label: &str, scanned: usize, findings: &[scan::Finding]) -> anyhow::Result<()> {
    eprintln!("scan ({label}): {scanned} dosya");
    if findings.is_empty() {
        return Ok(());
    }
    for f in findings {
        eprintln!("{}:{}: {:?} {}", f.path, f.line, f.rule, f.excerpt);
    }
    bail!("{} kişisel veri / politika bulgusu", findings.len())
}

/// Kişisel veri ve politika taraması; bulgu varsa başarısız olur.
pub fn scan_repo(root: &Path, source: &Source) -> anyhow::Result<()> {
    let (scanned, findings) = scan_files(root, source)?;
    report(&format!("{source:?}"), scanned, &findings)
}

fn is_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `pre-push` kancasının stdin satırları: `<yerel ref> <yerel sha> <uzak ref> <uzak sha>`.
/// Dönen: (yerel sha, uzak sha; uzakta yoksa `None`). Silmeler (yerel sha sıfır) ve onaltılık
/// olmayan değerler atlanır.
pub fn parse_push_lines(text: &str) -> Vec<(String, Option<String>)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let (local, remote) = (*f.get(1)?, *f.get(3)?);
            let zero = |s: &str| s.bytes().all(|b| b == b'0');
            if !is_sha(local) || !is_sha(remote) || zero(local) {
                return None;
            }
            Some((
                local.to_string(),
                (!zero(remote)).then(|| remote.to_string()),
            ))
        })
        .collect()
}

/// Push edilecek her commit'in eklediği ya da değiştirdiği dosyalar, o commit'teki halleriyle.
/// Böylece bir commit'te eklenip sonrakinde silinen bilgi de yakalanır.
fn pushed_entries(
    root: &Path,
    pushes: &[(String, Option<String>)],
) -> anyhow::Result<Vec<(Source, String)>> {
    let mut entries = Vec::new();
    for (local, remote) in pushes {
        let range = match remote {
            Some(r) => git(root, &["rev-list", &format!("{r}..{local}")])?,
            None => git(root, &["rev-list", local, "--not", "--remotes"])?,
        };
        for commit in String::from_utf8_lossy(&range)
            .lines()
            .filter(|c| is_sha(c))
        {
            let changed = git(
                root,
                &[
                    "diff-tree",
                    "--no-commit-id",
                    "-r",
                    "-z",
                    "--name-only",
                    "--diff-filter=d",
                    "--root",
                    commit,
                ],
            )?;
            for path in String::from_utf8_lossy(&changed)
                .split('\0')
                .filter(|p| !p.is_empty())
            {
                entries.push((Source::Tree(commit.to_string()), path.to_string()));
            }
        }
    }
    Ok(entries)
}

/// `cargo xtask check --pre-push`: kanca stdin'indeki her push için commit'leri tarar.
pub fn run_pre_push(mut input: impl std::io::Read) -> anyhow::Result<()> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    let root = repo_root()?;
    let entries = pushed_entries(&root, &parse_push_lines(&text))?;
    let (scanned, findings) = scan_entries(&root, &entries)?;
    report("pre-push", scanned, &findings)
}

/// `cargo xtask check --staged | --tree <rev>`: yalnızca tarama (push öncesi kanca için hızlı).
pub fn run_scan_only(source: &Source) -> anyhow::Result<()> {
    scan_repo(&repo_root()?, source)
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

    #[test]
    fn staged_scan_reads_index_not_worktree() {
        let dir = std::env::temp_dir().join(format!("alterdns-staged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).unwrap();
        let file = dir.join("not.txt");
        // Örnek adres parçalı kurulur; tarayıcı bu dosyayı yakalamasın.
        std::fs::write(&file, ["adres 192.168", ".7.9\n"].concat()).unwrap();
        git(&dir, &["add", "not.txt"]).unwrap();
        std::fs::write(&file, "temiz\n").unwrap();
        let (_, staged) = scan_files(&dir, &Source::Index).unwrap();
        assert!(
            staged.iter().any(|f| f.rule == scan::Rule::PrivateIpv4),
            "{staged:?}"
        );
        let (_, work) = scan_files(&dir, &Source::Worktree).unwrap();
        assert!(work.is_empty(), "{work:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_lines_skip_deletions_and_non_hex() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let z = "0".repeat(40);
        let text = format!(
            "refs/heads/x {a} refs/heads/x {b}\nrefs/heads/y {a} refs/heads/y {z}\nrefs/heads/d {z} refs/heads/d {b}\nrefs/heads/k --output=x refs/heads/k {b}\n"
        );
        assert_eq!(parse_push_lines(&text), [(a.clone(), Some(b)), (a, None)]);
        assert!(safe_rev("--output=x").is_err());
        assert!(safe_rev("HEAD").is_ok());
    }

    /// Bir commit'te eklenip sonrakinde silinen bilgi de yakalanır (inceleme bulgusu, 2026-11-20).
    #[test]
    fn pre_push_scans_every_pushed_commit() {
        let dir = std::env::temp_dir().join(format!("alterdns-prepush-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let commit = |msg: &str| {
            git(
                &dir,
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "commit",
                    "-q",
                    "-a",
                    "-m",
                    msg,
                ],
            )
            .unwrap();
        };
        git(&dir, &["init", "-q"]).unwrap();
        let file = dir.join("not.txt");
        std::fs::write(&file, ["adres 192.168", ".7.9\\n"].concat()).unwrap();
        git(&dir, &["add", "not.txt"]).unwrap();
        commit("bir");
        std::fs::write(&file, "temiz\\n").unwrap();
        commit("iki");
        let head = String::from_utf8(git(&dir, &["rev-parse", "HEAD"]).unwrap()).unwrap();
        let entries = pushed_entries(&dir, &[(head.trim().to_string(), None)]).unwrap();
        let (_, findings) = scan_entries(&dir, &entries).unwrap();
        assert!(
            findings.iter().any(|f| f.rule == scan::Rule::PrivateIpv4),
            "{findings:?}"
        );
        // Son ağaç temiz: yalnızca ağaç taraması bunu kaçırırdı.
        let (_, tip) = scan_files(&dir, &Source::Tree("HEAD".into())).unwrap();
        assert!(tip.is_empty(), "{tip:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
