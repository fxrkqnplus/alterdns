// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kullanıcı modu iletim: TUN'dan gelen TCP akışını gerçek hedefe kendi soketimizle kurar,
//! ClientHello'ya `SockTech` tekniğini uygular, sonra iki yönlü kopyalar. UDP datagramları
//! kendi UDP soketimizle iletilir. Giden soketler fiziksel arayüze bağlanır (`IP_UNICAST_IF`),
//! böylece TUN rotasına geri dönmez.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use std::time::Duration;

use serde::Serialize;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::io::{AsyncRead, AsyncWrite};
use xtask::lab::sockettech::SockTech;

/// İlk uçuşun (ClientHello) durumu.
#[derive(Debug, PartialEq, Eq)]
pub enum FirstFlight {
    /// TLS kaydı henüz tamamlanmadı.
    NeedMore,
    /// İlk TLS kaydı tamam; bayt uzunluğu (5 baytlık başlık dahil).
    Complete(usize),
    /// TLS değil (ör. düz HTTP): teknik uygulanmaz.
    NotTls,
}

/// Ara bellekteki ilk TLS kaydının tamamlanıp tamamlanmadığı (saf fonksiyon).
pub fn first_flight(buf: &[u8]) -> FirstFlight {
    match (buf.first(), buf.get(1)) {
        (None, _) => FirstFlight::NeedMore,
        (Some(0x16), None | Some(0x03)) => match buf.get(..5) {
            None => FirstFlight::NeedMore,
            Some(h) => {
                let len = 5 + usize::from(u16::from_be_bytes([h[3], h[4]]));
                if buf.len() >= len {
                    FirstFlight::Complete(len)
                } else {
                    FirstFlight::NeedMore
                }
            }
        },
        _ => FirstFlight::NotTls,
    }
}

/// QUIC Initial biçimli test datagramı: uzun başlık, sürüm 1, en az 1200 bayt (RFC 9000 §14.1).
/// İçerik anlamsızdır; yalnızca UDP iletim yolunu ve olası yanıtı ölçmek içindir.
pub fn quic_probe_datagram(dcid: [u8; 8]) -> Vec<u8> {
    // Uzun başlık (Initial, 4 baytlık paket numarası), sürüm 1, DCID, boş SCID, boş belirteç.
    let mut d = vec![0xC3, 0, 0, 0, 1, 8];
    d.extend_from_slice(&dcid);
    d.extend_from_slice(&[0, 0]);
    // Kalan uzunluk: 2 baytlık değişken uzunluklu tamsayı; geri kalanı sıfır dolgu.
    let remaining = 1200 - d.len() - 2;
    d.extend_from_slice(&(0x4000u16 | remaining as u16).to_be_bytes());
    d.resize(1200, 0);
    d
}

/// İletim sayaçları.
#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub tcp_sessions: AtomicU64,
    pub tcp_errors: AtomicU64,
    pub udp_sessions: AtomicU64,
    pub udp_datagrams_out: AtomicU64,
    pub udp_datagrams_in: AtomicU64,
}

impl Stats {
    pub fn add(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Hedef bağlantı zaman aşımı.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// İlk uçuşu toplamak için en çok bekleme ve bayt.
const FIRST_FLIGHT_WAIT: Duration = Duration::from_secs(2);
const MAX_FIRST_FLIGHT: usize = 16 * 1024;

/// Gerçek hedefe engelleyen soketle bağlanır; arayüz verilmişse `IP_UNICAST_IF` uygulanır.
fn connect_out(peer: SocketAddr, phys_if: Option<u32>) -> anyhow::Result<Socket> {
    let sock = Socket::new(Domain::for_address(peer), Type::STREAM, Some(Protocol::TCP))?;
    if let Some(i) = phys_if {
        crate::ffi::set_unicast_if_v4(&sock, i)?;
    }
    sock.connect_timeout(&SockAddr::from(peer), CONNECT_TIMEOUT)?;
    Ok(sock)
}

/// TCP aktarımı: `inbound` (TUN tarafı) ↔ `peer` (gerçek hedef).
pub async fn relay_tcp<I>(
    inbound: I,
    peer: SocketAddr,
    phys_if: Option<u32>,
    tech: SockTech,
    stats: Arc<Stats>,
) -> anyhow::Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    Stats::add(&stats.tcp_sessions);
    let result = relay_inner(inbound, peer, phys_if, tech).await;
    if result.is_err() {
        Stats::add(&stats.tcp_errors);
    }
    result
}

async fn relay_inner<I>(
    mut inbound: I,
    peer: SocketAddr,
    phys_if: Option<u32>,
    tech: SockTech,
) -> anyhow::Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::AsyncReadExt;
    let sock = tokio::task::spawn_blocking(move || connect_out(peer, phys_if)).await??;
    // İlk uçuş: tam TLS kaydı, TLS olmayan veri, EOF ya da süre dolması.
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + FIRST_FLIGHT_WAIT;
    let state = loop {
        match first_flight(&buf) {
            FirstFlight::NeedMore if buf.len() < MAX_FIRST_FLIGHT => {}
            other => break other,
        }
        match tokio::time::timeout_at(deadline, inbound.read(&mut chunk)).await {
            Ok(Ok(0)) | Err(_) => break FirstFlight::NotTls,
            Ok(Ok(n)) => buf.extend_from_slice(chunk.get(..n).unwrap_or_default()),
            Ok(Err(e)) => return Err(e.into()),
        }
    };
    let use_tech = if matches!(state, FirstFlight::Complete(_)) {
        tech
    } else {
        SockTech::None
    };
    let sock = tokio::task::spawn_blocking(move || -> anyhow::Result<Socket> {
        if !buf.is_empty() {
            xtask::lab::sockettech::send_first_flight(&sock, &buf, &use_tech)?;
        }
        sock.set_nonblocking(true)?;
        Ok(sock)
    })
    .await??;
    let std_stream: std::net::TcpStream = sock.into();
    let mut out = tokio::net::TcpStream::from_std(std_stream)?;
    tokio::io::copy_bidirectional(&mut inbound, &mut out).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::Arc as StdArc;

    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use xtask::lab::sockettech::SplitAt;

    fn hello(sni: &str) -> Vec<u8> {
        let cfg = ClientConfig::builder()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        let mut c = ClientConnection::new(
            StdArc::new(cfg),
            ServerName::try_from(sni.to_string()).unwrap(),
        )
        .unwrap();
        let mut out = Vec::new();
        while c.wants_write() {
            c.write_tls(&mut out).unwrap();
        }
        out
    }

    #[test]
    fn first_flight_waits_for_whole_tls_record() {
        let h = hello("discord.com");
        assert_eq!(first_flight(&h[..3]), FirstFlight::NeedMore);
        assert_eq!(first_flight(&h[..h.len() - 1]), FirstFlight::NeedMore);
        assert_eq!(first_flight(&h), FirstFlight::Complete(h.len()));
        let mut more = h.clone();
        more.extend_from_slice(b"ekveri");
        assert_eq!(first_flight(&more), FirstFlight::Complete(h.len()));
        assert_eq!(first_flight(b"GET / HTTP/1.1\r\n"), FirstFlight::NotTls);
        assert_eq!(first_flight(b""), FirstFlight::NeedMore);
    }

    #[test]
    fn quic_probe_datagram_has_initial_shape() {
        let d = quic_probe_datagram([7; 8]);
        assert!(d.len() >= 1200);
        assert_eq!(d[0] & 0xC0, 0xC0, "uzun başlık + sabit bit");
        assert_eq!(&d[1..5], &[0, 0, 0, 1], "QUIC sürüm 1");
        assert_eq!(d[5], 8, "DCID uzunluğu");
        assert_eq!(&d[6..14], &[7; 8]);
    }

    /// Yerel geri döngü: TUN tarafı yerine bellek içi akış, hedef yerine yerel dinleyici.
    /// Sunucunun alması gereken bayt dizisi tekniğin yazma planından hesaplanır.
    async fn relay_roundtrip(tech: SockTech, first: Vec<u8>) {
        let mut expected: Vec<u8> = match first_flight(&first) {
            FirstFlight::Complete(_) => xtask::lab::sockettech::plan_writes(&first, &tech)
                .unwrap()
                .into_iter()
                .filter(|w| !w.oob)
                .flat_map(|w| w.bytes)
                .collect(),
            _ => first.clone(),
        };
        expected.extend_from_slice(b"sonr");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let want_len = expected.len();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut s, _) = listener.accept().unwrap();
            let mut got = vec![0u8; want_len];
            s.read_exact(&mut got).unwrap();
            s.write_all(b"yanit").unwrap();
            got
        });
        let (mut app, tun_side) = tokio::io::duplex(64 * 1024);
        let stats = Arc::new(Stats::default());
        let relay = tokio::spawn(relay_tcp(tun_side, addr, None, tech, stats.clone()));
        app.write_all(&first).await.unwrap();
        app.write_all(b"sonr").await.unwrap();
        let mut reply = [0u8; 5];
        app.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"yanit");
        assert_eq!(server.join().unwrap(), expected, "{tech:?}");
        drop(app);
        relay.await.unwrap().unwrap();
        assert_eq!(stats.tcp_sessions.load(Ordering::Relaxed), 1);
    }

    /// OOB baytı geri döngüde alıcı akışına karışabildiği için burada sınanmaz; gerçek
    /// hedefte Wintun spike koşusunda ölçülür.
    #[tokio::test]
    async fn relay_applies_technique_and_preserves_stream() {
        let h = hello("discord.com");
        for tech in [
            SockTech::None,
            SockTech::Split {
                at: SplitAt::Offset(1),
            },
            SockTech::Split {
                at: SplitAt::SniMiddle,
            },
            SockTech::TlsRecordSplit {
                at: SplitAt::SniMiddle,
            },
        ] {
            relay_roundtrip(tech, h.clone()).await;
        }
    }

    #[tokio::test]
    async fn relay_passes_non_tls_unchanged() {
        relay_roundtrip(
            SockTech::Split {
                at: SplitAt::Offset(1),
            },
            b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec(),
        )
        .await;
    }
}
