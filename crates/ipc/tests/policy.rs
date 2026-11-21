// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Yöntemler, çağıran yetkisi (K10; T-IPC-001 saf) ve sürüm uyumu (K19; T-IPC-004 saf).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use alterdns_errors::AdnsErrorCode;
use alterdns_ipc::{Caller, CallerPolicy, Method, authorize, version_compatible};

/// Test bitince (panikte de) silinen geçici dizin.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("alterdns-ipcpol-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

fn caller(interactive: bool, elevated: bool, image: Option<&Path>) -> Caller {
    Caller {
        sid: "S-1-5-21-1-2-3-1001".into(),
        session_id: 1,
        elevated,
        interactive,
        image_path: image.map(Path::to_path_buf),
    }
}

fn denied(r: Result<(), alterdns_errors::AdnsError>) -> bool {
    r.is_err_and(|e| e.code() == AdnsErrorCode::IpcUnauthorized)
}

#[test]
fn methods_have_names_and_classes() {
    for (name, m, p) in [
        ("svc.hello", Method::Hello, CallerPolicy::Open),
        ("svc.ping", Method::Ping, CallerPolicy::Open),
        ("svc.status", Method::Status, CallerPolicy::InteractiveUser),
        (
            "events.subscribe",
            Method::Subscribe,
            CallerPolicy::InteractiveUser,
        ),
        (
            "svc.reload_config",
            Method::ReloadConfig,
            CallerPolicy::Elevated,
        ),
    ] {
        assert_eq!(Method::parse(name).unwrap(), m);
        assert_eq!(m.name(), name);
        assert_eq!(m.policy(), p);
    }
}

#[test]
fn unknown_method_is_bad_request() {
    for name in ["svc.yok", "", "SVC.PING", "svc.ping "] {
        let e = Method::parse(name).unwrap_err();
        assert_eq!(e.code(), AdnsErrorCode::IpcBadRequest, "{name:?}");
        assert_eq!(e.code().as_str(), "ADNS-IPC-002");
    }
}

#[test]
fn authorize_open_always() {
    let root = Path::new(r"C:\Program Files\AlterDNS");
    assert!(authorize(CallerPolicy::Open, &caller(false, false, None), root).is_ok());
}

#[test]
fn authorize_interactive_requires_interactive_or_admin() {
    let root = Path::new(r"C:\Program Files\AlterDNS");
    let p = CallerPolicy::InteractiveUser;
    assert!(authorize(p, &caller(true, false, None), root).is_ok());
    assert!(authorize(p, &caller(false, true, None), root).is_ok());
    assert!(denied(authorize(p, &caller(false, false, None), root)));
}

#[test]
fn authorize_elevated_requires_admin_and_install_dir() {
    let d = temp("elev");
    let root = d.0.join("AlterDNS");
    let ver = root.join("0.1.0");
    std::fs::create_dir_all(&ver).unwrap();
    let ours = ver.join("alterdns-ui.exe");
    std::fs::write(&ours, b"").unwrap();
    let other = ver.join("baska.exe");
    std::fs::write(&other, b"").unwrap();
    let deep = ver.join("alt");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("alterdns-ui.exe"), b"").unwrap();
    let p = CallerPolicy::Elevated;

    // Olumlu yol: yönetici + kurulum dizinindeki AlterDNS ikilisi.
    assert!(authorize(p, &caller(true, true, Some(&ours)), &root).is_ok());
    // T-IPC-001: yönetici ama kurulum dizini dışından (test ikilisi).
    let me = std::env::current_exe().unwrap();
    assert!(denied(authorize(p, &caller(true, true, Some(&me)), &root)));
    // Kurulum dizinindeki ikili ama yönetici değil.
    assert!(denied(authorize(
        p,
        &caller(true, false, Some(&ours)),
        &root
    )));
    // Başka ad, daha derin dizin, imaj yolu yok, kök yok.
    assert!(denied(authorize(
        p,
        &caller(true, true, Some(&other)),
        &root
    )));
    assert!(denied(authorize(
        p,
        &caller(true, true, Some(&deep.join("alterdns-ui.exe"))),
        &root
    )));
    assert!(denied(authorize(p, &caller(true, true, None), &root)));
    assert!(denied(authorize(
        p,
        &caller(true, true, Some(&ours)),
        &d.0.join("yok")
    )));
}

#[test]
fn version_rules() {
    assert!(version_compatible("0.1.0", "0.0.9").is_ok());
    assert!(version_compatible("0.1.0", "0.1.0").is_ok());
    assert!(version_compatible("1.0.0-alpha.1", "1.0.0").is_ok());
    for (have, min) in [
        ("0.1.0", "0.2.0"),
        ("x", "0.1.0"),
        ("0.1", "0.1.0"),
        ("0.1.0", ""),
    ] {
        assert_eq!(
            version_compatible(have, min).unwrap_err().code(),
            AdnsErrorCode::IpcVersionMismatch,
            "{have} {min}"
        );
    }
}
