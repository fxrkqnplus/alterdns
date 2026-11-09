// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lab DNS çözümü (H3): sistem çözücüsü, DoH (Cloudflare, RFC 8484 POST) ve şifresiz
//! standart dışı port (`77.88.8.8:1253`). Sistemin DNS ayarına dokunulmaz; yalnızca sorgu gönderilir.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, anyhow, bail};
use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{Name, RData, RecordType};
use serde::{Deserialize, Serialize};

use super::probe::{self, PROBE_TIMEOUT};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnsMode {
    System,
    Doh,
    AltUdp1253,
}

/// DoH sunucusunun TLS adı; bağlantı doğrudan `1.1.1.1:443` adresine yapılır.
pub const DOH_SNI: &str = "cloudflare-dns.com";
const MAX_DOH_RESPONSE: usize = 64 * 1024;

fn doh_addr() -> SocketAddr {
    SocketAddr::from(([1, 1, 1, 1], 443))
}

fn alt_addr() -> SocketAddr {
    SocketAddr::from(([77, 88, 8, 8], 1253))
}

fn random_id() -> u16 {
    Message::query().metadata.id
}

/// `domain` için A sorgusu (RD bayrağı açık).
pub fn build_query(id: u16, domain: &str) -> anyhow::Result<Vec<u8>> {
    let name = Name::from_ascii(domain).map_err(|e| anyhow!("geçersiz alan adı {domain}: {e}"))?;
    let mut msg = Message::new(id, MessageType::Query, OpCode::Query);
    msg.metadata.recursion_desired = true;
    msg.add_query(Query::query(name, RecordType::A));
    msg.to_vec()
        .map_err(|e| anyhow!("DNS sorgusu kodlanamadı: {e}"))
}

/// Yanıttaki A kayıtları; kimlik, yön veya yanıt kodu uymazsa hata.
pub fn parse_a_answers(resp: &[u8], id: u16) -> anyhow::Result<Vec<Ipv4Addr>> {
    let msg = Message::from_vec(resp).map_err(|e| anyhow!("DNS yanıtı ayrıştırılamadı: {e}"))?;
    if msg.metadata.id != id {
        bail!("DNS kimliği uyuşmuyor");
    }
    if msg.metadata.message_type != MessageType::Response {
        bail!("DNS mesajı yanıt değil");
    }
    if msg.metadata.response_code != ResponseCode::NoError {
        bail!("DNS yanıt kodu: {:?}", msg.metadata.response_code);
    }
    Ok(msg
        .answers
        .iter()
        .filter_map(|r| match &r.data {
            RData::A(a) => Some(a.0),
            _ => None,
        })
        .collect())
}

/// `domain`'i seçilen yolla çözer.
pub fn resolve(domain: &str, mode: DnsMode) -> anyhow::Result<Vec<IpAddr>> {
    let v4 = |v: Vec<Ipv4Addr>| v.into_iter().map(IpAddr::V4).collect();
    match mode {
        DnsMode::System => system(domain),
        DnsMode::Doh => doh(domain).map(v4),
        DnsMode::AltUdp1253 => alt_udp(domain).map(v4),
    }
}

/// İşletim sisteminin çözücüsü; çağrı zaman aşımına uğrarsa iş parçacığı arkada bırakılır.
fn system(domain: &str) -> anyhow::Result<Vec<IpAddr>> {
    let host = format!("{domain}:443");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = host
            .to_socket_addrs()
            .map(|it| it.map(|a| a.ip()).collect::<Vec<_>>());
        let _ = tx.send(r);
    });
    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(r) => Ok(r.context("sistem çözücüsü")?),
        Err(_) => bail!("sistem çözücüsü zaman aşımı"),
    }
}

fn alt_udp(domain: &str) -> anyhow::Result<Vec<Ipv4Addr>> {
    let id = random_id();
    let q = build_query(id, domain)?;
    let sock = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    sock.set_read_timeout(Some(PROBE_TIMEOUT))?;
    sock.connect(alt_addr())?;
    sock.send(&q)?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut buf = [0u8; 1500];
    loop {
        let n = sock.recv(&mut buf).context("UDP 1253 yanıtı")?;
        let reply = buf.get(..n).unwrap_or_default();
        // Kimliği tutmayan datagramlar atlanır; ilk eşleşen yanıt karar verir.
        if reply.get(..2) == Some(&id.to_be_bytes()[..]) {
            return parse_a_answers(reply, id);
        }
        if Instant::now() >= deadline {
            bail!("UDP 1253: eşleşen yanıt yok");
        }
    }
}

fn doh(domain: &str) -> anyhow::Result<Vec<Ipv4Addr>> {
    let id = random_id();
    let q = build_query(id, domain)?;
    let sock = probe::connect_tcp(doh_addr()).context("DoH TCP")?;
    let mut s = probe::handshake(sock, DOH_SNI).context("DoH TLS")?;
    let head = format!(
        "POST /dns-query HTTP/1.1\r\nHost: {DOH_SNI}\r\nUser-Agent: {}\r\nContent-Type: application/dns-message\r\nAccept: application/dns-message\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        probe::USER_AGENT,
        q.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(&q)?;
    s.flush()?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some((status, body)) = probe::parse_http_response(&raw)? {
            if status != 200 {
                bail!("DoH HTTP {status}");
            }
            return parse_a_answers(&body, id);
        }
        if raw.len() > MAX_DOH_RESPONSE {
            bail!("DoH yanıtı çok büyük");
        }
        let n = s.read(&mut chunk).context("DoH okuma")?;
        if n == 0 {
            bail!("DoH yanıtı eksik");
        }
        raw.extend_from_slice(chunk.get(..n).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{Name, RData, Record, RecordType};

    #[test]
    fn dns_wire_query_roundtrip() {
        let q = build_query(0x1234, "example.com").unwrap();
        let parsed = Message::from_vec(&q).unwrap();
        assert_eq!(parsed.metadata.id, 0x1234);
        assert!(parsed.metadata.recursion_desired);
        assert_eq!(parsed.queries.len(), 1);
        assert_eq!(parsed.queries[0].query_type(), RecordType::A);
        assert_eq!(
            parsed.queries[0].name().to_ascii().trim_end_matches('.'),
            "example.com"
        );

        // Fikstür yanıt: TEST-NET-1 adresi (RFC 5737), çalışma zamanında birleştirilir.
        let ip = Ipv4Addr::new(192, 0, 2, 7);
        let name = Name::from_ascii("example.com").unwrap();
        let mut resp = Message::response(0x1234, OpCode::Query);
        resp.add_query(Query::query(name.clone(), RecordType::A));
        resp.add_answer(Record::from_rdata(name, 60, RData::A(A(ip))));
        let wire = resp.to_vec().unwrap();
        assert_eq!(parse_a_answers(&wire, 0x1234).unwrap(), vec![ip]);
        assert!(
            parse_a_answers(&wire, 0x9999).is_err(),
            "kimlik uyuşmazlığı reddedilmeli"
        );

        let mut not_response = Message::new(0x1234, MessageType::Query, OpCode::Query);
        not_response.add_query(Query::query(
            Name::from_ascii("example.com").unwrap(),
            RecordType::A,
        ));
        assert!(parse_a_answers(&not_response.to_vec().unwrap(), 0x1234).is_err());
    }
}
