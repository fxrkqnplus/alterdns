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
    /// CGNAT (100.64/10) ya da link-local (169.254/16) IPv4.
    SpecialIpv4,
    /// Link-local (fe80/10) ya da benzersiz yerel (fc00/7) IPv6.
    LocalIpv6,
    Email,
    MacAddress,
    /// Depoya hiç girmemesi gereken dosya türü ya da yol (`.gitignore`'dan bağımsız).
    ForbiddenFile,
    /// Çözülemeyen metin (UTF-8 / UTF-16 değil ve ikili uzantılı da değil).
    UndecodableText,
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

/// `.rs` ve `.ts` dosyalarının ilk iki satırı (CONTRIBUTING.md "Kod kuralları").
const HEADER_LINES: [&str; 2] = [
    "// AlterDNS — Copyright (C) 2026 Furkan Okan",
    "// SPDX-License-Identifier: GPL-3.0-or-later",
];

/// Kullanıcı adı sayılmayan profil dizinleri.
const NON_PERSONAL_PROFILES: [&str; 4] = ["public", "default", "default user", "all users"];
/// `/home/<ad>` için kişisel sayılmayan adlar (CI ve örnek belgeler).
const NON_PERSONAL_HOMES: [&str; 3] = ["runner", "user", "ubuntu"];

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

// Kalıplar derleme zamanında sabittir; hatalı kalıp birim testlerinde hemen görülür.
#[allow(clippy::expect_used)]
fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("sabit regex kalıbı geçerli olmalı")
}

static IPV4: LazyLock<Regex> = LazyLock::new(|| compile(r"\d{1,3}(?:\.\d{1,3}){3}"));
static PROFILE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r#"(?i)(?:\b[a-z]:[\\/]{1,2}|\$[\\/]{1,2}|(?:^|[\s"'(=])/[a-z]/|/mnt/[a-z]/)users[\\/]{1,2}([^\\/\s<>"'%*?|:${}]+)"#,
    )
});
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?:gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{22,}|A(?:KIA|SIA)[0-9A-Z]{16}",
        r"|AIza[0-9A-Za-z_-]{35}|xox[baprs]-[A-Za-z0-9-]{10,}|npm_[A-Za-z0-9]{36}",
        r"|sk_live_[0-9A-Za-z]{24,}|glpat-[0-9A-Za-z_-]{20,}",
        r"|eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.)",
    ))
});
// Kalıp parçalı yazılır: bu dosya kendi kalıbını yakalamasın.
static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"-----BEGIN [A-Z ]*PRIVATE KEY(?: BLOCK)?-----|AGE-SECRET-KEY-1[0-9A-Z]{10,}",
        r"|untrusted comment: minisign (?:encrypted )?",
        r"secret key",
    ))
});
static HOME_PATH: LazyLock<Regex> =
    LazyLock::new(|| compile(r#"(?:^|[\s"'(=])/home/([^/\s<>"'%*?|:${}]+)"#));
static IPV6_LOCAL: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?i)\b(?:fe[89ab][0-9a-f]|f[cd][0-9a-f]{2})(?::[0-9a-f]{0,4}){2,7}")
});
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"\b([A-Za-z0-9._%+-]+)@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})\b")
});
static MAC: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"\b[0-9A-Fa-f]{2}(?::[0-9A-Fa-f]{2}){5}\b|\b[0-9A-Fa-f]{2}(?:-[0-9A-Fa-f]{2}){5}\b")
});

/// Büyük/küçük harf duyarsız karşılaştırma için katlama; Türkçe I/İ/ı/i aynı sayılır.
fn fold(s: &str) -> String {
    s.to_lowercase().replace('\u{307}', "").replace('ı', "i")
}

/// Eşleşme daha uzun bir onaltılık dizinin (parmak izi, uzun IPv6) parçası değilse.
fn hex_standalone(line: &str, start: usize, end: usize) -> bool {
    let sep = |c: char| c == ':' || c == '-';
    let before = line[..start].chars().next_back();
    let mut after = line[end..].chars();
    let before_ok = !matches!(before, Some(c) if sep(c) || c.is_ascii_hexdigit());
    let after_ok = !matches!((after.next(), after.next()), (Some(c), Some(d)) if sep(c) && d.is_ascii_hexdigit());
    before_ok && after_ok
}

/// Kişisel olmayan e-posta: GitHub noreply, `noreply`/`git` yerel adı, RFC 2606 örnek alanları.
fn is_impersonal_email(local: &str, domain: &str) -> bool {
    let local = local.to_ascii_lowercase();
    let domain = domain.to_ascii_lowercase();
    domain.ends_with("noreply.github.com")
        || ["noreply", "no-reply", "git"].contains(&local.as_str())
        || ["example.com", "example.org", "example.net"].contains(&domain.as_str())
}

/// Depoya hiç girmemesi gereken dosya türleri ve yollar (S-4). `.gitignore`'dan bağımsızdır:
/// `git add -f` ile eklenen dosya da yakalanır.
const FORBIDDEN_EXTS: [&str; 17] = [
    ".pfx",
    ".p12",
    ".key",
    ".pem",
    ".snk",
    ".sec",
    ".keystore",
    ".cer",
    ".etl",
    ".pcapng",
    ".pcap",
    ".evtx",
    ".dmp",
    ".har",
    ".ppk",
    ".jks",
    ".p8",
];
/// Uzantısız ya da tam adıyla yasak dosyalar; `.env.<ad>` de yasaktır (`.example`, `.sample`,
/// `.template` hariç).
const FORBIDDEN_NAMES: [&str; 8] = [
    ".env",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".npmrc",
    ".netrc",
    ".pypirc",
];
const FORBIDDEN_DIRS: [&str; 2] = ["lab/results/", "lab/local/"];

pub fn forbidden_file(path: &str) -> bool {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let name = p.rsplit('/').next().unwrap_or(&p);
    let env_variant = name.starts_with(".env.")
        && ![".example", ".sample", ".template"]
            .iter()
            .any(|s| name.ends_with(s));
    FORBIDDEN_EXTS.iter().any(|e| name.ends_with(e))
        || FORBIDDEN_NAMES.contains(&name)
        || env_variant
        || FORBIDDEN_DIRS.iter().any(|d| p.starts_with(d))
}

/// Metin çözümü: UTF-8 (BOM'lu ya da BOM'suz) ve BOM'lu UTF-16 LE/BE. Çözülemezse `None`.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    let utf16 = |rest: &[u8], le: bool| -> Option<String> {
        let (pairs, tail) = rest.as_chunks::<2>();
        if !tail.is_empty() {
            return None;
        }
        let units: Vec<u16> = pairs
            .iter()
            .map(|&pair| {
                if le {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
            .collect();
        String::from_utf16(&units).ok()
    };
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        return std::str::from_utf8(rest).ok().map(str::to_string);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    let text = std::str::from_utf8(bytes).ok()?;
    if !text.contains('\0') {
        return Some(text.to_string());
    }
    // NUL içeren "UTF-8" büyük olasılıkla BOM'suz UTF-16 LE'dir; öyle çözülemezse çözülemez.
    utf16(bytes, true).filter(|t| !t.contains('\0'))
}

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

/// CGNAT (100.64/10) ve link-local (169.254/16) adresleri (mobil operatör ağları).
fn is_special_ipv4(octets: [u16; 4]) -> bool {
    matches!(octets, [100, b, ..] if (64..=127).contains(&b)) || matches!(octets, [169, 254, ..])
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
    let terms: Vec<String> = ctx.local_terms.iter().map(|t| fold(t)).collect();

    for (idx, line) in text.lines().enumerate() {
        let n = idx + 1;
        for m in IPV4.find_iter(line) {
            let value = m.as_str();
            if !is_standalone(line, m.start(), m.end()) || allowed(value) {
                continue;
            }
            match parse_octets(value) {
                Some(o) if is_private_ipv4(o) => push(n, Rule::PrivateIpv4, value),
                Some(o) if is_special_ipv4(o) => push(n, Rule::SpecialIpv4, value),
                _ => {}
            }
        }
        for m in IPV6_LOCAL.find_iter(line) {
            if hex_standalone(line, m.start(), m.end()) && !allowed(m.as_str()) {
                push(n, Rule::LocalIpv6, m.as_str());
            }
        }
        for c in EMAIL.captures_iter(line) {
            if let (Some(all), Some(local), Some(domain)) = (c.get(0), c.get(1), c.get(2))
                && !is_impersonal_email(local.as_str(), domain.as_str())
                && !allowed(all.as_str())
            {
                push(n, Rule::Email, all.as_str());
            }
        }
        for m in MAC.find_iter(line) {
            if hex_standalone(line, m.start(), m.end()) && !allowed(m.as_str()) {
                push(n, Rule::MacAddress, m.as_str());
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
        for c in HOME_PATH.captures_iter(line) {
            if let (Some(all), Some(name)) = (c.get(0), c.get(1)) {
                let lower = name.as_str().to_lowercase();
                if !NON_PERSONAL_HOMES.contains(&lower.as_str())
                    && !allowed(all.as_str().trim_start())
                {
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
            let lower = fold(line);
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

    // Örnek değerler parçalı kurulur; tarayıcı bu dosyayı yakalamasın.
    fn scan_one(text: &str) -> Vec<Rule> {
        scan_text("ornek.txt", text, &ScanContext::default())
            .into_iter()
            .map(|f| f.rule)
            .collect()
    }

    #[test]
    fn scan_flags_cgnat_and_link_local() {
        assert_eq!(scan_one(&["100.64", ".1.2"].concat()), [Rule::SpecialIpv4]);
        assert_eq!(scan_one(&["169.254", ".3.4"].concat()), [Rule::SpecialIpv4]);
        assert!(scan_one(&["100.63", ".0.1"].concat()).is_empty());
        assert!(scan_one(&["sürüm 1.169", ".254.3.4"].concat()).is_empty());
    }

    #[test]
    fn scan_flags_local_ipv6() {
        assert_eq!(scan_one(&["fe80", "::1"].concat()), [Rule::LocalIpv6]);
        assert_eq!(scan_one(&["fd12", ":3456::1"].concat()), [Rule::LocalIpv6]);
        assert!(scan_one(&["2001", ":db8::1"].concat()).is_empty());
        assert!(scan_one("::1").is_empty());
        assert!(scan_one("-notlike 'fe80*'").is_empty());
    }

    #[test]
    fn scan_flags_email_but_not_noreply() {
        assert_eq!(scan_one(&["ad", "@ornek.com"].concat()), [Rule::Email]);
        assert!(scan_one(&["123+ad", "@users.noreply.github.com"].concat()).is_empty());
        assert!(scan_one(&["noreply", "@ornek.com"].concat()).is_empty());
        assert!(scan_one(&["git", "@github.com:owner/repo.git"].concat()).is_empty());
        let ctx = ScanContext {
            allowlist: vec![("ornek.txt".into(), ["security", "@ornek.com"].concat())],
            local_terms: vec![],
        };
        assert!(scan_text("ornek.txt", &["security", "@ornek.com"].concat(), &ctx).is_empty());
    }

    #[test]
    fn scan_flags_mac_address() {
        assert_eq!(
            scan_one(&["AA-BB-CC", "-11-22-33"].concat()),
            [Rule::MacAddress]
        );
        assert_eq!(
            scan_one(&["aa:bb:cc", ":11:22:33"].concat()),
            [Rule::MacAddress]
        );
        assert!(scan_one("saat 12:34:56, sürüm 1.2.3.4.5.6").is_empty());
        assert!(scan_one("7c2e4b61-9d3a-4f58-b0e7-5a1cd2f48e93").is_empty());
    }

    #[test]
    fn scan_flags_extra_token_formats() {
        let samples = [
            ["AI", "za", &"A".repeat(35)].concat(),
            ["xo", "xb-", &"1".repeat(12)].concat(),
            ["np", "m_", &"a".repeat(36)].concat(),
            [
                "ey",
                "J",
                &"a".repeat(12),
                ".ey",
                "J",
                &"b".repeat(12),
                ".sig",
            ]
            .concat(),
        ];
        for s in &samples {
            assert_eq!(scan_one(s), [Rule::Token], "{s}");
        }
        assert_eq!(
            scan_one(&["AGE-SECRET", "-KEY-1", &"Q".repeat(20)].concat()),
            [Rule::PrivateKey]
        );
        assert_eq!(
            scan_one(&["untrusted comment: minisign", " secret key"].concat()),
            [Rule::PrivateKey]
        );
    }

    #[test]
    fn scan_decodes_utf16_and_flags_private_ip() {
        let text = ["IPv4: 192.168", ".1.5\r\n"].concat();
        let mut bytes = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let decoded = decode_text(&bytes).unwrap();
        assert_eq!(scan_one(&decoded), [Rule::PrivateIpv4]);
        assert!(decode_text(&[0xC3, 0x28]).is_none());
        assert_eq!(decode_text("düz".as_bytes()).as_deref(), Some("düz"));
    }

    #[test]
    fn forbidden_files_are_findings() {
        for p in [
            "x/imza.pfx",
            "a.p12",
            "k.key",
            "c.pem",
            "s.snk",
            "k.sec",
            "a.keystore",
            "c.cer",
            ".env",
            "iz.etl",
            "y.pcapng",
            "o.evtx",
            "b.dmp",
            "t.har",
            "z.pcap",
            "p.ppk",
            "s.jks",
            "a.p8",
            "home/.ssh/id_rsa",
            "id_ed25519",
            ".npmrc",
            ".netrc",
            ".env.local",
            "app/.env.production",
            "lab/results/s1/rapor.md",
            "lab/local/notlar.txt",
        ] {
            assert!(forbidden_file(p), "{p}");
        }
        for p in [
            "lab/local.example.toml",
            "docs/keys.md",
            "src/env.rs",
            "lab/RUNBOOK.md",
            "tools/key",
            "env",
            ".env.example",
        ] {
            assert!(!forbidden_file(p), "{p}");
        }
    }

    #[test]
    fn scan_review_fixes_2026_11_20() {
        // BOM'suz UTF-16 LE.
        let mut bytes = Vec::new();
        for u in ["adres 10.1", ".2.3"].concat().encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(scan_one(&decode_text(&bytes).unwrap()), [Rule::PrivateIpv4]);
        // PGP blok ve şifreli minisign anahtarı.
        assert_eq!(
            scan_one(&["-----BEGIN PGP PRIVATE", " KEY BLOCK-----"].concat()),
            [Rule::PrivateKey]
        );
        assert_eq!(
            scan_one(&["untrusted comment: minisign", " encrypted secret key"].concat()),
            [Rule::PrivateKey]
        );
        // Kelime içindeki token ve yeni biçimler.
        assert_eq!(
            scan_one(&["X_gh", "p_", &"a".repeat(36)].concat()),
            [Rule::Token]
        );
        assert_eq!(
            scan_one(&["sk_li", "ve_", &"a".repeat(24)].concat()),
            [Rule::Token]
        );
        assert_eq!(
            scan_one(&["glp", "at-", &"a".repeat(20)].concat()),
            [Rule::Token]
        );
        // WSL, UNC ve /home profil yolları.
        let users = ["Us", "ers"].concat();
        assert_eq!(
            scan_one(&format!("/mnt/c/{users}/ali/x")),
            [Rule::UserProfilePath]
        );
        assert_eq!(
            scan_one(&format!("\\\\pc\\c$\\{users}\\ali")),
            [Rule::UserProfilePath]
        );
        assert_eq!(
            scan_one(&["cd /ho", "me/ali/proje"].concat()),
            [Rule::UserProfilePath]
        );
        assert!(scan_one(&["cd /ho", "me/runner/work"].concat()).is_empty());
        // Parmak izi ve uzun IPv6 yanlış pozitif değil.
        let fp = vec!["aa"; 20].join(":");
        assert!(scan_one(&fp).is_empty(), "{fp}");
        assert!(scan_one(&["2001:db8:fe", "80:1::"].concat()).is_empty());
        // Türkçe I/İ: yerel terim büyük harfle de yakalanır.
        let ctx = ScanContext {
            allowlist: vec![],
            local_terms: vec!["ışık".into()],
        };
        assert_eq!(
            scan_text("ornek.txt", "IŞIK burada", &ctx)
                .into_iter()
                .map(|f| f.rule)
                .collect::<Vec<_>>(),
            [Rule::LocalTerm]
        );
    }
}
