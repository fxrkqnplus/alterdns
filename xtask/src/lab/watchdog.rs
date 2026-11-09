// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ayrı işlemde çalışan geri alma bekçisi (CLAUDE.md kural 1; ARCHITECTURE §6).
//!
//! Protokol (bekçinin stdin'i, satır başına bir komut):
//! - `RENEW <sn>`: kirayı yeniler (en çok 60 sn).
//! - `ENGINE <pid>`: süre dolunca sonlandırılacak motor işlemi.
//! - `DONE`: ebeveyn temiz bitirdi; geri alma yapılmaz.
//!
//! stdin kapanırsa (ebeveyn öldü veya Ctrl+C) ya da kira dolarsa bekçi motorları sonlandırır,
//! journal'ı geri alır ve sonucu `watchdog-report.json` dosyasına yazar.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command};

use serde::Serialize;

use super::journal::SystemOps;

/// Kiranın üst sınırı (CLAUDE.md: bekçi ≤ 60 sn).
pub const MAX_LEASE_SECS: u64 = 60;
/// `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`: konsol Ctrl+C'si bekçiye ulaşmaz.
pub const WATCHDOG_CREATION_FLAGS: u32 = 0x0000_0008 | 0x0000_0200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WatchdogOutcome {
    CleanExit,
    ParentGone,
    DeadlineExpired,
}

pub struct WatchdogHandle {
    child: Child,
    stdin: ChildStdin,
}

/// `CREATE_BREAKAWAY_FROM_JOB`: ebeveyn bir iş nesnesindeyse bekçi onunla birlikte ölmesin.
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

impl WatchdogHandle {
    /// Bekçiyi bu `xtask` ikilisinden başlatır.
    pub fn spawn(journal: &Path, results: &Path) -> anyhow::Result<WatchdogHandle> {
        Self::spawn_with(&std::env::current_exe()?, journal, results)
    }

    /// Bekçiyi verilen `xtask` ikilisinden başlatır (testler için).
    pub fn spawn_with(
        exe: &Path,
        journal: &Path,
        results: &Path,
    ) -> anyhow::Result<WatchdogHandle> {
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        let make = |flags: u32| {
            let mut cmd = Command::new(exe);
            cmd.arg("lab")
                .arg("watchdog")
                .arg("--journal")
                .arg(journal)
                .arg("--results")
                .arg(results)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(flags);
            cmd
        };
        // İş nesnesinden ayrılmaya izin yoksa bayraksız dene.
        let mut child = match make(WATCHDOG_CREATION_FLAGS | CREATE_BREAKAWAY_FROM_JOB).spawn() {
            Ok(c) => c,
            Err(_) => make(WATCHDOG_CREATION_FLAGS).spawn()?,
        };
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("bekçi stdin'i yok"))?;
        Ok(WatchdogHandle { child, stdin })
    }

    fn send(&mut self, line: &str) -> anyhow::Result<()> {
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Kirayı `secs` saniye yeniler (en çok 60).
    pub fn renew(&mut self, secs: u64) -> anyhow::Result<()> {
        if secs == 0 || secs > MAX_LEASE_SECS {
            anyhow::bail!("kira 1–{MAX_LEASE_SECS} sn olmalı: {secs}");
        }
        self.send(&format!("RENEW {secs}"))
    }

    /// Süre dolunca sonlandırılacak motor işlemini kaydeder.
    pub fn register_engine(&mut self, pid: u32) -> anyhow::Result<()> {
        self.send(&format!("ENGINE {pid}"))
    }

    /// Temiz bitiş: bekçi geri alma yapmadan çıkar.
    pub fn done(mut self) -> anyhow::Result<()> {
        self.send("DONE")?;
        self.child.wait()?;
        Ok(())
    }
}

enum LeaseMsg {
    Renew(u64),
    Engine(u32),
    Done,
}

fn parse_line(line: &str) -> Option<LeaseMsg> {
    let mut it = line.split_whitespace();
    match (it.next(), it.next()) {
        (Some("RENEW"), Some(n)) => n
            .parse()
            .ok()
            .filter(|n| *n > 0 && *n <= MAX_LEASE_SECS)
            .map(LeaseMsg::Renew),
        (Some("ENGINE"), Some(p)) => p.parse().ok().map(LeaseMsg::Engine),
        (Some("DONE"), None) => Some(LeaseMsg::Done),
        _ => None,
    }
}

fn kill_engine(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Bekçi döngüsü (`cargo xtask lab watchdog`). Kira varsayılan olarak 60 sn'dir.
pub fn run_watchdog(
    journal: &Path,
    results: &Path,
    ops: &dyn SystemOps,
    input: impl std::io::Read + Send + 'static,
) -> anyhow::Result<WatchdogOutcome> {
    use std::io::BufRead;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(input).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut deadline = Instant::now() + Duration::from_secs(MAX_LEASE_SECS);
    let mut engines: Vec<u32> = Vec::new();
    let outcome = loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(line) => match parse_line(&line) {
                Some(LeaseMsg::Renew(n)) => deadline = Instant::now() + Duration::from_secs(n),
                Some(LeaseMsg::Engine(pid)) => engines.push(pid),
                Some(LeaseMsg::Done) => return Ok(WatchdogOutcome::CleanExit),
                None => {}
            },
            Err(RecvTimeoutError::Disconnected) => break WatchdogOutcome::ParentGone,
            Err(RecvTimeoutError::Timeout) => break WatchdogOutcome::DeadlineExpired,
        }
    };
    for pid in &engines {
        kill_engine(*pid);
    }
    let report = match super::journal::Journal::open(journal) {
        Ok(mut j) => match j.revert_all(ops) {
            Ok(r) => {
                serde_json::json!({ "reverted": r.reverted, "conflicts": r.conflicts, "failed": r.failed })
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        },
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    };
    std::fs::create_dir_all(results)?;
    std::fs::write(
        results.join("watchdog-report.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "outcome": outcome,
            "engines_killed": engines,
            "journal": journal.display().to_string(),
            "revert": report,
        }))?,
    )?;
    Ok(outcome)
}

/// `cargo xtask lab watchdog --journal <yol> --results <dizin>` (içsel).
pub fn main(args: &[String]) -> anyhow::Result<()> {
    let get = |name: &str| {
        args.windows(2)
            .find(|w| w[0] == name)
            .map(|w| std::path::PathBuf::from(&w[1]))
            .ok_or_else(|| anyhow::anyhow!("{name} yok"))
    };
    run_watchdog(
        &get("--journal")?,
        &get("--results")?,
        &super::ops::RealOps,
        std::io::stdin(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_spawn_flags_detach_from_console() {
        assert_ne!(WATCHDOG_CREATION_FLAGS & 0x8, 0, "DETACHED_PROCESS");
        assert_ne!(
            WATCHDOG_CREATION_FLAGS & 0x200,
            0,
            "CREATE_NEW_PROCESS_GROUP"
        );
    }
}
