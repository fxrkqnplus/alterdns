// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-maint` ayrı işlemde bekçi olarak (T-JRN-004); `test-ops` ister.

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const EXE: &str = env!("CARGO_BIN_EXE_alterdns-maint");

/// Test bitince (panikte de) silinen geçici dizin.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("alterdns-maint-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}

#[test]
fn t_jrn_004_parent_killed_then_watchdog_reverts() {
    use alterdns_journal::{
        Kind, Lifetime, RealOps, Snapshot, Targets, Txn, WatchdogHandle, WatchdogOutcome,
        WatchdogReport, WatchdogStatus, report_path,
    };

    let d = temp("t004");
    let journal = d.0.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let file = d.0.join("deger.txt");
    std::fs::write(&file, "before").unwrap();
    let targets = Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"));
    let mut txn = Txn::begin(&journal, "test", Lifetime::Session, targets).unwrap();
    txn.apply(
        &RealOps,
        Kind::FileValue,
        &file.to_string_lossy(),
        Snapshot::Present("after".into()),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "after");
    let id = txn.id().to_string();
    let mut wd = WatchdogHandle::spawn_with(
        Path::new(EXE),
        &id,
        &[("ALTERDNS_TEST_JOURNAL_DIR", journal.as_os_str())],
    )
    .unwrap();
    wd.renew(60).unwrap();
    // Ebeveynin ölümü: txn dosyası bırakılır ve bekçinin stdin'i kapanır.
    drop(txn);
    let mut child = wd.close_input();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "bekçi 60 sn'de bitmedi"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
    let report: WatchdogReport =
        serde_json::from_str(&std::fs::read_to_string(report_path(&journal, &id)).unwrap())
            .unwrap();
    assert_eq!(report.outcome, WatchdogOutcome::ParentGone);
    assert_eq!(
        report.status,
        WatchdogStatus::Reverted {
            reverted: vec![1],
            conflicts: vec![],
            failed: vec![]
        }
    );
}

#[test]
fn done_leaves_change_in_place() {
    use alterdns_journal::{
        Kind, Lifetime, RealOps, Snapshot, Targets, Txn, WatchdogHandle, WatchdogOutcome,
        WatchdogReport, report_path,
    };

    let d = temp("done");
    let journal = d.0.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let file = d.0.join("deger.txt");
    let targets = Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"));
    let mut txn = Txn::begin(&journal, "test", Lifetime::Session, targets).unwrap();
    txn.apply(
        &RealOps,
        Kind::FileValue,
        &file.to_string_lossy(),
        Snapshot::Present("after".into()),
    )
    .unwrap();
    let id = txn.id().to_string();
    let wd = WatchdogHandle::spawn_with(
        Path::new(EXE),
        &id,
        &[("ALTERDNS_TEST_JOURNAL_DIR", journal.as_os_str())],
    )
    .unwrap();
    assert!(wd.done().unwrap().success());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "after");
    let report: WatchdogReport =
        serde_json::from_str(&std::fs::read_to_string(report_path(&journal, &id)).unwrap())
            .unwrap();
    assert_eq!(report.outcome, WatchdogOutcome::CleanExit);
    txn.rollback(&RealOps).unwrap();
    assert!(!file.exists());
}
