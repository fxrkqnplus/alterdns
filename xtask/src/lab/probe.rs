// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bağlantı probları (CONNECTIVITY_RESEARCH §4): DNS → TCP → TLS → HTTP durum satırı.
//! Her aşama zaman aşımlıdır; sonuç aşamaya göre sınıflandırılır. TLS doğrulaması Windows
//! sertifika deposuyla yapılır (`rustls-platform-verifier`).

use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use super::dns::{self, DnsMode};
use super::sockettech::{self, SockTech};

/// Her aşamanın zaman aşımı.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
/// Problarda gönderilen genel istemci adı (kişisel bilgi içermez).
pub const USER_AGENT: &str = "alterdns-lab/0";
/// Durum satırı için okunacak en çok düz metin.
const MAX_STATUS_READ: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Control,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub name: String,
    pub domain: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_path")]
    pub path: String,
    pub role: Role,
}

fn default_port() -> u16 {
    443
}

fn default_path() -> String {
    "/".into()
}

#[derive(Deserialize)]
struct ProbeFile {
    #[serde(default)]
    probe: Vec<ProbeTarget>,
}

/// `data/probes/*.toml` biçimi: `[[probe]] name, domain, port?, path?, role`.
/// Alan adı ve yol HTTP başlığına girdiği için sıkı denetlenir.
pub fn parse_targets(text: &str) -> anyhow::Result<Vec<ProbeTarget>> {
    let file: ProbeFile = toml::from_str(text).context("prob dosyası ayrıştırılamadı")?;
    for t in &file.probe {
        let domain_ok = !t.domain.is_empty()
            && t.domain.len() <= 253
            && t.domain
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if !domain_ok {
            bail!("geçersiz alan adı: {:?}", t.domain);
        }
        let path_ok = t.path.starts_with('/') && t.path.chars().all(|c| c.is_ascii_graphic());
        if !path_ok {
            bail!("geçersiz yol: {:?}", t.path);
        }
    }
    Ok(file.probe)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Ok {
        http_status: u16,
    },
    DnsFail,
    TcpRefused,
    TcpTimeout,
    TlsReset,
    TlsTimeout,
    TlsCertMismatch,
    /// Sıfırlama ve zaman aşımı dışındaki TLS hataları (ör. beklenmeyen uyarı).
    TlsOther,
    HttpError,
}

impl Outcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, Outcome::Ok { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Tcp,
    Tls,
    Http,
}

/// G/Ç hatasını oluştuğu aşamaya göre sınıflandırır.
pub fn classify_io(stage: Stage, e: &io::Error) -> Outcome {
    use io::ErrorKind as K;
    let timeout = matches!(e.kind(), K::TimedOut | K::WouldBlock);
    match stage {
        Stage::Tcp if timeout => Outcome::TcpTimeout,
        Stage::Tcp => Outcome::TcpRefused,
        Stage::Tls if timeout => Outcome::TlsTimeout,
        Stage::Tls => match e.kind() {
            K::ConnectionReset | K::ConnectionAborted | K::UnexpectedEof => Outcome::TlsReset,
            _ => match e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>()) {
                Some(rustls::Error::InvalidCertificate(_)) => Outcome::TlsCertMismatch,
                _ => Outcome::TlsOther,
            },
        },
        Stage::Http => Outcome::HttpError,
    }
}

/// `HTTP/1.1 204 No Content` → 204.
pub fn parse_status_line(line: &str) -> Option<u16> {
    let mut it = line.split_whitespace();
    if !it.next()?.starts_with("HTTP/") {
        return None;
    }
    let code = it.next()?;
    if code.len() != 3 {
        return None;
    }
    code.parse().ok()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Ham HTTP/1.1 yanıtı: tamamsa `(durum, gövde)`, eksikse `None`.
/// Gövde `Content-Length` veya `chunked` ile belirlenir; ikisi de yoksa `None` (eksik sayılır).
pub fn parse_http_response(raw: &[u8]) -> anyhow::Result<Option<(u16, Vec<u8>)>> {
    let Some(head_end) = find(raw, b"\r\n\r\n") else {
        return Ok(None);
    };
    let head = std::str::from_utf8(raw.get(..head_end).unwrap_or_default())
        .context("HTTP başlığı UTF-8 değil")?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(parse_status_line)
        .ok_or_else(|| anyhow!("HTTP durum satırı geçersiz"))?;
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "content-length" {
                length = Some(v.parse::<usize>().context("Content-Length geçersiz")?);
            } else if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }
    let body = raw.get(head_end + 4..).unwrap_or_default();
    if chunked {
        return Ok(decode_chunked(body)?.map(|b| (status, b)));
    }
    Ok(match length {
        Some(n) => body.get(..n).map(|b| (status, b.to_vec())),
        None => None,
    })
}

fn decode_chunked(mut b: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    let mut out = Vec::new();
    loop {
        let Some(le) = find(b, b"\r\n") else {
            return Ok(None);
        };
        let line = std::str::from_utf8(b.get(..le).unwrap_or_default())?;
        let size_hex = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16).context("parça boyutu geçersiz")?;
        b = b.get(le + 2..).unwrap_or_default();
        if size == 0 {
            return Ok(Some(out));
        }
        let after = size
            .checked_add(2)
            .ok_or_else(|| anyhow!("parça boyutu çok büyük"))?;
        let (Some(data), Some(rest)) = (b.get(..size), b.get(after..)) else {
            return Ok(None);
        };
        out.extend_from_slice(data);
        b = rest;
    }
}

/// Soketin okuma/yazma zaman aşımını aşama son tarihine kalan süreye (en çok `PROBE_TIMEOUT`)
/// indirir. Böylece veriyi damla damla gönderen bir karşı taraf aşamayı uzatamaz.
pub fn arm(sock: &Socket, deadline: Instant) -> io::Result<()> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "aşama süresi doldu",
        ));
    }
    let t = left.min(PROBE_TIMEOUT);
    sock.set_read_timeout(Some(t))?;
    sock.set_write_timeout(Some(t))
}

/// Durum satırını okur; her okumadan önce `arm` ile son tarih uygulanır.
fn read_status_line(r: &mut impl Read, sock: &Socket, deadline: Instant) -> io::Result<(u16, u64)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = find(&buf, b"\r\n") {
            let line = String::from_utf8_lossy(buf.get(..end).unwrap_or_default());
            return parse_status_line(&line)
                .map(|code| (code, buf.len() as u64))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "HTTP durum satırı yok")
                });
        }
        if buf.len() >= MAX_STATUS_READ {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "durum satırı çok uzun",
            ));
        }
        arm(sock, deadline)?;
        let n = r.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
    }
}

/// Zaman aşımlı TCP bağlantısı; okuma/yazma zaman aşımları da ayarlanır.
pub fn connect_tcp(addr: SocketAddr) -> io::Result<Socket> {
    let sock = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    sock.connect_timeout(&SockAddr::from(addr), PROBE_TIMEOUT)?;
    sock.set_read_timeout(Some(PROBE_TIMEOUT))?;
    sock.set_write_timeout(Some(PROBE_TIMEOUT))?;
    Ok(sock)
}

pub type TlsStream = StreamOwned<ClientConnection, Socket>;

fn tls_config() -> io::Result<Arc<ClientConfig>> {
    ClientConfig::with_platform_verifier()
        .map(Arc::new)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// TLS el sıkışması (SNI = `sni`), Windows sertifika deposuyla doğrulanır.
pub fn handshake(sock: Socket, sni: &str) -> io::Result<TlsStream> {
    handshake_with(sock, sni, &SockTech::None)
}

/// El sıkışması; ilk uçuş (ClientHello) `tech` tekniğiyle gönderilir, gerisi olağan akar.
pub fn handshake_with(sock: Socket, sni: &str, tech: &SockTech) -> io::Result<TlsStream> {
    let name = ServerName::try_from(sni.to_string())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut conn = ClientConnection::new(tls_config()?, name)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut hello = Vec::new();
    while conn.wants_write() {
        conn.write_tls(&mut hello)?;
    }
    let deadline = Instant::now() + PROBE_TIMEOUT;
    arm(&sock, deadline)?;
    sockettech::send_first_flight(&sock, &hello, tech)?;
    let mut s = StreamOwned::new(conn, sock);
    while s.conn.is_handshaking() {
        arm(&s.sock, deadline)?;
        s.conn.complete_io(&mut s.sock)?;
    }
    Ok(s)
}

/// `GET path` gönderir ve yalnızca durum satırını okur; (durum, okunan düz metin baytı).
/// Bütün aşama `PROBE_TIMEOUT` ile sınırlıdır.
fn http_status(s: &mut TlsStream, host: &str, path: &str) -> io::Result<(u16, u64)> {
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {USER_AGENT}\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    arm(&s.sock, deadline)?;
    s.write_all(req.as_bytes())?;
    s.flush()?;
    // Aynı sokete ikinci tutamaç: zaman aşımı ayarı TLS akışı ödünç alınmışken de yapılabilsin.
    let guard = s.sock.try_clone()?;
    read_status_line(s, &guard, deadline)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub target: String,
    pub domain: String,
    pub dns_mode: DnsMode,
    pub tech: SockTech,
    pub addrs: Vec<String>,
    pub outcome: Outcome,
    pub dns_ms: Option<u64>,
    pub tcp_ms: Option<u64>,
    pub tls_ms: Option<u64>,
    pub total_ms: u64,
    pub bytes_in: u64,
    pub detail: Option<String>,
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Tek prob: DNS seçilen yolla çözülür; bağlantı o IP'ye, SNI alan adıyla kurulur.
pub fn run_probe(t: &ProbeTarget, dns_mode: DnsMode) -> ProbeResult {
    run_probe_with(t, dns_mode, &SockTech::None)
}

/// `run_probe`; ClientHello `tech` tekniğiyle gönderilir (S2-U).
pub fn run_probe_with(t: &ProbeTarget, dns_mode: DnsMode, tech: &SockTech) -> ProbeResult {
    let start = Instant::now();
    let mut r = ProbeResult {
        target: t.name.clone(),
        domain: t.domain.clone(),
        dns_mode,
        tech: *tech,
        addrs: Vec::new(),
        outcome: Outcome::DnsFail,
        dns_ms: None,
        tcp_ms: None,
        tls_ms: None,
        total_ms: 0,
        bytes_in: 0,
        detail: None,
    };
    r.outcome = probe_stages(t, dns_mode, tech, &mut r);
    r.total_ms = ms(start.elapsed());
    r
}

fn probe_stages(
    t: &ProbeTarget,
    dns_mode: DnsMode,
    tech: &SockTech,
    r: &mut ProbeResult,
) -> Outcome {
    let fail = |r: &mut ProbeResult, stage: Stage, e: &io::Error| {
        r.detail = Some(format!("{stage:?}: {e}"));
        classify_io(stage, e)
    };
    let t0 = Instant::now();
    let addrs = match dns::resolve(&t.domain, dns_mode) {
        Ok(a) => a,
        Err(e) => {
            r.detail = Some(format!("DNS: {e:#}"));
            return Outcome::DnsFail;
        }
    };
    r.dns_ms = Some(ms(t0.elapsed()));
    r.addrs = addrs.iter().map(ToString::to_string).collect();
    let Some(ip) = addrs.iter().find(|a| a.is_ipv4()).or(addrs.first()) else {
        r.detail = Some("DNS: boş yanıt".into());
        return Outcome::DnsFail;
    };
    let t1 = Instant::now();
    let sock = match connect_tcp(SocketAddr::new(*ip, t.port)) {
        Ok(s) => s,
        Err(e) => return fail(r, Stage::Tcp, &e),
    };
    r.tcp_ms = Some(ms(t1.elapsed()));
    let t2 = Instant::now();
    let mut tls = match handshake_with(sock, &t.domain, tech) {
        Ok(s) => s,
        Err(e) => return fail(r, Stage::Tls, &e),
    };
    r.tls_ms = Some(ms(t2.elapsed()));
    match http_status(&mut tls, &t.domain, &t.path) {
        Ok((status, n)) => {
            r.bytes_in = n;
            Outcome::Ok {
                http_status: status,
            }
        }
        Err(e) => fail(r, Stage::Http, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_reset_during_tls_is_tls_reset() {
        for kind in [
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::UnexpectedEof,
        ] {
            assert_eq!(
                classify_io(Stage::Tls, &io::Error::from(kind)),
                Outcome::TlsReset
            );
        }
    }

    #[test]
    fn classify_timeout_per_stage() {
        let t = || io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(classify_io(Stage::Tcp, &t()), Outcome::TcpTimeout);
        assert_eq!(classify_io(Stage::Tls, &t()), Outcome::TlsTimeout);
        assert_eq!(classify_io(Stage::Http, &t()), Outcome::HttpError);
        let wb = io::Error::from(io::ErrorKind::WouldBlock);
        assert_eq!(classify_io(Stage::Tls, &wb), Outcome::TlsTimeout);
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert_eq!(classify_io(Stage::Tcp, &refused), Outcome::TcpRefused);
    }

    #[test]
    fn classify_cert_error_is_cert_mismatch() {
        let cert = rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName);
        let e = io::Error::new(io::ErrorKind::InvalidData, cert);
        assert_eq!(classify_io(Stage::Tls, &e), Outcome::TlsCertMismatch);
        let alert = rustls::Error::AlertReceived(rustls::AlertDescription::HandshakeFailure);
        let e = io::Error::new(io::ErrorKind::InvalidData, alert);
        assert_eq!(classify_io(Stage::Tls, &e), Outcome::TlsOther);
    }

    #[test]
    fn http_status_line_parse() {
        assert_eq!(parse_status_line("HTTP/1.1 204 No Content"), Some(204));
        assert_eq!(parse_status_line("HTTP/1.0 200 OK\r"), Some(200));
        assert_eq!(parse_status_line("HTTP/2 301"), Some(301));
        assert_eq!(parse_status_line("SSH-2.0-x"), None);
        assert_eq!(parse_status_line("HTTP/1.1 abc"), None);
    }

    #[test]
    fn http_response_body_by_length_and_chunked() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc";
        assert_eq!(
            parse_http_response(raw).unwrap(),
            Some((200, b"abc".to_vec()))
        );
        assert_eq!(parse_http_response(&raw[..raw.len() - 1]).unwrap(), None);
        let chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab\r\n1\r\nc\r\n0\r\n\r\n";
        assert_eq!(
            parse_http_response(chunked).unwrap(),
            Some((200, b"abc".to_vec()))
        );
        assert_eq!(parse_http_response(b"HTTP/1.1 200 OK\r\n").unwrap(), None);
    }

    #[test]
    fn probe_timeout_is_4s() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(4));
    }

    #[test]
    fn probe_targets_reject_header_injection() {
        let ok = "[[probe]]\nname = \"k\"\ndomain = \"example.com\"\nrole = \"control\"\n";
        let t = parse_targets(ok).unwrap();
        assert_eq!(t[0].port, 443);
        assert_eq!(t[0].path, "/");
        assert_eq!(t[0].role, Role::Control);
        let bad_domain =
            "[[probe]]\nname = \"k\"\ndomain = \"a.com\\r\\nX: y\"\nrole = \"target\"\n";
        assert!(parse_targets(bad_domain).is_err());
        let bad_path = "[[probe]]\nname = \"k\"\ndomain = \"a.com\"\npath = \"/ HTTP/1.1\"\nrole = \"target\"\n";
        assert!(parse_targets(bad_path).is_err());
    }

    #[test]
    fn arm_caps_timeout_and_rejects_expired_deadline() {
        let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        let past = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        assert_eq!(
            arm(&sock, past).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        arm(&sock, Instant::now() + Duration::from_secs(60)).unwrap();
        assert!(sock.read_timeout().unwrap().unwrap() <= PROBE_TIMEOUT);
        arm(&sock, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(sock.read_timeout().unwrap().unwrap() <= Duration::from_secs(1));
    }

    /// Her okumada bir bayt gönderen (satır sonu göndermeyen) sunucu aşama süresini uzatamaz.
    #[test]
    fn slow_drip_hits_stage_deadline() {
        use std::net::{Ipv4Addr, TcpListener};
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            for _ in 0..20 {
                if s.write_all(b"H").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        let sock = connect_tcp(addr).unwrap();
        let start = Instant::now();
        let deadline = start + Duration::from_millis(1200);
        let err = read_status_line(&mut &sock, &sock, deadline).unwrap_err();
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ),
            "{err:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "{:?}",
            start.elapsed()
        );
        drop(sock);
        server.join().unwrap();
    }

    #[test]
    fn chunked_size_overflow_is_not_a_panic() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nab";
        assert!(parse_http_response(raw).is_err());
    }

    /// Ağ gerektirir: `cargo test -p xtask --lib -- --ignored net_control_probe_ok`.
    #[test]
    #[ignore]
    fn net_control_probe_ok() {
        let t = ProbeTarget {
            name: "kontrol".into(),
            domain: "connectivitycheck.gstatic.com".into(),
            port: 443,
            path: "/generate_204".into(),
            role: Role::Control,
        };
        let r = run_probe(&t, DnsMode::System);
        assert_eq!(
            r.outcome,
            Outcome::Ok { http_status: 204 },
            "{:?}",
            r.detail
        );
    }
}
