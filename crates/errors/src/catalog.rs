// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hata kataloğunun (`errors/catalog.toml`) ayrıştırıcısı ve doğrulayıcısı.
//!
//! `build.rs` ve testler bu dosyayı `#[path]` ile içe alır; çalışma zamanı crate'ine girmez
//! (crate'in çalışma zamanı bağımlılığı yoktur). Kurallar `docs/ERROR_CATALOG.md` §1–2'dedir.

#![allow(dead_code)]

use std::collections::BTreeSet;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    error: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub code: String,
    pub name: String,
    pub area: String,
    pub severity: String,
    pub faz: u8,
    pub retryable: bool,
    pub user_action_required: bool,
    pub params: Vec<String>,
    pub docs: String,
    #[serde(default)]
    pub deprecated: bool,
    pub tr: Texts,
    pub en: Texts,
    #[serde(default)]
    pub tech: Option<Tech>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Texts {
    pub title: String,
    pub message: String,
    pub causes: Vec<String>,
    pub solutions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tech {
    pub fields: Vec<String>,
}

pub const AREAS: [&str; 14] = [
    "GEN", "IPC", "SVC", "SYS", "NET", "DPI", "DNS", "SCN", "QRN", "WEB", "REP", "UPD", "INS", "UI",
];
pub const SEVERITIES: [&str; 4] = ["info", "warning", "error", "critical"];
pub const MAX_FAZ: u8 = 7;

pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    toml::from_str::<Catalog>(text)
        .map(|c| c.error)
        .map_err(|e| e.to_string())
}

/// Bütün kural ihlalleri; boşsa katalog geçerlidir.
pub fn validate(entries: &[Entry]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut codes = BTreeSet::new();
    let mut names = BTreeSet::new();
    for e in entries {
        let at = &e.code;
        match code_area(&e.code) {
            Some(area) if area == e.area => {}
            Some(area) => problems.push(format!(
                "{at}: area \"{}\" koddaki alanla ({area}) aynı değil",
                e.area
            )),
            None => problems.push(format!("{at}: kod biçimi ADNS-<ALAN>-<NNN> değil")),
        }
        if !codes.insert(e.code.clone()) {
            problems.push(format!("{at}: yinelenen kod"));
        }
        if !names.insert(e.name.clone()) {
            problems.push(format!("{at}: yinelenen ad {}", e.name));
        }
        if !name_ok(&e.name, &e.area) {
            problems.push(format!(
                "{at}: ad \"{}\" PascalCase değil ya da alan önekiyle başlamıyor",
                e.name
            ));
        }
        if !SEVERITIES.contains(&e.severity.as_str()) {
            problems.push(format!("{at}: bilinmeyen önem \"{}\"", e.severity));
        }
        if e.faz > MAX_FAZ {
            problems.push(format!("{at}: faz {} > {MAX_FAZ}", e.faz));
        }
        let docs = format!("errors#{}", e.code.to_ascii_lowercase());
        if e.docs != docs {
            problems.push(format!(
                "{at}: docs \"{}\" yerine \"{docs}\" olmalı",
                e.docs
            ));
        }
        let mut sorted = e.params.clone();
        sorted.sort();
        sorted.dedup();
        if sorted != e.params {
            problems.push(format!("{at}: params sıralı ve tekrarsız olmalı"));
        }
        for (lang, t) in [("tr", &e.tr), ("en", &e.en)] {
            check_texts(at, lang, t, &e.params, &mut problems);
        }
        if e.tr.title == e.en.title {
            problems.push(format!("{at}: TR ve EN başlık aynı"));
        }
    }
    problems
}

fn check_texts(at: &str, lang: &str, t: &Texts, params: &[String], problems: &mut Vec<String>) {
    if t.title.trim().is_empty() || t.message.trim().is_empty() {
        problems.push(format!("{at} [{lang}]: başlık ya da mesaj boş"));
    }
    if t.causes.is_empty() || t.solutions.is_empty() {
        problems.push(format!(
            "{at} [{lang}]: en az bir neden ve bir çözüm gerekir"
        ));
    }
    if t.causes
        .iter()
        .chain(&t.solutions)
        .any(|s| s.trim().is_empty())
    {
        problems.push(format!("{at} [{lang}]: listede boş öğe var"));
    }
    let used: BTreeSet<String> = std::iter::once(&t.title)
        .chain(std::iter::once(&t.message))
        .chain(&t.causes)
        .chain(&t.solutions)
        .flat_map(|s| placeholders(s))
        .collect();
    let declared: BTreeSet<String> = params.iter().cloned().collect();
    if used != declared {
        problems.push(format!(
            "{at} [{lang}]: yer tutucular {used:?} params {declared:?} ile eşleşmiyor"
        ));
    }
    for s in &t.solutions {
        if lowers_security(s) {
            problems.push(format!("{at} [{lang}]: güvenliği azaltan öneri: \"{s}\""));
        }
    }
}

/// `ADNS-<ALAN>-<NNN>` ise alan; numara `000` olamaz.
pub fn code_area(code: &str) -> Option<&str> {
    let rest = code.strip_prefix("ADNS-")?;
    let (area, num) = rest.split_once('-')?;
    let digits_ok = num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()) && num != "000";
    (AREAS.contains(&area) && digits_ok).then_some(area)
}

/// PascalCase, ASCII harf ve rakam, alan önekiyle başlar (`IPC` → `Ipc`).
fn name_ok(name: &str, area: &str) -> bool {
    let mut prefix = String::new();
    for (i, c) in area.chars().enumerate() {
        prefix.push(if i == 0 { c } else { c.to_ascii_lowercase() });
    }
    name.starts_with(&prefix)
        && name.len() > prefix.len()
        && name.chars().all(|c| c.is_ascii_alphanumeric())
        && name[prefix.len()..].starts_with(|c: char| c.is_ascii_uppercase())
}

/// Metindeki `{ad}` yer tutucuları (`ad`: küçük ASCII harf, rakam, `_`).
pub fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    out.push(name.to_string());
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    out
}

const SECURITY_TERMS: [&str; 10] = [
    "antivirüs",
    "defender",
    "güvenlik duvar",
    "akıllı uygulama denetimi",
    "güvenlik yazılım",
    "antivirus",
    "firewall",
    "smart app control",
    "security software",
    "security product",
];
const TR_LOWERING: [&str; 6] = [
    "kapatın",
    "kapatabilirsiniz",
    "devre dışı bırakın",
    "istisna ekleyin",
    "hariç tutun",
    "kaldırın",
];
const EN_LOWERING: [&str; 6] = [
    "disable",
    "turn off",
    "switch off",
    "add an exception",
    "exclude",
    "uninstall",
];

/// Çözüm, bir güvenlik ürününü kapatmayı, devre dışı bırakmayı, istisna eklemeyi ya da
/// kaldırmayı öneriyor mu? Olumsuz uyarılar ("kapatmayın", "do not disable") serbesttir.
pub fn lowers_security(solution: &str) -> bool {
    let text = solution.to_lowercase();
    if !SECURITY_TERMS.iter().any(|t| text.contains(t)) {
        return false;
    }
    if TR_LOWERING.iter().any(|v| text.contains(v)) {
        return true;
    }
    let mut en = text;
    for verb in EN_LOWERING {
        for neg in ["do not ", "don't ", "never "] {
            en = en.replace(&format!("{neg}{verb}"), "");
        }
    }
    EN_LOWERING.iter().any(|v| en.contains(v))
}
