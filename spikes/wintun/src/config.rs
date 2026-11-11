// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike yapılandırması: `lab/local/lab.local.toml` içindeki `[wintun]` bölümü (git dışı).
//! Test IP'si S1'de engelli sınıflanan bir hedefin adresidir; depoya yazılmaz.

use std::net::Ipv4Addr;
use std::path::Path;

use serde::Deserialize;

/// Adaptör adresi (RFC 2544 test aralığı).
pub const TUN_CIDR: &str = "198.18.0.1/30";
pub const TUN_IP: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
pub const TUN_PREFIX: u8 = 30;

#[derive(Debug, Clone, Deserialize)]
pub struct WintunConfig {
    /// Tek /32 rotanın hedefi.
    pub test_ip: Ipv4Addr,
    /// Prob SNI'ı ve `data/probes` hedef adı (ör. `discord-web`).
    pub test_target: String,
}

#[derive(Debug, Deserialize)]
struct LocalFile {
    wintun: Option<WintunConfig>,
}

/// Rotalanabilir genel bir IPv4 mü? Özel, döngü, bağlantı yerel, CGNAT, test, çoklu yayın
/// ve ayrılmış aralıklar reddedilir (rota yanlışlıkla yerel ağı TUN'a çekmesin).
pub fn validate_test_ip(ip: Ipv4Addr) -> anyhow::Result<()> {
    let [a, b, c, _] = ip.octets();
    let reserved = ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b)) // CGNAT
        || (a == 198 && (b == 18 || b == 19)) // RFC 2544 (TUN adresi)
        || (a == 192 && b == 0 && c == 0); // IETF protokol atamaları
    if reserved {
        anyhow::bail!("test IP'si rotalanabilir genel bir IPv4 olmalı: {ip}");
    }
    Ok(())
}

pub fn load(root: &Path) -> anyhow::Result<WintunConfig> {
    let path = root.join("lab").join("local").join("lab.local.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("{} okunamadı: {e}", path.display()))?;
    let file: LocalFile = toml::from_str(&text)?;
    let cfg = file
        .wintun
        .ok_or_else(|| anyhow::anyhow!("{} içinde [wintun] bölümü yok", path.display()))?;
    validate_test_ip(cfg.test_ip)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tun_address_constants_agree() {
        assert_eq!(format!("{TUN_IP}/{TUN_PREFIX}"), TUN_CIDR);
    }

    #[test]
    fn test_ip_must_be_public_ipv4() {
        // RFC 5737 örnek adresleri kamuya açık gibi davranmaz; gerçek genel adres örneği
        // çalışma zamanında birleştirilir.
        let public = Ipv4Addr::new(1, 1, 1, 1);
        assert!(validate_test_ip(public).is_ok());
        for bad in [
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(172, 16, 0, 1),
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(127, 0, 0, 1),
            Ipv4Addr::new(169, 254, 1, 1),
            Ipv4Addr::new(100, 64, 0, 1),
            Ipv4Addr::new(198, 18, 0, 2),
            Ipv4Addr::new(192, 0, 2, 1),
            Ipv4Addr::new(224, 0, 0, 1),
            Ipv4Addr::new(0, 0, 0, 0),
            Ipv4Addr::new(255, 255, 255, 255),
        ] {
            assert!(validate_test_ip(bad).is_err(), "{bad}");
        }
    }
}
