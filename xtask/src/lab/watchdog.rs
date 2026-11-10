// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ayrı işlemde çalışan geri alma bekçisi (CLAUDE.md kural 1; ARCHITECTURE §6).
//!
//! Protokol (bekçinin stdin'i, satır başına bir komut):
//! - `RENEW <sn>`: kirayı yeniler (en çok 60 sn).
//! - `ENGINE <pid> <başlangıç>`: süre dolunca sonlandırılacak motor işlemi. Başlangıç zamanı
//!   (UTC tick) PID yeniden kullanımına karşı öldürmeden önce yeniden doğrulanır.
//! - `DONE`: ebeveyn bitirdi. Bekçi yine de kayıtlı motorları ve journal'da açık kalmış
//!   kayıtları denetler; ebeveynin kapattığı kayıtlara dokunulmaz.
//!
//! stdin kapanırsa (ebeveyn öldü veya Ctrl+C) ya da kira dolarsa bekçi motorları sonlandırır,
//! journal'ı geri alır ve sonucu `watchdog-report.json` dosyasına yazar.
//!
//! İş nesnesi: bekçi `CREATE_BREAKAWAY_FROM_JOB` ile ebeveynin iş nesnesinden ayrılmaya çalışır.
//! Ayrılamazsa (ör. `cargo run` altında; cargo iş nesnesi kapanınca içindekileri öldürür)
//! `outside_job()` yanlış döner ve sistem değiştiren senaryolar başlamaz.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::journal::SystemOps;
use super::ops;

/// Kiranın üst sınırı (CLAUDE.md: bekçi ≤ 60 sn).
pub const MAX_LEASE_SECS: u64 = 60;
/// `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`: konsol Ctrl+C'si bekçiye ulaşmaz.
pub const WATCHDOG_CREATION_FLAGS: u32 = 0x0000_0008 | 0x0000_0200;
/// `CREATE_BREAKAWAY_FROM_JOB`: ebeveyn bir iş nesnesindeyse bekçi onunla birlikte ölmesin.
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WatchdogOutcome {
    CleanExit,
    ParentGone,
    DeadlineExpired,
}

pub struct WatchdogHandle {
    child: Child,
    stdin: ChildStdin,
    outside_job: bool,
}

/// İşlemin başlangıç zamanı (UTC tick); işlem yoksa veya okunamazsa `None`.
fn process_start_ticks(pid: u32) -> Option<String> {
    ops::ps_query(&format!(
        "(Get-Process -Id {pid} -ErrorAction SilentlyContinue).StartTime.ToUniversalTime().Ticks"
    ))
    .ok()
    .filter(|v| !v.is_empty())
}

fn plausible_engine_pid(pid: u32) -> bool {
    pid > 4 && pid != std::process::id()
}

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
        // İş nesnesinden ayrılmaya izin yoksa bayraksız başlatılır, ama bu durum gizlenmez.
        let (mut child, outside_job) =
            match make(WATCHDOG_CREATION_FLAGS | CREATE_BREAKAWAY_FROM_JOB).spawn() {
                Ok(c) => (c, true),
                Err(_) => (make(WATCHDOG_CREATION_FLAGS).spawn()?, false),
            };
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("bekçi stdin'i yok"))?;
        Ok(WatchdogHandle {
            child,
            stdin,
            outside_job,
        })
    }

    /// Bekçi ebeveynin iş nesnesinden ayrılabildi mi? Ayrılamadıysa ebeveynin iş nesnesi
    /// kapanınca (ör. cargo çıkınca) bekçi de ölebilir.
    pub fn outside_job(&self) -> bool {
        self.outside_job
    }

    fn send(&mut self, line: &str) -> anyhow::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            anyhow::bail!("bekçi sonlanmış ({status})");
        }
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

    /// Süre dolunca sonlandırılacak motor işlemini (PID + başlangıç zamanı) kaydeder.
    pub fn register_engine(&mut self, pid: u32) -> anyhow::Result<()> {
        if !plausible_engine_pid(pid) {
            anyhow::bail!("geçersiz motor PID'i: {pid}");
        }
        let ticks = process_start_ticks(pid)
            .ok_or_else(|| anyhow::anyhow!("motor işlemi bulunamadı: {pid}"))?;
        self.send(&format!("ENGINE {pid} {ticks}"))
    }

    /// Bitiş: bekçi açık kayıtları denetleyip çıkar; çıkmasını bekler.
    pub fn done(mut self) -> anyhow::Result<()> {
        self.send("DONE")?;
        self.child.wait()?;
        Ok(())
    }
}

enum LeaseMsg {
    Renew(u64),
    Engine(u32, String),
    Done,
}

fn parse_line(line: &str) -> Option<LeaseMsg> {
    let mut it = line.split_whitespace();
    match (it.next(), it.next(), it.next(), it.next()) {
        (Some("RENEW"), Some(n), None, None) => n
            .parse()
            .ok()
            .filter(|n| *n > 0 && *n <= MAX_LEASE_SECS)
            .map(LeaseMsg::Renew),
        (Some("ENGINE"), Some(p), Some(t), None) => {
            let pid: u32 = p.parse().ok().filter(|p| plausible_engine_pid(*p))?;
            t.chars()
                .all(|c| c.is_ascii_digit())
                .then(|| LeaseMsg::Engine(pid, t.to_string()))
        }
        (Some("DONE"), None, None, None) => Some(LeaseMsg::Done),
        _ => None,
    }
}

/// Motoru yalnızca PID hâlâ aynı işlemse sonlandırır ve çıkmasını bekler (≤ 5 sn).
fn kill_engine(pid: u32, ticks: &str) -> String {
    match process_start_ticks(pid) {
        None => "zaten yok".into(),
        Some(t) if t != ticks => "PID başka bir işleme geçmiş; dokunulmadı".into(),
        Some(_) => {
            let status = Command::new(ops::system32("taskkill.exe"))
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let deadline = Instant::now() + Duration::from_secs(5);
            while process_start_ticks(pid).as_deref() == Some(ticks) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
            let gone = process_start_ticks(pid).as_deref() != Some(ticks);
            format!(
                "taskkill: {}; işlem {}",
                status.map_or_else(|e| e.to_string(), |s| s.to_string()),
                if gone {
                    "sonlandı"
                } else {
                    "hâlâ çalışıyor"
                }
            )
        }
    }
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

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(input).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut deadline = Instant::now() + Duration::from_secs(MAX_LEASE_SECS);
    let mut engines: Vec<(u32, String)> = Vec::new();
    let outcome = loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(line) => match parse_line(&line) {
                Some(LeaseMsg::Renew(n)) => deadline = Instant::now() + Duration::from_secs(n),
                Some(LeaseMsg::Engine(pid, ticks)) => engines.push((pid, ticks)),
                Some(LeaseMsg::Done) => break WatchdogOutcome::CleanExit,
                None => {}
            },
            Err(RecvTimeoutError::Disconnected) => break WatchdogOutcome::ParentGone,
            Err(RecvTimeoutError::Timeout) => break WatchdogOutcome::DeadlineExpired,
        }
    };
    // Motorlar önce durur ki geri alma sırasında yeniden değişiklik yapamasınlar.
    let killed: Vec<serde_json::Value> = engines
        .iter()
        .map(|(pid, ticks)| serde_json::json!({ "pid": pid, "result": kill_engine(*pid, ticks) }))
        .collect();
    let report = match super::journal::Journal::open(journal) {
        Ok(mut j) => match j.revert_all(ops) {
            Ok(r) => {
                serde_json::json!({ "reverted": r.reverted, "conflicts": r.conflicts, "failed": r.failed })
            }
            Err(e) => serde_json::json!({ "error": format!("{e:#}") }),
        },
        Err(e) => serde_json::json!({ "error": format!("{e:#}") }),
    };
    std::fs::create_dir_all(results)?;
    std::fs::write(
        results.join("watchdog-report.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "outcome": outcome,
            "engines": killed,
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

    #[test]
    fn engine_line_requires_pid_and_start_time() {
        assert!(matches!(
            parse_line("ENGINE 1234 638000000000000000"),
            Some(LeaseMsg::Engine(1234, _))
        ));
        for bad in [
            "ENGINE 1234",
            "ENGINE 4 638000000000000000",
            "ENGINE 0 1",
            "ENGINE 1234 abc",
            "ENGINE 1234 1 x",
            "RENEW 61",
            "RENEW 0",
            "DONE now",
        ] {
            assert!(parse_line(bad).is_none(), "{bad:?}");
        }
        let own = format!("ENGINE {} 1", std::process::id());
        assert!(parse_line(&own).is_none(), "bekçi kendini öldürmez");
    }
}
