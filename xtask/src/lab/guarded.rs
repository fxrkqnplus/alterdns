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

/// Korunan değerleri okur (salt okunur PowerShell betiği; AST denetimli).
pub fn snapshot() -> anyhow::Result<Value> {
    let out = super::ops::ps_query(SCRIPT.trim_start_matches('\u{feff}'))?;
    Ok(serde_json::from_str(&out)?)
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
