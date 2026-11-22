// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kurulu servise karşı (yalnızca `cargo xtask dev-install` sonrasında, normal kullanıcı
//! terminalinden): sahibi SYSTEM olan pipe, durum, yetki. Servisi değiştirmez.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_errors::AdnsErrorCode;
use alterdns_ipc::{Client, ConnectOptions, PIPE_NAME, VERSION};
use serde_json::json;

#[test]
#[ignore = "dev-install sonrası, onaylı koşu"]
fn live_service_answers_and_enforces_policy() {
    // Ürün seçenekleri: pipe sahibi SYSTEM değilse bağlanılmaz (K21).
    let (mut c, hello) = Client::connect(&ConnectOptions::product(PIPE_NAME)).unwrap();
    assert_eq!(hello.server_version, VERSION);
    assert_eq!(
        c.call("svc.ping", json!({})).unwrap(),
        json!({"pong": true})
    );
    let status = c.call("svc.status", json!({})).unwrap();
    assert_eq!(status["version"], VERSION);
    // Test ikilisi kurulum dizininde değil: ayrıcalıklı yöntem reddedilir (T-IPC-001, canlı).
    let e = c.call("svc.reload_config", json!({})).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcUnauthorized);
}
