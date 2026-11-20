// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ayrı işlemde çalışan geri alma bekçisi (`docs/ARCHITECTURE.md` §6, K3).
//!
//! Protokol (bekçinin stdin'i, satır başına bir komut):
//! - `RENEW <sn>`: kirayı şu andan `sn` saniye sonrasına taşır (1–60). İlk kira 60 sn'dir.
//! - `DONE`: ebeveyn işini bitirdi (commit etti ya da geri aldı); bekçi dokunmadan çıkar.
//!
//! stdin kapanırsa (ebeveyn öldü ya da tutamacı bıraktı) veya kira dolarsa bekçi txn'i açılış
//! kurtarmasıyla aynı kuralla geri alır: commit edilmiş `Persistent` txn'e dokunulmaz.
//!
//! Ebeveyn hâlâ yaşıyor ve dosyayı tutuyorsa (kira doldu ama işlem askıda) bekçi `LOCK_WAIT`
//! kadar bekler; dosya bırakılmazsa geri alma bir sonraki açılış kurtarmasına kalır ve raporda
//! `InUse` yazar. Askıdaki ebeveynin ve motorun tutamaçla izlenmesi (K3) Faz 2'de gelir.
//!
//! Sonuç `<journal dizini>\<txn>.watchdog.json` dosyasına yazılır. Yollar txn kimliğinden
//! türetilir; komut satırından yol alınmaz (S-9).

use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use alterdns_errors::{AdnsError, AdnsErrorCode};
use serde::{Deserialize, Serialize};

use crate::file::{LOCK_WAIT, txn_path};
use crate::model::{now_ms, valid_txn_id};
use crate::ops::{SystemOps, Targets};
use crate::recover::{FileRecovery, recover_file};

/// Kiranın üst sınırı (bekçi ≤ 60 sn).
pub const MAX_LEASE_SECS: u64 = 60;
/// Bekçi ikilisi; çağıranla aynı dizinde aranır.
pub const MAINT_EXE: &str = "alterdns-maint.exe";

/// Konsol Ctrl+C'si bekçiye ulaşmasın: `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`.
const WATCHDOG_CREATION_FLAGS: u32 = 0x0000_0008 | 0x0000_0200;
/// Ebeveyn bir iş nesnesindeyse bekçi onunla birlikte ölmesin.
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchdogOutcome {
    CleanExit,
    ParentGone,
    DeadlineExpired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseMsg {
    Renew(u64),
    Done,
}

/// Bekçinin txn için yaptığı.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WatchdogStatus {
    /// Ebeveyn `DONE` dedi.
    NotNeeded,
    Reverted {
        reverted: Vec<u32>,
        conflicts: Vec<u32>,
        failed: Vec<(u32, String)>,
    },
    /// Commit edilmiş kalıcı txn; dokunulmadı.
    Kept,
    /// Dosya hâlâ canlı bir işlemde; açılış kurtarmasına kaldı.
    InUse,
    Error {
        code: String,
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchdogReport {
    pub txn_id: String,
    pub outcome: WatchdogOutcome,
    pub status: WatchdogStatus,
    pub ts: u64,
}

/// `<dizin>\<txn>.watchdog.json`
pub fn report_path(dir: &Path, txn_id: &str) -> PathBuf {
    dir.join(format!("{txn_id}.watchdog.json"))
}

pub fn parse_line(line: &str) -> Option<LeaseMsg> {
    let mut it = line.split_whitespace();
    match (it.next(), it.next(), it.next()) {
        (Some("RENEW"), Some(n), None) => n
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=MAX_LEASE_SECS).contains(n))
            .map(LeaseMsg::Renew),
        (Some("DONE"), None, None) => Some(LeaseMsg::Done),
        _ => None,
    }
}

fn revert_err(msg: impl Into<String>) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SysRevertFailed).with_source(std::io::Error::other(msg.into()))
}

/// Bekçi döngüsü (`alterdns-maint watchdog --txn <id>`).
pub fn run_watchdog(
    dir: &Path,
    txn_id: &str,
    ops: &dyn SystemOps,
    targets: &Targets,
    input: impl std::io::Read + Send + 'static,
) -> Result<WatchdogOutcome, AdnsError> {
    run(dir, txn_id, ops, targets, input, LOCK_WAIT)
}

fn run(
    dir: &Path,
    txn_id: &str,
    ops: &dyn SystemOps,
    targets: &Targets,
    input: impl std::io::Read + Send + 'static,
    lock_wait: Duration,
) -> Result<WatchdogOutcome, AdnsError> {
    if !valid_txn_id(txn_id) {
        return Err(revert_err(format!("geçersiz txn kimliği: {txn_id:?}")));
    }
    let outcome = wait_for_parent(input);
    let status = match outcome {
        WatchdogOutcome::CleanExit => WatchdogStatus::NotNeeded,
        _ => recover_with_wait(&txn_path(dir, txn_id), ops, targets, lock_wait),
    };
    let report = WatchdogReport {
        txn_id: txn_id.to_string(),
        outcome,
        status,
        ts: now_ms(),
    };
    let text = serde_json::to_string_pretty(&report).map_err(|e| revert_err(e.to_string()))?;
    std::fs::write(report_path(dir, txn_id), text).map_err(|e| revert_err(e.to_string()))?;
    Ok(outcome)
}

fn wait_for_parent(input: impl std::io::Read + Send + 'static) -> WatchdogOutcome {
    use std::sync::mpsc::{self, RecvTimeoutError};

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(input).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut deadline = Instant::now() + Duration::from_secs(MAX_LEASE_SECS);
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(line) => match parse_line(&line) {
                Some(LeaseMsg::Renew(n)) => deadline = Instant::now() + Duration::from_secs(n),
                Some(LeaseMsg::Done) => return WatchdogOutcome::CleanExit,
                None => {}
            },
            Err(RecvTimeoutError::Disconnected) => return WatchdogOutcome::ParentGone,
            Err(RecvTimeoutError::Timeout) => return WatchdogOutcome::DeadlineExpired,
        }
    }
}

fn recover_with_wait(
    path: &Path,
    ops: &dyn SystemOps,
    targets: &Targets,
    lock_wait: Duration,
) -> WatchdogStatus {
    let start = Instant::now();
    loop {
        match recover_file(path, ops, targets) {
            FileRecovery::InUse if start.elapsed() < lock_wait => {
                std::thread::sleep(Duration::from_millis(100));
            }
            FileRecovery::InUse => return WatchdogStatus::InUse,
            FileRecovery::Kept => return WatchdogStatus::Kept,
            FileRecovery::Reverted(r) => {
                return WatchdogStatus::Reverted {
                    reverted: r.reverted,
                    conflicts: r.conflicts,
                    failed: r.failed,
                };
            }
            FileRecovery::Error(e) => {
                return WatchdogStatus::Error {
                    code: e.code().as_str().to_string(),
                    detail: std::error::Error::source(&e).map(|s| s.to_string()),
                };
            }
        }
    }
}

/// Ebeveyn tarafı: bekçi işlemi ve stdin'i. Tutamaç `done` çağrılmadan düşerse stdin kapanır ve
/// bekçi txn'i geri alır.
pub struct WatchdogHandle {
    child: Child,
    stdin: ChildStdin,
    outside_job: bool,
}

impl WatchdogHandle {
    /// Bu ikilinin yanındaki `alterdns-maint.exe`'yi başlatır.
    pub fn spawn(txn_id: &str) -> std::io::Result<WatchdogHandle> {
        let exe = std::env::current_exe()?;
        let dir = exe.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "ikilinin dizini yok")
        })?;
        Self::spawn_with(&dir.join(MAINT_EXE), txn_id, &[])
    }

    /// Verilen bekçi ikilisini başlatır. Çağıran yükseltilmişse ikilinin dizini korumalı olmalıdır
    /// (yalnızca SYSTEM ve Administrators yazabilir): kullanıcının yazabildiği bir dizinden
    /// yönetici yetkisiyle ikili çalıştırılmaz (S-1).
    pub fn spawn_with(
        exe: &Path,
        txn_id: &str,
        envs: &[(&str, &OsStr)],
    ) -> std::io::Result<WatchdogHandle> {
        use std::os::windows::process::CommandExt;

        if !valid_txn_id(txn_id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("geçersiz txn kimliği: {txn_id:?}"),
            ));
        }
        if !exe.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("bekçi ikilisi yok: {}", exe.display()),
            ));
        }
        if alterdns_winsys::is_elevated()? {
            let dir = exe.parent().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "bekçi dizini yok")
            })?;
            alterdns_winsys::verify_protected_dir(dir).map_err(|p| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("bekçi dizini korumalı değil ({p:?}): {}", dir.display()),
                )
            })?;
        }
        let make = |flags: u32| {
            let mut cmd = Command::new(exe);
            cmd.arg("watchdog")
                .arg("--txn")
                .arg(txn_id)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(flags);
            for (k, v) in envs {
                cmd.env(k, v);
            }
            cmd
        };
        // İş nesnesinden ayrılmaya izin yoksa bayraksız başlatılır; durum `outside_job`'da görünür.
        let (mut child, outside_job) =
            match make(WATCHDOG_CREATION_FLAGS | CREATE_BREAKAWAY_FROM_JOB).spawn() {
                Ok(c) => (c, true),
                Err(_) => (make(WATCHDOG_CREATION_FLAGS).spawn()?, false),
            };
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err(std::io::Error::other("bekçi stdin'i yok"));
        };
        Ok(WatchdogHandle {
            child,
            stdin,
            outside_job,
        })
    }

    /// Bekçi ebeveynin iş nesnesinden ayrılabildi mi? Ayrılamadıysa iş nesnesi kapanınca bekçi de
    /// ölebilir.
    pub fn outside_job(&self) -> bool {
        self.outside_job
    }

    fn send(&mut self, line: &str) -> std::io::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            return Err(std::io::Error::other(format!("bekçi sonlanmış ({status})")));
        }
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()
    }

    /// Kirayı `secs` saniye yeniler (1–60).
    pub fn renew(&mut self, secs: u64) -> std::io::Result<()> {
        if !(1..=MAX_LEASE_SECS).contains(&secs) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("kira 1–{MAX_LEASE_SECS} sn olmalı: {secs}"),
            ));
        }
        self.send(&format!("RENEW {secs}"))
    }

    /// Bitiş: bekçi dokunmadan çıkar; çıkmasını bekler.
    pub fn done(mut self) -> std::io::Result<ExitStatus> {
        self.send("DONE")?;
        let WatchdogHandle {
            mut child, stdin, ..
        } = self;
        drop(stdin);
        child.wait()
    }

    /// stdin'i kapatır (ebeveynin ölümü gibi): bekçi txn'i geri alıp çıkar.
    pub fn close_input(self) -> Child {
        let WatchdogHandle { child, stdin, .. } = self;
        drop(stdin);
        child
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::model::{Kind, Lifetime, Snapshot};
    use crate::txn::Txn;

    struct NoWrite;

    impl SystemOps for NoWrite {
        fn read(&self, _kind: Kind, _target: &str) -> Result<Snapshot, String> {
            Ok(Snapshot::Absent)
        }
        fn write(&self, _kind: Kind, _target: &str, _value: &Snapshot) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn live_txn_is_left_for_startup_recovery() {
        let d = std::env::temp_dir().join(format!("alterdns-wdunit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let targets = Targets::product(PathBuf::from(r"C:\Program Files\AlterDNS"));
        let mut live = Txn::begin(&d, "test", Lifetime::Session, targets.clone()).unwrap();
        live.apply(
            &NoWrite,
            Kind::RegistryValue,
            r"HKLM\SOFTWARE\AlterDNS\Bekci\Canli",
            Snapshot::Present("1".into()),
        )
        .unwrap();
        let id = live.id().to_string();
        let (reader, writer) = std::io::pipe().unwrap();
        drop(writer);
        let out = run(
            &d,
            &id,
            &NoWrite,
            &targets,
            reader,
            Duration::from_millis(200),
        )
        .unwrap();
        assert_eq!(out, WatchdogOutcome::ParentGone);
        let report: WatchdogReport =
            serde_json::from_str(&std::fs::read_to_string(report_path(&d, &id)).unwrap()).unwrap();
        assert_eq!(report.status, WatchdogStatus::InUse);
        drop(live);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn creation_flags_detach_from_console() {
        assert_ne!(WATCHDOG_CREATION_FLAGS & 0x8, 0, "DETACHED_PROCESS");
        assert_ne!(
            WATCHDOG_CREATION_FLAGS & 0x200,
            0,
            "CREATE_NEW_PROCESS_GROUP"
        );
    }
}
