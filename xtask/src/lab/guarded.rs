// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Korunan değerler (ADR-014): kullanıcıya veya başka yazılımlara ait, makineden makineye
//! değişen sistem değerleri. Her lab senaryosu başında ve sonunda bunların anlık görüntüsü
//! alınır; fark varsa senaryo başarısız sayılır. Değerler yalnızca okunur, otomatik
//! düzeltme yapılmaz (değişikliği başka bir program yapmış olabilir).

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;

/// Anlık görüntünün bölümleri (betikteki anahtarlar).
pub const SECTIONS: [&str; 9] = [
    "hop_limit",
    "arayuzler",
    "varsayilan_rotalar",
    "dns",
    "doh",
    "nrpt",
    "proxy_wininet",
    "proxy_winhttp",
    "guvenlik_duvari",
];

pub fn script_path() -> PathBuf {
    super::cli::repo_root()
        .join("lab")
        .join("scripts")
        .join("korunan-degerler.ps1")
}

/// Betik ikiliye gömülür: yükseltilmiş işlem diskteki (kullanıcının yazabildiği) dosyayı değil,
/// derlemede denetlenmiş metni çalıştırır (2026-11-12 güvenlik incelemesi).
const SCRIPT: &str = include_str!("../../../lab/scripts/korunan-degerler.ps1");

/// Korunan değerleri okur (salt okunur PowerShell betiği; AST denetimli). Eksik ya da
/// okunamayan bir bölüm varsa hata döner: sessizce boş kalan bir bölüm sahte bir "değişmedi"
/// sonucu vermesin (2026-11-15 güvenlik incelemesi).
pub fn snapshot() -> anyhow::Result<Value> {
    let out = super::ops::ps_query(SCRIPT.trim_start_matches('\u{feff}'))?;
    let value: Value = serde_json::from_str(&out)?;
    let bad = unreadable(&value);
    if !bad.is_empty() {
        anyhow::bail!("okunamayan bölümler: {}", bad.join("; "));
    }
    Ok(value)
}

/// Eksik bölümler ve betiğin `{"hata": …}` yazdığı bölümler (`bölüm: neden`).
pub fn unreadable(snapshot: &Value) -> Vec<String> {
    let error_of = |v: &Value| v.get("hata").map(|h| h.as_str().unwrap_or("?").to_string());
    SECTIONS
        .iter()
        .filter_map(|key| match snapshot.get(*key) {
            None => Some(format!("{key}: yok")),
            Some(Value::Array(items)) => items
                .iter()
                .find_map(error_of)
                .map(|e| format!("{key}: {e}")),
            Some(v) => error_of(v).map(|e| format!("{key}: {e}")),
        })
        .collect()
}

/// Değeri farklı olan, eksik ya da yeni bölümlerin adları (sıralı).
pub fn diff(before: &Value, after: &Value) -> Vec<String> {
    let (Some(b), Some(a)) = (before.as_object(), after.as_object()) else {
        return vec!["<anlık görüntü>".to_string()];
    };
    let keys: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
    keys.into_iter()
        .filter(|k| b.get(*k) != a.get(*k))
        .map(|k| (*k).clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_snapshots_have_no_diff() {
        let a = json!({ "hop_limit": { "v4": 65, "v6": 128 }, "dns": [] });
        assert!(diff(&a, &a.clone()).is_empty());
    }

    #[test]
    fn changed_missing_and_added_sections_are_reported() {
        let before = json!({ "hop_limit": { "v4": 65 }, "dns": ["a"], "doh": [] });
        let after = json!({ "hop_limit": { "v4": 128 }, "dns": ["a"], "proxy_wininet": {} });
        assert_eq!(diff(&before, &after), ["doh", "hop_limit", "proxy_wininet"]);
    }

    #[test]
    fn non_object_snapshot_is_a_diff() {
        assert_eq!(diff(&json!({}), &json!(null)), ["<anlık görüntü>"]);
    }

    #[test]
    fn missing_or_failed_sections_are_unreadable() {
        let mut full = serde_json::Map::new();
        for key in SECTIONS {
            full.insert(key.to_string(), json!([]));
        }
        let ok = Value::Object(full.clone());
        assert!(unreadable(&ok).is_empty());

        let mut bad = full.clone();
        bad.remove("nrpt");
        bad.insert("hop_limit".into(), json!({ "hata": "erişim reddedildi" }));
        bad.insert("dns".into(), json!([{ "hata": "CIM hatası" }]));
        assert_eq!(
            unreadable(&Value::Object(bad)),
            [
                "hop_limit: erişim reddedildi",
                "dns: CIM hatası",
                "nrpt: yok"
            ]
        );
        // Bir değerin içinde geçen "hata" sözcüğü bölüm hatası değildir.
        let mut odd = full;
        odd.insert("doh".into(), json!([{ "DohTemplate": "hata" }]));
        assert!(unreadable(&Value::Object(odd)).is_empty());
    }

    #[test]
    fn embedded_script_is_the_audited_file() {
        assert_eq!(std::fs::read_to_string(script_path()).unwrap(), SCRIPT);
    }

    /// Betik yalnızca okur ve ADR-014'ün ağ değerlerini kapsar.
    #[test]
    fn snapshot_script_is_read_only_and_covers_sections() {
        let path = script_path();
        let violations = super::super::scenario::ps_script_violations(&path).unwrap();
        assert!(violations.is_empty(), "{violations:?}");
        let raw = std::fs::read_to_string(&path).unwrap();
        for key in SECTIONS {
            assert!(raw.contains(key), "{key}");
        }
    }
}
