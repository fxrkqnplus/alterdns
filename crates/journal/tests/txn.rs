// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Txn: uygula, commit, geri al; izin listesi ve çakışma (T-JRN-001 sahte, T-JRN-003).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;

use alterdns_errors::AdnsErrorCode;
use alterdns_journal::{
    JournalFile, Kind, Lifetime, Snapshot, State, SystemOps, Targets, Txn, target_allowed,
};

#[derive(Default)]
struct FakeOps {
    values: RefCell<BTreeMap<String, Snapshot>>,
    writes: RefCell<u32>,
    fail_write: Cell<bool>,
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
        *self.writes.borrow_mut() += 1;
        if self.fail_write.get() {
            return Err("yazma reddedildi".into());
        }
        self.values
            .borrow_mut()
            .insert(target.to_string(), value.clone());
        Ok(())
    }
}

mod common;

fn temp(name: &str) -> common::TempDir {
    common::temp("txn", name)
}

fn targets() -> Targets {
    Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"))
}

const KEY: &str = r"HKLM\SOFTWARE\AlterDNS\Ayar\Deger";

#[test]
fn apply_then_rollback_restores_before() {
    let d = temp("restore");
    let ops = FakeOps::default();
    ops.values
        .borrow_mut()
        .insert(KEY.into(), Snapshot::Present("eski".into()));
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    txn.apply(
        &ops,
        Kind::RegistryValue,
        KEY,
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Present("yeni".into())
    );
    let report = txn.rollback(&ops).unwrap();
    assert!(report.clean(), "{report:?}");
    assert!(report.error().is_none());
    assert_eq!(report.reverted, [1]);
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Present("eski".into())
    );
}

#[test]
fn journal_write_failure_means_no_apply() {
    let d = temp("nowrite");
    let ops = FakeOps::default();
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    // 4 KiB'ı aşan değer journal'a yazılamaz: sistem de değiştirilmez.
    let err = txn
        .apply(
            &ops,
            Kind::RegistryValue,
            KEY,
            Snapshot::Present("x".repeat(5000)),
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysApplyFailed);
    assert_eq!(*ops.writes.borrow(), 0);
}

#[test]
fn conflict_is_not_overwritten() {
    let d = temp("conflict");
    let ops = FakeOps::default();
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    txn.apply(
        &ops,
        Kind::RegistryValue,
        KEY,
        Snapshot::Present("bizim".into()),
    )
    .unwrap();
    // Kullanıcı arada değiştirir.
    ops.values
        .borrow_mut()
        .insert(KEY.into(), Snapshot::Present("kullanici".into()));
    let path = txn.journal_path();
    let report = txn.rollback(&ops).unwrap();
    assert_eq!(report.conflicts, [1]);
    assert_eq!(
        report.error().unwrap().code(),
        AdnsErrorCode::SysChangedByOther
    );
    assert_eq!(
        ops.read(Kind::RegistryValue, KEY).unwrap(),
        Snapshot::Present("kullanici".into())
    );
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(j.records()[0].state, State::Conflict);
}

#[test]
fn own_objects_are_only_removed() {
    let d = temp("own");
    let ops = FakeOps::default();
    let file = r"C:\Program Files\AlterDNS\0.1.0\alterdns-maint.exe";
    ops.values
        .borrow_mut()
        .insert(file.into(), Snapshot::Present("eski-hash".into()));
    let mut txn = Txn::begin(&d, "test", Lifetime::Persistent, targets()).unwrap();
    txn.apply(
        &ops,
        Kind::FileInstall,
        file,
        Snapshot::Present("yeni-hash".into()),
    )
    .unwrap();
    let report = txn.rollback(&ops).unwrap();
    // Önceden var olan dosya geri yazılmaz; çakışma olarak bildirilir.
    assert_eq!(report.conflicts, [1]);
    assert_eq!(
        ops.read(Kind::FileInstall, file).unwrap(),
        Snapshot::Present("yeni-hash".into())
    );
}

#[test]
fn disallowed_target_is_rejected_before_any_write() {
    let d = temp("disallowed");
    let ops = FakeOps::default();
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    for (kind, target) in [
        (
            Kind::RegistryValue,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\Baska",
        ),
        (
            Kind::RegistryValue,
            r"HKLM\SOFTWARE\AlterDNS\..\Microsoft\x",
        ),
        (Kind::FileInstall, r"C:\Windows\System32\x.dll"),
        (
            Kind::FileInstall,
            r"C:\Program Files\AlterDNS\..\Baska\x.exe",
        ),
        (Kind::ServiceCreate, "BaskaServis"),
        (Kind::Unknown, KEY),
    ] {
        assert!(
            txn.apply(&ops, kind, target, Snapshot::Present("1".into()))
                .is_err(),
            "{kind:?} {target}"
        );
    }
    assert_eq!(*ops.writes.borrow(), 0);
    // Yanlış kurulmuş Targets bile başka anahtara izin vermez.
    let mut wrong = targets();
    wrong.registry_root = r"HKLM\SOFTWARE\Microsoft".into();
    assert!(!target_allowed(
        Kind::RegistryValue,
        r"HKLM\SOFTWARE\Microsoft\x",
        &wrong
    ));
}

#[test]
fn failed_write_closes_record_when_value_unchanged() {
    let d = temp("failwrite");
    let ops = FakeOps {
        fail_write: Cell::new(true),
        ..FakeOps::default()
    };
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    let err = txn
        .apply(
            &ops,
            Kind::RegistryValue,
            KEY,
            Snapshot::Present("1".into()),
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysApplyFailed);
    let path = txn.journal_path();
    drop(txn);
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(j.records()[0].state, State::Failed);
}

#[test]
fn revert_write_failure_keeps_record_open() {
    let d = temp("revertfail");
    let ops = FakeOps::default();
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets()).unwrap();
    txn.apply(
        &ops,
        Kind::RegistryValue,
        KEY,
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    let path = txn.journal_path();
    ops.fail_write.set(true);
    let report = txn.rollback(&ops).unwrap();
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(
        report.error().unwrap().code(),
        AdnsErrorCode::SysRevertFailed
    );
    // Kayıt açık kalır: bir sonraki kurtarma yeniden dener.
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(j.records()[0].state, State::Applied);
}

#[test]
fn commit_marks_txn_committed() {
    let d = temp("commit");
    let ops = FakeOps::default();
    let mut txn = Txn::begin(&d, "test", Lifetime::Persistent, targets()).unwrap();
    txn.apply(
        &ops,
        Kind::RegistryValue,
        KEY,
        Snapshot::Present("1".into()),
    )
    .unwrap();
    let path = txn.journal_path();
    txn.commit().unwrap();
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert!(j.committed());
    assert_eq!(j.lifetime(), Some(Lifetime::Persistent));
}
