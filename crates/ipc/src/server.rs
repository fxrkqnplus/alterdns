// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! IPC sunucusu: kabul iş parçacığı, bağlantı başına bir iş parçacığı, abone başına bir yazıcı.
//!
//! - İlk mesaj 5 sn içinde `svc.hello` olmalıdır; değilse bağlantı kapanır.
//! - Çerçeve hatası (uzunluk, bozuk JSON, başka zarf sürümü) bağlantıyı kapatır. Yöntem ve yetki
//!   hataları yanıt olarak döner, bağlantı sürer.
//! - Her istek çağıranın belirteciyle yetkilendirilir (K10).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_winsys::{PipeConn, PipeListener, Stopper};
use serde_json::{Value, json};

use crate::envelope::{Event, Message, Request, Response};
use crate::events::EventBus;
use crate::frame::{decode, decode_len, encode};
use crate::method::Method;
use crate::policy::{Caller, authorize};
use crate::version::version_compatible;

/// Ürün DACL'i: SYSTEM ve Administrators tam; etkileşimli kullanıcı yalnızca okur ve yazar
/// (`FILE_GENERIC_READ | FILE_WRITE_DATA`). `FILE_APPEND_DATA` pipe'larda
/// `FILE_CREATE_PIPE_INSTANCE` demektir ve verilmez: kullanıcı aynı ada sunucu örneği açamaz.
///
/// Sahip açıkça SYSTEM'dir (`O:SY`): istemci sahibin SYSTEM olmasını ister (K21) ve servis
/// belirtecinin varsayılan sahibi Administrators olabilir. Sahibi SYSTEM olan bir pipe'ı
/// yalnızca SYSTEM açabilir; kullanıcı kendi açtığı pipe'a bu sahibi veremez.
pub const PRODUCT_SDDL: &str = "O:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)";
/// İlk mesaj (`svc.hello`, kimlik doğrulanmadan önce) en çok 4 KiB.
pub const MAX_HELLO: usize = 4 * 1024;
/// İstemciden gelen istek en çok 64 KiB (Faz 1 yöntemlerinin parametreleri küçüktür).
/// Yanıt ve olaylar `MAX_MESSAGE`'a (1 MiB) kadar olabilir.
pub const MAX_REQUEST: usize = 64 * 1024;
/// Testler: sahibi (test işlemi) kendi örneklerini de açabilir.
#[cfg(feature = "test-ops")]
pub const TEST_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)(A;;GA;;;OW)";
pub const MAX_INSTANCES: u32 = 16;
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Yöntemleri uygulayan taraf. `svc.hello`, `svc.ping` ve `events.subscribe` sunucuda karşılanır.
pub trait Handler: Send + Sync + 'static {
    fn call(&self, caller: &Caller, method: Method, params: &Value) -> Result<Value, AdnsError>;
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub pipe_name: String,
    /// `%ProgramFiles%\AlterDNS`: `Elevated` yöntemlerin imaj yolu denetimi.
    pub install_root: PathBuf,
    pub server_version: String,
    /// Servis başlangıcı (unix ms); istemci yeniden başlamayı buradan anlar.
    pub instance: u64,
    pub io_timeout: Duration,
    sddl: &'static str,
}

impl ServerConfig {
    pub fn product(install_root: PathBuf, server_version: &str, instance: u64) -> ServerConfig {
        ServerConfig {
            pipe_name: crate::PIPE_NAME.to_string(),
            install_root,
            server_version: server_version.to_string(),
            instance,
            io_timeout: IO_TIMEOUT,
            sddl: PRODUCT_SDDL,
        }
    }

    /// Yalnızca testler: test DACL'i.
    #[cfg(feature = "test-ops")]
    #[must_use]
    pub fn test_access(mut self) -> ServerConfig {
        self.sddl = TEST_SDDL;
        self
    }
}

struct Conn {
    pipe: PipeConn,
    write: Mutex<()>,
}

impl Conn {
    fn send(&self, msg: &Message, timeout: Duration) -> std::io::Result<()> {
        let frame = encode(msg).map_err(std::io::Error::other)?;
        let _g = self.write.lock().unwrap_or_else(PoisonError::into_inner);
        self.pipe.write_all_timeout(&frame, timeout)
    }
}

struct Shared {
    cfg: ServerConfig,
    handler: Arc<dyn Handler>,
    bus: EventBus,
    conns: Mutex<Vec<Weak<Conn>>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    stopping: AtomicBool,
}

impl Shared {
    fn track(&self, t: JoinHandle<()>) {
        let mut g = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        g.retain(|t| !t.is_finished());
        g.push(t);
    }
}

pub struct Server {
    shared: Arc<Shared>,
    stopper: Stopper,
    acceptor: Option<JoinHandle<()>>,
}

impl Server {
    /// Pipe başka bir işlemdeyse `IpcPipeInUse` (T-IPC-002).
    pub fn start(cfg: ServerConfig, handler: Arc<dyn Handler>) -> Result<Server, AdnsError> {
        let listener =
            PipeListener::bind(&cfg.pipe_name, cfg.sddl, MAX_INSTANCES).map_err(|e| {
                let code = if e.kind() == std::io::ErrorKind::AddrInUse {
                    AdnsErrorCode::IpcPipeInUse
                } else {
                    AdnsErrorCode::GenUnexpected
                };
                AdnsError::new(code).with_source(e)
            })?;
        let stopper = listener.stopper();
        let shared = Arc::new(Shared {
            cfg,
            handler,
            bus: EventBus::default(),
            conns: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            stopping: AtomicBool::new(false),
        });
        let s = shared.clone();
        let acceptor = std::thread::spawn(move || accept_loop(&listener, &s));
        Ok(Server {
            shared,
            stopper,
            acceptor: Some(acceptor),
        })
    }

    pub fn events(&self) -> EventBus {
        self.shared.bus.clone()
    }

    /// Bütün iş parçacıkları bitene kadar bekler.
    pub fn stop(self) {}

    fn shutdown(&mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        self.stopper.stop();
        if let Some(a) = self.acceptor.take() {
            let _ = a.join();
        }
        let conns = std::mem::take(
            &mut *self
                .shared
                .conns
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for c in conns.iter().filter_map(Weak::upgrade) {
            c.pipe.close();
        }
        self.shared.bus.clear();
        let threads = std::mem::take(
            &mut *self
                .shared
                .threads
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for t in threads {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: &PipeListener, sh: &Arc<Shared>) {
    loop {
        match listener.accept() {
            Ok(Some(pipe)) => {
                let conn = Arc::new(Conn {
                    pipe,
                    write: Mutex::new(()),
                });
                {
                    let mut g = sh.conns.lock().unwrap_or_else(PoisonError::into_inner);
                    g.retain(|w| w.strong_count() > 0);
                    g.push(Arc::downgrade(&conn));
                }
                let s = sh.clone();
                sh.track(std::thread::spawn(move || serve(&conn, &s)));
            }
            Ok(None) => break,
            Err(_) => {
                if sh.stopping.load(Ordering::SeqCst) {
                    break;
                }
                // Yeni örnek açılamadı (ör. bütün örnekler dolu): kısa bekleyip yeniden dene.
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Bir mesaj okur: önek `prefix_timeout` içinde, gövde `body_timeout` içinde gelmelidir.
/// `max`'tan uzun gövde okunmadan reddedilir (JSON ayrıştırması belleği büyütebilir).
fn read_message(
    pipe: &PipeConn,
    prefix_timeout: Option<Duration>,
    body_timeout: Duration,
    max: usize,
) -> Option<Message> {
    let mut prefix = [0u8; 4];
    pipe.read_exact_timeout(&mut prefix, prefix_timeout).ok()?;
    let n = decode_len(prefix).ok().filter(|n| *n <= max)?;
    let mut body = vec![0u8; n];
    pipe.read_exact_timeout(&mut body, Some(body_timeout))
        .ok()?;
    decode(&body).ok()
}

fn bad(detail: &str) -> AdnsError {
    AdnsError::new(AdnsErrorCode::IpcBadRequest)
        .with_source(std::io::Error::other(detail.to_string()))
}

/// `svc.hello {client_version, min_server}` → `{server_version, min_client, instance}`. İki yön
/// de denetlenir; `min_client` servisin kendi sürümüdür.
fn hello(params: &Value, cfg: &ServerConfig) -> Result<Value, AdnsError> {
    let field = |k: &str| params.get(k).and_then(Value::as_str).ok_or_else(|| bad(k));
    let client_version = field("client_version")?;
    let min_server = field("min_server")?;
    version_compatible(&cfg.server_version, min_server)?;
    version_compatible(client_version, &cfg.server_version)?;
    Ok(json!({
        "server_version": cfg.server_version,
        "min_client": cfg.server_version,
        "instance": cfg.instance,
    }))
}

fn serve(conn: &Arc<Conn>, sh: &Arc<Shared>) {
    let t = sh.cfg.io_timeout;
    // El sıkışma: ilk çerçeve süresinde gelmeli. Kimlik ancak bir okuma sonrası alınabilir.
    let Some(Message::Request(first)) = read_message(&conn.pipe, Some(t), t, MAX_HELLO) else {
        conn.pipe.close();
        return;
    };
    let Ok(caller) = conn.pipe.caller().map(Caller::from) else {
        conn.pipe.close();
        return;
    };
    let reply = if first.method == Method::Hello.name() {
        hello(&first.params, &sh.cfg)
    } else {
        Err(bad("ilk istek svc.hello olmalı"))
    };
    let ok = reply.is_ok();
    let resp = match reply {
        Ok(v) => Response::ok(&first.id, v),
        Err(e) => Response::err(&first.id, &e),
    };
    if conn.send(&Message::Response(resp), t).is_err() {
        conn.pipe.close();
        return;
    }
    if !ok {
        linger(conn, t);
        return;
    }
    let mut subscription: Option<u64> = None;
    while let Some(Message::Request(req)) = read_message(&conn.pipe, None, t, MAX_REQUEST) {
        let resp = handle(&req, &caller, conn, sh, &mut subscription);
        if conn.send(&Message::Response(resp), t).is_err() {
            break;
        }
    }
    if let Some(id) = subscription {
        sh.bus.unsubscribe(id);
    }
    conn.pipe.close();
}

fn handle(
    req: &Request,
    caller: &Caller,
    conn: &Arc<Conn>,
    sh: &Arc<Shared>,
    subscription: &mut Option<u64>,
) -> Response {
    let method = match Method::parse(&req.method) {
        Ok(m) => m,
        Err(e) => return Response::err(&req.id, &e),
    };
    if let Err(e) = authorize(method.policy(), caller, &sh.cfg.install_root) {
        return Response::err(&req.id, &e);
    }
    let result = match method {
        Method::Hello => hello(&req.params, &sh.cfg),
        Method::Ping => Ok(json!({"pong": true})),
        Method::Subscribe => {
            if subscription.is_none() {
                let (id, rx) = sh.bus.subscribe(caller.session_id);
                *subscription = Some(id);
                let c = conn.clone();
                let t = sh.cfg.io_timeout;
                sh.track(std::thread::spawn(move || write_events(&c, &rx, t)));
            }
            Ok(json!({"subscribed": true}))
        }
        other => sh.handler.call(caller, other, &req.params),
    };
    match result {
        Ok(v) => Response::ok(&req.id, v),
        Err(e) => Response::err(&req.id, &e),
    }
}

/// Hata yanıtından sonra bağlantıyı kapatmadan önce istemcinin kapatmasını bekler (en çok
/// `timeout`). `DisconnectNamedPipe` okunmamış veriyi siler; hemen kesilseydi istemci hata kodunu
/// göremezdi.
fn linger(conn: &Conn, timeout: Duration) {
    let mut b = [0u8; 1];
    let _ = conn.pipe.read_exact_timeout(&mut b, Some(timeout));
    conn.pipe.close();
}

/// Abonenin yazıcısı: yazma zaman aşımında (okumayan istemci) bağlantıyı kapatır.
fn write_events(conn: &Conn, rx: &Receiver<Event>, timeout: Duration) {
    for ev in rx {
        if conn.send(&Message::Event(ev), timeout).is_err() {
            conn.pipe.close();
            break;
        }
    }
}
