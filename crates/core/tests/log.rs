// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Makine yapılandırması ve boyutu sınırlı, maskelenmiş log (AR §10).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::PathBuf;

use alterdns_core::{CappedWriter, LoadOutcome, MachineConfig, load_machine};

/// Test bitince (panikte de) silinen geçici dizin.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("alterdns-log-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

#[test]
fn corrupt_machine_config_defaults() {
    let d = temp("corrupt");
    let path = d.0.join("machine.toml");
    std::fs::write(&path, "bozuk = [").unwrap();
    let (cfg, outcome) = load_machine(&path).unwrap();
    assert_eq!(cfg, MachineConfig::default());
    assert_eq!(outcome, LoadOutcome::Defaulted { backup: None });
    // Servis dosyayı ezmez ve taşımaz.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "bozuk = [");
    assert_eq!(
        load_machine(&d.0.join("yok.toml")).unwrap().1,
        LoadOutcome::Defaulted { backup: None }
    );
    std::fs::write(&path, "schema = 1\n").unwrap();
    assert_eq!(load_machine(&path).unwrap().1, LoadOutcome::Loaded);
}

#[test]
fn newer_machine_schema_is_read_only() {
    let d = temp("newer");
    let path = d.0.join("machine.toml");
    std::fs::write(&path, "schema = 9\n[yeni]\nalan = 1\n").unwrap();
    assert_eq!(load_machine(&path).unwrap().1, LoadOutcome::NewerSchema);
    assert!(std::fs::read_to_string(&path).unwrap().contains("[yeni]"));
}

#[test]
fn capped_writer_rotates_and_bounds_total() {
    let d = temp("rotate");
    let mut w = CappedWriter::new(&d.0, "svc", 1024).unwrap();
    let line = format!("{}\n", "a".repeat(99));
    for _ in 0..100 {
        w.write_all(line.as_bytes()).unwrap();
    }
    let cur = std::fs::metadata(d.0.join("svc.log")).unwrap().len();
    let old = std::fs::metadata(d.0.join("svc.log.1")).unwrap().len();
    assert!(cur <= 1024 + 100, "{cur}");
    assert!(old <= 1024 + 100, "{old}");
    assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 2);
}

#[test]
fn capped_writer_redacts_user_paths() {
    let d = temp("redact");
    let mut w = CappedWriter::new(&d.0, "svc", 1024 * 1024).unwrap();
    // Örnek yol parçalı kurulur; depo taraması bu dosyayı yakalamasın.
    let line = ["dosya C:", "\\Users\\Çağrı Öz\\AppData\\x açılamadı\n"].concat();
    w.write_all(line.as_bytes()).unwrap();
    let text = std::fs::read_to_string(d.0.join("svc.log")).unwrap();
    assert!(!text.contains("Çağrı"), "{text}");
    assert!(text.contains("açılamadı"), "{text}");
}

#[test]
fn json_lines_through_tracing() {
    let d = temp("json");
    let w = CappedWriter::new(&d.0, "svc", 1024 * 1024).unwrap();
    let sub = alterdns_core::log_subscriber(w);
    tracing::subscriber::with_default(sub, || {
        tracing::info!(txn = "t-1", "kurtarma tamam");
        tracing::debug!("görünmemeli");
    });
    let text = std::fs::read_to_string(d.0.join("svc.log")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["level"], "INFO");
    assert_eq!(v["fields"]["message"], "kurtarma tamam");
    assert_eq!(v["fields"]["txn"], "t-1");
}
