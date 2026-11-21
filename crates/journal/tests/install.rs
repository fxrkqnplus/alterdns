// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `FileInstall` (hash, reparse, alternatif veri akışı), `InstallOps` ve servis ikilisi
//! doğrulaması. Testler servis oluşturmaz ya da silmez.

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::Path;

use alterdns_journal::{
    InstallOps, Kind, Lifetime, RealOps, Snapshot, SystemOps, Targets, Txn, product_install_root,
    sha256_file, validate_service_binary,
};

mod common;

/// "abc" metninin SHA-256'sı (FIPS 180-2 örneği).
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn file_install_hash_and_reparse_rules() {
    let d = common::temp("inst", "rules");
    let f = d.join("a.exe");
    std::fs::write(&f, "abc").unwrap();
    assert_eq!(sha256_file(&f).unwrap(), ABC);
    assert_eq!(
        RealOps.read(Kind::FileInstall, &s(&f)).unwrap(),
        Snapshot::Present(ABC.into())
    );
    assert_eq!(
        RealOps
            .read(Kind::FileInstall, &s(&d.join("yok.exe")))
            .unwrap(),
        Snapshot::Absent
    );
    // Bağlantı noktası düz dosya değildir.
    let target = d.join("hedef");
    std::fs::create_dir_all(&target).unwrap();
    let link = d.join("baglanti");
    let cmd = alterdns_winsys::system_dir().unwrap().join("cmd.exe");
    assert!(
        std::process::Command::new(cmd)
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(RealOps.read(Kind::FileInstall, &s(&link)).is_err());
    assert!(
        RealOps
            .write(Kind::FileInstall, &s(&link), &Snapshot::Absent)
            .is_err()
    );
    assert!(link.exists(), "bağlantı silinmemeli");
    // Alternatif veri akışı ve izin dışı hedefler.
    for bad in [
        format!("{}:gizli", s(&f)),
        r"C:\Windows\x.dll".to_string(),
        "a.exe".to_string(),
        format!(r"{}\..\x.exe", s(&d)),
    ] {
        assert!(RealOps.read(Kind::FileInstall, &bad).is_err(), "{bad}");
    }
    // `RealOps` kopyalamaz; yalnızca siler.
    assert!(
        RealOps
            .write(Kind::FileInstall, &s(&f), &Snapshot::Present(ABC.into()))
            .is_err()
    );
    RealOps
        .write(Kind::FileInstall, &s(&f), &Snapshot::Absent)
        .unwrap();
    assert!(!f.exists());
    let _ = std::fs::remove_dir(&link);
}

#[test]
fn install_ops_copies_and_verifies_hash() {
    let d = common::temp("inst", "copy");
    let src = d.join("kaynak.bin");
    std::fs::write(&src, "abc").unwrap();
    let ok = d.join("a.exe");
    let wrong = d.join("b.exe");
    let ops = InstallOps::new(BTreeMap::from([
        (s(&ok), src.clone()),
        (s(&wrong), src.clone()),
    ]));
    ops.write(Kind::FileInstall, &s(&ok), &Snapshot::Present(ABC.into()))
        .unwrap();
    assert_eq!(std::fs::read_to_string(&ok).unwrap(), "abc");
    // Hash tutmazsa kopya bırakılmaz.
    assert!(
        ops.write(
            Kind::FileInstall,
            &s(&wrong),
            &Snapshot::Present("00".into())
        )
        .is_err()
    );
    assert!(!wrong.exists());
    // Var olan hedefin üzerine yazılmaz; kaynağı olmayan hedef reddedilir.
    assert!(
        ops.write(Kind::FileInstall, &s(&ok), &Snapshot::Present(ABC.into()))
            .is_err()
    );
    assert!(
        ops.write(
            Kind::FileInstall,
            &s(&d.join("c.exe")),
            &Snapshot::Present(ABC.into())
        )
        .is_err()
    );
}

/// Kurulum txn'i geri alınınca kopyalanan dosya kaldırılır (kendi nesnemiz).
#[test]
fn install_txn_reverts_by_removing_file() {
    let d = common::temp("inst", "txn");
    let journal = d.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let root = d.join("AlterDNS");
    let ver = root.join("0.0.0");
    std::fs::create_dir_all(&ver).unwrap();
    let src = d.join("kaynak.bin");
    std::fs::write(&src, "abc").unwrap();
    let dest = ver.join("alterdns-maint.exe");
    let ops = InstallOps::new(BTreeMap::from([(s(&dest), src)]));
    let mut txn = Txn::begin(
        &journal,
        "dev-install",
        Lifetime::Persistent,
        Targets::product(root),
    )
    .unwrap();
    txn.apply(
        &ops,
        Kind::FileInstall,
        &s(&dest),
        Snapshot::Present(ABC.into()),
    )
    .unwrap();
    assert!(dest.exists());
    let report = txn.rollback(&RealOps).unwrap();
    assert!(report.clean(), "{report:?}");
    assert!(!dest.exists());
}

#[test]
fn service_binary_must_be_svc_in_program_files() {
    let root = product_install_root().unwrap();
    for bad in [
        r"C:\Windows\x.exe".to_string(),
        r"alterdns-svc.exe".to_string(),
        format!(r"{}\0.0.0\baska.exe", root.display()),
        format!(r"{}\alterdns-svc.exe", root.display()),
        format!(r"{}\0.0.0\alt\alterdns-svc.exe", root.display()),
        format!(r"{}\0.0.0\..\..\x\alterdns-svc.exe", root.display()),
    ] {
        assert!(validate_service_binary(&bad).is_err(), "{bad}");
    }
    // Doğru biçim ama dosya yok: yine reddedilir (canonicalize).
    let missing = format!(
        r"{}\0.0.0-yok-{}\alterdns-svc.exe",
        root.display(),
        std::process::id()
    );
    assert!(validate_service_binary(&missing).is_err());
}
