// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! S4 `hold` kipinin saf kuralları: argümanlar, zamanlama, öğrenme bütçesi ve kullanıcı
//! yanıtları. Sistemle konuşan kısım `main.rs` içindedir.

use std::time::Duration;

use anyhow::bail;
use xtask::lab::sockettech::{SockTech, SplitAt};

/// Bir `hold` oturumunun üst sınırı (Görev 9 planı: en çok 10 dk).
pub const MAX_HOLD: Duration = Duration::from_secs(600);
/// Kontrol probu aralığı; kira yalnızca prob geçerse yenilenir (kira ≤ 60 sn).
pub const PROBE_EVERY: Duration = Duration::from_secs(20);
/// DNS önbelleği okuma aralığı (salt okunur `Get-DnsClientCache`).
pub const LEARN_EVERY: Duration = Duration::from_secs(5);
/// Çalışırken öğrenilecek tek IP rotalarının üst sınırı.
pub const MAX_LEARNED: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldArgs {
    pub tech: SockTech,
    pub max: Duration,
}

/// `hold [--tech tls-record-sni|oob-sni|split-1] [--max <sn>]`; varsayılan S2-U'nun en iyisi
/// (`TlsRecordSplit@SNI`) ve 600 sn.
pub fn parse_args(rest: &[String]) -> anyhow::Result<HoldArgs> {
    let mut out = HoldArgs {
        tech: SockTech::TlsRecordSplit {
            at: SplitAt::SniMiddle,
        },
        max: MAX_HOLD,
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let Some(value) = it.next() else {
            bail!("{flag} için değer yok");
        };
        match flag.as_str() {
            "--tech" => {
                out.tech = match value.as_str() {
                    "tls-record-sni" => SockTech::TlsRecordSplit {
                        at: SplitAt::SniMiddle,
                    },
                    "oob-sni" => SockTech::Oob {
                        at: SplitAt::SniMiddle,
                    },
                    "split-1" => SockTech::Split {
                        at: SplitAt::Offset(1),
                    },
                    other => {
                        bail!("bilinmeyen teknik: {other} (tls-record-sni | oob-sni | split-1)")
                    }
                }
            }
            "--max" => {
                let secs: u64 = value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--max saniye olmalı: {value}"))?;
                if secs == 0 || Duration::from_secs(secs) > MAX_HOLD {
                    bail!("--max 1…{} sn olmalı", MAX_HOLD.as_secs());
                }
                out.max = Duration::from_secs(secs);
            }
            other => bail!("bilinmeyen argüman: {other}"),
        }
    }
    Ok(out)
}

/// Periyodik iş zamanı geldi mi? İlk çağrıda hemen.
pub fn due(now: Duration, last: Option<Duration>, every: Duration) -> bool {
    last.is_none_or(|l| now.saturating_sub(l) >= every)
}

/// Daha kaç tek IP öğrenilebilir?
pub fn learn_budget(learned: usize, cap: usize) -> usize {
    cap.saturating_sub(learned)
}

/// `e`/`evet` → doğru, `h`/`hayır` → yanlış, boş ya da başka → yanıt yok.
pub fn parse_answer(line: &str) -> Option<bool> {
    match line.trim().to_lowercase().as_str() {
        "e" | "evet" => Some(true),
        "h" | "hayır" | "hayir" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use xtask::lab::sockettech::{SockTech, SplitAt};

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn args_default_and_explicit() {
        let a = parse_args(&args(&[])).unwrap();
        assert_eq!(
            a.tech,
            SockTech::TlsRecordSplit {
                at: SplitAt::SniMiddle
            }
        );
        assert_eq!(a.max, MAX_HOLD);
        let a = parse_args(&args(&["--tech", "oob-sni", "--max", "120"])).unwrap();
        assert_eq!(
            a.tech,
            SockTech::Oob {
                at: SplitAt::SniMiddle
            }
        );
        assert_eq!(a.max, Duration::from_secs(120));
        let a = parse_args(&args(&["--tech", "split-1"])).unwrap();
        assert_eq!(
            a.tech,
            SockTech::Split {
                at: SplitAt::Offset(1)
            }
        );
    }

    #[test]
    fn args_reject_unknown_and_too_long() {
        for bad in [
            &["--tech", "fake"][..],
            &["--max", "601"],
            &["--max", "0"],
            &["--max"],
            &["--bilinmeyen"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn due_fires_first_time_and_after_interval() {
        let every = Duration::from_secs(20);
        assert!(due(Duration::from_secs(0), None, every));
        assert!(!due(Duration::from_secs(19), Some(Duration::ZERO), every));
        assert!(due(Duration::from_secs(20), Some(Duration::ZERO), every));
    }

    #[test]
    fn learning_budget_is_capped() {
        assert_eq!(learn_budget(0, 3), 3);
        assert_eq!(learn_budget(2, 3), 1);
        assert_eq!(learn_budget(5, 3), 0);
    }

    #[test]
    fn answers_are_parsed() {
        assert_eq!(parse_answer("e"), Some(true));
        assert_eq!(parse_answer(" E \r\n"), Some(true));
        assert_eq!(parse_answer("evet"), Some(true));
        assert_eq!(parse_answer("h"), Some(false));
        assert_eq!(parse_answer("hayır"), Some(false));
        assert_eq!(parse_answer(""), None);
        assert_eq!(parse_answer("belki"), None);
    }
}
