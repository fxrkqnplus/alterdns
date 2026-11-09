// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Public depoya girmemesi gereken içeriği yakalayan tarayıcı (ADR-013, SECURITY.md §5a).

/// Bulgunun türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    PrivateIpv4,
    UserProfilePath,
    Token,
    PrivateKey,
    Eicar,
    LocalTerm,
    MissingHeader,
    MissingBom,
}

/// Tek bir bulgu. `excerpt` maskelidir; bulunan değerin tamamı yazdırılmaz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub rule: Rule,
    pub excerpt: String,
}

/// Tarama bağlamı: izin verilen (yol, değer) çiftleri ve git dışı yerel terimler.
#[derive(Debug, Default)]
pub struct ScanContext {
    pub allowlist: Vec<(String, String)>,
    pub local_terms: Vec<String>,
}

use std::sync::LazyLock;

use regex::Regex;

/// `.rs` ve `.ts` dosyalarının ilk iki satırı (CLAUDE.md "Kod kuralları").
const HEADER_LINES: [&str; 2] = [
    "// AlterDNS — Copyright (C) 2026 Furkan Okan",
    "// SPDX-License-Identifier: GPL-3.0-or-later",
];

/// Kullanıcı adı sayılmayan profil dizinleri.
const NON_PERSONAL_PROFILES: [&str; 4] = ["public", "default", "default user", "all users"];

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

// Kalıplar derleme zamanında sabittir; hatalı kalıp birim testlerinde hemen görülür.
#[allow(clippy::expect_used)]
fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("sabit regex kalıbı geçerli olmalı")
}

static IPV4: LazyLock<Regex> = LazyLock::new(|| compile(r"\d{1,3}(?:\.\d{1,3}){3}"));
static PROFILE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r#"(?i)(?:\b[a-z]:[\\/]{1,2}|(?:^|[\s"'(=])/[a-z]/)users[\\/]{1,2}([^\\/\s<>"'%*?|:${}]+)"#,
    )
});
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{22,}|AKIA[0-9A-Z]{16})\b")
});
static PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"));

/// EICAR test dizesinin ayırt edici parçası; kaynakta bütün hâlde durmaz.
fn eicar_marker() -> String {
    ["EICAR-STANDARD-", "ANTIVIRUS-TEST-FILE"].concat()
}

fn mask(value: &str) -> String {
    let head: String = value.chars().take(2).collect();
    format!("{head}…")
}

fn has_extension(path: &str, exts: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    exts.iter().any(|e| lower.ends_with(e))
}

fn is_private_ipv4(octets: [u16; 4]) -> bool {
    match octets {
        [10, ..] => true,
        [172, b, ..] => (16..=31).contains(&b),
        [192, 168, ..] => true,
        _ => false,
    }
}

/// Eşleşme daha uzun bir noktalı sayı dizisinin parçasıysa (sürüm/derleme numarası) IP sayılmaz.
fn is_standalone(line: &str, start: usize, end: usize) -> bool {
    let before = line[..start].chars().next_back();
    let mut after = line[end..].chars();
    let a1 = after.next();
    let a2 = after.next();
    let before_ok = !matches!(before, Some(c) if c.is_ascii_digit() || c == '.');
    let after_ok = match (a1, a2) {
        (Some(c), _) if c.is_ascii_digit() => false,
        (Some('.'), Some(d)) if d.is_ascii_digit() => false,
        _ => true,
    };
    before_ok && after_ok
}

fn parse_octets(s: &str) -> Option<[u16; 4]> {
    let mut out = [0u16; 4];
    let mut parts = s.split('.');
    for slot in &mut out {
        let v: u16 = parts.next()?.parse().ok()?;
        if v > 255 {
            return None;
        }
        *slot = v;
    }
    Some(out)
}

/// Metin dosyasını satır satır tarar.
pub fn scan_text(path: &str, text: &str, ctx: &ScanContext) -> Vec<Finding> {
    let allowed = |value: &str| ctx.allowlist.iter().any(|(p, v)| p == path && v == value);
    let mut findings = Vec::new();
    let mut push = |line: usize, rule: Rule, value: &str| {
        findings.push(Finding {
            path: path.to_string(),
            line,
            rule,
            excerpt: mask(value),
        });
    };

    if has_extension(path, &[".rs", ".ts"]) {
        let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
        let ok = HEADER_LINES.iter().all(|h| lines.next() == Some(*h));
        if !ok {
            push(1, Rule::MissingHeader, "//");
        }
    }

    let eicar = eicar_marker();
    let terms: Vec<String> = ctx.local_terms.iter().map(|t| t.to_lowercase()).collect();

    for (idx, line) in text.lines().enumerate() {
        let n = idx + 1;
        for m in IPV4.find_iter(line) {
            let value = m.as_str();
            if is_standalone(line, m.start(), m.end())
                && parse_octets(value).is_some_and(is_private_ipv4)
                && !allowed(value)
            {
                push(n, Rule::PrivateIpv4, value);
            }
        }
        for c in PROFILE_PATH.captures_iter(line) {
            if let (Some(all), Some(name)) = (c.get(0), c.get(1)) {
                let lower = name.as_str().to_lowercase();
                if !NON_PERSONAL_PROFILES.contains(&lower.as_str()) && !allowed(all.as_str()) {
                    push(n, Rule::UserProfilePath, all.as_str().trim_start());
                }
            }
        }
        for m in TOKEN.find_iter(line) {
            push(n, Rule::Token, m.as_str());
        }
        for m in PRIVATE_KEY.find_iter(line) {
            push(n, Rule::PrivateKey, m.as_str());
        }
        if line.contains(&eicar) {
            push(n, Rule::Eicar, &eicar);
        }
        if !terms.is_empty() {
            let lower = line.to_lowercase();
            for t in &terms {
                if lower.contains(t.as_str()) {
                    push(n, Rule::LocalTerm, t);
                }
            }
        }
    }
    findings
}

/// Dosya başı denetimleri (ör. `.ps1` için UTF-8 BOM).
pub fn scan_bytes_header(path: &str, bytes: &[u8]) -> Option<Finding> {
    if has_extension(path, &[".ps1"]) && !bytes.starts_with(UTF8_BOM) {
        return Some(Finding {
            path: path.to_string(),
            line: 1,
            rule: Rule::MissingBom,
            excerpt: String::new(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "// AlterDNS — Copyright (C) 2026 Furkan Okan\n// SPDX-License-Identifier: GPL-3.0-or-later\n";

    fn ip(o: [u8; 4]) -> String {
        o.map(|x| x.to_string()).join(".")
    }

    fn rules_in(path: &str, s: &str, ctx: &ScanContext) -> Vec<Rule> {
        scan_text(path, s, ctx)
            .into_iter()
            .map(|f| f.rule)
            .collect()
    }

    fn rules(s: &str) -> Vec<Rule> {
        rules_in("notlar.txt", s, &ScanContext::default())
    }

    #[test]
    fn scan_flags_private_ipv4() {
        for o in [[10, 1, 2, 3], [172, 16, 5, 4], [192, 168, 0, 5]] {
            assert_eq!(
                rules(&format!("adres {} burada", ip(o))),
                vec![Rule::PrivateIpv4],
                "{o:?}"
            );
        }
        for o in [[77, 88, 8, 8], [1, 1, 1, 1], [172, 32, 0, 1]] {
            assert!(rules(&ip(o)).is_empty(), "{o:?}");
        }
    }

    #[test]
    fn scan_ignores_longer_dotted_numbers() {
        let build = ["10", "0", "26100", "1"].join(".");
        let five = ["10", "1", "2", "3", "4"].join(".");
        assert!(rules(&build).is_empty());
        assert!(rules(&five).is_empty());
    }

    #[test]
    fn scan_flags_user_profile_path() {
        assert_eq!(
            rules(&["C:", "Users", "bob", "x"].join("\\")),
            vec![Rule::UserProfilePath]
        );
        assert_eq!(
            rules(&["/c", "Users", "bob", "x"].join("/")),
            vec![Rule::UserProfilePath]
        );
        assert!(rules(&["C:", "Users", "<ad>"].join("\\")).is_empty());
        assert!(rules(&["C:", "Users", "Public"].join("\\")).is_empty());
        assert!(rules("%USERPROFILE%").is_empty());
    }

    #[test]
    fn scan_flags_tokens_and_keys() {
        assert_eq!(
            rules(&format!("t=ghp_{}", "a".repeat(36))),
            vec![Rule::Token]
        );
        assert_eq!(
            rules(&format!("github_pat_{}", "A".repeat(40))),
            vec![Rule::Token]
        );
        assert_eq!(
            rules(&format!("-----BEGIN {}PRIVATE KEY-----", "OPENSSH ")),
            vec![Rule::PrivateKey]
        );
    }

    #[test]
    fn scan_flags_eicar_marker() {
        let s = ["EICAR-STANDARD-", "ANTIVIRUS-TEST-FILE"].concat();
        assert_eq!(rules(&s), vec![Rule::Eicar]);
    }

    #[test]
    fn scan_flags_local_terms() {
        let ctx = ScanContext {
            local_terms: vec!["ozel-terim".into()],
            ..Default::default()
        };
        assert_eq!(
            rules_in("a.md", "x OZEL-TERIM y", &ctx),
            vec![Rule::LocalTerm]
        );
        assert!(rules_in("a.md", "x baska y", &ctx).is_empty());
    }

    #[test]
    fn scan_respects_allowlist() {
        let value = ip([192, 168, 1, 1]);
        let ctx = ScanContext {
            allowlist: vec![("docs/UI_SPEC.md".into(), value.clone())],
            ..Default::default()
        };
        assert!(rules_in("docs/UI_SPEC.md", &value, &ctx).is_empty());
        assert_eq!(
            rules_in("docs/OTHER.md", &value, &ctx),
            vec![Rule::PrivateIpv4]
        );
    }

    #[test]
    fn scan_masks_excerpt() {
        let value = ip([10, 9, 8, 7]);
        let f = scan_text("a.txt", &value, &ScanContext::default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 1);
        assert!(!f[0].excerpt.contains(&value));
        assert!(f[0].excerpt.starts_with("10"));
    }

    #[test]
    fn scan_requires_header_in_rs_and_ts() {
        let ctx = ScanContext::default();
        assert_eq!(
            rules_in("src/a.rs", "fn main() {}\n", &ctx),
            vec![Rule::MissingHeader]
        );
        assert_eq!(
            rules_in("ext/a.ts", "export {};\n", &ctx),
            vec![Rule::MissingHeader]
        );
        assert!(rules_in("src/a.rs", &format!("{HEADER}fn main() {{}}\n"), &ctx).is_empty());
        assert!(
            rules_in(
                "src/a.rs",
                &format!("{}fn main() {{}}\n", HEADER.replace('\n', "\r\n")),
                &ctx
            )
            .is_empty()
        );
        assert!(rules_in("docs/a.md", "başlıksız\n", &ctx).is_empty());
    }

    #[test]
    fn scan_requires_bom_in_ps1() {
        assert_eq!(
            scan_bytes_header("lab/a.ps1", b"Get-Item").map(|f| f.rule),
            Some(Rule::MissingBom)
        );
        assert_eq!(
            scan_bytes_header("lab/A.PS1", b"Get-Item").map(|f| f.rule),
            Some(Rule::MissingBom)
        );
        assert_eq!(
            scan_bytes_header("lab/a.ps1", b"\xEF\xBB\xBFGet-Item"),
            None
        );
        assert_eq!(scan_bytes_header("lab/a.txt", b"Get-Item"), None);
    }
}
