// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! T-JRN-002: kayıt `Pending` iken işlem öldürülür (`TerminateProcess`); açılış kurtarması
//! geri alır. Test ikilisi kendini işaretli bir alt işlem olarak başlatır.

#![cfg(feature = "test-ops")]
// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use alterdns_journal::{
    JournalFile, Kind, Lifetime, RealOps, Snapshot, State, SystemOps, Targets, Txn, list,
    recover_at_startup,
};

mod common;

const DIR_MARKER: &str = "ALTERDNS_JOURNAL_KILL_DIR";
const MODE_MARKER: &str = "ALTERDNS_JOURNAL_KILL_MODE";

fn targets() -> Targets {
    Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"))
}

/// Yazmanın ortasında asılı kalan işlemler: `after` modunda sistem değeri yazıldıktan, `before`
/// modunda yazılmadan önce "HAZIR" basar ve öldürülmeyi bekler (en çok 60 sn).
struct HangingOps {
    write_first: bool,
}

impl SystemOps for HangingOps {
    fn read(&self, kind: Kind, target: &str) -> Result<Snapshot, String> {
        RealOps.read(kind, target)
    }
    fn write(&self, kind: Kind, target: &str, value: &Snapshot) -> Result<(), String> {
        if self.write_first {
            RealOps.write(kind, target, value)?;
        }
        println!("HAZIR");
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_secs(60));
        std::process::exit(3);
    }
}

/// Alt işlem: işaret yoksa hiçbir şey yapmaz.
#[test]
fn kill_child_helper() {
    let Some(dir) = std::env::var_os(DIR_MARKER) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let write_first = std::env::var(MODE_MARKER).as_deref() == Ok("after");
    let mut txn = Txn::begin(&dir.join("journal"), "kill", Lifetime::Session, targets()).unwrap();
    let file = dir.join("deger.txt");
    let _ = txn.apply(
        &HangingOps { write_first },
        Kind::FileValue,
        &file.to_string_lossy(),
        Snapshot::Present("after".into()),
    );
}

/// Alt işlemi başlatır, "HAZIR"ı bekler ve öldürür.
fn run_and_kill(dir: &Path, mode: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "kill_child_helper", "--nocapture"])
        .env(DIR_MARKER, dir)
        .env(MODE_MARKER, mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            if line.contains("HAZIR") {
                let _ = tx.send(());
            }
        }
    });
    if rx.recv_timeout(Duration::from_secs(30)).is_err() {
        let _ = child.kill();
        panic!("alt işlem hazır olmadı");
    }
    child.kill().unwrap();
    child.wait().unwrap();
}

fn check(mode: &str, value_at_kill: &str) {
    let d = common::temp("kill", mode);
    let journal = d.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let file = d.join("deger.txt");
    std::fs::write(&file, "before").unwrap();

    run_and_kill(&d, mode);

    assert_eq!(std::fs::read_to_string(&file).unwrap(), value_at_kill);
    let paths = list(&journal).unwrap();
    assert_eq!(paths.len(), 1);
    let j = JournalFile::open(&paths[0]).unwrap().unwrap();
    assert_eq!(j.records()[0].state, State::Pending);
    drop(j);

    let report = recover_at_startup(&journal, &RealOps, &targets());
    assert!(report.clean(), "{report:?}");
    assert_eq!(report.txns.len(), 1, "{report:?}");
    assert_eq!(report.txns[0].1.reverted, [1]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "before");
}

#[test]
fn t_jrn_002_pending_record_reverted_after_kill() {
    // Sistem değeri yazıldı, `Applied` satırı yazılamadan öldürüldü.
    check("after", "after");
}

#[test]
fn killed_before_system_write_closes_pending_record() {
    // `Pending` diske yazıldı, sistem değeri değişmeden öldürüldü.
    check("before", "before");
}
