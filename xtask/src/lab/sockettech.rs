// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sürücüsüz soket teknikleri (S2-U; CONNECTIVITY_RESEARCH H1, H8, H9). Sistem ayarı
//! değişmez: `IP_TTL` yalnızca probun kendi soketinde ve yalnızca ilgili yazma için değişir,
//! ardından soketin önceki değerine döner.

use std::io;
use std::ops::Range;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use socket2::Socket;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitAt {
    /// Sabit konum. TCP tekniklerinde ClientHello akışındaki mutlak bayt; `TlsRecordSplit`'te
    /// kayıt gövdesine göre (5 baytlık kayıt başlığından sonra).
    Offset(usize),
    /// SNI ad alanının ortası.
    SniMiddle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SockTech {
    None,
    Split { at: SplitAt },
    TlsRecordSplit { at: SplitAt },
    Disorder { at: SplitAt, ttl: u8 },
    Oob { at: SplitAt },
}

/// Sokete yapılacak tek yazma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub bytes: Vec<u8>,
    pub ttl: Option<u8>,
    pub oob: bool,
}

/// OOB tekniğinde gönderilen acil bayt.
const OOB_BYTE: u8 = b'a';
/// Ardışık yazmaların ayrı TCP bölütleri olarak çıkması için kısa bekleme.
const SEGMENT_GAP: Duration = Duration::from_millis(10);
const TLS_HEADER: usize = 5;

fn be16(b: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_be_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
    ])))
}

/// ClientHello içindeki SNI ad alanının bayt aralığı (saf fonksiyon; kesik girdide `None`).
pub fn find_sni(hello: &[u8]) -> Option<Range<usize>> {
    if *hello.first()? != 0x16 || *hello.get(TLS_HEADER)? != 0x01 {
        return None;
    }
    // Kayıt başlığı + el sıkışma başlığı (tür, 24 bit uzunluk) + istemci sürümü + rastgele.
    let mut p = TLS_HEADER + 4 + 2 + 32;
    p += 1 + usize::from(*hello.get(p)?); // oturum kimliği
    p += 2 + be16(hello, p)?; // şifre takımları
    p += 1 + usize::from(*hello.get(p)?); // sıkıştırma yöntemleri
    let ext_end = (p + 2 + be16(hello, p)?).min(hello.len());
    p += 2;
    while p + 4 <= ext_end {
        let ty = be16(hello, p)?;
        let len = be16(hello, p + 2)?;
        p += 4;
        if ty == 0 {
            // server_name_list: liste uzunluğu (2), ad türü (1) = host_name, ad uzunluğu (2), ad.
            let name_len = be16(hello, p + 3)?;
            let start = p + 5;
            if *hello.get(p + 2)? != 0 || start + name_len > hello.len() {
                return None;
            }
            return Some(start..start + name_len);
        }
        p += len;
    }
    None
}

/// Tek TLS kaydını `at` (mutlak konum) noktasında aynı içerikli iki kayda böler.
pub fn split_tls_record(hello: &[u8], at: usize) -> Option<Vec<u8>> {
    if *hello.first()? != 0x16 {
        return None;
    }
    let rec_end = TLS_HEADER + be16(hello, 3)?;
    if at <= TLS_HEADER || at >= rec_end || rec_end > hello.len() {
        return None;
    }
    let first_len = u16::try_from(at - TLS_HEADER).ok()?;
    let second_len = u16::try_from(rec_end - at).ok()?;
    let mut out = Vec::with_capacity(hello.len() + TLS_HEADER);
    out.extend_from_slice(hello.get(..3)?);
    out.extend_from_slice(&first_len.to_be_bytes());
    out.extend_from_slice(hello.get(TLS_HEADER..at)?);
    out.extend_from_slice(hello.get(..3)?);
    out.extend_from_slice(&second_len.to_be_bytes());
    out.extend_from_slice(hello.get(at..)?);
    Some(out)
}

/// `at`'i ClientHello'daki mutlak konuma çevirir; uç değerler (boş parça) reddedilir.
fn resolve_at(hello: &[u8], at: SplitAt, record_relative: bool) -> anyhow::Result<usize> {
    let pos = match at {
        SplitAt::Offset(n) if record_relative => TLS_HEADER + n,
        SplitAt::Offset(n) => n,
        SplitAt::SniMiddle => {
            let sni = find_sni(hello).ok_or_else(|| anyhow::anyhow!("ClientHello'da SNI yok"))?;
            sni.start + sni.len() / 2
        }
    };
    if pos == 0 || pos >= hello.len() {
        anyhow::bail!(
            "bölme konumu aralık dışında: {pos} (uzunluk {})",
            hello.len()
        );
    }
    Ok(pos)
}

fn plain(bytes: &[u8]) -> Write {
    Write {
        bytes: bytes.to_vec(),
        ttl: None,
        oob: false,
    }
}

/// Tekniğin yazma planı (saf fonksiyon). OOB baytı dışındaki yazmalar birleşince özgün akış olur.
pub fn plan_writes(hello: &[u8], tech: &SockTech) -> anyhow::Result<Vec<Write>> {
    let halves = |at: SplitAt| -> anyhow::Result<(&[u8], &[u8])> {
        let pos = resolve_at(hello, at, false)?;
        Ok(hello.split_at(pos))
    };
    Ok(match *tech {
        SockTech::None => vec![plain(hello)],
        SockTech::Split { at } => {
            let (a, b) = halves(at)?;
            vec![plain(a), plain(b)]
        }
        SockTech::TlsRecordSplit { at } => {
            let pos = resolve_at(hello, at, true)?;
            let rec = split_tls_record(hello, pos)
                .ok_or_else(|| anyhow::anyhow!("TLS kaydı {pos} konumunda bölünemedi"))?;
            vec![plain(&rec)]
        }
        SockTech::Disorder { at, ttl } => {
            let (a, b) = halves(at)?;
            vec![
                Write {
                    bytes: a.to_vec(),
                    ttl: Some(ttl),
                    oob: false,
                },
                plain(b),
            ]
        }
        SockTech::Oob { at } => {
            let (a, b) = halves(at)?;
            vec![
                plain(a),
                Write {
                    bytes: vec![OOB_BYTE],
                    ttl: None,
                    oob: true,
                },
                plain(b),
            ]
        }
    })
}

fn send_all(sock: &Socket, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let n = sock.send(buf)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        buf = buf.get(n..).unwrap_or_default();
    }
    Ok(())
}

/// İlk uçuşu (ClientHello) tekniğe göre gönderir: `TCP_NODELAY`, yazma başına `IP_TTL`
/// (yalnız bu soket) ve `MSG_OOB`. Hata olsa da TTL soketin önceki değerine döner.
pub fn send_first_flight(sock: &Socket, hello: &[u8], tech: &SockTech) -> io::Result<()> {
    let writes = plan_writes(hello, tech)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    sock.set_tcp_nodelay(true)?;
    let before = sock.ttl_v4()?;
    let sent = (|| {
        let last = writes.len().saturating_sub(1);
        for (i, w) in writes.iter().enumerate() {
            if let Some(ttl) = w.ttl {
                sock.set_ttl_v4(u32::from(ttl))?;
            }
            if w.oob {
                if sock.send_out_of_band(&w.bytes)? != w.bytes.len() {
                    return Err(io::Error::from(io::ErrorKind::WriteZero));
                }
            } else {
                send_all(sock, &w.bytes)?;
            }
            if i < last {
                std::thread::sleep(SEGMENT_GAP);
            }
            if w.ttl.is_some() {
                sock.set_ttl_v4(before)?;
            }
        }
        Ok(())
    })();
    let restored = sock.set_ttl_v4(before);
    sent.and(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};
    use std::sync::Arc;

    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore};

    /// Fikstür: rustls'in bu alan adı için ürettiği gerçek ClientHello.
    fn hello(sni: &str) -> Vec<u8> {
        let cfg = ClientConfig::builder()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        let name = ServerName::try_from(sni.to_string()).unwrap();
        let mut c = ClientConnection::new(Arc::new(cfg), name).unwrap();
        let mut out = Vec::new();
        while c.wants_write() {
            c.write_tls(&mut out).unwrap();
        }
        out
    }

    fn joined(ws: &[Write]) -> Vec<u8> {
        ws.iter()
            .filter(|w| !w.oob)
            .flat_map(|w| w.bytes.clone())
            .collect()
    }

    #[test]
    fn find_sni_locates_server_name() {
        let h = hello("discord.com");
        let r = find_sni(&h).unwrap();
        assert_eq!(&h[r.clone()], b"discord.com");
        let naive = h.windows(11).position(|w| w == b"discord.com").unwrap();
        assert_eq!(r.start, naive);
        assert!(find_sni(b"GET / HTTP/1.1\r\n\r\n").is_none());
        assert!(
            find_sni(&h[..20]).is_none(),
            "kesik ClientHello paniklememeli"
        );
    }

    #[test]
    fn split_tls_record_preserves_handshake_bytes() {
        let h = hello("discord.com");
        let at = 5 + 40;
        let out = split_tls_record(&h, at).unwrap();
        assert_eq!(out.len(), h.len() + 5);
        let len1 = usize::from(u16::from_be_bytes([out[3], out[4]]));
        assert_eq!(len1, at - 5);
        let h2 = 5 + len1;
        assert_eq!(&out[h2..h2 + 3], &h[..3], "ikinci kayıt aynı tür ve sürüm");
        let len2 = usize::from(u16::from_be_bytes([out[h2 + 3], out[h2 + 4]]));
        let orig_len = usize::from(u16::from_be_bytes([h[3], h[4]]));
        assert_eq!(len1 + len2, orig_len);
        let mut bodies = out[5..h2].to_vec();
        bodies.extend_from_slice(&out[h2 + 5..]);
        assert_eq!(bodies, &h[5..]);
        assert!(split_tls_record(&h, 5).is_none(), "boş ilk kayıt yok");
        assert!(
            split_tls_record(&h, h.len()).is_none(),
            "boş ikinci kayıt yok"
        );
    }

    #[test]
    fn plan_split_at_sni_middle() {
        let h = hello("discord.com");
        let sni = find_sni(&h).unwrap();
        let ws = plan_writes(
            &h,
            &SockTech::Split {
                at: SplitAt::SniMiddle,
            },
        )
        .unwrap();
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].bytes.len(), sni.start + sni.len() / 2);
        assert!(ws.iter().all(|w| w.ttl.is_none() && !w.oob));
        assert_eq!(joined(&ws), h);
    }

    #[test]
    fn plan_disorder_marks_first_write_low_ttl() {
        let h = hello("discord.com");
        let ws = plan_writes(
            &h,
            &SockTech::Disorder {
                at: SplitAt::Offset(1),
                ttl: 3,
            },
        )
        .unwrap();
        assert_eq!(ws.len(), 2);
        assert_eq!(ws[0].ttl, Some(3));
        assert_eq!(ws[0].bytes.len(), 1);
        assert_eq!(ws[1].ttl, None);
        assert_eq!(joined(&ws), h);
    }

    #[test]
    fn plan_oob_inserts_single_urgent_byte() {
        let h = hello("discord.com");
        let ws = plan_writes(
            &h,
            &SockTech::Oob {
                at: SplitAt::SniMiddle,
            },
        )
        .unwrap();
        let oob: Vec<_> = ws.iter().filter(|w| w.oob).collect();
        assert_eq!(oob.len(), 1);
        assert_eq!(oob[0].bytes.len(), 1);
        assert_eq!(
            joined(&ws),
            h,
            "OOB dışındaki yazmalar özgün akışı oluşturur"
        );
    }

    #[test]
    fn plan_none_is_single_write() {
        let h = hello("discord.com");
        let ws = plan_writes(&h, &SockTech::None).unwrap();
        assert_eq!(
            ws,
            vec![Write {
                bytes: h.clone(),
                ttl: None,
                oob: false
            }]
        );
    }

    #[test]
    fn plan_tls_record_split_is_one_write_two_records() {
        let h = hello("discord.com");
        let ws = plan_writes(
            &h,
            &SockTech::TlsRecordSplit {
                at: SplitAt::Offset(1),
            },
        )
        .unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].bytes, split_tls_record(&h, 6).unwrap());
    }

    #[test]
    fn plan_rejects_out_of_range_offset() {
        let h = hello("discord.com");
        for tech in [
            SockTech::Split {
                at: SplitAt::Offset(0),
            },
            SockTech::Split {
                at: SplitAt::Offset(h.len()),
            },
            SockTech::TlsRecordSplit {
                at: SplitAt::Offset(h.len()),
            },
        ] {
            assert!(plan_writes(&h, &tech).is_err(), "{tech:?}");
        }
    }

    /// Yerel geri döngü dinleyicisi; dış ağa çıkmaz.
    #[test]
    fn send_first_flight_delivers_planned_bytes_and_restores_ttl() {
        let h = hello("discord.com");
        for tech in [
            SockTech::None,
            SockTech::Split {
                at: SplitAt::SniMiddle,
            },
            SockTech::TlsRecordSplit {
                at: SplitAt::SniMiddle,
            },
            SockTech::Disorder {
                at: SplitAt::Offset(1),
                ttl: 2,
            },
        ] {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let addr: SocketAddr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut got = Vec::new();
                s.read_to_end(&mut got).unwrap();
                got
            });
            let sock = crate::lab::probe::connect_tcp(addr).unwrap();
            let before = sock.ttl_v4().unwrap();
            send_first_flight(&sock, &h, &tech).unwrap();
            assert_eq!(sock.ttl_v4().unwrap(), before, "{tech:?}: TTL geri dönmeli");
            sock.shutdown(std::net::Shutdown::Write).unwrap();
            let expected = joined(&plan_writes(&h, &tech).unwrap());
            assert_eq!(server.join().unwrap(), expected, "{tech:?}");
        }
    }
}
