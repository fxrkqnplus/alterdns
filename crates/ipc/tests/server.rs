// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sunucu ve istemci, gerçek pipe'larla (T-IPC-001…004 canlı; K21, K22).

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{
    Caller, Client, ConnectOptions, Handler, MAX_MESSAGE, Method, Server, ServerConfig, TEST_SDDL,
    VERSION, routes_to,
};
use alterdns_winsys::{PipeListener, PipeOwner, connect_pipe, current_session_id};
use serde_json::{Value, json};

struct Fake;

impl Handler for Fake {
    fn call(&self, _caller: &Caller, method: Method, _params: &Value) -> Result<Value, AdnsError> {
        match method {
            Method::Status => Ok(json!({"version": "test"})),
            _ => Ok(json!({})),
        }
    }
}

fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\AlterDNS.Test.{}.ipc-{name}", std::process::id())
}

fn config(name: &str) -> ServerConfig {
    let mut c = ServerConfig::product(PathBuf::from(r"C:\Program Files\AlterDNS"), VERSION, 42)
        .test_access();
    c.pipe_name = pipe(name);
    c
}

fn start(cfg: ServerConfig) -> Server {
    Server::start(cfg, Arc::new(Fake)).unwrap()
}

fn connect(name: &str) -> Client {
    Client::connect(&ConnectOptions::product(&pipe(name)).trust_own_pipes())
        .unwrap()
        .0
}

#[test]
fn hello_ping_status() {
    let _s = start(config("basic"));
    let (mut c, hello) =
        Client::connect(&ConnectOptions::product(&pipe("basic")).trust_own_pipes()).unwrap();
    assert_eq!(hello.server_version, VERSION);
    assert_eq!(hello.instance, 42);
    assert_eq!(
        c.call("svc.ping", json!({})).unwrap(),
        json!({"pong": true})
    );
    assert_eq!(
        c.call("svc.status", json!({})).unwrap(),
        json!({"version": "test"})
    );
}

#[test]
fn unknown_method_returns_ipc_002_and_keeps_connection() {
    let _s = start(config("unknown"));
    let mut c = connect("unknown");
    let e = c.call("svc.yok", json!({})).unwrap_err();
    assert_eq!(e.code().as_str(), "ADNS-IPC-002");
    assert!(c.call("svc.ping", json!({})).is_ok());
}

/// T-IPC-001: kurulum dizini dışındaki ikili (test) ayrıcalıklı yöntemi çağıramaz; yönetici
/// belirteci olsa bile (CI).
#[test]
fn privileged_method_from_test_binary_is_unauthorized() {
    let _s = start(config("priv"));
    let mut c = connect("priv");
    let e = c.call("svc.reload_config", json!({})).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcUnauthorized);
}

/// T-IPC-002: Aynı adla önceden açılmış pipe varsa sunucu başlamaz.
#[test]
fn pipe_squatting_refuses_start() {
    let name = pipe("squat");
    let _squatter = PipeListener::bind(&name, TEST_SDDL, 1).unwrap();
    let mut cfg = config("squat");
    cfg.pipe_name = name;
    let e = Server::start(cfg, Arc::new(Fake)).err().unwrap();
    assert_eq!(e.code(), AdnsErrorCode::IpcPipeInUse);
}

fn raw(name: &str) -> alterdns_winsys::PipeConn {
    connect_pipe(
        &pipe(name),
        PipeOwner::ProcessDefaultOwner,
        Duration::from_secs(5),
    )
    .unwrap()
}

fn closed_by_server(conn: &alterdns_winsys::PipeConn) -> bool {
    let mut b = [0u8; 1];
    conn.read_exact_timeout(&mut b, Some(Duration::from_secs(5)))
        .is_err_and(|e| e.kind() != std::io::ErrorKind::TimedOut)
}

/// T-IPC-003: 1 MiB üstü uzunluk ve bozuk JSON bağlantıyı kapatır, sunucu çalışmaya devam eder.
#[test]
fn garbage_does_not_kill_server() {
    let _s = start(config("garbage"));
    let big = raw("garbage");
    let len = u32::try_from(MAX_MESSAGE + 1).unwrap().to_le_bytes();
    big.write_all_timeout(&len, Duration::from_secs(5)).unwrap();
    assert!(closed_by_server(&big));
    let junk = raw("garbage");
    let body = b"{bozuk";
    let mut frame = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
    frame.extend_from_slice(body);
    junk.write_all_timeout(&frame, Duration::from_secs(5))
        .unwrap();
    assert!(closed_by_server(&junk));
    let mut c = connect("garbage");
    assert!(c.call("svc.ping", json!({})).is_ok());
}

/// Kimlik doğrulanmadan önceki ilk mesaj 4 KiB, sonraki istekler 64 KiB ile sınırlı: büyük bir
/// JSON gövdesi SYSTEM işleminde belleği büyütemez.
#[test]
fn oversized_hello_and_request_close_connection() {
    let _s = start(config("caps"));
    let big_hello = raw("caps");
    let body = format!(
        r#"{{"v":1,"id":"1","method":"svc.hello","params":{{"pad":"{}"}}}}"#,
        "x".repeat(alterdns_ipc::MAX_HELLO)
    );
    let mut frame = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
    frame.extend_from_slice(body.as_bytes());
    big_hello
        .write_all_timeout(&frame, Duration::from_secs(5))
        .unwrap_or(());
    assert!(closed_by_server(&big_hello));
    let mut c = connect("caps");
    let pad = "x".repeat(alterdns_ipc::MAX_REQUEST);
    let e = c.call("svc.ping", json!({ "pad": pad })).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcConnect);
    assert!(connect("caps").call("svc.ping", json!({})).is_ok());
}

/// T-IPC-004: sürüm uyuşmazlığı iki yönde de `IpcVersionMismatch`.
#[test]
fn version_mismatch() {
    // İstemci daha yeni bir servis istiyor.
    let _s = start(config("vnew"));
    let name_new = pipe("vnew");
    let mut wants_newer = ConnectOptions::product(&name_new).trust_own_pipes();
    wants_newer.min_server = "99.0.0";
    let e = Client::connect(&wants_newer).err().unwrap();
    assert_eq!(e.code(), AdnsErrorCode::IpcVersionMismatch);
    // Servis istemciden yeni (min_client = servis sürümü).
    let mut cfg = config("vold");
    cfg.server_version = "99.0.0".into();
    let _s2 = start(cfg);
    let name_old = pipe("vold");
    let mut older = ConnectOptions::product(&name_old).trust_own_pipes();
    older.min_server = "0.0.0";
    let e = Client::connect(&older).err().unwrap();
    assert_eq!(e.code(), AdnsErrorCode::IpcVersionMismatch);
}

#[test]
fn idle_client_is_dropped() {
    let mut cfg = config("idle");
    cfg.io_timeout = Duration::from_millis(300);
    let _s = start(cfg);
    let silent = raw("idle");
    let start = Instant::now();
    assert!(closed_by_server(&silent));
    assert!(start.elapsed() < Duration::from_secs(3));
}

/// K21: istemci, sahibi SYSTEM olmayan pipe'a bağlanmaz.
#[test]
fn fake_server_owned_by_user_is_rejected() {
    let _s = start(config("fake"));
    let e = Client::connect(&ConnectOptions::product(&pipe("fake")))
        .err()
        .unwrap();
    assert_eq!(e.code(), AdnsErrorCode::IpcConnect);
}

#[test]
fn client_survives_server_restart() {
    let s = start(config("restart"));
    let mut c = connect("restart");
    s.stop();
    let e = c.call("svc.ping", json!({})).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcConnect);
    let mut cfg = config("restart");
    cfg.instance = 43;
    let _s2 = start(cfg);
    let (mut c2, hello) =
        Client::connect(&ConnectOptions::product(&pipe("restart")).trust_own_pipes()).unwrap();
    assert_eq!(hello.instance, 43);
    assert!(c2.call("svc.ping", json!({})).is_ok());
}

#[test]
fn events_are_routed_by_session() {
    assert!(routes_to(None, 2));
    assert!(routes_to(Some(2), 2));
    assert!(!routes_to(Some(1), 2));

    let s = start(config("events"));
    let mut c = connect("events");
    c.call("events.subscribe", json!({})).unwrap();
    let mine = current_session_id().unwrap();
    let bus = s.events();
    bus.publish("test.mine", json!({"n": 1}), Some(mine));
    let ev = c.next_event(Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(ev.event, "test.mine");
    bus.publish("test.other", json!({}), Some(mine + 1));
    assert!(c.next_event(Duration::from_millis(300)).unwrap().is_none());
    bus.publish("test.all", json!({}), None);
    assert_eq!(
        c.next_event(Duration::from_secs(5)).unwrap().unwrap().event,
        "test.all"
    );
    // Yanıt beklerken gelen olay kaybolmaz.
    bus.publish("test.during", json!({}), None);
    std::thread::sleep(Duration::from_millis(100));
    assert!(c.call("svc.ping", json!({})).is_ok());
    assert_eq!(
        c.next_event(Duration::from_secs(5)).unwrap().unwrap().event,
        "test.during"
    );
}

/// Olayları okumayan abone, yazma zaman aşımında düşürülür; sunucu takılmaz.
#[test]
fn slow_subscriber_is_dropped() {
    let mut cfg = config("slow");
    cfg.io_timeout = Duration::from_millis(500);
    let s = start(cfg);
    let mut c = connect("slow");
    c.call("events.subscribe", json!({})).unwrap();
    let big = "x".repeat(16 * 1024);
    for _ in 0..64 {
        s.events().publish("test.big", json!({"d": big}), None);
    }
    std::thread::sleep(Duration::from_secs(2));
    let mut other = connect("slow");
    assert!(other.call("svc.ping", json!({})).is_ok());
    // Abonenin bağlantısı kapanmıştır: kuyruktaki olaylar okunduktan sonra hata gelir.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match c.next_event(Duration::from_millis(200)) {
            Ok(Some(_)) => {}
            Ok(None) => panic!("bağlantı kapanmadı"),
            Err(e) => {
                assert_eq!(e.code(), AdnsErrorCode::IpcConnect);
                break;
            }
        }
        assert!(Instant::now() < deadline);
    }
}
