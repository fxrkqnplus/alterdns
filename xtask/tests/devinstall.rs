// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme kurulumu: adım planı, yükseltme kapısı ve kalıntı taraması. Testler kurulum
//! yapmaz; `dev_install` ve `dev_uninstall` çağrılmaz.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_core::Paths;
use alterdns_errors::AdnsErrorCode;
use alterdns_journal::{Kind, SERVICE_NAME, Snapshot, sha256_file};
use alterdns_winsys::{DirAccess, PipeListener, create_protected_dir};
use xtask::devinstall::{BINARIES, plan, require_elevated};
use xtask::residue::{Residue, scan};

fn temp(name: &str) -> xtask::testutil::TempDir {
    xtask::testutil::temp("devinst", name).unwrap()
}

fn paths(d: &xtask::testutil::TempDir) -> Paths {
    Paths::rooted(
        d.join("pd").join("AlterDNS"),
        d.join("lad").join("AlterDNS"),
        d.join("pf").join("AlterDNS"),
    )
}

#[test]
fn plan_lists_files_service_then_startup() {
    let d = temp("plan");
    let target = d.join("target");
    std::fs::create_dir_all(&target).unwrap();
    for b in BINARIES {
        std::fs::write(target.join(b), b).unwrap();
    }
    let p = paths(&d);
    let changes = plan(&p, "1.2.3", &target).unwrap();
    assert_eq!(changes.len(), 5);
    let dir = p.install_dir("1.2.3");
    for (c, b) in changes.iter().zip(BINARIES) {
        assert_eq!(c.kind, Kind::FileInstall);
        assert_eq!(c.target, dir.join(b).to_string_lossy());
        assert_eq!(c.source.as_deref(), Some(target.join(b).as_path()));
        assert_eq!(
            c.after,
            Snapshot::Present(sha256_file(&target.join(b)).unwrap())
        );
    }
    let svc = &changes[3];
    assert_eq!(svc.kind, Kind::ServiceCreate);
    assert_eq!(svc.target, SERVICE_NAME);
    assert_eq!(
        svc.after,
        Snapshot::Present(dir.join("alterdns-svc.exe").to_string_lossy().into_owned())
    );
    let run = &changes[4];
    assert_eq!(run.kind, Kind::StartupEntry);
    assert_eq!(run.target, alterdns_journal::RUN_VALUE);
    assert_eq!(
        run.after,
        Snapshot::Present(format!(
            "\"{}\" --tray --logon",
            dir.join("alterdns-ui.exe").display()
        ))
    );
    // Test derlemesi (test-ops) kurulmaz.
    std::fs::write(
        target.join("alterdns-maint.exe"),
        [b"MZ...".as_slice(), b"ALTERDNS_TEST_", b"JOURNAL_DIR"].concat(),
    )
    .unwrap();
    assert!(plan(&p, "1.2.3", &target).is_err());
    // Derlenmemiş ikili: plan kurulmaz.
    std::fs::remove_file(target.join("alterdns-maint.exe")).unwrap();
    assert!(plan(&p, "1.2.3", &target).is_err());
}

/// Ürün derlemesinde test dizeleri yoktur; `test-ops` derlemesinde vardır. `target\release`'teki
/// ikililer varsa onlarla da sınanır.
#[test]
fn product_build_has_no_test_markers() {
    use xtask::devinstall::is_test_build;
    assert!(is_test_build(b"..ALTERDNS_TEST_JOURNAL_DIR.."));
    assert!(is_test_build(b"..(A;;GA;;;OW).."));
    assert!(!is_test_build(b"O:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)"));
    let release = xtask::check::repo_root().unwrap().join(r"target\release");
    for b in BINARIES {
        if let Ok(bytes) = std::fs::read(release.join(b)) {
            assert!(!is_test_build(&bytes), "{b}");
        }
    }
}

#[test]
fn elevation_gate_is_sys_004() {
    let e = require_elevated(false).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::SysElevationDenied);
    assert_eq!(e.code().as_str(), "ADNS-SYS-004");
    assert!(require_elevated(true).is_ok());
}

/// Standart kullanıcının önceden oluşturduğu dizin kurulumda kullanılmaz (K20).
#[test]
fn existing_untrusted_dir_is_refused() {
    let d = temp("untrusted");
    let dir = d.join("AlterDNS");
    std::fs::create_dir_all(&dir).unwrap();
    let e = create_protected_dir(&dir, DirAccess::AdminOnly).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn residue_scan_detects_planted_objects() {
    let d = temp("residue");
    let p = paths(&d);
    let service = format!("AlterDNS-Yok-{}", std::process::id());
    let pipe = format!(r"\\.\pipe\AlterDNS.Test.{}.residue", std::process::id());
    let base = scan(&p, &service, &pipe);
    // Ürünün sabit kayıt defteri anahtarları bu makinede olmamalı (kurulum onları yazmaz).
    assert!(base.is_empty(), "{base:?}");
    std::fs::create_dir_all(&p.machine).unwrap();
    let _listener = PipeListener::bind(&pipe, "D:P(A;;GA;;;OW)", 1).unwrap();
    let found = scan(&p, &service, &pipe);
    assert!(
        found.contains(&Residue::Dir(p.machine.clone())),
        "{found:?}"
    );
    assert!(found.contains(&Residue::Pipe(pipe.clone())), "{found:?}");
    assert_eq!(found.len(), 2, "{found:?}");
}
