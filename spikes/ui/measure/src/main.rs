// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI spike ölçüm aracı (T-SPK-001; TESTING.md §5).
//!
//! Kullanım:
//! `ui-measure --label <ad> --exe <yol> [--arg <a>]... [--env K=V]... [--cold 5] [--warm 50] [--idle 10] [--out <json>]`
//!
//! Ölçülenler: tepsi boşta bellek (PrivateUsage, WorkingSet), soğuk açılış (her biri yeni işlem),
//! sıcak aç/kapa (p50/p95), flyout açıkken bellek, kapandıktan 5 sn sonra bellek, ikili boyutu.
//! Açılış süresi = ölçüm aracının aç/kapa iletisini göndermeden hemen önceki QPC ile
//! uygulamanın ilk kare sonrasında yazdığı `READY <qpc>` arasındaki fark.

mod stats;

use std::io::{BufRead, BufReader};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{Context, bail};
use serde_json::{Value, json};
use ui_common::{CMD_CLOSE, CMD_OPEN, MEASURE_ENV, TOGGLE_MESSAGE, TRAY_CLASS, qpc, qpc_frequency};
use windows::Win32::Foundation::{HANDLE, LPARAM, WPARAM};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, RegisterWindowMessageW};
use windows::core::PCWSTR;

const LINE_TIMEOUT: Duration = Duration::from_secs(20);

struct Options {
    label: String,
    exe: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cold: usize,
    warm: usize,
    idle_secs: u64,
    out: Option<String>,
}

fn parse() -> anyhow::Result<Options> {
    let mut o = Options {
        label: String::new(),
        exe: String::new(),
        args: Vec::new(),
        env: Vec::new(),
        cold: 5,
        warm: 50,
        idle_secs: 10,
        out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let v = it.next().with_context(|| format!("{k} için değer yok"))?;
        match k.as_str() {
            "--label" => o.label = v,
            "--exe" => o.exe = v,
            "--arg" => o.args.push(v),
            "--env" => {
                let (a, b) = v.split_once('=').context("--env K=V biçiminde olmalı")?;
                o.env.push((a.into(), b.into()));
            }
            "--cold" => o.cold = v.parse()?,
            "--warm" => o.warm = v.parse()?,
            "--idle" => o.idle_secs = v.parse()?,
            "--out" => o.out = Some(v),
            _ => bail!("bilinmeyen argüman: {k}"),
        }
    }
    if o.label.is_empty() || o.exe.is_empty() {
        bail!("--label ve --exe gerekli");
    }
    Ok(o)
}

struct App {
    child: Child,
    lines: Receiver<String>,
}

impl App {
    fn launch(o: &Options) -> anyhow::Result<App> {
        let mut child = Command::new(&o.exe)
            .args(&o.args)
            .envs(o.env.iter().cloned())
            // Ölçüm kipi: odak kaybında kendiliğinden kapanma kapalı; yalnız komutlarla açılır/kapanır.
            .env(MEASURE_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("{} başlatılamadı", o.exe))?;
        let stdout = child.stdout.take().context("stdout yok")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let app = App { child, lines: rx };
        app.wait_line("TRAY_READY")?;
        Ok(app)
    }

    fn wait_line(&self, prefix: &str) -> anyhow::Result<String> {
        loop {
            let line = self
                .lines
                .recv_timeout(LINE_TIMEOUT)
                .with_context(|| format!("'{prefix}' satırı gelmedi"))?;
            if line.starts_with(prefix) {
                return Ok(line);
            }
        }
    }

    /// (PrivateUsage, WorkingSetSize) bayt.
    fn memory(&self) -> anyhow::Result<(u64, u64)> {
        let mut pmc = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: Alt işlem handle'ı bu nesne yaşadıkça geçerli; yapı doğru boyutla geçirilir.
        unsafe {
            GetProcessMemoryInfo(
                HANDLE(self.child.as_raw_handle()),
                &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
                pmc.cb,
            )
        }?;
        Ok((pmc.PrivateUsage as u64, pmc.WorkingSetSize as u64))
    }

    /// Komut iletisini (`CMD_OPEN`/`CMD_CLOSE`) gönderir; gönderimden hemen önceki QPC'yi döndürür.
    fn command(&self, cmd: usize) -> anyhow::Result<i64> {
        // SAFETY: Sabit geniş dizeler; pencere tanıtıcısı yalnızca ileti göndermek için kullanılır.
        unsafe {
            let hwnd = FindWindowW(TRAY_CLASS, PCWSTR::null())?;
            let msg = RegisterWindowMessageW(TOGGLE_MESSAGE);
            let t0 = qpc();
            PostMessageW(Some(hwnd), msg, WPARAM(cmd), LPARAM(0))?;
            Ok(t0)
        }
    }

    /// Açar ve ilk karenin ekrana gelmesine kadar geçen süreyi (ms) döndürür.
    fn open_ms(&self) -> anyhow::Result<f64> {
        let t0 = self.command(CMD_OPEN)?;
        loop {
            let line = self.wait_line("READY ")?;
            let t1: i64 = line
                .split_whitespace()
                .nth(1)
                .context("READY satırında QPC yok")?
                .parse()?;
            // Gönderimden önceki bir READY bayattır (önceki bir açılıştan kalmış); atlanır.
            if t1 >= t0 {
                return Ok((t1 - t0) as f64 * 1000.0 / qpc_frequency() as f64);
            }
        }
    }

    fn close(&self) -> anyhow::Result<()> {
        self.command(CMD_CLOSE)?;
        self.wait_line("HIDDEN")?;
        Ok(())
    }

}

impl Drop for App {
    /// Hata yolunda da alt işlem kalmasın (aksi hâlde devralınan pipe'ları açık tutar).
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn mb(b: u64) -> f64 {
    (b as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0
}

fn summary(v: &[f64]) -> Value {
    let r = |x: Option<f64>| x.map(|x| (x * 10.0).round() / 10.0);
    json!({
        "n": v.len(),
        "p50": r(stats::median(v)),
        "p95": r(stats::percentile(v, 95.0)),
        "max": r(stats::percentile(v, 100.0)),
    })
}

fn main() -> anyhow::Result<()> {
    let o = parse()?;
    let binary_bytes = std::fs::metadata(&o.exe)?.len();
    let mut cold = Vec::new();
    let mut warm = Vec::new();
    let mut memory = json!({});

    for run in 0..o.cold {
        let app = App::launch(&o)?;
        if run == 0 {
            std::thread::sleep(Duration::from_secs(o.idle_secs));
            let (p, w) = app.memory()?;
            memory["idle"] = json!({ "private_mb": mb(p), "working_set_mb": mb(w) });
        } else {
            std::thread::sleep(Duration::from_secs(1));
        }
        cold.push(app.open_ms()?);
        if run == 0 {
            std::thread::sleep(Duration::from_secs(1));
            let (p, w) = app.memory()?;
            memory["open"] = json!({ "private_mb": mb(p), "working_set_mb": mb(w) });
            app.close()?;
            std::thread::sleep(Duration::from_secs(5));
            let (p, w) = app.memory()?;
            memory["after_close"] = json!({ "private_mb": mb(p), "working_set_mb": mb(w) });
            for _ in 0..o.warm {
                warm.push(app.open_ms()?);
                app.close()?;
                std::thread::sleep(Duration::from_millis(100));
            }
            std::thread::sleep(Duration::from_secs(2));
            let (p, w) = app.memory()?;
            memory["after_warm"] = json!({ "private_mb": mb(p), "working_set_mb": mb(w) });
        }
        drop(app);
        std::thread::sleep(Duration::from_millis(500));
    }

    let report = json!({
        "label": o.label,
        "binary_mb": mb(binary_bytes),
        "memory": memory,
        "cold_open_ms": summary(&cold),
        "warm_open_ms": summary(&warm),
        "cold_samples_ms": cold.iter().map(|x| (x * 10.0).round() / 10.0).collect::<Vec<_>>(),
    });
    println!("{report}");
    if let Some(path) = o.out {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)?;
    }
    Ok(())
}
