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

/// Sürüm anlaşmasını zorlayan ayrılmış QUIC sürümü (RFC 9000 §15: `0x?a?a?a?a`). Sunucu bu
/// sürümü tanımaz ve Version Negotiation paketiyle yanıt verir; böylece UDP gidiş-dönüşü ölçülür.
pub const QUIC_GREASE_VERSION: u32 = 0x1a2a_3a4a;

/// QUIC Initial biçimli test datagramı: uzun başlık, verilen sürüm, 1200 bayt (RFC 9000 §14.1).
/// İçerik anlamsızdır; yalnızca UDP iletim yolunu ve olası yanıtı ölçmek içindir.
pub fn quic_probe_datagram(dcid: [u8; 8], version: u32) -> Vec<u8> {
    // Uzun başlık (Initial, 4 baytlık paket numarası), sürüm, DCID, boş SCID, boş belirteç.
    let mut d = vec![0xC3];
    d.extend_from_slice(&version.to_be_bytes());
    d.push(8);
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
    /// Hedef dışı olduğu, motor durdurulduğu ya da eşzamanlı oturum sınırı aşıldığı için
    /// aktarılmayan akışlar.
    pub ignored: AtomicU64,
    /// Motorun kendi giden portundan TUN'a geri dönen akışlar (IP_UNICAST_IF etkisiz demektir).
    pub loops: AtomicU64,
}

impl Stats {
    pub fn add(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// TUN paket giriş/çıkışı (Wintun oturumu ya da testlerde bellek içi sahte).
pub trait PacketIo: Send + Sync + 'static {
    /// Bir IP paketi; şu an yoksa `None`.
    fn recv(&self) -> std::io::Result<Option<Vec<u8>>>;
    /// Okunacak paket olana kadar (en çok `timeout`) bekler.
    fn wait_readable(&self, timeout: Duration) -> bool;
    /// Bir IP paketi yazar.
    fn send(&self, packet: &[u8]) -> std::io::Result<()>;
}

/// Okuma iş parçacığının durdurma bayrağını denetleme aralığı.
const READER_POLL: Duration = Duration::from_millis(200);
/// Okuma kuyruğu (paket). Dolarsa okuma iş parçacığı bekler; fazlası Wintun halkasında düşer.
const READ_QUEUE: usize = 1024;

/// `ipstack` için TUN aygıtı: her okuma bir IP paketi, her yazma bir IP paketi.
/// Okuma ayrı bir iş parçacığında yapılır (Wintun olay tutamacını engelleyerek bekler).
pub struct TunDevice {
    io: Arc<dyn PacketIo>,
    rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl TunDevice {
    pub fn new(io: Arc<dyn PacketIo>) -> TunDevice {
        let (tx, rx) = tokio::sync::mpsc::channel(READ_QUEUE);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (reader_io, reader_stop) = (io.clone(), stop.clone());
        std::thread::spawn(move || {
            while !reader_stop.load(Ordering::Relaxed) {
                match reader_io.recv() {
                    Ok(Some(packet)) => {
                        if tx.blocking_send(packet).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        reader_io.wait_readable(READER_POLL);
                    }
                    // Adaptör kapanıyor (ERROR_HANDLE_EOF) ya da halka bozuk: okuma biter.
                    Err(_) => break,
                }
            }
        });
        TunDevice { io, rx, stop }
    }
}

impl Drop for TunDevice {
    fn drop(&mut self) {
        // İş parçacığı en geç `READER_POLL` sonra çıkar ve TUN başvurusunu bırakır.
        self.stop.store(true, Ordering::Relaxed);
        self.rx.close();
    }
}

impl AsyncRead for TunDevice {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(packet)) => {
                // Ara bellekten büyük paket kırpılır (ipstack MTU'su adaptörünkine eşit tutulur).
                let n = packet.len().min(buf.remaining());
                buf.put_slice(packet.get(..n).unwrap_or_default());
                std::task::Poll::Ready(Ok(()))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Ok(())),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl AsyncWrite for TunDevice {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        // Wintun halkasına yazmak engellemez; ipstack her paketi tek `write_all` ile verir.
        std::task::Poll::Ready(self.io.send(buf).map(|()| buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// Taşıma protokolü (döngü korumasında portlar protokole göre ayrı tutulur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Proto {
    Tcp,
    Udp,
}

/// Giden soketin kaydı; düşünce port kayıttan silinir.
pub struct OutboundGuard {
    engine: Arc<Engine>,
    key: OutboundKey,
}

/// (protokol, motorun yerel portu, bağlandığı uzak adres). Döngüde TUN'a dönen akışın kaynak
/// portu ve hedefi bunlarla aynıdır; yalnız porta bakmak, aynı port numarasını başka bir
/// hedef için kullanan uygulama akışlarında yanlış alarm verirdi.
type OutboundKey = (Proto, u16, SocketAddr);

impl Drop for OutboundGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = self.engine.outbound_ports.lock() {
            set.remove(&self.key);
        }
    }
}

/// Eşzamanlı aktarım üst sınırı. S4'te gerçek istemciler (tarayıcı, Discord) çok sayıda
/// bağlantı açar; sınır kaynak tüketimini yine de sınırlar.
const MAX_ACTIVE: u64 = 256;

/// Kaydedilecek en çok bağlantı (bellek sınırı).
const MAX_FLOWS: usize = 4000;

/// Bağlantı başına canlı kayıt (S4 tanısı). Açılışta eklenir; söküme kadar kapanmayan akış
/// "açık" görünür (ör. yanıtsız kalan bir istek).
#[derive(Debug)]
pub struct Flow {
    dst: SocketAddr,
    started: std::time::Instant,
    sni: std::sync::Mutex<Option<String>>,
    first_bytes: AtomicU64,
    tls: std::sync::atomic::AtomicBool,
    up: AtomicU64,
    down: AtomicU64,
    end: std::sync::Mutex<Option<String>>,
    ms: AtomicU64,
}

/// `hold.json` için bağlantı kaydının anlık görüntüsü.
#[derive(Debug, Clone, Serialize)]
pub struct FlowRecord {
    pub dst: String,
    pub sni: Option<String>,
    pub first_bytes: u64,
    pub tls: bool,
    pub up: u64,
    pub down: u64,
    pub ms: u64,
    pub end: Option<String>,
}

impl Flow {
    fn finish(&self, end: String) {
        if let Ok(mut e) = self.end.lock() {
            *e = Some(end);
        }
        self.ms.store(
            u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
}

/// Giden akışı sarar ve iki yöndeki baytı akış kaydına sayar.
struct Counted<S> {
    inner: S,
    flow: Arc<Flow>,
}

impl<S: AsyncRead + Unpin> AsyncRead for Counted<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = buf.filled().len().saturating_sub(before);
        self.flow
            .down
            .fetch_add(u64::try_from(n).unwrap_or(u64::MAX), Ordering::Relaxed);
        r
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Counted<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = &r {
            self.flow
                .up
                .fetch_add(u64::try_from(*n).unwrap_or(u64::MAX), Ordering::Relaxed);
        }
        r
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// İletim motorunun paylaşılan durumu: geçerli teknik, sayaçlar ve döngü koruması.
pub struct Engine {
    tech: std::sync::Mutex<SockTech>,
    pub stats: Arc<Stats>,
    outbound_ports: std::sync::Mutex<std::collections::HashSet<OutboundKey>>,
    halted: std::sync::atomic::AtomicBool,
    active: AtomicU64,
    flows: std::sync::Mutex<Vec<Arc<Flow>>>,
}

impl Engine {
    pub fn new(tech: SockTech) -> Arc<Engine> {
        Arc::new(Engine {
            tech: std::sync::Mutex::new(tech),
            stats: Arc::new(Stats::default()),
            outbound_ports: std::sync::Mutex::new(std::collections::HashSet::new()),
            halted: std::sync::atomic::AtomicBool::new(false),
            active: AtomicU64::new(0),
            flows: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Motorun açtığı giden soketi (yerel port + uzak adres) kaydeder.
    /// Kayıt, dönen nesne yaşadıkça (soket açıkken) geçerlidir.
    pub fn mark_outbound(
        self: &Arc<Self>,
        proto: Proto,
        port: u16,
        remote: SocketAddr,
    ) -> OutboundGuard {
        let key = (proto, port, remote);
        if let Ok(mut set) = self.outbound_ports.lock() {
            set.insert(key);
        }
        OutboundGuard {
            engine: self.clone(),
            key,
        }
    }

    /// Bu akış (kaynak port + hedef), motorun şu an açık bir giden soketi mi?
    pub fn is_own(&self, proto: Proto, port: u16, remote: SocketAddr) -> bool {
        // Kilit zehirlendiyse güvenli tarafta kal: döngü say.
        self.outbound_ports
            .lock()
            .map_or(true, |set| set.contains(&(proto, port, remote)))
    }

    /// Yeni TCP akışı kaydı açar (sınır dolduysa kayıt tutulmaz ama akış yine aktarılır).
    fn open_flow(&self, dst: SocketAddr) -> Arc<Flow> {
        let flow = Arc::new(Flow {
            dst,
            started: std::time::Instant::now(),
            sni: std::sync::Mutex::new(None),
            first_bytes: AtomicU64::new(0),
            tls: std::sync::atomic::AtomicBool::new(false),
            up: AtomicU64::new(0),
            down: AtomicU64::new(0),
            end: std::sync::Mutex::new(None),
            ms: AtomicU64::new(0),
        });
        if let Ok(mut v) = self.flows.lock()
            && v.len() < MAX_FLOWS
        {
            v.push(flow.clone());
        }
        flow
    }

    /// Bağlantı kayıtlarının anlık görüntüsü; kapanmamış akışın sonucu `None`.
    pub fn flows(&self) -> Vec<FlowRecord> {
        let Ok(v) = self.flows.lock() else {
            return Vec::new();
        };
        v.iter()
            .map(|f| {
                let end = f.end.lock().ok().and_then(|e| e.clone());
                let ms = if end.is_some() {
                    f.ms.load(Ordering::Relaxed)
                } else {
                    u64::try_from(f.started.elapsed().as_millis()).unwrap_or(u64::MAX)
                };
                FlowRecord {
                    dst: f.dst.to_string(),
                    sni: f.sni.lock().ok().and_then(|s| s.clone()),
                    first_bytes: f.first_bytes.load(Ordering::Relaxed),
                    tls: f.tls.load(Ordering::Relaxed),
                    up: f.up.load(Ordering::Relaxed),
                    down: f.down.load(Ordering::Relaxed),
                    ms,
                    end,
                }
            })
            .collect()
    }

    /// Döngü saptandıysa motor durmuştur; artık hiçbir akış aktarılmaz.
    pub fn halted(&self) -> bool {
        self.halted.load(Ordering::Relaxed)
    }

    /// Yeni aktarım için yer ayırır; sınır aşıldıysa `false`.
    fn try_enter(&self) -> bool {
        if self.active.fetch_add(1, Ordering::AcqRel) >= MAX_ACTIVE {
            self.active.fetch_sub(1, Ordering::AcqRel);
            return false;
        }
        true
    }

    fn leave(&self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }

    /// Sonraki bağlantılarda uygulanacak tekniği değiştirir.
    pub fn set_tech(&self, tech: SockTech) {
        if let Ok(mut t) = self.tech.lock() {
            *t = tech;
        }
    }

    fn current_tech(&self) -> SockTech {
        self.tech.lock().map(|t| *t).unwrap_or(SockTech::None)
    }
}

/// Motorun ilettiği hedefler: sabit IPv4 önekleri (sağlayıcının kendi bloğu) ve çalışırken
/// öğrenilen tek IP'ler (paylaşımlı CDN'lerdeki alan adları). Rota kümesiyle aynı tutulur.
#[derive(Debug)]
pub struct RouteSet {
    /// (ağ, maske)
    prefixes: Vec<(u32, u32)>,
    hosts: std::sync::RwLock<std::collections::HashSet<std::net::Ipv4Addr>>,
}

impl RouteSet {
    pub fn new(prefixes: &[(std::net::Ipv4Addr, u8)]) -> anyhow::Result<Self> {
        let mut out = Vec::with_capacity(prefixes.len());
        for (net, len) in prefixes {
            if *len > 32 {
                anyhow::bail!("geçersiz önek uzunluğu: {net}/{len}");
            }
            let mask = u32::MAX.checked_shl(32 - u32::from(*len)).unwrap_or(0);
            let bits = u32::from(*net);
            if bits & !mask != 0 {
                anyhow::bail!("önek ağ biçiminde değil: {net}/{len}");
            }
            out.push((bits, mask));
        }
        Ok(Self {
            prefixes: out,
            hosts: std::sync::RwLock::default(),
        })
    }

    /// Tek test IP'si (Görev 7 `run` / `kill-test`).
    pub fn single(ip: std::net::Ipv4Addr) -> Self {
        let set = Self {
            prefixes: Vec::new(),
            hosts: std::sync::RwLock::default(),
        };
        set.add_host(ip);
        set
    }

    /// Yeni tek IP ekler; zaten varsa `false`.
    pub fn add_host(&self, ip: std::net::Ipv4Addr) -> bool {
        self.hosts.write().is_ok_and(|mut h| h.insert(ip))
    }

    pub fn host_count(&self) -> usize {
        self.hosts.read().map_or(0, |h| h.len())
    }

    pub fn contains(&self, ip: std::net::IpAddr) -> bool {
        let std::net::IpAddr::V4(v4) = ip else {
            return false;
        };
        let bits = u32::from(v4);
        self.prefixes.iter().any(|(net, mask)| bits & mask == *net)
            || self.hosts.read().is_ok_and(|h| h.contains(&v4))
    }
}

/// Kabul döngüsü: yalnızca rota kümesindeki adreslere giden TCP/UDP akışları aktarılır; diğer
/// her şey (adaptöre düşen çoklu yayın, yayın, başka hedefler) sayılır ve düşürülür.
pub async fn serve(
    mut stack: ipstack::IpStack,
    routes: Arc<RouteSet>,
    phys_if: Option<u32>,
    engine: Arc<Engine>,
) {
    use ipstack::IpStackStream;
    // Akışı aktarabilir miyiz? Döngü saptanırsa motor kalıcı olarak durur.
    let admit = |engine: &Engine, proto: Proto, local: SocketAddr, peer: SocketAddr| -> bool {
        if engine.halted() {
            Stats::add(&engine.stats.ignored);
            return false;
        }
        if engine.is_own(proto, local.port(), peer) {
            Stats::add(&engine.stats.loops);
            engine.halted.store(true, Ordering::Relaxed);
            return false;
        }
        if !engine.try_enter() {
            Stats::add(&engine.stats.ignored);
            return false;
        }
        true
    };
    loop {
        match stack.accept().await {
            Ok(IpStackStream::Tcp(stream)) if routes.contains(stream.peer_addr().ip()) => {
                if !admit(&engine, Proto::Tcp, stream.local_addr(), stream.peer_addr()) {
                    continue;
                }
                let (peer, tech, engine) =
                    (stream.peer_addr(), engine.current_tech(), engine.clone());
                tokio::spawn(async move {
                    let _ = relay_tcp(stream, peer, phys_if, tech, engine.clone()).await;
                    engine.leave();
                });
            }
            Ok(IpStackStream::Udp(stream)) if routes.contains(stream.peer_addr().ip()) => {
                if !admit(&engine, Proto::Udp, stream.local_addr(), stream.peer_addr()) {
                    continue;
                }
                let (peer, engine) = (stream.peer_addr(), engine.clone());
                tokio::spawn(async move {
                    let _ = relay_udp(stream, peer, phys_if, engine.clone()).await;
                    engine.leave();
                });
            }
            Ok(_) => Stats::add(&engine.stats.ignored),
            Err(_) => break,
        }
    }
}

/// UDP oturumunun boşta kalma süresi.
const UDP_IDLE: Duration = Duration::from_secs(20);

/// UDP aktarımı: `inbound` (TUN tarafı) ↔ `peer` (gerçek hedef); datagram sınırları korunur.
pub async fn relay_udp<I>(
    mut inbound: I,
    peer: SocketAddr,
    phys_if: Option<u32>,
    engine: Arc<Engine>,
) -> anyhow::Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let stats = engine.stats.clone();
    Stats::add(&stats.udp_sessions);
    let sock = Socket::new(Domain::for_address(peer), Type::DGRAM, Some(Protocol::UDP))?;
    if let Some(i) = phys_if {
        crate::ffi::set_unicast_if_v4(&sock, i)?;
    }
    sock.bind(&SockAddr::from(SocketAddr::from(([0, 0, 0, 0], 0))))?;
    let _guard = match sock.local_addr()?.as_socket() {
        Some(local) => engine.mark_outbound(Proto::Udp, local.port(), peer),
        None => anyhow::bail!("UDP soketinin yerel adresi yok"),
    };
    sock.connect(&SockAddr::from(peer))?;
    sock.set_nonblocking(true)?;
    let std_sock: std::net::UdpSocket = sock.into();
    let out = tokio::net::UdpSocket::from_std(std_sock)?;
    let mut from_tun = vec![0u8; 65_536];
    let mut from_net = vec![0u8; 65_536];
    loop {
        tokio::select! {
            r = inbound.read(&mut from_tun) => match r {
                // 0 bayt: oturum kapandı; hata: ipstack'in boşta kalma zaman aşımı.
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.send(from_tun.get(..n).unwrap_or_default()).await?;
                    Stats::add(&stats.udp_datagrams_out);
                }
            },
            r = out.recv(&mut from_net) => match r {
                Ok(n) => {
                    inbound.write_all(from_net.get(..n).unwrap_or_default()).await?;
                    Stats::add(&stats.udp_datagrams_in);
                }
                // Windows, ICMP "port unreachable" yanıtını bağlı UDP soketinde hata olarak verir.
                Err(_) => break,
            },
            () = tokio::time::sleep(UDP_IDLE) => break,
        }
    }
    Ok(())
}

/// Hedef bağlantı zaman aşımı.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// İlk uçuşu toplamak için en çok bekleme ve bayt.
const FIRST_FLIGHT_WAIT: Duration = Duration::from_secs(2);
const MAX_FIRST_FLIGHT: usize = 16 * 1024;

/// Gerçek hedefe engelleyen soketle bağlanır; arayüz verilmişse `IP_UNICAST_IF` uygulanır.
/// Yerel port bağlanmadan önce alınır ve döngü koruması için kaydedilir.
fn connect_out(
    peer: SocketAddr,
    phys_if: Option<u32>,
    engine: &Arc<Engine>,
) -> anyhow::Result<(Socket, OutboundGuard)> {
    let sock = Socket::new(Domain::for_address(peer), Type::STREAM, Some(Protocol::TCP))?;
    if let Some(i) = phys_if {
        crate::ffi::set_unicast_if_v4(&sock, i)?;
    }
    sock.bind(&SockAddr::from(SocketAddr::from(([0, 0, 0, 0], 0))))?;
    let guard = match sock.local_addr()?.as_socket() {
        Some(local) => engine.mark_outbound(Proto::Tcp, local.port(), peer),
        None => anyhow::bail!("TCP soketinin yerel adresi yok"),
    };
    sock.connect_timeout(&SockAddr::from(peer), CONNECT_TIMEOUT)?;
    Ok((sock, guard))
}

/// TCP aktarımı: `inbound` (TUN tarafı) ↔ `peer` (gerçek hedef).
pub async fn relay_tcp<I>(
    inbound: I,
    peer: SocketAddr,
    phys_if: Option<u32>,
    tech: SockTech,
    engine: Arc<Engine>,
) -> anyhow::Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    Stats::add(&engine.stats.tcp_sessions);
    let flow = engine.open_flow(peer);
    let result = relay_inner(inbound, peer, phys_if, tech, engine.clone(), flow.clone()).await;
    match &result {
        Ok(()) => flow.finish("kapandı".into()),
        Err(e) => {
            Stats::add(&engine.stats.tcp_errors);
            flow.finish(format!("hata: {e:#}"));
        }
    }
    result
}

async fn relay_inner<I>(
    mut inbound: I,
    peer: SocketAddr,
    phys_if: Option<u32>,
    tech: SockTech,
    engine: Arc<Engine>,
    flow: Arc<Flow>,
) -> anyhow::Result<()>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::AsyncReadExt;
    // Kayıt aktarım bitene kadar tutulur (`_guard`).
    let (sock, _guard) =
        tokio::task::spawn_blocking(move || connect_out(peer, phys_if, &engine)).await??;
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
    let is_tls = matches!(state, FirstFlight::Complete(_));
    let use_tech = if is_tls { tech } else { SockTech::None };
    let first_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
    flow.first_bytes.store(first_len, Ordering::Relaxed);
    flow.up.fetch_add(first_len, Ordering::Relaxed);
    flow.tls.store(is_tls, Ordering::Relaxed);
    if let Some(name) = xtask::lab::sockettech::find_sni(&buf)
        .and_then(|r| buf.get(r))
        .map(|b| String::from_utf8_lossy(b).into_owned())
        && let Ok(mut s) = flow.sni.lock()
    {
        *s = Some(name);
    }
    let sock = tokio::task::spawn_blocking(move || -> anyhow::Result<Socket> {
        if !buf.is_empty() {
            xtask::lab::sockettech::send_first_flight(&sock, &buf, &use_tech)?;
        }
        sock.set_nonblocking(true)?;
        Ok(sock)
    })
    .await??;
    let std_stream: std::net::TcpStream = sock.into();
    let mut out = Counted {
        inner: tokio::net::TcpStream::from_std(std_stream)?,
        flow,
    };
    tokio::io::copy_bidirectional(&mut inbound, &mut out).await?;
    Ok(())
}

#[cfg(test)]
mod route_set_tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn prefixes_cover_exact_range() {
        let r = RouteSet::new(&[(Ipv4Addr::new(128, 116, 0, 0), 17)]).unwrap();
        assert!(r.contains(v4(128, 116, 0, 0)));
        assert!(r.contains(v4(128, 116, 127, 255)));
        assert!(!r.contains(v4(128, 116, 128, 0)));
        assert!(!r.contains(v4(128, 115, 255, 255)));
        assert!(!r.contains(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn learned_hosts_are_added_once() {
        let r = RouteSet::new(&[]).unwrap();
        let ip = Ipv4Addr::new(162, 159, 128, 233);
        assert!(!r.contains(IpAddr::V4(ip)));
        assert!(r.add_host(ip));
        assert!(!r.add_host(ip));
        assert!(r.contains(IpAddr::V4(ip)));
        assert_eq!(r.host_count(), 1);
    }

    #[test]
    fn single_matches_only_that_ip() {
        let r = RouteSet::single(Ipv4Addr::new(203, 0, 113, 9));
        assert!(r.contains(v4(203, 0, 113, 9)));
        assert!(!r.contains(v4(203, 0, 113, 10)));
    }

    #[test]
    fn non_network_prefix_is_rejected() {
        assert!(RouteSet::new(&[(Ipv4Addr::new(128, 116, 1, 0), 17)]).is_err());
        assert!(RouteSet::new(&[(Ipv4Addr::new(1, 0, 0, 0), 33)]).is_err());
    }
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
        let d = quic_probe_datagram([7; 8], 1);
        assert_eq!(d.len(), 1200);
        assert_eq!(d[0] & 0xC0, 0xC0, "uzun başlık + sabit bit");
        assert_eq!(&d[1..5], &[0, 0, 0, 1], "QUIC sürüm 1");
        assert_eq!(d[5], 8, "DCID uzunluğu");
        assert_eq!(&d[6..14], &[7; 8]);
        let g = quic_probe_datagram([7; 8], QUIC_GREASE_VERSION);
        assert_eq!(&g[1..5], &[0x1a, 0x2a, 0x3a, 0x4a]);
        assert_eq!(
            QUIC_GREASE_VERSION & 0x0f0f_0f0f,
            0x0a0a_0a0a,
            "RFC 9000 §15 kalıbı"
        );
    }

    /// Bellek içi TUN: gelen paket kuyruğu ve yazılan paketlerin kaydı.
    #[derive(Default)]
    struct FakeIo {
        inbound: std::sync::Mutex<std::collections::VecDeque<Vec<u8>>>,
        ready: std::sync::Condvar,
        sent: std::sync::Mutex<Vec<Vec<u8>>>,
    }

    impl FakeIo {
        fn push(&self, p: Vec<u8>) {
            self.inbound.lock().unwrap().push_back(p);
            self.ready.notify_all();
        }
        fn sent(&self) -> Vec<Vec<u8>> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl PacketIo for FakeIo {
        fn recv(&self) -> std::io::Result<Option<Vec<u8>>> {
            Ok(self.inbound.lock().unwrap().pop_front())
        }
        fn wait_readable(&self, timeout: Duration) -> bool {
            let q = self.inbound.lock().unwrap();
            let (q, _) = self
                .ready
                .wait_timeout_while(q, timeout, |q| q.is_empty())
                .unwrap();
            !q.is_empty()
        }
        fn send(&self, packet: &[u8]) -> std::io::Result<()> {
            self.sent.lock().unwrap().push(packet.to_vec());
            Ok(())
        }
    }

    async fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
        for _ in 0..150 {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        cond()
    }

    #[tokio::test]
    async fn tun_device_moves_whole_packets_both_ways() {
        let io = Arc::new(FakeIo::default());
        let mut dev = TunDevice::new(io.clone());
        io.push(vec![1; 40]);
        io.push(vec![2; 1300]);
        let mut buf = vec![0u8; 2000];
        let n = tokio::time::timeout(Duration::from_secs(2), dev.read(&mut buf))
            .await
            .expect("okuma zaman aşımı")
            .unwrap();
        assert_eq!(&buf[..n], &[1u8; 40][..], "ilk okuma tam olarak ilk paket");
        let n = tokio::time::timeout(Duration::from_secs(2), dev.read(&mut buf))
            .await
            .expect("okuma zaman aşımı")
            .unwrap();
        assert_eq!(n, 1300, "ikinci okuma ikinci paket; paketler birleşmez");
        dev.write_all(&[9; 60]).await.unwrap();
        dev.write_all(&[8; 20]).await.unwrap();
        assert_eq!(io.sent(), vec![vec![9; 60], vec![8; 20]]);
    }

    #[tokio::test]
    async fn tun_device_reader_stops_on_drop() {
        let io = Arc::new(FakeIo::default());
        let dev = TunDevice::new(io.clone());
        io.push(vec![1; 40]);
        assert!(
            wait_until(|| io.inbound.lock().unwrap().is_empty()).await,
            "okuma iş parçacığı paketi almalı"
        );
        drop(dev);
        assert!(
            wait_until(|| Arc::strong_count(&io) == 1).await,
            "aygıt düşünce okuma iş parçacığı TUN'u bırakmalı"
        );
    }

    fn udp_packet(dst: SocketAddr, payload: &[u8]) -> Vec<u8> {
        let std::net::IpAddr::V4(dst_ip) = dst.ip() else {
            panic!("IPv4 bekleniyor")
        };
        let b = etherparse::PacketBuilder::ipv4([198, 18, 0, 1], dst_ip.octets(), 64)
            .udp(5555, dst.port());
        let mut out = Vec::with_capacity(b.size(payload.len()));
        b.write(&mut out, payload).unwrap();
        out
    }

    /// IPv4 (seçeneksiz) + UDP paketinden (kaynak, hedef, yük).
    fn parse_udp(p: &[u8]) -> (SocketAddr, SocketAddr, Vec<u8>) {
        assert_eq!(p[0], 0x45, "IPv4, 20 baytlık başlık");
        assert_eq!(p[9], 17, "UDP");
        let ip = |o: usize| Ipv4Addr::new(p[o], p[o + 1], p[o + 2], p[o + 3]);
        let port = |o: usize| u16::from_be_bytes([p[o], p[o + 1]]);
        (
            SocketAddr::from((ip(12), port(20))),
            SocketAddr::from((ip(16), port(22))),
            p[28..].to_vec(),
        )
    }

    /// Uçtan uca UDP: TUN'a giren datagram yığından ve aktarımdan geçip yerel yankı sunucusuna
    /// gider; yanıt TUN'a, adresleri yer değiştirmiş bir IP paketi olarak yazılır.
    #[tokio::test]
    async fn udp_roundtrip_through_stack_and_relay() {
        let echo = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let echo_addr = echo.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut b = [0u8; 2048];
            if let Ok((n, from)) = echo.recv_from(&mut b) {
                let reply: Vec<u8> = b[..n].iter().map(u8::to_ascii_uppercase).collect();
                let _ = echo.send_to(&reply, from);
            }
        });
        let io = Arc::new(FakeIo::default());
        let stack = ipstack::IpStack::new(
            ipstack::IpStackConfig::default(),
            TunDevice::new(io.clone()),
        );
        let engine = Engine::new(SockTech::None);
        let task = tokio::spawn(serve(
            stack,
            Arc::new(RouteSet::single(Ipv4Addr::LOCALHOST)),
            None,
            engine.clone(),
        ));
        io.push(udp_packet(echo_addr, b"merhaba"));
        assert!(
            wait_until(|| !io.sent().is_empty()).await,
            "yanıt TUN'a yazılmadı"
        );
        let (src, dst, payload) = parse_udp(&io.sent()[0]);
        assert_eq!(src, echo_addr);
        assert_eq!(dst, SocketAddr::from(([198, 18, 0, 1], 5555)));
        assert_eq!(payload, b"MERHABA");
        assert_eq!(engine.stats.udp_sessions.load(Ordering::Relaxed), 1);
        assert_eq!(engine.stats.udp_datagrams_out.load(Ordering::Relaxed), 1);
        assert_eq!(engine.stats.udp_datagrams_in.load(Ordering::Relaxed), 1);
        task.abort();
    }

    fn udp_packet_from(src_port: u16, dst: SocketAddr, payload: &[u8]) -> Vec<u8> {
        let std::net::IpAddr::V4(dst_ip) = dst.ip() else {
            panic!("IPv4 bekleniyor")
        };
        let b = etherparse::PacketBuilder::ipv4([198, 18, 0, 1], dst_ip.octets(), 64)
            .udp(src_port, dst.port());
        let mut out = Vec::with_capacity(b.size(payload.len()));
        b.write(&mut out, payload).unwrap();
        out
    }

    /// Motorun kendi giden portundan gelen akış döngü demektir (IP_UNICAST_IF etkisiz): sayılır,
    /// aktarılmaz ve motor bütünüyle durur; sonraki akışlar da aktarılmaz.
    #[tokio::test]
    async fn loop_guard_halts_on_flow_from_own_outbound_port() {
        let echo = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        echo.set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let echo_addr = echo.local_addr().unwrap();
        let io = Arc::new(FakeIo::default());
        let stack = ipstack::IpStack::new(
            ipstack::IpStackConfig::default(),
            TunDevice::new(io.clone()),
        );
        let engine = Engine::new(SockTech::None);
        let _guard = engine.mark_outbound(Proto::Udp, 5555, echo_addr);
        assert!(engine.is_own(Proto::Udp, 5555, echo_addr));
        assert!(!engine.is_own(Proto::Udp, 6000, echo_addr));
        assert!(
            !engine.is_own(Proto::Tcp, 5555, echo_addr),
            "protokoller ayrı"
        );
        // Aynı port başka bir hedefe: uygulamanın kendi akışı olabilir, döngü sayılmaz
        // (2026-11-12 arıza incelemesi: yalnız porta bakmak yanlış alarm verirdi).
        let other: SocketAddr = (Ipv4Addr::new(1, 1, 1, 1), 443).into();
        assert!(!engine.is_own(Proto::Udp, 5555, other));
        let task = tokio::spawn(serve(
            stack,
            Arc::new(RouteSet::single(Ipv4Addr::LOCALHOST)),
            None,
            engine.clone(),
        ));
        io.push(udp_packet_from(5555, echo_addr, b"dongu"));
        assert!(wait_until(|| engine.stats.loops.load(Ordering::Relaxed) == 1).await);
        assert!(engine.halted());
        io.push(udp_packet_from(6000, echo_addr, b"sonraki"));
        assert!(wait_until(|| engine.stats.ignored.load(Ordering::Relaxed) == 1).await);
        let mut b = [0u8; 64];
        assert!(echo.recv_from(&mut b).is_err(), "durmuş motor aktardı");
        assert_eq!(engine.stats.udp_sessions.load(Ordering::Relaxed), 0);
        task.abort();
    }

    /// Hedef dışı trafik (ör. adaptöre düşen çoklu yayın) gerçek ağa aktarılmaz.
    #[tokio::test]
    async fn non_target_traffic_is_counted_and_dropped() {
        let other = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        other
            .set_read_timeout(Some(Duration::from_millis(400)))
            .unwrap();
        let other_addr = other.local_addr().unwrap();
        let io = Arc::new(FakeIo::default());
        let stack = ipstack::IpStack::new(
            ipstack::IpStackConfig::default(),
            TunDevice::new(io.clone()),
        );
        let engine = Engine::new(SockTech::None);
        // Hedef başka bir adres: yerel sokete giden datagram aktarılmamalı.
        let routes = Arc::new(RouteSet::single(Ipv4Addr::new(203, 0, 113, 9)));
        let task = tokio::spawn(serve(stack, routes, None, engine.clone()));
        io.push(udp_packet(other_addr, b"gitmemeli"));
        assert!(wait_until(|| engine.stats.ignored.load(Ordering::Relaxed) == 1).await);
        let mut b = [0u8; 64];
        assert!(
            other.recv_from(&mut b).is_err(),
            "hedef dışı datagram aktarıldı"
        );
        assert_eq!(engine.stats.udp_sessions.load(Ordering::Relaxed), 0);
        task.abort();
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
            let (mut s, from) = listener.accept().unwrap();
            let mut got = vec![0u8; want_len];
            s.read_exact(&mut got).unwrap();
            s.write_all(b"yanit").unwrap();
            (got, from.port())
        });
        let (mut app, tun_side) = tokio::io::duplex(64 * 1024);
        let engine = Engine::new(tech);
        let relay = tokio::spawn(relay_tcp(tun_side, addr, None, tech, engine.clone()));
        app.write_all(&first).await.unwrap();
        app.write_all(b"sonr").await.unwrap();
        let mut reply = [0u8; 5];
        app.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"yanit");
        let (got, out_port) = server.join().unwrap();
        assert_eq!(got, expected, "{tech:?}");
        assert!(
            engine.is_own(Proto::Tcp, out_port, addr),
            "giden port döngü koruması için kaydedilmeli"
        );
        drop(app);
        relay.await.unwrap().unwrap();
        assert!(
            !engine.is_own(Proto::Tcp, out_port, addr),
            "oturum kapanınca port kayıttan silinmeli"
        );
        assert_eq!(engine.stats.tcp_sessions.load(Ordering::Relaxed), 1);
        // Bağlantı kaydı (S4 tanısı): hedef, ilk uçuş, iki yönde bayt, sonuç.
        let flows = engine.flows();
        assert_eq!(flows.len(), 1);
        let f = &flows[0];
        assert_eq!(f.dst, addr.to_string());
        // İlk okuma sonraki baytları da getirebilir: ilk uçuş en az ClientHello kadardır.
        assert!((first.len() as u64..=first.len() as u64 + 4).contains(&f.first_bytes));
        assert_eq!(f.up, first.len() as u64 + 4, "ilk uçuş + \"sonr\"");
        assert_eq!(f.down, 5, "\"yanit\"");
        assert_eq!(f.end.as_deref(), Some("kapandı"));
        if matches!(first_flight(&first), FirstFlight::Complete(_)) {
            assert!(f.tls);
            assert!(f.sni.is_some(), "TLS ilk uçuşunda SNI okunmalı");
        }
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
