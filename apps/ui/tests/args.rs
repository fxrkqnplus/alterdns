// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Komut satırı kipleri ve oturum açılışında çıkış kuralı.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_core::UserConfig;
use alterdns_ui_app::args::{Mode, PageName, exit_at_logon, parse_args};

fn parse(args: &[&str]) -> Option<Mode> {
    parse_args(&args.iter().map(ToString::to_string).collect::<Vec<_>>()).ok()
}

#[test]
fn parse_args_modes() {
    assert_eq!(parse(&["--tray"]), Some(Mode::Tray { logon: false }));
    assert_eq!(
        parse(&["--tray", "--logon"]),
        Some(Mode::Tray { logon: true })
    );
    assert_eq!(
        parse(&["--logon", "--tray"]),
        Some(Mode::Tray { logon: true })
    );
    assert_eq!(parse(&[]), Some(Mode::Window { page: None }));
    assert_eq!(
        parse(&["--page", "settings"]),
        Some(Mode::Window {
            page: Some(PageName::Settings)
        })
    );
    for bad in [
        &["--x"][..],
        &["--logon"],
        &["--page"],
        &["--page", "nope"],
        &["--tray", "--tray"],
        &["--tray", "--page", "about"],
        &["--page", "about", "--page", "about"],
    ] {
        assert_eq!(parse(bad), None, "{bad:?}");
    }
}

#[test]
fn page_names_round_trip() {
    for page in PageName::ALL {
        assert_eq!(PageName::parse(page.as_str()), Some(page));
        assert_eq!(PageName::from_code(page.code()), Some(page));
    }
    assert_eq!(PageName::from_code(0), None);
}

#[test]
fn exit_at_logon_rules() {
    let on = UserConfig::default();
    let off = UserConfig {
        run_at_logon: false,
        ..UserConfig::default()
    };
    assert!(exit_at_logon(&Mode::Tray { logon: true }, &off));
    assert!(!exit_at_logon(&Mode::Tray { logon: true }, &on));
    assert!(!exit_at_logon(&Mode::Tray { logon: false }, &off));
    assert!(!exit_at_logon(&Mode::Window { page: None }, &off));
}
