// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ölçüm yardımcıları: gecikme özeti, sürücü deposu sorgusu ve UDP gidiş-dönüş probu.

use serde::Serialize;
use xtask::lab::probe::ProbeResult;

/// Sıralı dizide en yakın sıra yöntemiyle yüzdelik (`p` 0–100).
pub fn percentile(sorted: &[u64], p: u8) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    // En yakın sıra: ceil(p/100 · n), 1 tabanlı.
    let rank = (usize::from(p.min(100)) * sorted.len())
        .div_ceil(100)
        .max(1);
    sorted.get(rank - 1).copied()
}

/// Bir prob dizisinin gecikme özeti (yalnızca başarılı problar sayılır).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Latency {
    pub n: usize,
    pub ok: usize,
    pub p50_total_ms: Option<u64>,
    pub p95_total_ms: Option<u64>,
    pub p50_tls_ms: Option<u64>,
    pub p95_tls_ms: Option<u64>,
}

impl Latency {
    pub fn from_results(rows: &[ProbeResult]) -> Latency {
        let ok: Vec<&ProbeResult> = rows.iter().filter(|r| r.outcome.is_ok()).collect();
        let sorted = |f: &dyn Fn(&ProbeResult) -> Option<u64>| {
            let mut v: Vec<u64> = ok.iter().filter_map(|r| f(r)).collect();
            v.sort_unstable();
            v
        };
        let total = sorted(&|r| Some(r.total_ms));
        let tls = sorted(&|r| r.tls_ms);
        Latency {
            n: rows.len(),
            ok: ok.len(),
            p50_total_ms: percentile(&total, 50),
            p95_total_ms: percentile(&total, 95),
            p50_tls_ms: percentile(&tls, 50),
            p95_tls_ms: percentile(&tls, 95),
        }
    }
}

/// `pnputil /enum-drivers` çıktısındaki Wintun sürücü paketlerinin yayımlanan adları
/// (`oemNN.inf`). Etiketler yerelleştirildiği için bloklar değerlerle tanınır.
pub fn wintun_driver_packages(pnputil_output: &str) -> Vec<String> {
    let text = pnputil_output.replace("\r\n", "\n");
    text.split("\n\n")
        .filter(|block| block.to_ascii_lowercase().contains("wintun.inf"))
        .filter_map(|block| {
            block
                .split(|c: char| c.is_whitespace())
                .map(str::to_ascii_lowercase)
                .find(|w| {
                    w.strip_prefix("oem")
                        .and_then(|r| r.strip_suffix(".inf"))
                        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                })
        })
        .collect()
}

/// Sürücü deposundaki Wintun paketleri (salt okunur sorgu).
pub fn driver_store() -> anyhow::Result<Vec<String>> {
    let out = std::process::Command::new(xtask::lab::ops::system32("pnputil.exe"))
        .arg("/enum-drivers")
        .output()?;
    Ok(wintun_driver_packages(&String::from_utf8_lossy(
        &out.stdout,
    )))
}

/// UDP gidiş-dönüş sonucu.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UdpProbe {
    pub sent: u32,
    pub replies: u32,
    pub first_reply_len: Option<usize>,
    /// İlk yanıt QUIC Version Negotiation biçiminde mi (uzun başlık, sürüm 0)?
    pub version_negotiation: bool,
    pub error: Option<String>,
}

/// `addr`'e sürüm anlaşmasını zorlayan QUIC biçimli 3 datagram gönderir ve yanıt bekler.
/// Sistem ayarı değiştirmez; hangi yoldan gideceğini o anki rota tablosu belirler.
pub fn udp_probe(addr: std::net::SocketAddr) -> UdpProbe {
    use std::time::Duration;
    let mut r = UdpProbe::default();
    let run = |r: &mut UdpProbe| -> std::io::Result<()> {
        let sock = std::net::UdpSocket::bind(("0.0.0.0", 0))?;
        sock.connect(addr)?;
        sock.set_read_timeout(Some(Duration::from_millis(700)))?;
        let mut buf = [0u8; 2048];
        for i in 0..3u8 {
            let dcid = [0xA0 | i, 1, 2, 3, 4, 5, 6, 7];
            sock.send(&crate::forward::quic_probe_datagram(
                dcid,
                crate::forward::QUIC_GREASE_VERSION,
            ))?;
            r.sent += 1;
            if let Ok(n) = sock.recv(&mut buf) {
                r.replies += 1;
                if r.first_reply_len.is_none() {
                    r.first_reply_len = Some(n);
                    r.version_negotiation = n >= 5
                        && buf.first().is_some_and(|b| b & 0x80 != 0)
                        && buf.get(1..5) == Some(&[0, 0, 0, 0][..]);
                }
            }
        }
        Ok(())
    };
    if let Err(e) = run(&mut r) {
        r.error = Some(e.to_string());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtask::lab::dns::DnsMode;
    use xtask::lab::probe::Outcome;
    use xtask::lab::sockettech::SockTech;

    #[test]
    fn percentile_nearest_rank() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50), Some(50));
        assert_eq!(percentile(&v, 95), Some(95));
        assert_eq!(percentile(&v, 100), Some(100));
        assert_eq!(percentile(&[7], 95), Some(7));
        assert_eq!(percentile(&[10, 20, 30], 50), Some(20));
        assert_eq!(percentile(&[], 50), None);
    }

    fn row(outcome: Outcome, total: u64, tls: Option<u64>) -> ProbeResult {
        ProbeResult {
            socket_ttl: None,
            target: "t".into(),
            domain: "t.example".into(),
            dns_mode: DnsMode::Fixed,
            tech: SockTech::None,
            addrs: Vec::new(),
            outcome,
            dns_ms: None,
            tcp_ms: Some(1),
            tls_ms: tls,
            total_ms: total,
            bytes_in: 0,
            detail: None,
        }
    }

    #[test]
    fn latency_counts_only_successful_probes() {
        let ok = Outcome::Ok { http_status: 200 };
        let rows = [
            row(ok, 300, Some(100)),
            row(ok, 100, Some(40)),
            row(Outcome::TlsReset, 5, None),
            row(ok, 200, Some(60)),
        ];
        let l = Latency::from_results(&rows);
        assert_eq!((l.n, l.ok), (4, 3));
        assert_eq!(l.p50_total_ms, Some(200));
        assert_eq!(l.p95_total_ms, Some(300));
        assert_eq!(l.p50_tls_ms, Some(60));
        let none = Latency::from_results(&[row(Outcome::TlsReset, 5, None)]);
        assert_eq!((none.ok, none.p50_total_ms), (0, None));
    }

    #[test]
    fn pnputil_parser_finds_wintun_packages_in_any_language() {
        let en = "Microsoft PnP Utility\r\n\r\n\
Published Name:     oem12.inf\r\n\
Original Name:      wintun.inf\r\n\
Provider Name:      WireGuard LLC\r\n\
Class Name:         Network adapters\r\n\r\n\
Published Name:     oem7.inf\r\n\
Original Name:      prnms009.inf\r\n\
Provider Name:      Microsoft\r\n\r\n";
        assert_eq!(wintun_driver_packages(en), vec!["oem12.inf".to_string()]);
        let tr = "Microsoft PnP Yardımcı Programı\n\n\
Yayımlanan Ad:      oem3.inf\n\
Özgün Ad:           netvmini.inf\n\n\
Yayımlanan Ad:      oem41.inf\n\
Özgün Ad:           Wintun.inf\n\
Sağlayıcı Adı:      WireGuard LLC\n";
        assert_eq!(wintun_driver_packages(tr), vec!["oem41.inf".to_string()]);
        assert!(wintun_driver_packages("").is_empty());
        assert!(wintun_driver_packages(en.split("oem7").next().unwrap()).len() == 1);
    }
}
