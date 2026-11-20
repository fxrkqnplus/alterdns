// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gerçek sistem işlemleri, yalnızca AlterDNS'in kendi test nesneleriyle (T-JRN-001):
//! `HKCU\Software\AlterDNS-Test\<pid>` ve `%TEMP%\alterdns-*`.

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use alterdns_errors::AdnsErrorCode;
use alterdns_journal::{
    Kind, Lifetime, RealOps, Snapshot, SystemOps, TEST_REGISTRY_ROOT, Targets, Txn,
};
use windows_registry::CURRENT_USER;

/// `TEST_REGISTRY_ROOT`'un HKCU altındaki yolu.
const TEST_KEY: &str = r"Software\AlterDNS-Test";

/// Kayıt defteri testleri sırayla çalışır: biri `AlterDNS-Test`'i boş görüp silerken diğeri
/// altına anahtar oluşturabilirdi.
static REGISTRY: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Test anahtarlarını sonda (panikte de) siler; `AlterDNS-Test` boş kalırsa onu da. Kilit
/// alan olarak tutulur: temizlik kilit altında yapılır, kilit en son bırakılır.
struct Cleanup {
    keys: Vec<String>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Cleanup {
    fn new(keys: Vec<String>) -> Cleanup {
        let lock = REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for k in &keys {
            let _ = CURRENT_USER.remove_tree(k);
        }
        Cleanup { keys, _lock: lock }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for k in &self.keys {
            let _ = CURRENT_USER.remove_tree(k);
        }
        let empty = CURRENT_USER
            .open(TEST_KEY)
            .and_then(|k| Ok(k.keys()?.next().is_none()))
            .unwrap_or(false);
        if empty {
            let _ = CURRENT_USER.remove_tree(TEST_KEY);
        }
    }
}

/// Her test kendi alt anahtarını kullanır: `<pid>-<ad>`.
fn test_root(name: &str) -> (String, String, Cleanup) {
    let sub = format!("{}-{name}", std::process::id());
    let hkcu_path = format!(r"{TEST_KEY}\{sub}");
    let root = format!(r"{TEST_REGISTRY_ROOT}\{sub}");
    (root, hkcu_path.clone(), Cleanup::new(vec![hkcu_path]))
}

fn targets(registry_root: &str) -> Targets {
    Targets {
        registry_root: registry_root.into(),
        ..Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"))
    }
}

mod common;

fn journal_dir(name: &str) -> common::TempDir {
    common::temp("real", name)
}

#[test]
fn t_jrn_001_registry_apply_revert_restores_before() {
    let (root, hkcu_path, _c) = test_root("t001");
    let target = format!(r"{root}\Deger");
    assert_eq!(
        RealOps.read(Kind::RegistryValue, &target).unwrap(),
        Snapshot::Absent
    );
    let d = journal_dir("t001");
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets(&root)).unwrap();
    txn.apply(
        &RealOps,
        Kind::RegistryValue,
        &target,
        Snapshot::Present("1".into()),
    )
    .unwrap();
    let key = CURRENT_USER.open(&hkcu_path).unwrap();
    assert_eq!(key.get_string("Deger").unwrap(), "1");
    let report = txn.rollback(&RealOps).unwrap();
    assert!(report.clean(), "{report:?}");
    assert_eq!(report.reverted, [1]);
    assert_eq!(
        RealOps.read(Kind::RegistryValue, &target).unwrap(),
        Snapshot::Absent
    );
}

#[test]
fn registry_revert_restores_previous_string() {
    let (root, _, _c) = test_root("eski");
    let target = format!(r"{root}\Deger");
    RealOps
        .write(
            Kind::RegistryValue,
            &target,
            &Snapshot::Present("eski".into()),
        )
        .unwrap();
    let d = journal_dir("eski");
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets(&root)).unwrap();
    txn.apply(
        &RealOps,
        Kind::RegistryValue,
        &target,
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    assert!(txn.rollback(&RealOps).unwrap().clean());
    assert_eq!(
        RealOps.read(Kind::RegistryValue, &target).unwrap(),
        Snapshot::Present("eski".into())
    );
}

#[test]
fn non_string_value_is_an_error_not_absent() {
    let (root, hkcu_path, _c) = test_root("dword");
    CURRENT_USER
        .create(&hkcu_path)
        .unwrap()
        .set_u32("Deger", 7)
        .unwrap();
    let target = format!(r"{root}\Deger");
    // Başka türdeki bir değer "yok" sayılsaydı geri alma onu silebilirdi.
    assert!(RealOps.read(Kind::RegistryValue, &target).is_err());
    let d = journal_dir("dword");
    let mut txn = Txn::begin(&d, "test", Lifetime::Session, targets(&root)).unwrap();
    let err = txn
        .apply(
            &RealOps,
            Kind::RegistryValue,
            &target,
            Snapshot::Present("1".into()),
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysApplyFailed);
    let key = CURRENT_USER.open(&hkcu_path).unwrap();
    assert_eq!(key.get_u32("Deger").unwrap(), 7);
}

#[test]
fn real_ops_rejects_targets_outside_fixed_roots() {
    // Hata olsa bile yalnızca AlterDNS-Test adlı kendi anahtarlarımıza yazılabilirdi.
    let sibling = format!(r"Software\AlterDNS-Test-Baska-{}", std::process::id());
    let _c = Cleanup::new(vec![sibling.clone()]);
    for target in [
        format!(r"HKCU\{sibling}\Deger"),
        format!(r"{TEST_REGISTRY_ROOT}\..\Deger"),
        TEST_REGISTRY_ROOT.to_string(),
        format!(r"HKCU\{TEST_KEY}\Deger\"),
    ] {
        assert!(
            RealOps
                .write(Kind::RegistryValue, &target, &Snapshot::Present("1".into()))
                .is_err(),
            "{target}"
        );
    }
    assert!(CURRENT_USER.open(&sibling).is_err());
    // Okuma da sabit köklerle sınırlı.
    assert!(
        RealOps
            .read(
                Kind::RegistryValue,
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\x"
            )
            .is_err()
    );
}

#[test]
fn kinds_of_later_phases_are_not_supported_yet() {
    for kind in [
        Kind::ServiceCreate,
        Kind::ServiceConfig,
        Kind::StartupEntry,
        Kind::FileInstall,
        Kind::UninstallEntry,
        Kind::Unknown,
    ] {
        assert!(RealOps.read(kind, "AlterDNS").is_err(), "{kind:?}");
        assert!(
            RealOps.write(kind, "AlterDNS", &Snapshot::Absent).is_err(),
            "{kind:?}"
        );
    }
}

#[test]
fn file_value_apply_revert() {
    let dir = journal_dir("dosya");
    let file = dir.join("deger.txt");
    let target = file.to_string_lossy().into_owned();
    let mut txn = Txn::begin(&dir, "test", Lifetime::Session, targets(TEST_REGISTRY_ROOT)).unwrap();
    txn.apply(
        &RealOps,
        Kind::FileValue,
        &target,
        Snapshot::Present("yeni".into()),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "yeni");
    assert!(txn.rollback(&RealOps).unwrap().clean());
    assert!(!file.exists());
    // Geçici dizin dışındaki dosyaya dokunulmaz.
    assert!(
        RealOps
            .write(
                Kind::FileValue,
                r"C:\Windows\Temp\alterdns-x\y.txt",
                &Snapshot::Absent
            )
            .is_err()
    );
}
