// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Faz 0 spike'ı (T-SPK-002, D8): Wintun TUN adaptörü + tek IP rotası + kullanıcı modu iletim.
//!
//! Komutlar (yönetici terminali; derlenmiş ikiliyle, `cargo run` ile değil — bekçi iş nesnesi):
//! - `run`: doğrudan ölçüm → tünel → TUN ölçümleri → söküm ve doğrulama (Adım b, c, d, f).
//! - `kill-test`: tüneli kuran alt işlemi `TerminateProcess` ile öldürür; bekçinin geri
//!   aldığını ölçer (Adım e).
//! - `status`: salt okunur durum (adaptör, rota, sürücü deposu, bayat journal).
//! - `serve`: içsel; `kill-test`'in alt işlemi.

mod config;
mod ffi;
mod forward;
mod harden;
mod measure;
mod s4;
mod tunnel;

use std::io::{BufRead, Write as _};
use std::net::SocketAddr;
use std::path::Path;
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use serde::Serialize;
use xtask::lab::cli::{journal_dir, results_dir, run_id};
use xtask::lab::dns::DnsMode;
use xtask::lab::journal::{Journal, SPIKE_ADAPTER};
use xtask::lab::ops::ps_query;
use xtask::lab::probe::{self, ProbeResult};
use xtask::lab::scenario::{PROBE_BYTES, Summary, confirm, preflight};
use xtask::lab::sockettech::{SockTech, SplitAt};

use crate::measure::{Latency, UdpProbe};
use crate::tunnel::{
    AdapterSnapshot, Ctx, Teardown, Tunnel, adapter_snapshot, is_absent, other_wintun_adapters,
    route_op, tun_op, wait_for,
};

/// `kill-test` alt işlemine verilen iç kullanım belirteci (`serve` doğrudan çalıştırılmasın).
const SERVE_TOKEN_ENV: &str = "ALTERDNS_SPIKE_SERVE";
const SERVE_TOKEN: &str = "kill-test";

/// Gecikme ölçümündeki el sıkışma sayısı (Adım d).
const MEASURE_N: usize = 50;
/// Teknik başına TUN probu.
const PER_TECH: usize = 3;
/// S2-U'nun en iyi teknikleri (CONNECTIVITY_RESEARCH §7).
const BEST: SockTech = SockTech::TlsRecordSplit {
    at: SplitAt::SniMiddle,
};
const TUN_TECHS: [SockTech; 4] = [
    SockTech::None,
    BEST,
    SockTech::Oob {
        at: SplitAt::SniMiddle,
    },
    SockTech::Split {
        at: SplitAt::Offset(1),
    },
];
/// `kill-test` alt işleminin kendiliğinden söküm yapacağı süre.
const SERVE_LIMIT: Duration = Duration::from_secs(45);
/// Bekçinin geri alması için tanınan süre (kira 60 sn + pay).
const REVERT_WAIT: Duration = Duration::from_secs(75);

fn main() -> ExitCode {
    // Sonraki bütün DLL yüklemeleri yalnızca System32 ve tam yolu verilen dosyayla sınırlı.
    if let Err(e) = ffi::restrict_dll_search() {
        eprintln!("wintun-spike: DLL arama yolu daraltılamadı: {e}");
        return ExitCode::FAILURE;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => guarded(run),
        Some("kill-test") => guarded(kill_test),
        Some("serve") => serve(),
        Some("status") => status(),
        Some("s4-plan") => s4_plan(),
        _ => {
            eprintln!("kullanım: wintun-spike run | kill-test | status | s4-plan");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wintun-spike: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// ADR-014: kullanıcıya ait sistem değerleri (hop limit, DNS, proxy, varsayılan rotalar…)
/// komuttan önce ve sonra okunur. Fark varsa komut başarısız sayılır; değer geri yazılmaz
/// (AlterDNS bunlara yazmaz, farkı başka bir program veya ağ yapmış olabilir).
fn guarded(f: fn() -> anyhow::Result<()>) -> anyhow::Result<()> {
    use xtask::lab::guarded;
    let before = guarded::snapshot();
    let result = f();
    match (before, guarded::snapshot()) {
        (Ok(b), Ok(a)) => {
            let changed = guarded::diff(&b, &a);
            if changed.is_empty() {
                println!("Korunan değerler: değişmedi.");
            } else {
                eprintln!(
                    "UYARI: korunan değerler değişti: {changed:?}. AlterDNS bunlara yazmaz; dokunulmadı."
                );
                result?;
                bail!("korunan değerler değişti: {changed:?}");
            }
        }
        (Err(e), _) | (_, Err(e)) => eprintln!("Uyarı: korunan değerler denetlenemedi: {e:#}"),
    }
    result
}

#[derive(Debug, Serialize)]
struct Phase {
    label: String,
    tech: SockTech,
    ok: usize,
    results: Vec<ProbeResult>,
}

/// `n` prob; her probdan önce `beat` (bekçi kirası) çağrılır. Probun kendi tekniği `probe_tech`.
fn phase(
    ctx: &Ctx,
    label: &str,
    probe_tech: SockTech,
    n: usize,
    beat: &mut dyn FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Phase> {
    let ip = std::net::IpAddr::V4(ctx.test_ip());
    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        beat()?;
        let r = probe::run_probe_at(&ctx.target, ip, &probe_tech);
        if n <= 5 || i == n - 1 {
            println!("  [{label} {}/{n}] → {:?}", i + 1, r.outcome);
        }
        results.push(r);
    }
    let ok = results.iter().filter(|r| r.outcome.is_ok()).count();
    println!("  {label}: {ok}/{n} başarılı");
    Ok(Phase {
        label: label.to_string(),
        tech: probe_tech,
        ok,
        results,
    })
}

fn defender_detections() -> Option<usize> {
    ps_query("@(Get-MpThreatDetection -ErrorAction SilentlyContinue).Count")
        .ok()?
        .parse()
        .ok()
}

fn hvci_services() -> String {
    ps_query(
        "(Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root/Microsoft/Windows/DeviceGuard).SecurityServicesRunning -join ','",
    )
    .unwrap_or_else(|e| format!("okunamadı: {e}"))
}

fn write_json(dir: &Path, name: &str, value: &impl Serialize) -> anyhow::Result<()> {
    std::fs::write(dir.join(name), serde_json::to_string_pretty(value)?)?;
    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct RunReport {
    run: String,
    test_target: String,
    elapsed_ms: u64,
    phys_if: u32,
    tun_if: u32,
    mtu: u16,
    driver_version: Option<(u16, u16)>,
    driver_store_before: Vec<String>,
    driver_store_after: Vec<String>,
    defender_detections_before: Option<usize>,
    defender_detections_after: Option<usize>,
    hvci_services_running: String,
    direct: Vec<Phase>,
    tun: Vec<Phase>,
    control_during_tunnel: Option<ProbeResult>,
    udp_direct: Option<UdpProbe>,
    udp_tun: Option<UdpProbe>,
    latency_direct: Option<Latency>,
    latency_tun: Option<Latency>,
    cpu_ms_tun_measure: Option<u64>,
    private_bytes_before_tunnel: Option<u64>,
    private_bytes_peak: Option<u64>,
    engine: Option<serde_json::Value>,
    measure_error: Option<String>,
    teardown: Option<Teardown>,
    /// Her TUN aşamasından önce doğrudan kontrol probu (ağ kopması spike hatasıyla karışmasın).
    controls: Vec<(String, String)>,
    other_wintun_before: String,
    other_wintun_after: String,
    adapter_snapshot: Option<AdapterSnapshot>,
}

fn change_summary(ctx: &Ctx, store: &[String]) -> String {
    let driver = if store.is_empty() {
        "sistemde yok; DLL kurar ve kalır (kaldırma ayrıca onayla)".to_string()
    } else {
        format!(
            "sistemde zaten var ({}); spike bu pakete dokunmaz, DLL davranışı (sürüm, öteki Wintun \
             adaptörleri) önce/sonra kaydedilir",
            store.join(", ")
        )
    };
    format!(
        "Wintun adaptörü {SPIKE_ADAPTER} + adres {} + yalnızca {}/32 için rota (ActiveStore). \
         Yalnızca bu IP'ye giden trafik TUN'dan geçer. Wintun sürücü paketi: {driver}.",
        config::TUN_CIDR,
        ctx.test_ip()
    )
}

const REVERT_TEXT: &str = "Her değişiklik önce journal'a yazılır. Ayrı işlemdeki bekçi (kira ≤ 60 sn) çökme, Ctrl+C veya süre dolunca geri alır; koşu sonunda spike kendisi söker ve kalıntı olmadığını doğrular.";

/// Wintun sürücü paketi sistemde yoksa çalışmaz: DLL paketi kurar ve bu değişiklik journal'a
/// yazılamaz (CLAUDE.md kural 1). Bu makinede paket başka bir yazılımdan zaten var.
fn require_driver_store() -> anyhow::Result<()> {
    let store = measure::driver_store().context("sürücü deposu okunamadı")?;
    if store.is_empty() {
        bail!(
            "Wintun sürücü paketi sistemde yok; DLL kurar ve bu journal'a yazılamaz. \
             Ayrı onay olmadan çalıştırılmaz."
        );
    }
    Ok(())
}

/// Adım b, c, d, f.
fn run() -> anyhow::Result<()> {
    let ctx = Ctx::load()?;
    harden::elevated_preconditions(&ctx.root, ctx.test_ip())?;
    preflight(&ctx.env, true).map_err(|e| anyhow!("{e}"))?;
    require_driver_store()?;
    let store_before = measure::driver_store().unwrap_or_default();
    let probes = (PER_TECH + MEASURE_N) + (TUN_TECHS.len() * PER_TECH + MEASURE_N) + 1;
    let summary = Summary {
        scenario: "Görev 7 — Wintun spike (adaptör + tek IP rotası + kullanıcı modu iletim)".into(),
        changes: change_summary(&ctx, &store_before),
        revert: REVERT_TEXT.into(),
        probes: probes as u64,
        bytes: probes as u64 * PROBE_BYTES + 6 * 1200,
    };
    if !confirm(&summary) {
        println!("İptal edildi; hiçbir değişiklik yapılmadı.");
        return Ok(());
    }
    let started = Instant::now();
    let run = run_id("wintun");
    let out_dir = results_dir(&ctx.root).join(&run);
    std::fs::create_dir_all(&out_dir)?;
    let mut rep = RunReport {
        run: run.clone(),
        test_target: ctx.target.name.clone(),
        driver_store_before: store_before,
        defender_detections_before: defender_detections(),
        hvci_services_running: hvci_services(),
        ..RunReport::default()
    };
    rep.other_wintun_before = other_wintun_adapters();
    // Başlangıç verisi hemen yazılır: işlem öldürülse de kaybolmasın.
    write_json(&out_dir, "wintun-run.json", &rep)?;
    let udp_addr = SocketAddr::new(ctx.test_ip().into(), 443);

    // 1. Doğrudan (rota yok): tekniksiz, UDP ve en iyi teknikle gecikme.
    println!("Doğrudan ölçüm (rota yok)…");
    let mut no_beat = || Ok(());
    rep.direct.push(phase(
        &ctx,
        "doğrudan, tekniksiz",
        SockTech::None,
        PER_TECH,
        &mut no_beat,
    )?);
    rep.udp_direct = Some(measure::udp_probe(udp_addr));
    let d = phase(
        &ctx,
        "doğrudan, TlsRecordSplit@SNI",
        BEST,
        MEASURE_N,
        &mut no_beat,
    )?;
    rep.latency_direct = Some(Latency::from_results(&d.results));
    rep.direct.push(d);
    rep.private_bytes_before_tunnel = ffi::process_metrics().ok().map(|(_, m)| m);

    // 2. Tünel.
    println!("Wintun yükleniyor ve tünel kuruluyor…");
    let lib = ffi::Wintun::load(&ctx.dll, ffi::WINTUN_DLL_SHA256)?;
    let mut t = Tunnel::up(&ctx, &lib, &out_dir, &run, SockTech::None)?;
    rep.phys_if = t.phys_if;
    rep.tun_if = t.tun_if;
    rep.mtu = t.mtu;
    rep.driver_version = t.driver_version;
    println!(
        "Tünel hazır: fiziksel arayüz {}, TUN {}, MTU {}",
        t.phys_if, t.tun_if, t.mtu
    );
    rep.adapter_snapshot = Some(adapter_snapshot());
    write_json(&out_dir, "wintun-run.json", &rep)?;
    let engine = t.engine.clone();
    let mut peak = rep.private_bytes_before_tunnel.unwrap_or(0);
    let measured = (|| -> anyhow::Result<()> {
        t.beat()?;
        rep.control_during_tunnel = Some(probe::run_probe(&ctx.control, DnsMode::System));
        for tech in TUN_TECHS {
            t.beat()?;
            let c = probe::run_probe(&ctx.control, DnsMode::System);
            rep.controls
                .push((format!("{tech:?} öncesi"), format!("{:?}", c.outcome)));
            if !c.outcome.is_ok() {
                bail!(
                    "kontrol probu başarısız ({:?}); ağ kopmuş olabilir, ölçüm durduruldu",
                    c.outcome
                );
            }
            engine.set_tech(tech);
            let label = format!("TUN, motor {tech:?}");
            rep.tun
                .push(phase(&ctx, &label, SockTech::None, PER_TECH, &mut || {
                    t.beat()
                })?);
            if engine.halted() {
                bail!("döngü saptandı: IP_UNICAST_IF etkisiz; ölçüm durduruldu");
            }
        }
        t.beat()?;
        rep.udp_tun = Some(measure::udp_probe(udp_addr));
        engine.set_tech(BEST);
        let cpu0 = ffi::process_metrics().ok().map(|(c, _)| c);
        let m = phase(
            &ctx,
            "TUN, motor TlsRecordSplit@SNI",
            SockTech::None,
            MEASURE_N,
            &mut || {
                if let Ok((_, mem)) = ffi::process_metrics() {
                    peak = peak.max(mem);
                }
                t.beat()
            },
        )?;
        if let (Some(c0), Ok((c1, _))) = (cpu0, ffi::process_metrics()) {
            rep.cpu_ms_tun_measure = Some(c1.saturating_sub(c0).as_millis() as u64);
        }
        rep.latency_tun = Some(Latency::from_results(&m.results));
        rep.tun.push(m);
        Ok(())
    })();
    if let Err(e) = &measured {
        eprintln!("Ölçüm durdu: {e:#}");
        rep.measure_error = Some(format!("{e:#}"));
    }
    rep.private_bytes_peak = Some(peak);
    rep.engine = serde_json::to_value(&*engine.stats).ok();

    // 3. Söküm ve doğrulama.
    println!("Söküm…");
    let td = t.down();
    rep.driver_store_after = measure::driver_store().unwrap_or_default();
    rep.other_wintun_after = other_wintun_adapters();
    rep.defender_detections_after = defender_detections();
    rep.elapsed_ms = started.elapsed().as_millis() as u64;
    let clean = td.adapter_absent && td.route_absent && td.failed.is_empty();
    rep.teardown = Some(td);
    write_json(&out_dir, "wintun-run.json", &rep)?;
    print_run_summary(&rep, &out_dir);
    if !clean {
        bail!(
            "söküm tam değil; `target\\xtask\\debug\\xtask.exe lab recover` çalıştırın (lab/RUNBOOK.md)"
        );
    }
    Ok(())
}

fn print_run_summary(rep: &RunReport, out_dir: &Path) {
    println!("\n=== Özet ===");
    for p in rep.direct.iter().chain(rep.tun.iter()) {
        println!("{:<44} {}/{}", p.label, p.ok, p.results.len());
    }
    if let Some(c) = &rep.control_during_tunnel {
        println!("Kontrol (tünel açıkken, doğrudan): {:?}", c.outcome);
    }
    println!("UDP doğrudan: {:?}", rep.udp_direct);
    println!("UDP TUN:      {:?}", rep.udp_tun);
    println!("Gecikme doğrudan: {:?}", rep.latency_direct);
    println!("Gecikme TUN:      {:?}", rep.latency_tun);
    println!(
        "CPU (TUN ölçümü): {:?} ms · özel bellek önce/tepe: {:?}/{:?} bayt",
        rep.cpu_ms_tun_measure, rep.private_bytes_before_tunnel, rep.private_bytes_peak
    );
    println!(
        "Sürücü deposu önce/sonra: {:?} / {:?} · Defender tespit sayısı önce/sonra: {:?}/{:?}",
        rep.driver_store_before,
        rep.driver_store_after,
        rep.defender_detections_before,
        rep.defender_detections_after
    );
    if let Some(t) = &rep.teardown {
        println!(
            "Söküm: adaptör yok={}, rota yok={}, geri alınan={:?}, çakışma={:?}, hata={:?}, not={:?}",
            t.adapter_absent, t.route_absent, t.reverted, t.conflicts, t.failed, t.notes
        );
    }
    println!("Sonuçlar: {}", out_dir.display());
}

/// İçsel: tüneli kurar, `READY <dizin>` yazar ve öldürülene kadar kirayı yeniler.
fn serve() -> anyhow::Result<()> {
    if std::env::var(SERVE_TOKEN_ENV).as_deref() != Ok(SERVE_TOKEN) {
        bail!("`serve` içseldir; `wintun-spike kill-test` kullanın");
    }
    let ctx = Ctx::load()?;
    harden::elevated_preconditions(&ctx.root, ctx.test_ip())?;
    preflight(&ctx.env, true).map_err(|e| anyhow!("{e}"))?;
    require_driver_store()?;
    let lib = ffi::Wintun::load(&ctx.dll, ffi::WINTUN_DLL_SHA256)?;
    let run = run_id("wintun-kill");
    let out_dir = results_dir(&ctx.root).join(&run);
    std::fs::create_dir_all(&out_dir)?;
    let mut t = Tunnel::up(&ctx, &lib, &out_dir, &run, BEST)?;
    println!("READY {}", out_dir.display());
    std::io::stdout().flush()?;
    let deadline = Instant::now() + SERVE_LIMIT;
    while Instant::now() < deadline {
        if t.beat().is_err() {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    // Öldürülmediyse kendisi söker.
    let td = t.down();
    write_json(&out_dir, "serve-teardown.json", &td)?;
    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct KillReport {
    ready_after_ms: u64,
    probe_before_kill: Option<ProbeResult>,
    adapter_absent_after_ms: Option<u64>,
    route_absent_after_ms: Option<u64>,
    adapter_absent_at_kill_plus_1s: bool,
    watchdog_report: Option<serde_json::Value>,
    stale_journals: Vec<String>,
}

/// Adım e.
fn kill_test() -> anyhow::Result<()> {
    let ctx = Ctx::load()?;
    harden::elevated_preconditions(&ctx.root, ctx.test_ip())?;
    preflight(&ctx.env, true).map_err(|e| anyhow!("{e}"))?;
    require_driver_store()?;
    let store = measure::driver_store().unwrap_or_default();
    let summary = Summary {
        scenario: "Görev 7 Adım e — spike işlemi öldürülünce bekçi geri alıyor mu".into(),
        changes: change_summary(&ctx, &store),
        revert: "Tüneli kuran alt işlem TerminateProcess ile öldürülür; ayrı işlemdeki bekçi ≤ 60 sn içinde rotayı, adresi ve kalakaldıysa adaptörü kaldırmalı. Kalmazsa `xtask lab recover`.".into(),
        probes: 2,
        bytes: 2 * PROBE_BYTES,
    };
    if !confirm(&summary) {
        println!("İptal edildi; hiçbir değişiklik yapılmadı.");
        return Ok(());
    }
    let mut rep = KillReport::default();
    let started = Instant::now();
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("serve")
        .env(SERVE_TOKEN_ENV, SERVE_TOKEN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("serve alt işlemi başlatılamadı")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("alt işlem çıktısı yok"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let out_dir = loop {
        match rx.recv_timeout(Duration::from_secs(90)) {
            Ok(line) => {
                if let Some(dir) = line.strip_prefix("READY ") {
                    break std::path::PathBuf::from(dir);
                }
                println!("  serve: {line}");
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                // Yarım kurulum olabilir: bekçinin geri almasını bekle.
                let route = route_op(ctx.test_ip());
                let gone = wait_for(REVERT_WAIT, || is_absent(&tun_op()) && is_absent(&route));
                bail!(
                    "serve 90 sn içinde hazır olmadı (kalıntı temizlendi mi: {gone}); \
                     `wintun-spike status` ve gerekirse `xtask lab recover` (lab/RUNBOOK.md)"
                );
            }
        }
    };
    rep.ready_after_ms = started.elapsed().as_millis() as u64;
    println!("Tünel hazır; TUN üzerinden bir prob…");
    rep.probe_before_kill = Some(probe::run_probe_at(
        &ctx.target,
        ctx.test_ip().into(),
        &SockTech::None,
    ));
    println!(
        "  → {:?}",
        rep.probe_before_kill.as_ref().map(|r| r.outcome)
    );
    println!("serve öldürülüyor (TerminateProcess)…");
    child.kill()?;
    let _ = child.wait();
    let killed = Instant::now();
    std::thread::sleep(Duration::from_secs(1));
    rep.adapter_absent_at_kill_plus_1s = is_absent(&tun_op());
    let route = route_op(ctx.test_ip());
    while killed.elapsed() < REVERT_WAIT
        && (rep.adapter_absent_after_ms.is_none() || rep.route_absent_after_ms.is_none())
    {
        let ms = killed.elapsed().as_millis() as u64;
        if rep.adapter_absent_after_ms.is_none() && is_absent(&tun_op()) {
            rep.adapter_absent_after_ms = Some(ms);
        }
        if rep.route_absent_after_ms.is_none() && is_absent(&route) {
            rep.route_absent_after_ms = Some(ms);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let report_path = out_dir.join("watchdog-report.json");
    wait_for(Duration::from_secs(10), || report_path.exists());
    rep.watchdog_report = std::fs::read_to_string(&report_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    rep.stale_journals = Journal::find_stale(&journal_dir(&ctx.root))
        .unwrap_or_default()
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    write_json(&out_dir, "kill-test.json", &rep)?;
    println!("\n=== Adım e özeti ===");
    println!("Hazır olma süresi: {} ms", rep.ready_after_ms);
    println!(
        "Adaptör öldürmeden 1 sn sonra yok mu: {} · adaptör yok: {:?} ms · rota yok: {:?} ms",
        rep.adapter_absent_at_kill_plus_1s, rep.adapter_absent_after_ms, rep.route_absent_after_ms
    );
    println!(
        "Bekçi raporu: {}",
        rep.watchdog_report
            .as_ref()
            .map_or("yok".into(), |v| v.to_string())
    );
    println!("Bayat journal: {:?}", rep.stale_journals);
    println!("Sonuçlar: {}", out_dir.display());
    if rep.adapter_absent_after_ms.is_none()
        || rep.route_absent_after_ms.is_none()
        || !rep.stale_journals.is_empty()
    {
        bail!(
            "geri alma tam değil; `target\\xtask\\debug\\xtask.exe lab recover` çalıştırın (lab/RUNBOOK.md)"
        );
    }
    Ok(())
}

/// Salt okunur durum.
/// S4 rota planı (salt okunur): sağlayıcı önekleri, alan adlarının sistem çözücüsüyle çözümü
/// ve DNS önbelleğinden öğrenilecek tek IP'ler. Hiçbir rota veya ayar yazılmaz; K10 onayında
/// kullanıcıya gösterilir.
fn s4_plan() -> anyhow::Result<()> {
    use std::net::ToSocketAddrs as _;
    let root = xtask::lab::cli::repo_root();
    let targets = s4::load(&root)?;
    let prefixes = s4::prefixes(&targets)?;
    let suffixes: Vec<String> = targets.iter().flat_map(|t| t.sonekler.clone()).collect();
    let mut hosts = std::collections::BTreeSet::new();
    for name in targets.iter().flat_map(|t| &t.alan_adlari) {
        match (name.as_str(), 443).to_socket_addrs() {
            Ok(addrs) => {
                for a in addrs {
                    if let std::net::IpAddr::V4(v4) = a.ip()
                        && config::validate_test_ip(v4).is_ok()
                    {
                        hosts.insert(v4);
                    }
                }
            }
            Err(e) => eprintln!("{name}: çözülemedi: {e}"),
        }
    }
    let cache = ps_query(
        "Get-DnsClientCache | Select-Object Entry, Name, Type, Data | ConvertTo-Json -Compress",
    )
    .unwrap_or_default();
    let learned = s4::learn_from_dns_cache(&cache, &suffixes)?;
    let from_cache = learned.iter().filter(|ip| !hosts.contains(*ip)).count();
    hosts.extend(learned);
    // Motorun kullanacağı küme; plan, motorun ileteceği adreslerle birebir aynıdır.
    let set = forward::RouteSet::new(&prefixes)?;
    for ip in &hosts {
        set.add_host(*ip);
    }
    println!("S4 rota planı (salt okunur; hiçbir rota yazılmadı)");
    for t in &targets {
        println!(
            "- {}: {} alan adı, {} sonek, {} önek",
            t.ad,
            t.alan_adlari.len(),
            t.sonekler.len(),
            t.onekler.len()
        );
    }
    println!("Önek rotaları ({}):", prefixes.len());
    for (net, len) in &prefixes {
        println!("  {net}/{len}");
    }
    println!(
        "Tek IP rotaları (/32): {} ({} tanesi DNS önbelleğinden)",
        set.host_count(),
        from_cache
    );
    match harden::live_prefix_conflict(&set)? {
        None => println!("Canlı çakışma (DNS sunucusu, ağ geçidi): yok"),
        Some(why) => println!("Canlı çakışma: {why} — S4 bu kümeyle başlamaz"),
    }
    Ok(())
}

fn status() -> anyhow::Result<()> {
    let ctx = Ctx::load()?;
    println!(
        "Yükseltilmiş çalıştırma ön koşulları: {}",
        match harden::elevated_preconditions(&ctx.root, ctx.test_ip()) {
            Ok(()) => "tamam".to_string(),
            Err(e) => format!("KARŞILANMIYOR: {e:#}"),
        }
    );
    println!(
        "{SPIKE_ADAPTER}: {}",
        if is_absent(&tun_op()) { "yok" } else { "VAR" }
    );
    println!(
        "Rota {}/32: {}",
        ctx.test_ip(),
        if is_absent(&route_op(ctx.test_ip())) {
            "yok"
        } else {
            "VAR"
        }
    );
    println!(
        "Wintun sürücü deposu: {:?}",
        measure::driver_store().unwrap_or_default()
    );
    println!(
        "Bayat journal: {:?}",
        Journal::find_stale(&journal_dir(&ctx.root)).unwrap_or_default()
    );
    let lib = ffi::Wintun::load(&ctx.dll, ffi::WINTUN_DLL_SHA256)?;
    println!(
        "Çalışan Wintun sürücüsü: {:?}",
        lib.running_driver_version()
    );
    Ok(())
}
