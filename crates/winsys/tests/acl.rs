// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Korumalı dizin, sahip/DACL değerlendirmesi ve reparse noktası (K20).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_winsys::{
    DirAccess, DirProblem, create_protected_dir, evaluate_security, is_reparse_point, pin_dir,
    verify_protected_chain, verify_protected_dir,
};

#[test]
fn pinned_dir_cannot_be_renamed() {
    let d = temp("pin");
    let dir = d.join("journal");
    std::fs::create_dir_all(&dir).unwrap();
    let pin = pin_dir(&dir).unwrap();
    assert!(std::fs::rename(&dir, d.join("baska")).is_err());
    // Sabitlenmiş dizinde dosya oluşturmak serbesttir.
    std::fs::write(dir.join("x.jsonl"), "").unwrap();
    drop(pin);
    std::fs::rename(&dir, d.join("baska")).unwrap();
    assert!(pin_dir(&d.join("baska").join("x.jsonl")).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn evaluate_security_fails_closed_on_object_aces_and_skips_deny() {
    // İzin veren nesne ACE'si (OA) değerlendirilmez: dizin reddedilir.
    let r = evaluate_security(
        "O:SYD:P(A;OICI;FA;;;SY)(OA;;CC;bf967aba-0de6-11d0-a285-00aa003049e2;;BU)",
    );
    assert!(matches!(r, Err(DirProblem::Unreadable(_))), "{r:?}");
    // Reddetme ACE'si yazma hakkı vermez.
    assert_eq!(
        evaluate_security("O:SYD:P(D;OICI;FA;;;BU)(A;OICI;FA;;;SY)"),
        Ok(())
    );
}

#[test]
fn evaluate_security_accepts_admin_only_and_users_read() {
    assert_eq!(
        evaluate_security(&format!("O:SY{}", DirAccess::AdminOnly.sddl())),
        Ok(())
    );
    assert_eq!(
        evaluate_security(&format!("O:BA{}", DirAccess::AdminWriteUsersRead.sddl())),
        Ok(())
    );
}

#[test]
fn evaluate_security_rejects_user_owner() {
    assert_eq!(
        evaluate_security("O:S-1-5-21-1-2-3-1001D:P(A;OICI;FA;;;SY)"),
        Err(DirProblem::UntrustedOwner)
    );
    assert_eq!(
        evaluate_security("D:P(A;OICI;FA;;;SY)"),
        Err(DirProblem::UntrustedOwner)
    );
}

#[test]
fn evaluate_security_rejects_users_write() {
    assert_eq!(
        evaluate_security("O:SYD:P(A;OICI;FA;;;SY)(A;OICI;FW;;;BU)"),
        Err(DirProblem::WritableByOthers)
    );
}

#[test]
fn evaluate_security_rejects_everyone_full_and_null_dacl() {
    assert_eq!(
        evaluate_security("O:SYD:P(A;OICI;FA;;;WD)"),
        Err(DirProblem::WritableByOthers)
    );
    assert_eq!(
        evaluate_security("O:SYD:NO_ACCESS_CONTROL"),
        Err(DirProblem::WritableByOthers)
    );
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("alterdns-acl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn reparse_point_is_detected() {
    let d = temp("junction");
    let target = d.join("hedef");
    std::fs::create_dir_all(&target).unwrap();
    let link = d.join("baglanti");
    let cmd = alterdns_winsys::system_dir().unwrap().join("cmd.exe");
    let ok = std::process::Command::new(cmd)
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "bağlantı noktası oluşturulamadı");
    assert!(is_reparse_point(&link).unwrap());
    assert!(!is_reparse_point(&target).unwrap());
    assert_eq!(verify_protected_dir(&link), Err(DirProblem::ReparsePoint));
    let _ = std::fs::remove_dir(&link);
    let _ = std::fs::remove_dir_all(&d);
}

/// Yükseltilmemiş işlemin oluşturduğu dizin güvenilir değildir (sahibi kullanıcıdır ya da DACL
/// kullanıcıya yazma verir). Olumlu yol 1C'de yönetici `dev-install` ile doğrulanır.
#[test]
fn user_created_dir_is_untrusted() {
    let d = temp("user");
    let r = verify_protected_dir(&d);
    assert!(
        matches!(
            r,
            Err(DirProblem::UntrustedOwner | DirProblem::WritableByOthers)
        ),
        "{r:?}"
    );
    assert_eq!(
        verify_protected_dir(&d.join("yok")),
        Err(DirProblem::Missing)
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Zincirin her halkası denetlenir; kökün altında olmayan yaprak reddedilir. Var olan ama
/// güvenilmeyen dizin "oluşturuldu" sayılmaz.
#[test]
fn chain_and_existing_dirs_are_verified() {
    let d = temp("chain");
    std::fs::create_dir_all(d.join("journal")).unwrap();
    let r = verify_protected_chain(&d, &d.join("journal"));
    assert!(
        matches!(
            r,
            Err(DirProblem::UntrustedOwner | DirProblem::WritableByOthers)
        ),
        "{r:?}"
    );
    let r = verify_protected_chain(&d, std::path::Path::new(r"C:\baska\journal"));
    assert!(matches!(r, Err(DirProblem::Unreadable(_))), "{r:?}");
    let e = create_protected_dir(&d.join("journal"), DirAccess::AdminOnly).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
    let _ = std::fs::remove_dir_all(&d);
}
