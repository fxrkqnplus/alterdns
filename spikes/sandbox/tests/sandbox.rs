// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! T-SPK-003: işçinin dosya sistemine, ağa ve alt işleme erişemediğini kanıtlar.
//!
//! Testler aynı AppContainer profilini ve exe ACL'sini paylaşır; sırayla koşturulur:
//! `cargo test --manifest-path spikes/sandbox/Cargo.toml -- --test-threads=1`

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn input_file() -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("alterdns-sandbox-input-{}.bin", std::process::id()));
    std::fs::write(&path, b"AlterDNS sandbox spike girdi dosyasi").unwrap();
    path
}

fn run(probe: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_sandbox-spike"))
        .arg("broker")
        .arg("--file")
        .arg(input_file())
        .args(["--probe", probe])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "broker başarısız: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Olumsuz sonucun anlamlı olması için işçi gerçekten çalışmış ve kendi sonucunu yazmış olmalı.
fn run_and_assert_ran(probe: &str) -> Value {
    let r = run(probe);
    assert!(r["detail"]["no_output"].is_null(), "işçi çalışmadı: {r}");
    assert_eq!(r["exit_code"], 0, "işçi hatayla çıktı: {r}");
    assert_eq!(r["probe"], probe, "{r}");
    r
}

#[test]
fn worker_starts_and_reports_in_sandbox() {
    let r = run_and_assert_ran("noop");
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn worker_runs_in_appcontainer_at_low_il() {
    let r = run_and_assert_ran("identity");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["detail"]["is_appcontainer"], true, "{r}");
    assert_eq!(r["detail"]["integrity_rid"], 0x1000, "{r}");
}

#[test]
fn worker_reads_inherited_handle() {
    let r = run_and_assert_ran("read-handle");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["detail"]["sha256"], r["expected_sha256"], "{r}");
}

#[test]
fn worker_cannot_open_user_file() {
    let r = run_and_assert_ran("open-user-file");
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], 5, "{r}");
}

#[test]
fn worker_cannot_write_temp() {
    let r = run_and_assert_ran("write-temp");
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], 5, "{r}");
}

#[test]
fn worker_own_temp_is_private_appcontainer_storage() {
    // AppContainer içinde temp_dir() kapsayıcının kendi deposunu gösterir ve yazılabilir.
    // Kullanıcının temp dizini değildir (bkz. worker_cannot_write_temp). Faz 3 notu.
    let r = run_and_assert_ran("write-own-temp");
    assert_eq!(r["ok"], true, "{r}");
    let path = r["detail"]["path"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        path.contains(r"\packages\") && path.contains(r"\ac\temp"),
        "{r}"
    );
}

#[test]
fn worker_cannot_connect_loopback() {
    let r = run_and_assert_ran("connect-loopback");
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn worker_cannot_connect_external() {
    let r = run_and_assert_ran("connect-external");
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn worker_cannot_spawn_child() {
    let r = run_and_assert_ran("spawn-child");
    assert_eq!(r["ok"], false, "{r}");
}

#[test]
fn worker_stopped_over_memory_limit() {
    // 300 MiB ayırma, 256 MiB iş nesnesi sınırına takılmalı: işçi çalışır ve ayırmanın
    // başarısız olduğunu kendisi bildirir.
    let r = run_and_assert_ran("alloc");
    assert_eq!(r["ok"], false, "{r}");
    assert!(r["detail"]["message"].is_string(), "{r}");
}
