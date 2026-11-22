// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask perf svc | ui | size` (TESTING §3 T-PERF-001…003, 010; §5 yöntem).
//!
//! - `svc` (yönetici): kurulu servisin boştaki belleği, CPU'su ve uyanmaları; 10 sn aralıkla.
//! - `ui` (normal terminal): tepsinin boştaki belleği, flyout açılış süresi ve belleği, ana
//!   pencerenin ilk çizimi, belleği ve kapanması. Arayüz yalnızca istenince ölçüm iletisi
//!   gönderir (`alterdns_core::ui::PERF_CLASS`).
//! - `size`: sürüm ikililerinin boyutu.
//!
//! Sonuç bölümleri `perf/results/YYYY-MM-DD.json` dosyasına birleştirilir (yalnızca sayılar,
//! sürüm ve tarih). Bütçe aşımı ya da önceki güne göre gürültü tabanını aşan > %10 kötüleşme
//! çıkış 1'dir.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alterdns_core::Paths;
use alterdns_core::ui::{
    MSG_PERF_FLYOUT_CLOSE, MSG_PERF_FLYOUT_OPEN, MSG_QUIT, PERF_CLASS, TRAY_CLASS, WINDOW_CLASS,
};
use alterdns_winsys::{
    MessageWindow, ShellEvent, allow_foreground, logical_cpus, post_broadcast, process_image,
    pump_messages, sample_process, service_pid, window_owner,
};
use anyhow::{Context, bail};
use serde_json::{Map, Value, json};

pub const MIB: u64 = 1024 * 1024;
const RESULTS_DIR: &str = "perf/results";
const USAGE: &str = "kullanım: cargo xtask perf svc [--minutes N] | ui [--opens N] | size";
const SVC_STEP: Duration = Duration::from_secs(10);
const WAKE_PERIOD: Duration = Duration::from_secs(60);
const REGRESSION_RATIO: f64 = 1.10;

/// (bölüm, alan, sınır) — sınırı aşan değer bütçe dışıdır (Global Constraints, TESTING §3).
const BUDGETS: [(&str, &str, f64); 10] = [
    ("svc", "private_max_bytes", (12 * MIB) as f64),
    ("svc", "cpu_percent", 0.1),
    ("ui", "tray_idle_private_bytes", (8 * MIB) as f64),
    ("ui", "flyout_open_private_bytes", (45 * MIB) as f64),
    ("ui", "flyout_cold_ms", 400.0),
    ("ui", "flyout_warm_p95_ms", 150.0),
    ("ui", "main_first_paint_ms", 600.0),
    ("ui", "main_private_bytes", (90 * MIB) as f64),
    ("ui", "main_exit_ms", 5000.0),
    ("size", "total_bytes", (25 * MIB) as f64),
];

/// Sonuç dosyasında izin verilen alanlar.
const KEYS: [(&str, &[&str]); 3] = [
    (
        "svc",
        &[
            "minutes",
            "private_median_bytes",
            "private_max_bytes",
            "cpu_percent",
            "active_intervals",
            "allowed_intervals",
        ],
    ),
    (
        "ui",
        &[
            "source",
            "tray_idle_private_bytes",
            "flyout_open_private_bytes",
            "flyout_cold_ms",
            "flyout_warm_p95_ms",
            "main_first_paint_ms",
            "main_private_bytes",
            "main_exit_ms",
        ],
    ),
    (
        "size",
        &["ui_bytes", "svc_bytes", "maint_bytes", "total_bytes"],
    ),
];

fn sorted(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s
}

pub fn median(v: &[f64]) -> f64 {
    let s = sorted(v);
    match s.len() {
        0 => 0.0,
        n if n % 2 == 1 => s[n / 2],
        n => (s[n / 2 - 1] + s[n / 2]) / 2.0,
    }
}

/// 95. yüzdelik, en yakın sıra yöntemiyle.
pub fn p95(v: &[f64]) -> f64 {
    let s = sorted(v);
    if s.is_empty() {
        return 0.0;
    }
    let rank = ((0.95 * s.len() as f64).ceil() as usize).clamp(1, s.len());
    s[rank - 1]
}

pub fn cpu_percent(cpu: Duration, wall: Duration, cpus: u32) -> f64 {
    if wall.is_zero() || cpus == 0 {
        return 0.0;
    }
    cpu.as_secs_f64() / wall.as_secs_f64() / f64::from(cpus) * 100.0
}

/// Ardışık örnek çiftlerinde herhangi bir iş parçacığının döngüsü arttıysa (yeni iş parçacığı
/// dahil) o aralık etkindir.
pub fn active_intervals(samples: &[Vec<(u32, u64)>]) -> usize {
    samples
        .windows(2)
        .filter(|w| {
            w[1].iter().any(|(tid, cycles)| {
                w[0].iter()
                    .find(|(t, _)| t == tid)
                    .is_none_or(|(_, before)| cycles > before)
            })
        })
        .count()
}

/// Dakikada en çok bir uyanma: ⌈aralık sayısı × adım / 60 sn⌉.
pub fn allowed_intervals(intervals: usize, step: Duration) -> usize {
    let total = step.as_secs_f64() * intervals as f64;
    (total / WAKE_PERIOD.as_secs_f64()).ceil() as usize
}

fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

/// Bütçe aşımlarını açıklayan satırlar.
pub fn over_budget(section: &str, data: &Value) -> Vec<String> {
    let mut out: Vec<String> = BUDGETS
        .iter()
        .filter(|(s, _, _)| *s == section)
        .filter_map(|(_, key, limit)| {
            let v = number(data, key)?;
            (v > *limit).then(|| format!("{section}.{key} = {v} > bütçe {limit}"))
        })
        .collect();
    if section == "svc"
        && let (Some(active), Some(allowed)) = (
            number(data, "active_intervals"),
            number(data, "allowed_intervals"),
        )
        && active > allowed
    {
        out.push(format!(
            "svc.active_intervals = {active} > {allowed} (60 sn'den sık uyanma)"
        ));
    }
    out
}

/// Alan başına gürültü tabanı; tabanı olmayan alanlar karşılaştırılmaz.
fn noise_floor(key: &str) -> Option<f64> {
    if key.ends_with("_bytes") {
        Some(512.0 * 1024.0)
    } else if key.ends_with("_ms") {
        Some(5.0)
    } else if key == "cpu_percent" {
        Some(0.01)
    } else if key == "active_intervals" {
        Some(1.0)
    } else {
        None
    }
}

/// Önceki tabana göre kötüleşmeler (düşük değer iyidir): > %10 ve gürültü tabanından fazla.
pub fn regressions(prev: &Value, cur: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(sections) = cur.as_object() else {
        return out;
    };
    for (section, data) in sections {
        let (Some(fields), Some(before)) = (data.as_object(), prev.get(section)) else {
            continue;
        };
        for (key, value) in fields {
            let (Some(floor), Some(c), Some(p)) =
                (noise_floor(key), value.as_f64(), number(before, key))
            else {
                continue;
            };
            if c > p * REGRESSION_RATIO && c - p > floor {
                let pct = if p > 0.0 {
                    (c / p - 1.0) * 100.0
                } else {
                    100.0
                };
                out.push(format!("{section}.{key}: {p} → {c} (+%{pct:.0})"));
            }
        }
    }
    out
}

/// Günün dosyasına bölümü yazar; diğer bölümler korunur.
pub fn merge_section(file: Option<Value>, section: &str, data: Value) -> Value {
    let mut map = match file {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    map.insert(section.to_string(), data);
    Value::Object(map)
}

/// `dir` içindeki `YYYY-MM-DD.json` dosyalarından `today`'den önceki ve `section` bölümünü
/// taşıyan en yenisi. Bölümü olmayan dosya o bölümün tabanı değildir.
pub fn previous_baseline(dir: &Path, today: &str, section: &str) -> Option<PathBuf> {
    let mut days: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
        .filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-')
        .filter(|d| d.as_str() < today)
        .collect();
    days.sort();
    days.into_iter()
        .rev()
        .map(|d| dir.join(format!("{d}.json")))
        .find(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .is_some_and(|v| v.get(section).is_some())
        })
}

/// Yalnızca belgelenen alanlar; hiçbir metin değeri yol ya da makineye özgü bilgi içermez.
pub fn validate_results(v: &Value) -> anyhow::Result<()> {
    let top = v.as_object().context("sonuç bir nesne değil")?;
    for (key, value) in top {
        match key.as_str() {
            "schema" | "date" | "version" => {}
            section => {
                let allowed = KEYS
                    .iter()
                    .find(|(s, _)| *s == section)
                    .map(|(_, k)| *k)
                    .with_context(|| format!("bilinmeyen bölüm: {section}"))?;
                let fields = value
                    .as_object()
                    .with_context(|| format!("{section} bir nesne değil"))?;
                for (field, fv) in fields {
                    if !allowed.contains(&field.as_str()) {
                        bail!("bilinmeyen alan: {section}.{field}");
                    }
                    if let Some(s) = fv.as_str()
                        && (s.contains('\\') || s.contains(':') || s.contains('/'))
                    {
                        bail!("{section}.{field} yol benzeri değer içeriyor");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Unix saniyesinden UTC takvim tarihi (`YYYY-MM-DD`).
pub fn date_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // Howard Hinnant, civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[derive(Debug, PartialEq, Eq)]
pub enum UiPreflight {
    Ready,
    /// Kurulu dizinden çalışan arayüz kapatılır ve beklenir.
    CloseInstalled,
    Blocked(String),
}

pub fn ui_preflight(
    lock_held: bool,
    owner_image: Option<&Path>,
    install_root: &Path,
) -> UiPreflight {
    if !lock_held {
        return UiPreflight::Ready;
    }
    match owner_image {
        Some(image) if image.starts_with(install_root) => UiPreflight::CloseInstalled,
        Some(_) => UiPreflight::Blocked(
            "AlterDNS arayüzü kurulum dışından (ör. target\\) çalışıyor; kapatıp yeniden deneyin"
                .into(),
        ),
        None => UiPreflight::Blocked(
            "AlterDNS arayüzü açık ama işlemi okunamadı; kapatıp yeniden deneyin".into(),
        ),
    }
}

/// Pencere sınıfı başına kapatma kararı (`true`: kurulu arayüz kapatılır). Biri engelliyse
/// hiçbiri kapatılmaz; karar bütün sınıflar için kapatmadan önce verilir.
pub fn plan_ui_close(
    found: &[(bool, Option<&Path>)],
    install_root: &Path,
) -> Result<Vec<bool>, String> {
    found
        .iter()
        .map(
            |(lock_held, image)| match ui_preflight(*lock_held, *image, install_root) {
                UiPreflight::Ready => Ok(false),
                UiPreflight::CloseInstalled => Ok(true),
                UiPreflight::Blocked(why) => Err(why),
            },
        )
        .collect()
}

/// `svc` yönetici ister (ADNS-SYS-004); `ui` yönetici terminalinde reddeder (başlattığı
/// arayüz yükseltilmiş çalışırdı).
pub fn check_mode(sub: &str, elevated: bool) -> anyhow::Result<()> {
    match sub {
        "svc" => Ok(crate::devinstall::require_elevated(elevated)?),
        "ui" if elevated => bail!(
            "perf ui yönetici terminalinde çalıştırılmaz: başlattığı arayüz yükseltilmiş çalışırdı. Normal terminalde çalıştırın"
        ),
        "ui" | "size" => Ok(()),
        _ => bail!("{USAGE}"),
    }
}

/// `perf svc` ön koşulu: servis kurulu ve çalışıyor.
pub fn svc_pid(pid: Option<u32>) -> anyhow::Result<u32> {
    pid.context(
        "AlterDNS servisi çalışmıyor ya da kurulu değil: önce yönetici terminalinde cargo xtask dev-install",
    )
}

/// `PERF_CLASS` sınıflı gizli pencere ve gelen değerlerin kuyruğu (mikro saniye).
pub struct PerfReceiver {
    _window: MessageWindow,
    queue: Rc<RefCell<VecDeque<u32>>>,
}

impl PerfReceiver {
    pub fn new() -> anyhow::Result<PerfReceiver> {
        let queue: Rc<RefCell<VecDeque<u32>>> = Rc::default();
        let sink = queue.clone();
        let window = MessageWindow::create(
            PERF_CLASS,
            Box::new(move |e| {
                if let ShellEvent::Broadcast(v) = e {
                    sink.borrow_mut().push_back(v);
                }
            }),
        )?;
        Ok(PerfReceiver {
            _window: window,
            queue,
        })
    }

    /// Sıradaki değer; süre dolarsa hata.
    pub fn wait(&self, timeout: Duration) -> anyhow::Result<u32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(v) = self.queue.borrow_mut().pop_front() {
                return Ok(v);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                bail!("ölçüm değeri {} ms içinde gelmedi", timeout.as_millis());
            }
            pump_messages(left.min(Duration::from_millis(50)));
        }
    }
}

/// Başlatılan işlemler: hata ya da zaman aşımında düşünce sonlandırılır.
#[derive(Default)]
struct Spawned(Vec<Child>);

impl Drop for Spawned {
    fn drop(&mut self) {
        for child in &mut self.0 {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn wait_exit(child: &mut Child, timeout: Duration) -> anyhow::Result<Option<Duration>> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if child.try_wait()?.is_some() {
            return Ok(Some(start.elapsed()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(None)
}

fn option_value(args: &[String], name: &str, default: u64, min: u64) -> anyhow::Result<u64> {
    match args.iter().position(|a| a == name) {
        None => Ok(default),
        Some(i) => {
            let v: u64 = args
                .get(i + 1)
                .and_then(|s| s.parse().ok())
                .with_context(|| format!("{name} için sayı gerekli"))?;
            if v < min {
                bail!("{name} en az {min}");
            }
            Ok(v)
        }
    }
}

fn file_size(path: &Path) -> anyhow::Result<u64> {
    Ok(std::fs::metadata(path)
        .with_context(|| {
            format!(
                "{} yok: önce cargo build --release -p alterdns-svc -p alterdns-maint -p alterdns-ui-app",
                path.display()
            )
        })?
        .len())
}

fn measure_size(root: &Path) -> anyhow::Result<Value> {
    let dir = root.join(r"target\release");
    let ui = file_size(&dir.join("alterdns-ui.exe"))?;
    let svc = file_size(&dir.join("alterdns-svc.exe"))?;
    let maint = file_size(&dir.join("alterdns-maint.exe"))?;
    Ok(json!({
        "ui_bytes": ui, "svc_bytes": svc, "maint_bytes": maint, "total_bytes": ui + svc + maint,
    }))
}

fn measure_svc(minutes: u64) -> anyhow::Result<Value> {
    let pid = svc_pid(service_pid(alterdns_journal::SERVICE_NAME)?)?;
    let intervals = (minutes * 60 / SVC_STEP.as_secs()) as usize;
    eprintln!(
        "servis ölçülüyor: {minutes} dk, {} sn aralıkla (bu sürede bilgisayarı boşta bırakın)",
        SVC_STEP.as_secs()
    );
    let started = Instant::now();
    let first = sample_process(pid).context("servis işlemi okunamadı")?;
    let mut private = vec![first.private_bytes as f64];
    let mut cycles = vec![first.thread_cycles.clone()];
    let mut last = first.clone();
    for i in 1..=intervals {
        std::thread::sleep(SVC_STEP);
        if service_pid(alterdns_journal::SERVICE_NAME)? != Some(pid) {
            bail!("servis ölçüm sırasında durdu ya da yeniden başladı");
        }
        last = sample_process(pid).context("servis işlemi okunamadı")?;
        private.push(last.private_bytes as f64);
        cycles.push(last.thread_cycles.clone());
        if i % 6 == 0 {
            eprintln!("  {} / {minutes} dk", i / 6);
        }
    }
    let wall = started.elapsed();
    let cpu = last.cpu_time.saturating_sub(first.cpu_time);
    Ok(json!({
        "minutes": minutes,
        "private_median_bytes": median(&private) as u64,
        "private_max_bytes": private.iter().copied().fold(0.0, f64::max) as u64,
        "cpu_percent": cpu_percent(cpu, wall, logical_cpus()),
        "active_intervals": active_intervals(&cycles),
        "allowed_intervals": allowed_intervals(intervals, SVC_STEP),
    }))
}

/// Kurulu arayüz (`installed`) ya da `target\release` derlemesi (`target`).
fn ui_exe(root: &Path, paths: &Paths) -> anyhow::Result<(PathBuf, &'static str)> {
    let installed = paths
        .install_dir(alterdns_core::version::VERSION)
        .join("alterdns-ui.exe");
    if installed.is_file() {
        return Ok((installed, "installed"));
    }
    let built = root.join(r"target\release\alterdns-ui.exe");
    if built.is_file() {
        return Ok((built, "target"));
    }
    bail!("alterdns-ui.exe yok: önce cargo build --release -p alterdns-ui-app")
}

/// Kurulu dizinden çalışan arayüzü kapatır. Kapatılan tepsinin ikilisini döndürür; ölçümden
/// sonra yeniden başlatılır.
fn close_running_ui(install_root: &Path) -> anyhow::Result<Option<PathBuf>> {
    let classes = [TRAY_CLASS, WINDOW_CLASS];
    let found: Vec<(bool, Option<PathBuf>)> = classes
        .iter()
        .map(|class| {
            let owner = window_owner(class);
            (
                owner.is_some(),
                owner.and_then(|pid| process_image(pid).ok()),
            )
        })
        .collect();
    let refs: Vec<(bool, Option<&Path>)> = found.iter().map(|(l, i)| (*l, i.as_deref())).collect();
    let plan = plan_ui_close(&refs, install_root).map_err(|why| anyhow::anyhow!(why))?;
    let mut tray = None;
    for ((class, close), (_, image)) in classes.iter().zip(plan).zip(&found) {
        if !close {
            continue;
        }
        eprintln!("açık AlterDNS arayüzü kapatılıyor ({class})");
        let start = Instant::now();
        while post_broadcast(class, MSG_QUIT)? {
            if start.elapsed() > Duration::from_secs(10) {
                bail!("AlterDNS arayüzü kapanmadı ({class})");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_secs(1));
        if *class == TRAY_CLASS {
            tray.clone_from(image);
        } else {
            eprintln!("ana pencere kapatıldı; ölçümden sonra yeniden açabilirsiniz");
        }
    }
    Ok(tray)
}

/// Ölçüm için kapatılan kurulu tepsiyi geri açar.
fn restart_tray(exe: &Path) {
    match Command::new(exe).arg("--tray").spawn() {
        Ok(_) => eprintln!("AlterDNS tepsisi yeniden başlatıldı"),
        Err(e) => eprintln!(
            "AlterDNS tepsisi yeniden başlatılamadı ({e}); oturum yeniden açılınca kendiliğinden başlar"
        ),
    }
}

fn measure_ui(root: &Path, opens: u64) -> anyhow::Result<Value> {
    let paths = Paths::system()?;
    let (exe, source) = ui_exe(root, &paths)?;
    let closed_tray = close_running_ui(&paths.program_files)?;
    let result = measure_ui_with(&exe, source, opens);
    if let Some(tray) = closed_tray {
        restart_tray(&tray);
    }
    result
}

fn measure_ui_with(exe: &Path, source: &str, opens: u64) -> anyhow::Result<Value> {
    let rx = PerfReceiver::new()?;
    let mut spawned = Spawned::default();
    eprintln!("tepsi başlatılıyor ({source}); ölçüm sırasında fare ve klavye kullanmayın");
    spawned.0.push(Command::new(exe).arg("--tray").spawn()?);
    let tray_pid = spawned.0[0].id();
    let start = Instant::now();
    while window_owner(TRAY_CLASS) != Some(tray_pid) {
        if start.elapsed() > Duration::from_secs(15) {
            bail!("tepsi 15 sn içinde açılmadı");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Boşta: 30 sn yerleşme, sonra 30 sn boyunca 1 sn aralıkla bellek.
    std::thread::sleep(Duration::from_secs(30));
    let mut idle = Vec::new();
    for _ in 0..30 {
        idle.push(sample_process(tray_pid)?.private_bytes as f64);
        std::thread::sleep(Duration::from_secs(1));
    }
    eprintln!("flyout {opens} kez açılıyor");
    let mut open_ms = Vec::new();
    let mut open_private = 0u64;
    for _ in 0..opens {
        allow_foreground(tray_pid);
        post_broadcast(TRAY_CLASS, MSG_PERF_FLYOUT_OPEN)?;
        let us = rx.wait(Duration::from_secs(5)).context("flyout açılışı")?;
        open_ms.push(f64::from(us) / 1000.0);
        open_private = open_private.max(sample_process(tray_pid)?.private_bytes);
        post_broadcast(TRAY_CLASS, MSG_PERF_FLYOUT_CLOSE)?;
        pump_messages(Duration::from_millis(400));
    }
    post_broadcast(TRAY_CLASS, MSG_QUIT)?;
    if wait_exit(&mut spawned.0[0], Duration::from_secs(10))?.is_none() {
        bail!("tepsi kapanma iletisinden sonra 10 sn içinde bitmedi");
    }
    eprintln!("ana pencere 5 kez açılıp kapatılıyor");
    let (mut paint, mut main_private, mut exit_ms) = (Vec::new(), 0u64, 0f64);
    for _ in 0..5 {
        spawned.0.push(Command::new(exe).arg("--perf").spawn()?);
        let child = spawned.0.len() - 1;
        let us = rx
            .wait(Duration::from_secs(15))
            .context("ana pencere ilk çizimi")?;
        paint.push(f64::from(us) / 1000.0);
        std::thread::sleep(Duration::from_secs(2));
        main_private = main_private.max(sample_process(spawned.0[child].id())?.private_bytes);
        post_broadcast(WINDOW_CLASS, MSG_QUIT)?;
        let took = wait_exit(&mut spawned.0[child], Duration::from_secs(10))?;
        exit_ms = exit_ms.max(took.map_or(10_000.0, |d| d.as_secs_f64() * 1000.0));
    }
    let warm = open_ms.get(1..).unwrap_or_default();
    Ok(json!({
        "source": source,
        "tray_idle_private_bytes": median(&idle) as u64,
        "flyout_open_private_bytes": open_private,
        "flyout_cold_ms": open_ms.first().copied().unwrap_or_default(),
        "flyout_warm_p95_ms": p95(warm),
        "main_first_paint_ms": median(&paint),
        "main_private_bytes": main_private,
        "main_exit_ms": exit_ms,
    }))
}

fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_from_unix(secs)
}

fn record(root: &Path, section: &str, data: Value) -> anyhow::Result<()> {
    record_in(&root.join(RESULTS_DIR), &today(), section, data)
}

/// Bölümü `dir/<day>.json` dosyasına birleştirir. Bütçe aşımı ya da önceki tabana göre
/// kötüleşme varsa dosya değiştirilmez: başarısız bir ölçüm sonraki günün tabanı olamaz.
pub fn record_in(dir: &Path, day: &str, section: &str, data: Value) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{day}.json"));
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => Some(serde_json::from_str(&text).with_context(|| path.display().to_string())?),
        Err(_) => None,
    };
    let mut merged = merge_section(existing, section, data);
    if let Some(map) = merged.as_object_mut() {
        map.insert("schema".into(), json!(1));
        map.insert("date".into(), json!(day));
        map.insert("version".into(), json!(alterdns_core::version::VERSION));
    }
    validate_results(&merged)?;
    let mut problems = over_budget(section, &merged[section]);
    match previous_baseline(dir, day, section) {
        Some(prev_path) => {
            let prev: Value = serde_json::from_str(&std::fs::read_to_string(&prev_path)?)
                .with_context(|| prev_path.display().to_string())?;
            let cur = json!({ section: merged[section].clone() });
            problems.extend(
                regressions(&prev, &cur)
                    .into_iter()
                    .map(|l| format!("kötüleşme: {l}")),
            );
            eprintln!(
                "taban: {}",
                prev_path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
        None => eprintln!(
            "taban yok: {section} bölümü olan daha eski bir sonuç dosyası bulunamadı; kötüleşme karşılaştırması yapılmadı"
        ),
    }
    eprintln!("{section}:");
    if let Some(fields) = merged[section].as_object() {
        for (k, v) in fields {
            eprintln!("  {k:<28} {v}");
        }
    }
    if !problems.is_empty() {
        bail!(
            "{}\nsonuç {} dosyasına yazılmadı",
            problems.join("\n"),
            path.display()
        );
    }
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&merged)?),
    )?;
    eprintln!(
        "bütçe ve regresyon denetimi temiz; {} güncellendi",
        path.display()
    );
    Ok(())
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map_or("", String::as_str);
    check_mode(sub, alterdns_winsys::is_elevated()?)?;
    let root = crate::check::repo_root()?;
    let data = match sub {
        "size" => measure_size(&root)?,
        "svc" => measure_svc(option_value(args, "--minutes", 10, 1)?)?,
        "ui" => measure_ui(&root, option_value(args, "--opens", 50, 2)?)?,
        _ => bail!("{USAGE}"),
    };
    record(&root, sub, data)
}
