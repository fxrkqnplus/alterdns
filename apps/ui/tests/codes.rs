// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arayüzün ürettiği hata ve bildirim kodları: başlatma hatası, bildirim gösterilememesi ve
//! servisin yeniden başlaması. Metinler katalogdan, iki dilde de dolu.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_core::Lang;
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ui_app::common::{notification_error, ui_error};
use alterdns_ui_app::status::Notice;

fn rendered(e: &AdnsError) {
    for lang in [Lang::Tr, Lang::En] {
        let r = e.render(lang);
        assert!(!r.title.is_empty() && !r.message.is_empty(), "{lang:?}");
    }
}

#[test]
fn startup_failure_is_ui_001() {
    let e = ui_error(std::io::Error::other("pencere oluşturulamadı"));
    assert_eq!(e.code(), AdnsErrorCode::UiStartFailed);
    rendered(&e);
}

#[test]
fn notification_failure_is_ui_002() {
    let e = notification_error(std::io::Error::other("tepsi simgesi görünmüyor"));
    assert_eq!(e.code(), AdnsErrorCode::UiNotificationsUnavailable);
    rendered(&e);
}

#[test]
fn service_restart_notice_is_svc_002() {
    let code = Notice::ServiceRestarted.code();
    assert_eq!(code, AdnsErrorCode::SvcRestarted);
    rendered(&AdnsError::new(code));
}
