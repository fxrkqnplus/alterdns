// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Katalog doğrulayıcısı (T-ERR-001, T-ERR-002) ve hata API'si.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/catalog.rs"]
mod catalog;

use alterdns_errors::{AdnsError, AdnsErrorCode, Lang};

fn real_catalog() -> Vec<catalog::Entry> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../errors/catalog.toml");
    catalog::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Geçerli tek girdilik örnek; testler buradan bozuk örnekler türetir.
fn sample(code: &str, name: &str, tr_title: &str, en_title: &str, extra: &str) -> String {
    format!(
        r#"[[error]]
code = "{code}"
name = "{name}"
area = "IPC"
severity = "error"
faz = 1
retryable = false
user_action_required = true
params = []
docs = "errors#{lower}"
{extra}
[error.tr]
title = "{tr_title}"
message = "Mesaj."
causes = ["Neden"]
solutions = ["Çözüm"]

[error.en]
title = "{en_title}"
message = "Message."
causes = ["Cause"]
solutions = ["Solution"]
"#,
        lower = code.to_ascii_lowercase()
    )
}

#[test]
fn t_err_001_codes_unique_and_well_formed() {
    let entries = real_catalog();
    assert_eq!(entries.len(), 68);
    assert_eq!(catalog::validate(&entries), Vec::<String>::new());

    let twice = format!(
        "{}\n{}",
        sample("ADNS-IPC-001", "IpcOne", "Bir", "One", ""),
        sample("ADNS-IPC-001", "IpcTwo", "İki", "Two", "")
    );
    let problems = catalog::validate(&catalog::parse(&twice).unwrap());
    assert!(
        problems.iter().any(|p| p.contains("yinelenen kod")),
        "{problems:?}"
    );

    for bad in ["ADNS-IPC-1", "ADNS-XYZ-001", "ADNS-IPC-000", "IPC-001"] {
        assert!(catalog::code_area(bad).is_none(), "{bad}");
    }
    let wrong_name = sample("ADNS-IPC-001", "connectFailed", "Bir", "One", "");
    assert!(!catalog::validate(&catalog::parse(&wrong_name).unwrap()).is_empty());
}

#[test]
fn t_err_002_both_languages_complete() {
    let empty_en = sample("ADNS-IPC-001", "IpcOne", "Bir", "", "");
    let problems = catalog::validate(&catalog::parse(&empty_en).unwrap());
    assert!(problems.iter().any(|p| p.contains("[en]")), "{problems:?}");

    let no_causes = sample("ADNS-IPC-001", "IpcOne", "Bir", "One", "").replacen(
        "causes = [\"Neden\"]",
        "causes = []",
        1,
    );
    let problems = catalog::validate(&catalog::parse(&no_causes).unwrap());
    assert!(problems.iter().any(|p| p.contains("[tr]")), "{problems:?}");
}

#[test]
fn placeholders_match_params() {
    let undeclared = sample("ADNS-IPC-001", "IpcOne", "Bir {kaynak}", "One {kaynak}", "");
    assert!(!catalog::validate(&catalog::parse(&undeclared).unwrap()).is_empty());

    let unused = sample("ADNS-IPC-001", "IpcOne", "Bir", "One", "").replacen(
        "params = []",
        "params = [\"kaynak\"]",
        1,
    );
    assert!(!catalog::validate(&catalog::parse(&unused).unwrap()).is_empty());

    let ok = sample("ADNS-IPC-001", "IpcOne", "Bir {kaynak}", "One {kaynak}", "").replacen(
        "params = []",
        "params = [\"kaynak\"]",
        1,
    );
    assert_eq!(
        catalog::validate(&catalog::parse(&ok).unwrap()),
        Vec::<String>::new()
    );
}

#[test]
fn solutions_never_suggest_lowering_security() {
    for lowering in [
        "Antivirüsü kapatın",
        "Windows Defender'a istisna ekleyin",
        "Güvenlik duvarını devre dışı bırakın",
        "Disable your antivirus",
        "Turn off Smart App Control",
        "Add an exception in Windows Defender",
    ] {
        assert!(catalog::lowers_security(lowering), "{lowering}");
    }
    for fine in [
        "Güvenlik yazılımınızın bildirimlerini kontrol edin; antivirüsü kapatmayın",
        "Do not disable your antivirus",
        "Bilgisayarı yeniden başlatın",
        "Disable the proxy in the app settings",
    ] {
        assert!(!catalog::lowers_security(fine), "{fine}");
    }
}

#[test]
fn display_is_code_and_title() {
    assert_eq!(
        AdnsError::new(AdnsErrorCode::IpcUnauthorized).to_string(),
        "ADNS-IPC-003: Yetkisiz istek reddedildi"
    );
}

fn parametric() -> AdnsErrorCode {
    AdnsErrorCode::ALL
        .iter()
        .copied()
        .find(|c| c.params().contains(&"kaynak") && c.text(Lang::Tr).message.contains("{kaynak}"))
        .unwrap()
}

#[test]
fn render_substitutes_params() {
    let r = AdnsError::new(parametric())
        .with("kaynak", "abuse.ch")
        .render(Lang::Tr);
    assert!(r.message.contains("abuse.ch"), "{}", r.message);
    assert!(!r.message.contains("{kaynak}"));
}

#[test]
fn render_replaces_missing_param_with_placeholder() {
    let r = AdnsError::new(parametric())
        .with("fazla", "x")
        .render(Lang::En);
    assert!(!r.message.contains("{kaynak}"), "{}", r.message);
    assert!(r.message.contains('?'), "{}", r.message);
}

#[test]
fn from_code_roundtrip() {
    for &c in AdnsErrorCode::ALL {
        assert_eq!(AdnsErrorCode::from_code(c.as_str()), Some(c));
    }
    assert_eq!(AdnsErrorCode::from_code("ADNS-IPC-999"), None);
}

#[test]
fn english_text_differs_from_turkish() {
    for &c in AdnsErrorCode::ALL {
        assert_ne!(c.text(Lang::Tr).title, c.text(Lang::En).title, "{c}");
    }
}

#[test]
fn faz1_codes_have_fixed_names() {
    use AdnsErrorCode::*;
    let expected = [
        (GenUnexpected, "ADNS-GEN-900"),
        (IpcConnect, "ADNS-IPC-001"),
        (IpcBadRequest, "ADNS-IPC-002"),
        (IpcUnauthorized, "ADNS-IPC-003"),
        (IpcVersionMismatch, "ADNS-IPC-004"),
        (IpcPipeInUse, "ADNS-IPC-005"),
        (SvcStartFailed, "ADNS-SVC-001"),
        (SvcRestarted, "ADNS-SVC-002"),
        (SysApplyFailed, "ADNS-SYS-001"),
        (SysRevertFailed, "ADNS-SYS-002"),
        (SysChangedByOther, "ADNS-SYS-003"),
        (SysElevationDenied, "ADNS-SYS-004"),
        (UiStartFailed, "ADNS-UI-001"),
        (UiNotificationsUnavailable, "ADNS-UI-002"),
    ];
    for (code, s) in expected {
        assert_eq!(code.as_str(), s);
        assert_eq!(code.faz(), 1, "{s}");
    }
}

#[test]
fn lang_codes_roundtrip() {
    for l in [Lang::Tr, Lang::En] {
        assert_eq!(Lang::from_code(l.code()), Some(l));
    }
    assert_eq!(Lang::default(), Lang::Tr);
}
