// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bekçi entegrasyon testleri: gerçek `xtask lab watchdog` işlemi, geçici dizindeki bir
//! test dosyası (`FileValue`) üzerinde. Sisteme dokunmaz.

// Bütün dosya test kodudur; yardımcı işlevler `#[test]` dışında olduğu için açıkça izin verilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use xtask::lab::journal::{Journal, OpKind};
use xtask::lab::ops::RealOps;
use xtask::lab::watchdog::WatchdogHandle;

struct Fixture {
    file: PathBuf,
    journal: PathBuf,
    results: PathBuf,
}

fn fixture(name: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("alterdns-wd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("value.txt");
    std::fs::write(&file, "before").unwrap();
    let mut j = Journal::create(&dir.join("journal"), "run1").unwrap();
    j.apply(&RealOps, OpKind::FileValue { path: file.clone() }, "after")
        .unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "after");
    Fixture {
        file,
        journal: j.path().to_path_buf(),
        results: dir.join("results"),
    }
}

fn spawn(f: &Fixture) -> WatchdogHandle {
    WatchdogHandle::spawn_with(
        Path::new(env!("CARGO_BIN_EXE_xtask")),
        &f.journal,
        &f.results,
    )
    .unwrap()
}

fn wait_for(mut cond: impl FnMut() -> bool, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

fn content(f: &Fixture) -> String {
    std::fs::read_to_string(&f.file).unwrap_or_default()
}

fn outcome(f: &Fixture) -> String {
    let report =
        std::fs::read_to_string(f.results.join("watchdog-report.json")).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&report)
        .ok()
        .and_then(|v| v["outcome"].as_str().map(str::to_string))
        .unwrap_or_default()
}

#[test]
fn watchdog_reverts_after_deadline() {
    let f = fixture("deadline");
    let mut w = spawn(&f);
    w.renew(2).unwrap();
    assert_eq!(content(&f), "after");
    assert!(
        wait_for(|| content(&f) == "before", Duration::from_secs(6)),
        "süre dolunca geri alınmadı"
    );
    assert!(wait_for(
        || outcome(&f) == "DeadlineExpired",
        Duration::from_secs(2)
    ));
}

#[test]
fn watchdog_reverts_when_parent_dies() {
    let f = fixture("parent");
    let mut w = spawn(&f);
    w.renew(30).unwrap();
    drop(w); // stdin kapanır: ebeveynin ölmesiyle aynı etki
    assert!(
        wait_for(|| content(&f) == "before", Duration::from_secs(2)),
        "ebeveyn gidince geri alınmadı"
    );
    assert!(wait_for(
        || outcome(&f) == "ParentGone",
        Duration::from_secs(2)
    ));
}

#[test]
fn watchdog_clean_done_does_not_revert() {
    let f = fixture("done");
    let mut w = spawn(&f);
    w.renew(30).unwrap();
    w.done().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(content(&f), "after");
    let mut j = Journal::open(&f.journal).unwrap();
    j.revert_all(&RealOps).unwrap();
}

#[test]
fn watchdog_rejects_lease_over_60s() {
    let f = fixture("lease");
    let mut w = spawn(&f);
    assert!(w.renew(61).is_err());
    w.renew(30).unwrap();
    w.done().unwrap();
    let mut j = Journal::open(&f.journal).unwrap();
    j.revert_all(&RealOps).unwrap();
}

#[test]
fn watchdog_kills_registered_engine() {
    let f = fixture("engine");
    let mut engine = Command::new("ping")
        .args(["-n", "60", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut w = spawn(&f);
    w.register_engine(engine.id()).unwrap();
    w.renew(2).unwrap();
    assert!(
        wait_for(
            || matches!(engine.try_wait(), Ok(Some(_))),
            Duration::from_secs(6)
        ),
        "motor işlemi sonlandırılmadı"
    );
    assert!(wait_for(|| content(&f) == "before", Duration::from_secs(2)));
}
