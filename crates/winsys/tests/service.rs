// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! SCM sorguları (salt okunur) ve kurulu servisin ayarları (T-SVC-001, T-SVC-003).
//!
//! `#[ignore]` testleri yalnızca `cargo xtask dev-install` sonrasında, kullanıcının onayıyla
//! koşulur; servisi değiştirmezler, yalnızca okurlar.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_winsys::{
    REQUIRED_PRIVILEGES, RESET_PERIOD_SECS, RESTART_DELAYS_SECS, query_service,
    service_config_report,
};

#[test]
fn query_service_absent_and_present() {
    let name = format!("AlterDNS-Yok-{}", std::process::id());
    assert_eq!(query_service(&name).unwrap(), None);
    let event_log = query_service("EventLog").unwrap().unwrap();
    assert!(!event_log.binary_path.is_empty());
}

#[test]
#[ignore = "dev-install sonrası, onaylı yönetici koşusu"]
fn t_svc_001_installed_service_running() {
    let s = query_service("AlterDNS")
        .unwrap()
        .expect("servis kurulu değil");
    assert!(s.running, "{s:?}");
}

#[test]
#[ignore = "dev-install sonrası, onaylı yönetici koşusu"]
fn t_svc_003_recovery_and_privileges() {
    let r = service_config_report("AlterDNS").unwrap();
    assert!(r.binary_path_raw.starts_with('"'), "tırnaklı yol: {r:?}");
    assert!(
        r.binary_path_raw
            .to_ascii_lowercase()
            .ends_with("alterdns-svc.exe\""),
        "{r:?}"
    );
    assert!(r.auto_start, "{r:?}");
    assert!(r.local_system, "{r:?}");
    assert!(r.sid_unrestricted, "{r:?}");
    assert_eq!(r.restart_delays_secs, RESTART_DELAYS_SECS, "{r:?}");
    assert_eq!(r.reset_period_secs, Some(RESET_PERIOD_SECS), "{r:?}");
    assert!(r.non_crash_failures, "{r:?}");
    let mut got = r.required_privileges.clone();
    got.sort();
    let mut want: Vec<String> = REQUIRED_PRIVILEGES
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    want.sort();
    assert_eq!(got, want, "{r:?}");
}
