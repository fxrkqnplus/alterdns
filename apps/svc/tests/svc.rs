// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Servis kütüphanesi süreç içinde: açılış denetimi (K20), kurtarma, IPC yöntemleri.

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use alterdns_core::Paths;
use alterdns_errors::AdnsErrorCode;
use alterdns_ipc::{Client, ConnectOptions};
use alterdns_journal::{Kind, Lifetime, RealOps, Snapshot, Targets, Txn};
use alterdns_svc::{SvcConfig, start};
use serde_json::json;

/// Test bitince (panikte de) silinen geçici dizin.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("alterdns-svc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

fn paths(d: &TempDir) -> Paths {
    Paths::rooted(
        d.0.join("pd").join("AlterDNS"),
        d.0.join("lad").join("AlterDNS"),
        d.0.join("pf").join("AlterDNS"),
    )
}

fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\AlterDNS.Test.{}.svc-{name}", std::process::id())
}

fn connect(name: &str) -> Client {
    let p = pipe(name);
    Client::connect(&ConnectOptions::product(&p).trust_own_pipes())
        .unwrap()
        .0
}

#[test]
fn start_status_stop() {
    let d = temp("status");
    let running = start(SvcConfig::test(paths(&d), &pipe("status"), false)).unwrap();
    let mut c = connect("status");
    let s = c.call("svc.status", json!({})).unwrap();
    assert_eq!(s["version"], alterdns_core::version::VERSION);
    assert!(s["uptime_s"].is_u64());
    for m in ["connection", "file", "browser"] {
        assert_eq!(s["modules"][m], "not_installed", "{s}");
    }
    running.stop();
    assert_eq!(
        c.call("svc.ping", json!({})).unwrap_err().code(),
        AdnsErrorCode::IpcConnect
    );
}

/// K20: veri dizini korumalı değilse servis başlamaz (`ADNS-SVC-001`). Geçici dizin kullanıcınındır.
#[test]
fn unprotected_data_dir_refuses_start() {
    let d = temp("unprotected");
    let p = paths(&d);
    std::fs::create_dir_all(p.journal_dir()).unwrap();
    std::fs::create_dir_all(p.machine_logs()).unwrap();
    let e = start(SvcConfig::test(p, &pipe("unprotected"), true))
        .err()
        .unwrap();
    assert_eq!(e.code(), AdnsErrorCode::SvcStartFailed);
    assert_eq!(e.code().as_str(), "ADNS-SVC-001");
}

#[test]
fn startup_recovery_runs_before_server() {
    let d = temp("recover");
    let p = paths(&d);
    std::fs::create_dir_all(p.journal_dir()).unwrap();
    let file = d.0.join("deger.txt");
    std::fs::write(&file, "before").unwrap();
    let mut txn = Txn::begin(
        &p.journal_dir(),
        "test",
        Lifetime::Session,
        Targets::product(p.program_files.clone()),
    )
    .unwrap();
    txn.apply(
        &RealOps,
        Kind::FileValue,
        &file.to_string_lossy(),
        Snapshot::Present("after".into()),
    )
    .unwrap();
    // Servis çöktü: txn dosyası bırakıldı.
    drop(txn);
    let running = start(SvcConfig::test(p, &pipe("recover"), false)).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
    running.stop();
}

#[test]
fn reload_config_is_elevated_only() {
    let d = temp("reload");
    let running = start(SvcConfig::test(paths(&d), &pipe("reload"), false)).unwrap();
    let mut c = connect("reload");
    let e = c.call("svc.reload_config", json!({})).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcUnauthorized);
    running.stop();
}

/// Aynı adla ikinci servis örneği başlamaz.
#[test]
fn second_instance_is_pipe_in_use() {
    let d = temp("twice");
    let running = start(SvcConfig::test(paths(&d), &pipe("twice"), false)).unwrap();
    let d2 = temp("twice2");
    let e = start(SvcConfig::test(paths(&d2), &pipe("twice"), false))
        .err()
        .unwrap();
    assert_eq!(e.code(), AdnsErrorCode::IpcPipeInUse);
    running.stop();
}
