// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Named pipe: gidiş-dönüş, çağıran kimliği, sunucu sahibi, zaman aşımı, durdurma ve DACL.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::ErrorKind;
use std::time::{Duration, Instant};

use alterdns_winsys::{PipeListener, PipeOwner, connect_pipe, current_session_id, is_elevated};

/// Ürün DACL'i (etkileşimli kullanıcı okur ve yazar, örnek açamaz).
const PRODUCT_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)";
/// Testler: sahibi (bu işlem) kendi örneklerini de açabilsin.
const TEST_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)(A;;GA;;;OW)";
const T: Duration = Duration::from_secs(5);

fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\AlterDNS.Test.{}.{name}", std::process::id())
}

#[test]
fn roundtrip_and_caller_identity() {
    let name = pipe("rt");
    let listener = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let server = std::thread::spawn(move || {
        let conn = listener.accept().unwrap().unwrap();
        let mut buf = [0u8; 4];
        conn.read_exact_timeout(&mut buf, Some(T)).unwrap();
        // Kimlik ancak pipe'tan veri okunduktan sonra alınabilir.
        let who = conn.caller().unwrap();
        conn.write_all_timeout(b"pong", T).unwrap();
        (buf, who)
    });
    let client = connect_pipe(&name, PipeOwner::ProcessDefaultOwner, T).unwrap();
    client.write_all_timeout(b"ping", T).unwrap();
    let mut reply = [0u8; 4];
    client.read_exact_timeout(&mut reply, Some(T)).unwrap();
    let (got, who) = server.join().unwrap();
    assert_eq!(&got, b"ping");
    assert_eq!(&reply, b"pong");
    assert!(who.sid.starts_with("S-1-5-"), "{who:?}");
    assert_eq!(who.session_id, current_session_id().unwrap());
    assert_eq!(who.elevated, is_elevated().unwrap());
    let image = std::fs::canonicalize(who.image_path.unwrap()).unwrap();
    assert_eq!(
        image,
        std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap()
    );
}

#[test]
fn second_bind_is_addr_in_use() {
    let name = pipe("twice");
    let _first = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let e = PipeListener::bind(&name, TEST_SDDL, 4).err().unwrap();
    assert_eq!(e.kind(), ErrorKind::AddrInUse, "{e}");
}

#[test]
fn system_owner_expected_rejects_user_pipe() {
    let name = pipe("owner");
    let _listener = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let e = connect_pipe(&name, PipeOwner::System, T).err().unwrap();
    assert_eq!(e.kind(), ErrorKind::PermissionDenied, "{e}");
}

#[test]
fn missing_pipe_is_not_found() {
    let e = connect_pipe(&pipe("yok"), PipeOwner::System, T)
        .err()
        .unwrap();
    assert_eq!(e.kind(), ErrorKind::NotFound, "{e}");
}

#[test]
fn read_times_out() {
    let name = pipe("timeout");
    let listener = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let _client = connect_pipe(&name, PipeOwner::ProcessDefaultOwner, T).unwrap();
    let conn = listener.accept().unwrap().unwrap();
    let start = Instant::now();
    let mut buf = [0u8; 4];
    let e = conn
        .read_exact_timeout(&mut buf, Some(Duration::from_millis(200)))
        .err()
        .unwrap();
    assert_eq!(e.kind(), ErrorKind::TimedOut, "{e}");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn closed_client_reads_as_eof() {
    let name = pipe("eof");
    let listener = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let client = connect_pipe(&name, PipeOwner::ProcessDefaultOwner, T).unwrap();
    let conn = listener.accept().unwrap().unwrap();
    drop(client);
    let mut buf = [0u8; 4];
    let e = conn.read_exact_timeout(&mut buf, Some(T)).err().unwrap();
    assert_eq!(e.kind(), ErrorKind::UnexpectedEof, "{e}");
}

#[test]
fn stop_ends_accept() {
    let name = pipe("stop");
    let listener = PipeListener::bind(&name, TEST_SDDL, 4).unwrap();
    let stopper = listener.stopper();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        stopper.stop();
    });
    assert!(listener.accept().unwrap().is_none());
}

/// Etkileşimli kullanıcı ürün pipe'ının yeni bir örneğini açamaz (sahte sunucu örneği). CI
/// yönetici olarak koştuğundan orada yöneticinin hakkı geçerlidir ve test atlanır.
#[test]
fn product_dacl_denies_new_instances_to_users() {
    if is_elevated().unwrap() {
        eprintln!("yükseltilmiş koşu: test atlandı");
        return;
    }
    let name = pipe("dacl");
    let listener = PipeListener::bind(&name, PRODUCT_SDDL, 4).unwrap();
    let _client = connect_pipe(&name, PipeOwner::ProcessDefaultOwner, T).unwrap();
    let _conn = listener.accept().unwrap().unwrap();
    let e = listener.accept().err().unwrap();
    assert_eq!(e.kind(), ErrorKind::PermissionDenied, "{e}");
}

/// Bağlı örnek varken ad boşa çıkmaz: dinleyen örnek kalmasa da başkası aynı adı alamaz.
#[test]
fn pipe_name_persists_while_connected() {
    let name = pipe("persist");
    let listener = PipeListener::bind(&name, TEST_SDDL, 1).unwrap();
    let _client = connect_pipe(&name, PipeOwner::ProcessDefaultOwner, T).unwrap();
    let _conn = listener.accept().unwrap().unwrap();
    let e = PipeListener::bind(&name, TEST_SDDL, 1).err().unwrap();
    assert_eq!(e.kind(), ErrorKind::AddrInUse, "{e}");
}
