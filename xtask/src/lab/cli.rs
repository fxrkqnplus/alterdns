// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask lab` komutları: gerçek ortam (`RealEnv`), canlı prob çalıştırıcısı ve
//! senaryo akışı (ön kontrol → özet + `evet` onayı → journal + bekçi → hücreler → rapor).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

use super::dns::DnsMode;
use super::journal::{Journal, OpKind, SPIKE_ADAPTER, SystemOps};
use super::ops::{self, RealOps};
use super::probe::{self, Outcome, ProbeResult, ProbeTarget, Role};
use super::report;
use super::scenario::{
    self, Abort, Cell, CellHooks, LabEnv, ProbeRunner, Summary, confirm, estimate_bytes, job_gate,
    preflight, run_cells,
};
use super::sockettech::SockTech;
use super::watchdog::{MAX_LEASE_SECS, WatchdogHandle};

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub fn results_dir(root: &Path) -> PathBuf {
    root.join("lab").join("results")
}

pub fn journal_dir(root: &Path) -> PathBuf {
    results_dir(root).join("journal")
}

pub fn run_id(prefix: &str) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format!("{prefix}-{secs}")
}

/// `lab/local/lab.local.toml` (git dışı); yoksa varsayılan.
#[derive(Debug, Default, Deserialize)]
struct LocalConfig {
    #[serde(default)]
    history_dirs: Vec<String>,
}

fn local_config(root: &Path) -> anyhow::Result<LocalConfig> {
    let p = root.join("lab").join("local").join("lab.local.toml");
    if !p.exists() {
        return Ok(LocalConfig::default());
    }
    let cfg: LocalConfig = toml::from_str(&std::fs::read_to_string(&p)?)
        .with_context(|| format!("{} ayrıştırılamadı", p.display()))?;
    if cfg.history_dirs.iter().any(|d| d.contains('|')) {
        bail!("history_dirs girdileri '|' içeremez");
    }
    Ok(cfg)
}

/// `data/probes/*.toml` + varsa `lab/local/probes.local.toml`.
pub fn load_targets(root: &Path) -> anyhow::Result<Vec<ProbeTarget>> {
    let mut all = Vec::new();
    let mut files: Vec<PathBuf> = ["control", "discord", "roblox"]
        .iter()
        .map(|n| root.join("data").join("probes").join(format!("{n}.toml")))
        .collect();
    let local = root.join("lab").join("local").join("probes.local.toml");
    if local.exists() {
        files.push(local);
    }
    for f in files {
        let text =
            std::fs::read_to_string(&f).with_context(|| format!("{} okunamadı", f.display()))?;
        all.extend(probe::parse_targets(&text).with_context(|| f.display().to_string())?);
    }
    Ok(all)
}

/// Gerçek sistem. Sorgu başarısız olursa güvenli tarafta kalınır (koşu başlamaz).
pub struct RealEnv {
    journal_dir: PathBuf,
    control: ProbeTarget,
}

impl LabEnv for RealEnv {
    fn is_elevated(&self) -> bool {
        ops::ps_query(
            "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
        )
        .is_ok_and(|v| v == "True")
    }

    fn stale_journals(&self) -> Vec<PathBuf> {
        Journal::find_stale(&self.journal_dir).unwrap_or_else(|e| {
            eprintln!("uyarı: journal dizini okunamadı, bayat sayıldı: {e:#}");
            vec![self.journal_dir.clone()]
        })
    }

    fn foreign_process(&self, image: &str) -> bool {
        let stem = image.trim_end_matches(".exe");
        let answer = ops::quoted(stem).and_then(|q| {
            ops::ps_query(&format!(
                "if (Get-Process -Name {q} -ErrorAction SilentlyContinue) {{ 'present' }} else {{ 'absent' }}"
            ))
        });
        match answer {
            Ok(v) => v != "absent",
            Err(e) => {
                eprintln!("uyarı: {image} denetlenemedi, var sayıldı: {e:#}");
                true
            }
        }
    }

    fn windivert_service(&self) -> Option<String> {
        match ops::ps_query(
            "Get-CimInstance -ClassName Win32_SystemDriver -Filter \"Name LIKE 'WinDivert%'\" | Select-Object -First 1 -ExpandProperty Name",
        ) {
            Ok(v) if v.is_empty() => None,
            Ok(v) => Some(v),
            Err(e) => Some(format!("sorgu başarısız: {e}")),
        }
    }

    fn spike_adapter_present(&self) -> bool {
        match RealOps.read(&OpKind::TunAdapter {
            name: SPIKE_ADAPTER.into(),
        }) {
            Ok(v) => v != "absent",
            Err(e) => {
                eprintln!("uyarı: {SPIKE_ADAPTER} denetlenemedi, var sayıldı: {e:#}");
                true
            }
        }
    }

    fn control_probe(&self) -> Outcome {
        probe::run_probe(&self.control, DnsMode::System).outcome
    }
}

pub fn real_env(root: &Path, targets: &[ProbeTarget]) -> anyhow::Result<RealEnv> {
    let control = targets
        .iter()
        .find(|t| t.role == Role::Control)
        .cloned()
        .ok_or_else(|| anyhow!("data/probes/control.toml içinde kontrol probu yok"))?;
    Ok(RealEnv {
        journal_dir: journal_dir(root),
        control,
    })
}

struct LiveRunner;

impl ProbeRunner for LiveRunner {
    fn probe(&self, t: &ProbeTarget, dns: DnsMode, tech: &SockTech) -> ProbeResult {
        probe::run_probe_with(t, dns, tech)
    }
}

struct LiveHooks<'a> {
    wd: &'a mut WatchdogHandle,
    journal: &'a mut Journal,
    out: std::fs::File,
    done: u64,
    total: u64,
}

impl CellHooks for LiveHooks<'_> {
    fn heartbeat(&mut self) -> anyhow::Result<()> {
        self.wd.renew(MAX_LEASE_SECS)
    }

    fn on_row(&mut self, row: &ProbeResult) -> anyhow::Result<()> {
        writeln!(self.out, "{}", serde_json::to_string(row)?)?;
        self.done += 1;
        println!(
            "[{}/{}] {} · {:?} · {:?} → {:?}",
            self.done, self.total, row.target, row.tech, row.dns_mode, row.outcome
        );
        Ok(())
    }

    fn on_abort(&mut self, why: &Abort) {
        eprintln!("Koşu durduruldu: {why}");
        match self.journal.revert_all(&RealOps) {
            Ok(r) => eprintln!(
                "Journal geri alındı: {} öğe, {} çakışma, {} hata",
                r.reverted.len(),
                r.conflicts.len(),
                r.failed.len()
            ),
            Err(e) => eprintln!("Journal geri alınamadı: {e:#}"),
        }
    }
}

/// Senaryo akışı. Sistem değişikliği yapan senaryolar (Görev 7, 9) `changes_system = true`
/// ile gelir: bekçi ebeveynin iş nesnesinden ayrılamazsa başlamazlar (`job_gate`).
#[allow(clippy::too_many_arguments)]
fn run_scenario(
    title: &str,
    prefix: &str,
    changes: &str,
    changes_system: bool,
    cells: &[Cell],
    targets: &[ProbeTarget],
    env: &RealEnv,
    extra: &mut dyn FnMut(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let root = repo_root();
    preflight(env, false).map_err(|e| anyhow!("{e}"))?;
    let per_rep = u64::try_from(targets.len()).unwrap_or(u64::MAX);
    let probes = cells
        .iter()
        .map(|c| u64::from(c.reps).saturating_mul(per_rep))
        .sum();
    let summary = Summary {
        scenario: title.to_string(),
        changes: changes.to_string(),
        revert: if changes_system {
            format!(
                "Her değişiklik önce journal'a yazılır; ayrı işlemdeki bekçi (kira ≤ {MAX_LEASE_SECS} sn) Ctrl+C, çökme veya süre dolunca geri alır."
            )
        } else {
            format!(
                "Sistem değişikliği yok. Bekçi yine de ayrı işlemde çalışır (kira ≤ {MAX_LEASE_SECS} sn); Ctrl+C veya çökmede journal'ı denetler."
            )
        },
        probes,
        bytes: estimate_bytes(cells, targets.len()),
    };
    if !confirm(&summary) {
        println!("İptal edildi; hiçbir prob gönderilmedi.");
        return Ok(());
    }
    let id = run_id(prefix);
    let out_dir = results_dir(&root).join(&id);
    std::fs::create_dir_all(&out_dir)?;
    extra(&out_dir)?;
    let mut journal = Journal::create(&journal_dir(&root), &id)?;
    let mut wd = WatchdogHandle::spawn(journal.path(), &out_dir)?;
    if let Err(why) = job_gate(wd.outside_job(), changes_system) {
        let _ = wd.done();
        bail!("{why}");
    }
    if !wd.outside_job() {
        eprintln!(
            "Uyarı: bekçi ebeveynin iş nesnesinden ayrılamadı (ör. `cargo xtask` altında). Bu senaryo sistem değiştirmediği için devam ediliyor; Ctrl+C sonrasında bekçi raporu yazılmayabilir. Ctrl+C denemesi için ikiliyi doğrudan çalıştırın (lab/RUNBOOK.md)."
        );
    }
    wd.renew(MAX_LEASE_SECS)?;
    let outcome = {
        let mut hooks = LiveHooks {
            wd: &mut wd,
            journal: &mut journal,
            out: std::fs::File::create(out_dir.join(format!("{prefix}.jsonl")))?,
            done: 0,
            total: probes,
        };
        run_cells(env, &LiveRunner, cells, targets, &mut hooks)
    };
    // Bekçi bu arada ölmüş olabilir; sonuç dosyaları yine yazılır.
    let watchdog_done = wd.done().map_err(|e| format!("{e:#}"));
    if let Err(e) = &watchdog_done {
        eprintln!("Uyarı: bekçiye DONE iletilemedi: {e}");
    }
    std::fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "scenario": title,
            "cells": cells,
            "targets": targets.iter().map(|t| &t.name).collect::<Vec<_>>(),
            "rows": outcome.rows,
            "aborted": outcome.aborted.as_ref().map(ToString::to_string),
            "watchdog_done_error": watchdog_done.err(),
        }))?,
    )?;
    let rows = report::read_rows(&out_dir)?;
    std::fs::write(out_dir.join("rapor.md"), report::summarize(&rows))?;
    println!("Sonuçlar: {}", out_dir.display());
    match outcome.aborted {
        Some(a) => bail!("koşu durdu: {a}"),
        None => Ok(()),
    }
}

/// S0: salt okunur envanter.
fn inventory() -> anyhow::Result<()> {
    let root = repo_root();
    let cfg = local_config(&root)?;
    let mut cmd = Command::new(ops::powershell());
    cmd.args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(root.join("lab").join("scripts").join("s0-inventory.ps1"));
    if !cfg.history_dirs.is_empty() {
        cmd.arg("-HistoryDirs").arg(cfg.history_dirs.join("|"));
    }
    let out = cmd.output().context("powershell.exe çalıştırılamadı")?;
    if !out.status.success() {
        bail!(
            "S0 betiği başarısız: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let dir = results_dir(&root).join(run_id("s0"));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("s0-inventory.json");
    std::fs::write(&path, &out.stdout)?;
    let json = out
        .stdout
        .strip_prefix(b"\xef\xbb\xbf")
        .unwrap_or(&out.stdout);
    let v: serde_json::Value = serde_json::from_slice(json).context("S0 çıktısı JSON değil")?;
    println!("S0 envanteri: {}", path.display());
    if let Some(obj) = v.as_object() {
        for (k, section) in obj {
            println!("  {k}: {}", section_state(section));
        }
    }
    Ok(())
}

/// S0 bölümünün özeti. PowerShell 5.1 boş diziyi `{}`, tek öğeli diziyi nesne olarak yazar.
fn section_state(section: &serde_json::Value) -> String {
    use serde_json::Value;
    let denied = "erişilemedi (yönetici gerekebilir)".to_string();
    match section {
        Value::Array(a) if a.iter().any(|x| x.get("hata").is_some()) => denied,
        Value::Array(a) => format!("{} kayıt", a.len()),
        Value::Object(o) if o.contains_key("hata") => denied,
        Value::Object(o) if o.is_empty() => "0 kayıt".to_string(),
        Value::Null => "0 kayıt".to_string(),
        _ => "1 kayıt".to_string(),
    }
}

fn baseline() -> anyhow::Result<()> {
    let root = repo_root();
    let targets = load_targets(&root)?;
    let env = real_env(&root, &targets)?;
    run_scenario(
        "S1 — motor kapalı temel ölçüm (DNS: sistem, DoH, 77.88.8.8:1253)",
        "s1",
        "Yok. Yalnızca problar gönderilir; DNS ayarına, rotaya ve hop limite dokunulmaz.",
        false,
        &scenario::s1_cells(),
        &targets,
        &env,
        &mut |_| Ok(()),
    )
}

/// Path MTU (H9): DF bayraklı `ping` ile ikili arama; sistem ayarına dokunmaz.
fn path_mtu(host: &str) -> Option<u32> {
    scenario::bisect_mtu(576, 1500, &mut |mtu| {
        Command::new(ops::system32("PING.EXE"))
            .args([
                "-n",
                "1",
                "-w",
                "1500",
                "-f",
                "-l",
                &(mtu - 28).to_string(),
                host,
            ])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("TTL="))
    })
}

fn techniques() -> anyhow::Result<()> {
    let root = repo_root();
    let all = load_targets(&root)?;
    let env = real_env(&root, &all)?;
    let targets: Vec<ProbeTarget> = all.into_iter().filter(|t| t.role == Role::Target).collect();
    run_scenario(
        "S2-U — sürücüsüz soket teknikleri (DNS: sistem, DoH)",
        "s2u",
        "Yok. IP_TTL yalnızca probun kendi soketinde, tek yazma için değişir; sistem hop limiti okunmaz bile.",
        false,
        &scenario::s2u_cells(),
        &targets,
        &env,
        &mut |dir| {
            println!("Path MTU ölçülüyor (DF bayraklı ping, 1.1.1.1)…");
            let mtu = path_mtu("1.1.1.1");
            println!("Path MTU: {mtu:?}");
            std::fs::write(
                dir.join("pmtu.json"),
                serde_json::to_string(&serde_json::json!({ "host": "1.1.1.1", "mtu": mtu }))?,
            )?;
            Ok(())
        },
    )
}

fn recover() -> anyhow::Result<()> {
    let root = repo_root();
    let stale = Journal::find_stale(&journal_dir(&root))?;
    if stale.is_empty() {
        println!("Bitmemiş journal yok.");
        return Ok(());
    }
    let mut open_problems = 0usize;
    for p in stale {
        // Her dosya ayrı ele alınır: biri bozuksa diğerleri yine geri alınır.
        let mut j = match Journal::open(&p) {
            Ok(j) => j,
            Err(e) => {
                open_problems += 1;
                eprintln!(
                    "{}: okunamadı: {e:#}\n  Onarım: lab/RUNBOOK.md, \"Bozuk journal\"",
                    p.display()
                );
                continue;
            }
        };
        let r = match j.revert_all(&RealOps) {
            Ok(r) => r,
            Err(e) => {
                open_problems += 1;
                eprintln!("{}: geri alma başlatılamadı: {e:#}", p.display());
                continue;
            }
        };
        println!("{}: {} kayıt geri alındı", p.display(), r.reverted.len());
        for (id, err) in &r.failed {
            open_problems += 1;
            println!(
                "  kayıt {id}: geri alınamadı ({err}); kayıt açık kaldı, nedeni giderip yeniden çalıştırın"
            );
        }
        if r.conflicts.is_empty() {
            continue;
        }
        for e in j.entries().iter().filter(|e| r.conflicts.contains(&e.id)) {
            println!(
                "  çakışma {}: {:?}; önceki {:?}, bizim yazdığımız {:?}. Şu anki değer ikisinden farklı ya da öğe lab'a ait değil; dokunulmadı.",
                e.id, e.op, e.before, e.after
            );
        }
        let question = format!(
            "{} çakışma var. Değerlere dokunmadan bu kayıtlar kapatılsın mı?",
            r.conflicts.len()
        );
        if scenario::ask_evet(&question) {
            let ids = j.acknowledge_conflicts()?;
            println!("  {} kayıt kapatıldı.", ids.len());
        } else {
            open_problems += 1;
            println!("  Çakışmalar açık kaldı; yeni koşu başlamaz.");
        }
    }
    if open_problems > 0 {
        bail!("{open_problems} sorun açık kaldı (lab/RUNBOOK.md)");
    }
    Ok(())
}

/// Bekçinin ebeveynin iş nesnesinden ayrılıp ayrılamadığını, boş bir geçici journal'la sınar.
fn watchdog_job_check() -> anyhow::Result<bool> {
    let dir = std::env::temp_dir().join(format!("alterdns-wdcheck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let result = (|| {
        let j = Journal::create(&dir.join("journal"), "wdcheck")?;
        let wd = WatchdogHandle::spawn(j.path(), &dir.join("results"))?;
        let outside = wd.outside_job();
        wd.done()?;
        anyhow::Ok(outside)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Salt okunur ön kontrol: eski journal, lab dışı DPI aracı, WinDivert servisi, kontrol probu.
fn preflight_cmd() -> anyhow::Result<()> {
    let root = repo_root();
    let targets = load_targets(&root)?;
    let env = real_env(&root, &targets)?;
    println!(
        "Yönetici: {}",
        if env.is_elevated() { "evet" } else { "hayır" }
    );
    println!(
        "{} adaptörü: {}",
        SPIKE_ADAPTER,
        if env.spike_adapter_present() {
            "var"
        } else {
            "yok"
        }
    );
    match watchdog_job_check() {
        Ok(true) => {
            println!("Bekçi: iş nesnesi dışında (sistem değiştiren senaryolar başlayabilir)")
        }
        Ok(false) => println!(
            "Bekçi: ebeveynin iş nesnesinde (ör. `cargo xtask` altında); sistem değiştiren senaryolar başlamaz, ikiliyi doğrudan çalıştırın (lab/RUNBOOK.md)"
        ),
        Err(e) => println!("Bekçi denetlenemedi: {e:#}"),
    }
    match preflight(&env, false) {
        Ok(()) => {
            println!("Ön kontrol geçti.");
            Ok(())
        }
        Err(e) => bail!("ön kontrol: {e}"),
    }
}

fn clean_bin() -> anyhow::Result<()> {
    let root = repo_root();
    let targets = load_targets(&root)?;
    scenario::lab_bin_cleanup(&real_env(&root, &targets)?, &root.join("lab").join("bin"))?;
    println!("lab/bin temizlendi.");
    Ok(())
}

/// `cargo xtask lab <komut>`.
pub fn dispatch(cmd: Option<&str>, rest: &[String]) -> anyhow::Result<()> {
    match cmd {
        Some("inventory") => inventory(),
        Some("baseline") => baseline(),
        Some("techniques") => techniques(),
        Some("recover") => recover(),
        Some("report") => report::main(rest),
        Some("clean-bin") => clean_bin(),
        Some("preflight") => preflight_cmd(),
        Some("watchdog") => super::watchdog::main(rest),
        other => bail!(
            "bilinmeyen lab komutu: {other:?}\nkomutlar: preflight | inventory | baseline | techniques | recover | report <koşu-dizini> | clean-bin"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s0_section_state_reads_powershell_shapes() {
        use serde_json::json;
        assert_eq!(section_state(&json!({})), "0 kayıt");
        assert_eq!(section_state(&json!(null)), "0 kayıt");
        assert_eq!(section_state(&json!({ "Name": "x" })), "1 kayıt");
        assert_eq!(section_state(&json!([1, 2])), "2 kayıt");
        assert!(section_state(&json!({ "hata": "erişim reddedildi" })).starts_with("erişilemedi"));
    }

    #[test]
    fn repo_probe_files_parse_with_control_first() {
        let targets = load_targets(&repo_root()).unwrap();
        assert!(targets.iter().any(|t| t.role == Role::Control));
        assert!(targets.iter().filter(|t| t.role == Role::Target).count() >= 6);
        assert_eq!(targets[0].role, Role::Control);
    }
}
