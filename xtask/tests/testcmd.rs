// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask test [--live]`: sistem değiştirmeyen testler ve kurulu servise karşı salt
//! okunur, `#[ignore]` testler.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_winsys::ServiceState;
use xtask::check::{LIVE_STEPS, require_running, test_steps};

#[test]
fn live_steps_are_read_only_ignored_tests() {
    let mut names = Vec::new();
    for (_, args) in LIVE_STEPS {
        assert!(args.contains(&"--ignored"), "{args:?}");
        let at = args.iter().position(|a| *a == "--test").unwrap();
        names.push(args[at + 1]);
        for a in args {
            assert!(
                !a.contains("dev-install") && !a.contains("dev_install"),
                "{args:?}"
            );
        }
    }
    names.sort_unstable();
    assert_eq!(names, ["live", "service"]);
}

#[test]
fn test_steps_match_check() {
    let names: Vec<&str> = test_steps().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["test", "test-ops"]);
}

#[test]
fn live_requires_running_service() {
    let err = format!("{:#}", require_running(None).unwrap_err());
    assert!(err.contains("dev-install"), "{err}");
    let stopped = ServiceState {
        binary_path: String::new(),
        running: false,
    };
    assert!(require_running(Some(&stopped)).is_err());
    let running = ServiceState {
        binary_path: String::new(),
        running: true,
    };
    require_running(Some(&running)).unwrap();
}
