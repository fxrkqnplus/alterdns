// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! S4 (gerçek istemciler): hedef listesi (`data/s4/hedefler.toml`) ve Windows DNS
//! önbelleğinden tek IP öğrenme. Önbellek yalnızca okunur (`Get-DnsClientCache`).

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::path::Path;

use serde::Deserialize;

use crate::config::validate_test_ip;

#[derive(Debug, Clone, Deserialize)]
pub struct S4Target {
    pub ad: String,
    /// Başta sistem çözücüsüyle çözülüp /32 olarak yönlendirilen adlar.
    pub alan_adlari: Vec<String>,
    /// DNS önbelleğinde bu soneklerle biten adların A kayıtları öğrenilir.
    pub sonekler: Vec<String>,
    /// Yalnızca sağlayıcının kendi AS'i (paylaşımlı CDN'de boş).
    #[serde(default)]
    pub onekler: Vec<String>,
}

#[derive(Deserialize)]
struct S4File {
    hedef: Vec<S4Target>,
}

pub fn load(root: &Path) -> anyhow::Result<Vec<S4Target>> {
    let path = root.join("data").join("s4").join("hedefler.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("{} okunamadı: {e}", path.display()))?;
    let targets = toml::from_str::<S4File>(&text)?.hedef;
    validate_targets(&targets)?;
    Ok(targets)
}

/// Sonekler en az iki etiketli, yalnızca harf, rakam ve tire içeren alan adları olmalı: `com`
/// gibi bir sonek keyfi kayıtları öğretirdi (2026-11-12 güvenlik incelemesi).
pub fn validate_targets(targets: &[S4Target]) -> anyhow::Result<()> {
    for s in targets.iter().flat_map(|t| &t.sonekler) {
        let labels: Vec<&str> = s.split('.').collect();
        let ok = labels.len() >= 2
            && labels
                .iter()
                .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        if !ok {
            anyhow::bail!("geçersiz sonek (en az iki etiketli alan adı olmalı): {s:?}");
        }
    }
    Ok(())
}

/// Bütün hedeflerin önekleri. Kural `xtask` rota denetimiyle aynı: /16…/32, ağ biçiminde,
/// ilk ve son adres rotalanabilir genel IPv4 (rota yazımı ayrıca yeniden denetlenir).
pub fn prefixes(targets: &[S4Target]) -> anyhow::Result<Vec<(Ipv4Addr, u8)>> {
    let mut out = Vec::new();
    for p in targets.iter().flat_map(|t| &t.onekler) {
        let (ip, len) = p
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("önek uzunluğu yok: {p}"))?;
        let ip: Ipv4Addr = ip
            .parse()
            .map_err(|_| anyhow::anyhow!("geçersiz önek: {p}"))?;
        let len: u8 = len
            .parse()
            .map_err(|_| anyhow::anyhow!("geçersiz önek: {p}"))?;
        if !(16..=32).contains(&len) {
            anyhow::bail!("önek /16…/32 olmalı: {p}");
        }
        let host = u32::MAX.checked_shr(u32::from(len)).unwrap_or(0);
        if u32::from(ip) & host != 0 {
            anyhow::bail!("önek ağ biçiminde değil: {p}");
        }
        validate_test_ip(ip)?;
        validate_test_ip(Ipv4Addr::from(u32::from(ip) | host))?;
        out.push((ip, len));
    }
    Ok(out)
}

/// Ad, soneklerden birine eşit mi ya da onun alt alanı mı (etiket sınırıyla)?
pub fn suffix_match(name: &str, suffixes: &[String]) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    suffixes.iter().any(|s| {
        let s = s.trim_end_matches('.').to_ascii_lowercase();
        name == s || name.ends_with(&format!(".{s}"))
    })
}

/// Olumsuz önbellek kayıtlarında alanlar `null` olabilir.
#[derive(Deserialize)]
struct CacheRow {
    #[serde(rename = "Entry", default)]
    entry: Option<String>,
    #[serde(rename = "Name", default)]
    name: Option<String>,
    #[serde(rename = "Type", default)]
    kind: Option<u16>,
    #[serde(rename = "Data", default)]
    data: Option<String>,
}

/// `Get-DnsClientCache | Select-Object Entry, Name, Type, Data | ConvertTo-Json` çıktısından,
/// adı sonekle eşleşen A kayıtlarının rotalanabilir genel IPv4 adresleri (sıralı, tekil).
pub fn learn_from_dns_cache(json: &str, suffixes: &[String]) -> anyhow::Result<Vec<Ipv4Addr>> {
    let json = json.trim();
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<CacheRow> = if json.starts_with('[') {
        serde_json::from_str(json)?
    } else {
        vec![serde_json::from_str(json)?]
    };
    let mut out = BTreeSet::new();
    for r in rows {
        let named = |n: &Option<String>| n.as_deref().is_some_and(|n| suffix_match(n, suffixes));
        if r.kind != Some(1) || !(named(&r.entry) || named(&r.name)) {
            continue;
        }
        if let Some(Ok(ip)) = r.data.as_deref().map(str::parse::<Ipv4Addr>)
            && validate_test_ip(ip).is_ok()
        {
            out.insert(ip);
        }
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn sfx(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn repo_targets_follow_prefix_rule() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let targets = load(&root).unwrap();
        let names: Vec<&str> = targets.iter().map(|t| t.ad.as_str()).collect();
        assert_eq!(names, ["discord", "roblox", "wattpad"]);
        // Paylaşımlı CDN'lerde önek yok.
        for t in &targets {
            if t.ad != "roblox" {
                assert!(t.onekler.is_empty(), "{}", t.ad);
            }
        }
        let prefixes = prefixes(&targets).unwrap();
        assert_eq!(prefixes.len(), 4);
        assert!(prefixes.contains(&(Ipv4Addr::new(128, 116, 0, 0), 17)));
    }

    #[test]
    fn bad_prefixes_are_rejected() {
        let t = |p: &str| S4Target {
            ad: "x".into(),
            alan_adlari: Vec::new(),
            sonekler: Vec::new(),
            onekler: vec![p.to_string()],
        };
        for bad in [
            "128.116.1.0/17",
            "128.0.0.0/15",
            "100.64.0.0/16",
            "198.18.0.0/16",
            "1.1.1.1",
            "x/24",
        ] {
            assert!(prefixes(&[t(bad)]).is_err(), "{bad}");
        }
    }

    #[test]
    fn suffix_match_respects_label_boundary() {
        let s = sfx(&["discord.media"]);
        assert!(suffix_match("discord.media", &s));
        assert!(suffix_match("c-ist01-abc.discord.media", &s));
        assert!(suffix_match("C-IST01.Discord.Media.", &s));
        assert!(!suffix_match("evil-discord.media", &s));
        assert!(!suffix_match("discord.media.evil.com", &s));
    }

    #[test]
    fn learns_public_a_records_for_matching_names() {
        let json = r#"[
            {"Entry":"c-ist01.discord.media","Name":"c-ist01.discord.media","Type":1,"Data":"162.159.128.233"},
            {"Entry":"c-ist01.discord.media","Name":"c-ist01.discord.media","Type":1,"Data":"162.159.128.233"},
            {"Entry":"www.roblox.com","Name":"www.roblox.com","Type":5,"Data":"www.roblox.com.cdn.example"},
            {"Entry":"apis.roblox.com","Name":"apis.roblox.com","Type":1,"Data":"100.64.2.3"},
            {"Entry":"evil-discord.media","Name":"evil-discord.media","Type":1,"Data":"8.8.4.4"},
            {"Entry":"wattpad.com","Name":"wattpad.com","Type":28,"Data":"2600:9000::1"},
            {"Entry":"WWW.WATTPAD.COM","Name":"www.wattpad.com","Type":1,"Data":"13.32.12.52"}
        ]"#;
        let got = learn_from_dns_cache(json, &sfx(&["discord.media", "roblox.com", "wattpad.com"]))
            .unwrap();
        assert_eq!(
            got,
            [
                Ipv4Addr::new(13, 32, 12, 52),
                Ipv4Addr::new(162, 159, 128, 233)
            ]
        );
    }

    /// Olumsuz önbellek kayıtlarında alanlar `null` olabilir; dizinin tamamı düşmemeli.
    #[test]
    fn null_fields_are_tolerated() {
        let json = r#"[
            {"Entry":null,"Name":null,"Type":1,"Data":null},
            {"Entry":"wattpad.com","Name":"wattpad.com","Type":null,"Data":"13.32.12.31"},
            {"Entry":"wattpad.com","Name":"wattpad.com","Type":1,"Data":"13.32.12.43"}
        ]"#;
        assert_eq!(
            learn_from_dns_cache(json, &sfx(&["wattpad.com"])).unwrap(),
            [Ipv4Addr::new(13, 32, 12, 43)]
        );
    }

    /// Tek etiketli ya da biçimsiz sonek (ör. `com`) keyfi kayıt öğrenirdi.
    #[test]
    fn suffixes_need_two_labels() {
        let t = |s: &str| S4Target {
            ad: "x".into(),
            alan_adlari: Vec::new(),
            sonekler: vec![s.to_string()],
            onekler: Vec::new(),
        };
        assert!(validate_targets(&[t("discord.media")]).is_ok());
        for bad in [
            "com",
            ".com",
            "discord..media",
            "",
            "dis cord.media",
            "*.discord.media",
        ] {
            assert!(validate_targets(&[t(bad)]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn single_object_and_empty_cache_are_handled() {
        let one = r#"{"Entry":"wattpad.com","Name":"wattpad.com","Type":1,"Data":"13.32.12.31"}"#;
        assert_eq!(
            learn_from_dns_cache(one, &sfx(&["wattpad.com"])).unwrap(),
            [Ipv4Addr::new(13, 32, 12, 31)]
        );
        assert!(
            learn_from_dns_cache("", &sfx(&["wattpad.com"]))
                .unwrap()
                .is_empty()
        );
    }
}
