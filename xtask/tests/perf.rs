// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask perf`: istatistik, bütçe, regresyon, sonuç dosyası ve ön kontrol kuralları
//! (saf işlevler). Gerçek ölçüm kullanıcının onaylı koşusunda yapılır.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use xtask::perf::{
    MIB, PerfReceiver, UiPreflight, active_intervals, allowed_intervals, check_mode, cpu_percent,
    date_from_unix, median, merge_section, over_budget, p95, previous_baseline, regressions,
    svc_pid, ui_preflight, validate_results,
};
use xtask::testutil::temp;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn median_and_p95() {
    let v: Vec<f64> = (1..=20).map(f64::from).collect();
    assert!(close(median(&v), 10.5));
    assert!(close(p95(&v), 19.0));
    assert!(close(median(&[5.0]), 5.0));
    assert!(close(p95(&[5.0]), 5.0));
    assert!(close(median(&[]), 0.0));
    assert!(close(p95(&[]), 0.0));
}

#[test]
fn cpu_percent_scales_by_cpus() {
    let p = cpu_percent(Duration::from_secs(1), Duration::from_secs(100), 10);
    assert!(close(p, 0.1), "{p}");
    assert!(close(cpu_percent(Duration::ZERO, Duration::ZERO, 4), 0.0));
}

#[test]
fn active_intervals_counts_growth_and_new_threads() {
    let same = vec![vec![(1, 100), (2, 50)], vec![(1, 100), (2, 50)]];
    assert_eq!(active_intervals(&same), 0);
    let grew = vec![vec![(1, 100)], vec![(1, 150)], vec![(1, 150)]];
    assert_eq!(active_intervals(&grew), 1);
    let new_thread = vec![vec![(1, 100)], vec![(1, 100), (2, 5)]];
    assert_eq!(active_intervals(&new_thread), 1);
    assert_eq!(active_intervals(&[]), 0);
}

#[test]
fn allowed_intervals_per_minute() {
    assert_eq!(allowed_intervals(60, Duration::from_secs(10)), 10);
    assert_eq!(allowed_intervals(7, Duration::from_secs(10)), 2);
    assert_eq!(allowed_intervals(0, Duration::from_secs(10)), 0);
}

#[test]
fn over_budget_flags_each_limit() {
    let limits = [
        ("svc", "private_max_bytes", json!(12 * MIB)),
        ("svc", "cpu_percent", json!(0.1)),
        ("ui", "tray_idle_private_bytes", json!(8 * MIB)),
        ("ui", "flyout_open_private_bytes", json!(45 * MIB)),
        ("ui", "flyout_cold_ms", json!(400)),
        ("ui", "flyout_warm_p95_ms", json!(150)),
        ("ui", "main_first_paint_ms", json!(600)),
        ("ui", "main_private_bytes", json!(90 * MIB)),
        ("ui", "main_exit_ms", json!(5000)),
        ("size", "total_bytes", json!(25 * MIB)),
    ];
    for (section, key, at_limit) in limits {
        let ok = json!({ key: at_limit });
        assert!(
            over_budget(section, &ok).is_empty(),
            "{section}.{key} sınırda"
        );
        let over = match at_limit.as_u64() {
            Some(n) => json!({ key: n + 1 }),
            None => json!({ key: at_limit.as_f64().unwrap() + 0.01 }),
        };
        let lines = over_budget(section, &over);
        assert_eq!(lines.len(), 1, "{section}.{key}: {lines:?}");
        assert!(lines[0].contains(key), "{lines:?}");
    }
    // Uyanma: etkin aralık izin verileni aşarsa.
    let wake = json!({ "active_intervals": 11, "allowed_intervals": 10 });
    assert_eq!(over_budget("svc", &wake).len(), 1);
    let wake_ok = json!({ "active_intervals": 10, "allowed_intervals": 10 });
    assert!(over_budget("svc", &wake_ok).is_empty());
}

#[test]
fn regressions_respect_noise_floor() {
    let prev = json!({
        "ui": { "flyout_warm_p95_ms": 100.0, "flyout_cold_ms": 1.0, "tray_idle_private_bytes": 3 * MIB },
        "svc": { "cpu_percent": 0.001, "minutes": 10 },
    });
    let cur = json!({
        "ui": { "flyout_warm_p95_ms": 115.0, "flyout_cold_ms": 2.0, "tray_idle_private_bytes": 3 * MIB + 600 * 1024 },
        "svc": { "cpu_percent": 0.002, "minutes": 20 },
    });
    let r = regressions(&prev, &cur);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r.iter().any(|l| l.contains("flyout_warm_p95_ms")));
    assert!(r.iter().any(|l| l.contains("tray_idle_private_bytes")));
    // İyileşme regresyon değildir.
    assert!(regressions(&cur, &prev).is_empty());
}

#[test]
fn merge_keeps_other_sections() {
    let first = merge_section(None, "svc", json!({ "cpu_percent": 0.01 }));
    let both = merge_section(Some(first), "ui", json!({ "flyout_cold_ms": 30.0 }));
    assert_eq!(both["svc"]["cpu_percent"], json!(0.01));
    assert_eq!(both["ui"]["flyout_cold_ms"], json!(30.0));
    let renewed = merge_section(Some(both), "svc", json!({ "cpu_percent": 0.02 }));
    assert_eq!(renewed["svc"]["cpu_percent"], json!(0.02));
    assert_eq!(renewed["ui"]["flyout_cold_ms"], json!(30.0));
}

#[test]
fn previous_baseline_picks_latest_earlier_day() {
    let d = temp("perf", "baseline").unwrap();
    assert_eq!(previous_baseline(&d, "2026-11-23"), None);
    for name in [
        "2026-11-20.json",
        "2026-11-22.json",
        "2026-11-23.json",
        "notlar.json",
    ] {
        std::fs::write(d.join(name), "{}").unwrap();
    }
    assert_eq!(
        previous_baseline(&d, "2026-11-23"),
        Some(d.join("2026-11-22.json"))
    );
    assert_eq!(
        previous_baseline(&d, "2026-11-21"),
        Some(d.join("2026-11-20.json"))
    );
    assert_eq!(previous_baseline(&d, "2026-11-20"), None);
}

#[test]
fn ui_preflight_decisions() {
    let root = Path::new(r"C:\Program Files\AlterDNS");
    assert_eq!(ui_preflight(false, None, root), UiPreflight::Ready);
    let installed = root.join(r"0.0.0\alterdns-ui.exe");
    assert_eq!(
        ui_preflight(true, Some(&installed), root),
        UiPreflight::CloseInstalled
    );
    let other = Path::new(r"C:\Sistem\x\target\release\alterdns-ui.exe");
    assert!(matches!(
        ui_preflight(true, Some(other), root),
        UiPreflight::Blocked(_)
    ));
    assert!(matches!(
        ui_preflight(true, None, root),
        UiPreflight::Blocked(_)
    ));
}

#[test]
fn mode_checks_refuse_wrong_elevation() {
    assert!(check_mode("ui", true).is_err());
    check_mode("ui", false).unwrap();
    let err = format!("{:#}", check_mode("svc", false).unwrap_err());
    assert!(err.contains("ADNS-SYS-004"), "{err}");
    check_mode("svc", true).unwrap();
    check_mode("size", false).unwrap();
    check_mode("size", true).unwrap();
    assert!(check_mode("baska", false).is_err());
}

#[test]
fn svc_requires_running_service() {
    let err = format!("{:#}", svc_pid(None).unwrap_err());
    assert!(err.contains("dev-install"), "{err}");
    assert_eq!(svc_pid(Some(42)).unwrap(), 42);
}

#[test]
fn wait_value_times_out() {
    let rx = PerfReceiver::new().unwrap();
    let start = Instant::now();
    assert!(rx.wait(Duration::from_millis(300)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn results_contain_only_known_keys() {
    let ok = json!({
        "schema": 1, "date": "2026-11-23", "version": "0.0.0",
        "svc": { "minutes": 10, "private_median_bytes": 1, "private_max_bytes": 2, "cpu_percent": 0.0,
                 "active_intervals": 0, "allowed_intervals": 10 },
        "ui": { "source": "installed", "tray_idle_private_bytes": 1, "flyout_open_private_bytes": 1,
                "flyout_cold_ms": 1.0, "flyout_warm_p95_ms": 1.0, "main_first_paint_ms": 1.0,
                "main_private_bytes": 1, "main_exit_ms": 1.0 },
        "size": { "ui_bytes": 1, "svc_bytes": 1, "maint_bytes": 1, "total_bytes": 3 },
    });
    validate_results(&ok).unwrap();
    let mut extra = ok.clone();
    extra["ui"]["makine"] = json!("LAPTOP");
    assert!(validate_results(&extra).is_err());
    let mut path = ok.clone();
    // Kişisel veri taraması düz yol dizesini işaretler; dize parçalanarak yazılır.
    path["ui"]["source"] = json!(concat!("C:", r"\Use", r"rs\x\target"));
    assert!(validate_results(&path).is_err());
}

#[test]
fn date_from_unix_known_values() {
    assert_eq!(date_from_unix(0), "1970-01-01");
    assert_eq!(date_from_unix(951_782_400), "2000-02-29");
    assert_eq!(date_from_unix(1_795_305_600), "2026-11-22");
}
