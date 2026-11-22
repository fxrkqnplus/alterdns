// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Açılışta kurtarma: Persistent/Session, canlı txn, bozuk dosya.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alterdns_errors::AdnsErrorCode;
use alterdns_journal::{Kind, Lifetime, Snapshot, SystemOps, Targets, Txn, recover_at_startup};

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

mod common;

fn temp(name: &str) -> common::TempDir {
    common::temp("rec", name)
}

fn targets() -> Targets {
    Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"))
}

fn key(n: u32) -> String {
    format!(r"HKLM\SOFTWARE\AlterDNS\Test\D{n}")
}

/// Txn başlatır, bir değer uygular; `commit` isteğe bağlı. Txn düşer (çökme gibi kilit bırakılır).
fn txn(dir: &Path, ops: &FakeOps, lifetime: Lifetime, n: u32, commit: bool) {
    let mut t = Txn::begin(dir, "test", lifetime, targets()).unwrap();
    t.apply(
        ops,
        Kind::RegistryValue,
        &key(n),
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    if commit {
        t.commit().unwrap();
    }
}

#[test]
fn uncommitted_txn_is_reverted() {
    let d = temp("uncommitted");
    let ops = FakeOps::default();
    txn(&d, &ops, Lifetime::Persistent, 1, false);
    let r = recover_at_startup(&d, &ops, &targets());
    assert!(r.clean(), "{r:?}");
    assert_eq!(r.txns.len(), 1);
    assert_eq!(
        ops.read(Kind::RegistryValue, &key(1)).unwrap(),
        Snapshot::Absent
    );
}

#[test]
fn session_txn_is_reverted_even_if_committed() {
    let d = temp("session");
    let ops = FakeOps::default();
    txn(&d, &ops, Lifetime::Session, 2, true);
    let r = recover_at_startup(&d, &ops, &targets());
    assert!(r.clean(), "{r:?}");
    assert_eq!(
        ops.read(Kind::RegistryValue, &key(2)).unwrap(),
        Snapshot::Absent
    );
}

#[test]
fn persistent_committed_txn_survives_recovery() {
    let d = temp("persistent");
    let ops = FakeOps::default();
    txn(&d, &ops, Lifetime::Persistent, 3, true);
    let r = recover_at_startup(&d, &ops, &targets());
    assert_eq!(r.kept.len(), 1, "{r:?}");
    assert!(r.txns.is_empty());
    assert_eq!(
        ops.read(Kind::RegistryValue, &key(3)).unwrap(),
        Snapshot::Present("yeni".into())
    );
}

#[test]
fn live_txn_is_left_alone() {
    let d = temp("live");
    let ops = FakeOps::default();
    let mut live = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    live.apply(
        &ops,
        Kind::RegistryValue,
        &key(4),
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    let r = recover_at_startup(&d, &ops, &targets());
    assert_eq!(r.in_use.len(), 1, "{r:?}");
    assert_eq!(
        ops.read(Kind::RegistryValue, &key(4)).unwrap(),
        Snapshot::Present("yeni".into())
    );
    drop(live);
}

#[test]
fn corrupt_file_does_not_block_others() {
    let d = temp("corrupt");
    let ops = FakeOps::default();
    std::fs::write(d.join("bozuk-1.jsonl"), "çöp\n{}\n").unwrap();
    txn(&d, &ops, Lifetime::Session, 5, false);
    let r = recover_at_startup(&d, &ops, &targets());
    assert_eq!(r.errors.len(), 1, "{r:?}");
    assert_eq!(r.errors[0].1.code(), AdnsErrorCode::SysRevertFailed);
    assert_eq!(r.txns.len(), 1);
    assert_eq!(
        ops.read(Kind::RegistryValue, &key(5)).unwrap(),
        Snapshot::Absent
    );
    assert!(!r.clean());
}

/// Aynı değeri art arda değiştiren iki txn: yeni olan önce geri alınır, değer en baştaki haline
/// döner. Ad sırası (alfa < zeta) eskiyi öne alırdı (inceleme bulgusu, 2026-11-21).
#[test]
fn newer_txn_is_reverted_first() {
    let d = temp("order");
    let ops = FakeOps::default();
    let k = key(7);
    let mut older = Txn::begin(&d, "alfa", Lifetime::Session, targets()).unwrap();
    older
        .apply(&ops, Kind::RegistryValue, &k, Snapshot::Present("a".into()))
        .unwrap();
    drop(older);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let mut newer = Txn::begin(&d, "zeta", Lifetime::Session, targets()).unwrap();
    newer
        .apply(&ops, Kind::RegistryValue, &k, Snapshot::Present("b".into()))
        .unwrap();
    drop(newer);
    let r = recover_at_startup(&d, &ops, &targets());
    assert!(r.clean(), "{r:?}");
    assert_eq!(ops.read(Kind::RegistryValue, &k).unwrap(), Snapshot::Absent);
}

#[test]
fn recovery_twice_is_idempotent() {
    let d = temp("twice");
    let ops = FakeOps::default();
    txn(&d, &ops, Lifetime::Session, 6, false);
    let first = recover_at_startup(&d, &ops, &targets());
    let second = recover_at_startup(&d, &ops, &targets());
    assert_eq!(first.txns[0].1.reverted, [1]);
    assert!(second.txns[0].1.reverted.is_empty(), "{second:?}");
    assert!(second.clean());
}
