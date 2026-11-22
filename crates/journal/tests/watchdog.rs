// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bekçi protokolü ve geri alma, süreç içinde (borudan girdi; T-JRN-004'ün süreç içi sürümü).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alterdns_journal::{
    Kind, LeaseMsg, Lifetime, Snapshot, SystemOps, Targets, Txn, WatchdogHandle, WatchdogOutcome,
    WatchdogReport, WatchdogStatus, parse_line, report_path, run_watchdog,
};

mod common;

#[derive(Default)]
struct FakeOps {
    values: RefCell<BTreeMap<String, Snapshot>>,
}

impl SystemOps for FakeOps {
    fn read(&self, _kind: Kind, target: &str) -> Result<Snapshot, String> {
        Ok(self
            .values
            .borrow()
            .get(target)
            .cloned()
            .unwrap_or(Snapshot::Absent))
    }
    fn write(&self, _kind: Kind, target: &str, value: &Snapshot) -> Result<(), String> {
        self.values
            .borrow_mut()
            .insert(target.to_string(), value.clone());
        Ok(())
    }
}

const KEY: &str = r"HKLM\SOFTWARE\AlterDNS\Bekci\Deger";

fn targets() -> Targets {
    Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"))
}

/// Bir değer uygulanmış txn; `commit` isteğe bağlı. Txn düşer: ebeveyn öldü, kilit bırakıldı.
fn txn(dir: &Path, ops: &FakeOps, lifetime: Lifetime, commit: bool) -> String {
    let mut t = Txn::begin(dir, "test", lifetime, targets()).unwrap();
    t.apply(
        ops,
        Kind::RegistryValue,
        KEY,
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    let id = t.id().to_string();
    if commit {
        t.commit().unwrap();
    }
    id
}

fn report(dir: &Path, id: &str) -> WatchdogReport {
    serde_json::from_str(&std::fs::read_to_string(report_path(dir, id)).unwrap()).unwrap()
}

#[test]
fn done_means_clean_exit_and_no_revert() {
    let d = common::temp("wd", "done");
    let ops = FakeOps::default();
    let id = txn(&d, &ops, Lifetime::Session, false);
    let (reader, mut writer) = std::io::pipe().unwrap();
    writeln!(writer, "RENEW 30").unwrap();
    writeln!(writer, "DONE").unwrap();
    let out = run_watchdog(&d, &id, &ops, &targets(), reader).unwrap();
    assert_eq!(out, WatchdogOutcome::CleanExit);
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Present("yeni".into())
    );
    let r = report(&d, &id);
    assert_eq!(r.outcome, WatchdogOutcome::CleanExit);
    assert_eq!(r.status, WatchdogStatus::NotNeeded);
}

#[test]
fn closed_stdin_reverts() {
    let d = common::temp("wd", "closed");
    let ops = FakeOps::default();
    let id = txn(&d, &ops, Lifetime::Session, false);
    let (reader, writer) = std::io::pipe().unwrap();
    drop(writer);
    let out = run_watchdog(&d, &id, &ops, &targets(), reader).unwrap();
    assert_eq!(out, WatchdogOutcome::ParentGone);
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Absent
    );
    let r = report(&d, &id);
    assert_eq!(r.outcome, WatchdogOutcome::ParentGone);
    assert_eq!(
        r.status,
        WatchdogStatus::Reverted {
            reverted: vec![1],
            conflicts: vec![],
            failed: vec![]
        }
    );
}

#[test]
fn lease_expiry_reverts() {
    let d = common::temp("wd", "lease");
    let ops = FakeOps::default();
    let id = txn(&d, &ops, Lifetime::Session, false);
    let (reader, mut writer) = std::io::pipe().unwrap();
    writeln!(writer, "RENEW 1").unwrap();
    let start = Instant::now();
    // Yazıcı açık kalır: ebeveyn yaşıyor ama kirayı yenilemiyor.
    let out = run_watchdog(&d, &id, &ops, &targets(), reader).unwrap();
    assert_eq!(out, WatchdogOutcome::DeadlineExpired);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Absent
    );
    drop(writer);
}

#[test]
fn committed_persistent_txn_is_kept() {
    let d = common::temp("wd", "kept");
    let ops = FakeOps::default();
    let id = txn(&d, &ops, Lifetime::Persistent, true);
    let (reader, writer) = std::io::pipe().unwrap();
    drop(writer);
    assert_eq!(
        run_watchdog(&d, &id, &ops, &targets(), reader).unwrap(),
        WatchdogOutcome::ParentGone
    );
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Present("yeni".into())
    );
    assert_eq!(report(&d, &id).status, WatchdogStatus::Kept);
}

#[test]
fn parse_line_rejects_bad_input() {
    assert_eq!(parse_line("RENEW 1"), Some(LeaseMsg::Renew(1)));
    assert_eq!(parse_line("RENEW 60"), Some(LeaseMsg::Renew(60)));
    assert_eq!(parse_line("DONE"), Some(LeaseMsg::Done));
    assert_eq!(parse_line("DONE\r"), Some(LeaseMsg::Done));
    for bad in [
        "RENEW 0",
        "RENEW 61",
        "RENEW x",
        "RENEW -1",
        "RENEW 5 6",
        "ENGINE 4",
        "ENGINE 1234 638000000000000000",
        "renew 5",
        "DONE now",
        "",
    ] {
        assert_eq!(parse_line(bad), None, "{bad:?}");
    }
}

#[test]
fn invalid_txn_id_is_rejected_without_report() {
    let d = common::temp("wd", "badid");
    let ops = FakeOps::default();
    for id in ["../x", "Büyük", "", r"a\b"] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(writer);
        assert!(
            run_watchdog(&d, id, &ops, &targets(), reader).is_err(),
            "{id}"
        );
    }
    assert_eq!(std::fs::read_dir(&*d).unwrap().count(), 0);
}

#[test]
fn spawn_requires_existing_exe_and_valid_id() {
    let d = common::temp("wd", "spawn");
    let missing = d.join("alterdns-maint.exe");
    let e = WatchdogHandle::spawn_with(&missing, "txn-1", &[])
        .err()
        .unwrap();
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    let e = WatchdogHandle::spawn_with(&missing, "../x", &[])
        .err()
        .unwrap();
    assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
}
