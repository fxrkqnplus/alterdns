// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Senaryo katmanı (CONNECTIVITY_RESEARCH §4): ön kontrol, hücre döngüsü, veri tahmini, onay.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::dns::DnsMode;
use super::probe::{Outcome, ProbeResult, ProbeTarget};
use super::sockettech::SockTech;

/// Lab dışından çalışıyorsa hücre başlatılmayan DPI araçları.
pub const FOREIGN_DPI: [&str; 2] = ["goodbyedpi.exe", "winws.exe"];
/// Prob başına tahmini veri (TLS el sıkışması + sertifika zinciri + durum satırı), üst sınır.
pub const PROBE_BYTES: u64 = 8 * 1024;
/// DoH çözümünün prob başına ek maliyeti (ayrı TLS bağlantısı).
pub const DOH_EXTRA_BYTES: u64 = 7 * 1024;
/// Her hücreden önceki kontrol probu.
pub const CONTROL_BYTES: u64 = PROBE_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub tech: SockTech,
    pub dns: DnsMode,
    pub reps: u8,
    /// Probun kendi soketinin taban `IP_TTL`'i; `None` = sistem varsayılanı. Sistem hop
    /// limiti hiçbir zaman yazılmaz (ADR-010, ADR-014).
    pub socket_ttl: Option<u8>,
}

/// Lab'ın gördüğü sistem; testlerde sahte uygulama kullanılır.
pub trait LabEnv {
    fn is_elevated(&self) -> bool;
    fn stale_journals(&self) -> Vec<PathBuf>;
    fn foreign_process(&self, image: &str) -> bool;
    fn windivert_service(&self) -> Option<String>;
    fn spike_adapter_present(&self) -> bool;
    fn control_probe(&self) -> Outcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abort {
    NotElevated,
    StaleJournal,
    OtherDpiRunning(String),
    WinDivertPresent(String),
    NoInternet,
    WatchdogLost(String),
    /// Bekçi ebeveynin iş nesnesinden ayrılamadı; ebeveyn ölünce bekçi de ölebilir.
    WatchdogInJob,
}

impl std::fmt::Display for Abort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Abort::NotElevated => write!(f, "bu senaryo yönetici terminali gerektiriyor"),
            Abort::StaleJournal => write!(
                f,
                "bitmemiş journal var; önce `cargo xtask lab recover` çalıştırın"
            ),
            Abort::OtherDpiRunning(p) => write!(
                f,
                "lab dışından çalışan DPI aracı var ({p}); ona dokunulmadı, senaryo başlamadı"
            ),
            Abort::WinDivertPresent(s) => write!(
                f,
                "WinDivert sürücü servisi var ({s}); ona dokunulmadı, senaryo başlamadı"
            ),
            Abort::NoInternet => write!(
                f,
                "kontrol probu başarısız (internet yok veya captive portal); hiçbir hedef engelli sayılmadı"
            ),
            Abort::WatchdogLost(e) => write!(f, "geri alma bekçisine ulaşılamadı: {e}"),
            Abort::WatchdogInJob => write!(
                f,
                "geri alma bekçisi ebeveynin iş nesnesinden ayrılamadı (ör. `cargo xtask` altında); \
                 sistem değiştiren senaryo başlamadı. Derlenmiş ikiliyi doğrudan çalıştırın: \
                 `target\\xtask\\debug\\xtask.exe lab ...` (lab/RUNBOOK.md)"
            ),
        }
    }
}

/// Bekçi kapısı: sistem değiştiren senaryo yalnızca bekçi iş nesnesi dışındaysa başlar.
/// Sistem değiştirmeyen senaryolarda (S1, S2-U) uyarı yeter.
pub fn job_gate(outside_job: bool, changes_system: bool) -> Result<(), Abort> {
    if changes_system && !outside_job {
        return Err(Abort::WatchdogInJob);
    }
    Ok(())
}

impl std::error::Error for Abort {}

/// Hücreden önceki denetim: lab dışı DPI aracı, WinDivert servisi ve kontrol probu.
fn cell_guard(env: &dyn LabEnv) -> Result<(), Abort> {
    for image in FOREIGN_DPI {
        if env.foreign_process(image) {
            return Err(Abort::OtherDpiRunning(image.to_string()));
        }
    }
    if let Some(service) = env.windivert_service() {
        return Err(Abort::WinDivertPresent(service));
    }
    if !env.control_probe().is_ok() {
        return Err(Abort::NoInternet);
    }
    Ok(())
}

/// Senaryo öncesi denetim: yönetici yetkisi (gerekiyorsa), eski journal, sonra hücre denetimi.
pub fn preflight(env: &dyn LabEnv, needs_admin: bool) -> Result<(), Abort> {
    if needs_admin && !env.is_elevated() {
        return Err(Abort::NotElevated);
    }
    if !env.stale_journals().is_empty() {
        return Err(Abort::StaleJournal);
    }
    cell_guard(env)
}

/// Tahmini veri tüketimi (üst sınır): hedef probları + her hücreden önceki kontrol probu.
pub fn estimate_bytes(cells: &[Cell], targets: usize) -> u64 {
    let targets = u64::try_from(targets).unwrap_or(u64::MAX);
    cells
        .iter()
        .map(|c| {
            let per_probe = PROBE_BYTES
                + if c.dns == DnsMode::Doh {
                    DOH_EXTRA_BYTES
                } else {
                    0
                };
            u64::from(c.reps)
                .saturating_mul(targets)
                .saturating_mul(per_probe)
                .saturating_add(CONTROL_BYTES)
        })
        .fold(0u64, u64::saturating_add)
}

/// S1: motor kapalı; DNS {sistem, DoH, 77.88.8.8:1253} × 3 tekrar.
pub fn s1_cells() -> Vec<Cell> {
    [DnsMode::System, DnsMode::Doh, DnsMode::AltUdp1253]
        .into_iter()
        .map(|dns| Cell {
            tech: SockTech::None,
            dns,
            reps: 3,
            socket_ttl: None,
        })
        .collect()
}

/// S2-U: teknik × bölme konumu {sabit ofset, SNI ortası} × DNS {sistem, DoH} × 3 tekrar.
pub fn s2u_cells() -> Vec<Cell> {
    use super::sockettech::SplitAt;
    let mut techs = vec![SockTech::None];
    for at in [SplitAt::Offset(1), SplitAt::SniMiddle] {
        techs.push(SockTech::Split { at });
        techs.push(SockTech::TlsRecordSplit { at });
        techs.push(SockTech::Oob { at });
        techs.extend((1..=8).map(|ttl| SockTech::Disorder { at, ttl }));
    }
    techs
        .into_iter()
        .flat_map(|tech| {
            [DnsMode::System, DnsMode::Doh]
                .into_iter()
                .map(move |dns| Cell {
                    tech,
                    dns,
                    reps: 3,
                    socket_ttl: None,
                })
        })
        .collect()
}

/// `[lo, hi]` aralığında `ok` olan en büyük MTU (ikili arama; `ok` tek düzeli varsayılır).
pub fn bisect_mtu(lo: u32, hi: u32, ok: &mut dyn FnMut(u32) -> bool) -> Option<u32> {
    if lo > hi || !ok(lo) {
        return None;
    }
    let (mut good, mut bad) = (lo, hi + 1);
    while bad - good > 1 {
        let mid = good + (bad - good) / 2;
        if ok(mid) {
            good = mid;
        } else {
            bad = mid;
        }
    }
    Some(good)
}

/// Hücre döngüsünün yan etkileri: kira yenileme, satır yazma, geri alma.
pub trait CellHooks {
    /// Her hücre ve her probdan önce çağrılır (bekçi kirası). Hata koşuyu durdurur.
    fn heartbeat(&mut self) -> anyhow::Result<()>;
    fn on_row(&mut self, row: &ProbeResult) -> anyhow::Result<()>;
    /// Koşu durdurulduğunda journal'ı geri alır.
    fn on_abort(&mut self, why: &Abort);
}

/// Bir prob çalıştırıcısı; testlerde sahte uygulama kullanılır.
/// S5-soket (H7): tekniksiz ve S2-U'nun en iyi 3 tekniği, probun kendi soketinde taban TTL
/// 64 / 65 / 128 ile; sistem DNS'i, 3 tekrar. Sistem hop limiti değişmez (D5 kapandı).
pub fn s5_cells() -> Vec<Cell> {
    use super::sockettech::SplitAt;
    let techs = [
        SockTech::None,
        SockTech::TlsRecordSplit {
            at: SplitAt::SniMiddle,
        },
        SockTech::Oob {
            at: SplitAt::SniMiddle,
        },
        SockTech::Split {
            at: SplitAt::Offset(1),
        },
    ];
    [64u8, 65, 128]
        .into_iter()
        .flat_map(|ttl| {
            techs.into_iter().map(move |tech| Cell {
                tech,
                dns: DnsMode::System,
                reps: 3,
                socket_ttl: Some(ttl),
            })
        })
        .collect()
}

pub trait ProbeRunner {
    fn probe(&self, t: &ProbeTarget, cell: &Cell) -> ProbeResult;
}

#[derive(Debug, PartialEq, Eq)]
pub struct RunOutcome {
    pub rows: usize,
    pub aborted: Option<Abort>,
}

/// Hücre döngüsü. Her hücreden önce `cell_guard` çalışır; başarısızsa o hücrenin hiçbir hedef
/// satırı kaydedilmez, `on_abort` (journal geri alma) çağrılır ve koşu durur.
pub fn run_cells(
    env: &dyn LabEnv,
    runner: &dyn ProbeRunner,
    cells: &[Cell],
    targets: &[ProbeTarget],
    hooks: &mut dyn CellHooks,
) -> RunOutcome {
    let mut rows = 0;
    let stop = |hooks: &mut dyn CellHooks, why: Abort, rows: usize| {
        hooks.on_abort(&why);
        RunOutcome {
            rows,
            aborted: Some(why),
        }
    };
    for cell in cells {
        if let Err(e) = hooks.heartbeat() {
            return stop(hooks, Abort::WatchdogLost(format!("{e:#}")), rows);
        }
        if let Err(why) = cell_guard(env) {
            return stop(hooks, why, rows);
        }
        // Denetim (PowerShell + kontrol probu) uzun sürmüş olabilir: kira yeniden tazelenir.
        if let Err(e) = hooks.heartbeat() {
            return stop(hooks, Abort::WatchdogLost(format!("{e:#}")), rows);
        }
        for _ in 0..cell.reps {
            for t in targets {
                if let Err(e) = hooks.heartbeat() {
                    return stop(hooks, Abort::WatchdogLost(format!("{e:#}")), rows);
                }
                let row = runner.probe(t, cell);
                if let Err(e) = hooks.on_row(&row) {
                    return stop(
                        hooks,
                        Abort::WatchdogLost(format!("sonuç yazılamadı: {e:#}")),
                        rows,
                    );
                }
                rows += 1;
            }
        }
    }
    RunOutcome {
        rows,
        aborted: None,
    }
}

/// Onaydan önce kullanıcıya gösterilen özet.
#[derive(Debug, Clone)]
pub struct Summary {
    pub scenario: String,
    pub changes: String,
    pub revert: String,
    pub probes: u64,
    pub bytes: u64,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Senaryo: {}", self.scenario)?;
        writeln!(f, "Ne değişecek: {}", self.changes)?;
        writeln!(f, "Geri alma: {}", self.revert)?;
        writeln!(f, "Prob sayısı: {}", self.probes)?;
        write!(
            f,
            "Tahmini veri (üst sınır): {:.1} MB",
            self.bytes as f64 / (1024.0 * 1024.0)
        )
    }
}

/// Özeti gösterir; yalnızca tam olarak `evet` yazılırsa kabul eder.
pub fn confirm_with(
    summary: &Summary,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
) -> bool {
    ask_evet_with(&summary.to_string(), input, out)
}

/// Soruyu gösterir; yalnızca tam olarak `evet` yazılırsa kabul eder.
pub fn ask_evet_with(
    question: &str,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
) -> bool {
    let _ = writeln!(out, "{question}");
    let _ = write!(out, "Devam etmek için `evet` yazın: ");
    let _ = out.flush();
    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return false;
    }
    line.trim_end_matches(['\r', '\n']) == "evet"
}

/// Standart giriş/çıkışla `evet` sorusu.
pub fn ask_evet(question: &str) -> bool {
    ask_evet_with(
        question,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
    )
}

/// Standart giriş/çıkışla onay.
pub fn confirm(summary: &Summary) -> bool {
    confirm_with(
        summary,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
    )
}

/// `lab/bin`'i siler; `AlterDNS-Spike` adaptörü varken reddeder (DLL yüklüyken silinmez).
pub fn lab_bin_cleanup(env: &dyn LabEnv, bin: &Path) -> anyhow::Result<()> {
    if env.spike_adapter_present() {
        anyhow::bail!(
            "{} adaptörü hâlâ var; önce `cargo xtask lab recover` çalıştırın",
            super::journal::SPIKE_ADAPTER
        );
    }
    match std::fs::remove_dir_all(bin) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Salt okunur betikte izin verilen komutlar (`Get-*` dışında; küçük harf).
const PS_COMMANDS_ALLOWED: [&str; 6] = [
    "select-object",
    "where-object",
    "foreach-object",
    "sort-object",
    "measure-object",
    "convertto-json",
];
/// İzin verilen .NET üye çağrıları (yan etkisiz dönüşümler).
const PS_MEMBERS_ALLOWED: [&str; 2] = ["ToString", "ToCharArray"];
/// İzin verilen dinamik çağrı (S0'ın bölüm yardımcısı).
const PS_DYNAMIC_ALLOWED: [&str; 1] = ["& $Block"];

/// PowerShell betiğinin salt okunurluk ihlalleri, PowerShell'in kendi AST'sinden
/// (`lab/scripts/ps-ast-dump.ps1`; betik çalıştırılmaz). İzin listesi: `Get-*` ve
/// `PS_COMMANDS_ALLOWED` komutları, `PS_MEMBERS_ALLOWED` üye çağrıları, yerel değişkene atama.
/// Takma adlar (`rm`, `%`), yerel araçlar, diğer .NET çağrıları, özellik/ortam atamaları,
/// yönlendirmeler ve ayrıştırma hataları ihlaldir.
pub fn ps_script_violations(script: &Path) -> anyhow::Result<Vec<String>> {
    let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lab/scripts/ps-ast-dump.ps1");
    let out = std::process::Command::new(super::ops::powershell())
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(&dump)
        .arg("-Path")
        .arg(script)
        .output()?;
    if !out.status.success() {
        anyhow::bail!(
            "AST dökümü başarısız: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let mut found = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((kind, value)) = line.trim_end().split_once('|') else {
            continue;
        };
        let ok = match kind {
            "command" => {
                let l = value.to_ascii_lowercase();
                l.starts_with("get-") || PS_COMMANDS_ALLOWED.contains(&l.as_str())
            }
            "member" => PS_MEMBERS_ALLOWED.contains(&value),
            "dynamic" => PS_DYNAMIC_ALLOWED.contains(&value.trim()),
            "assign-var" => true,
            _ => false,
        };
        if !ok {
            found.push(format!("{kind}: {value}"));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::probe::Role;
    use crate::lab::sockettech::SplitAt;
    use std::cell::{Cell as StdCell, RefCell};

    #[derive(Default)]
    struct FakeEnv {
        elevated: bool,
        stale: Vec<PathBuf>,
        foreign: Vec<String>,
        windivert: Option<String>,
        adapter: bool,
        /// Sırayla döndürülecek kontrol probu sonuçları; biterse `Ok`.
        controls: RefCell<Vec<Outcome>>,
    }

    impl LabEnv for FakeEnv {
        fn is_elevated(&self) -> bool {
            self.elevated
        }
        fn stale_journals(&self) -> Vec<PathBuf> {
            self.stale.clone()
        }
        fn foreign_process(&self, image: &str) -> bool {
            self.foreign.iter().any(|f| f == image)
        }
        fn windivert_service(&self) -> Option<String> {
            self.windivert.clone()
        }
        fn spike_adapter_present(&self) -> bool {
            self.adapter
        }
        fn control_probe(&self) -> Outcome {
            let mut c = self.controls.borrow_mut();
            if c.is_empty() {
                Outcome::Ok { http_status: 204 }
            } else {
                c.remove(0)
            }
        }
    }

    struct FakeRunner {
        calls: StdCell<usize>,
    }

    impl ProbeRunner for FakeRunner {
        fn probe(&self, t: &ProbeTarget, c: &Cell) -> ProbeResult {
            self.calls.set(self.calls.get() + 1);
            ProbeResult {
                target: t.name.clone(),
                domain: t.domain.clone(),
                dns_mode: c.dns,
                tech: c.tech,
                socket_ttl: c.socket_ttl,
                addrs: Vec::new(),
                outcome: Outcome::TlsReset,
                dns_ms: None,
                tcp_ms: None,
                tls_ms: None,
                total_ms: 0,
                bytes_in: 0,
                detail: None,
            }
        }
    }

    #[derive(Default)]
    struct RecHooks {
        beats: usize,
        rows: Vec<String>,
        aborts: Vec<Abort>,
    }

    impl CellHooks for RecHooks {
        fn heartbeat(&mut self) -> anyhow::Result<()> {
            self.beats += 1;
            Ok(())
        }
        fn on_row(&mut self, row: &ProbeResult) -> anyhow::Result<()> {
            self.rows.push(row.target.clone());
            Ok(())
        }
        fn on_abort(&mut self, why: &Abort) {
            self.aborts.push(why.clone());
        }
    }

    fn target(name: &str) -> ProbeTarget {
        ProbeTarget {
            name: name.into(),
            domain: format!("{name}.example"),
            port: 443,
            path: "/".into(),
            role: Role::Target,
        }
    }

    fn cell(dns: DnsMode, reps: u8) -> Cell {
        Cell {
            tech: SockTech::None,
            dns,
            reps,
            socket_ttl: None,
        }
    }

    /// Hücrenin soket TTL'ini kaydeden koşucu.
    struct TtlRunner {
        seen: std::cell::RefCell<Vec<Option<u8>>>,
    }

    impl ProbeRunner for TtlRunner {
        fn probe(&self, t: &ProbeTarget, c: &Cell) -> ProbeResult {
            self.seen.borrow_mut().push(c.socket_ttl);
            FakeRunner {
                calls: StdCell::new(0),
            }
            .probe(t, c)
        }
    }

    #[test]
    fn s5_cells_vary_only_socket_ttl() {
        let cells = s5_cells();
        assert_eq!(cells.len(), 12);
        let best = vec![
            SockTech::None,
            SockTech::TlsRecordSplit {
                at: SplitAt::SniMiddle,
            },
            SockTech::Oob {
                at: SplitAt::SniMiddle,
            },
            SockTech::Split {
                at: SplitAt::Offset(1),
            },
        ];
        for ttl in [64u8, 65, 128] {
            let techs: Vec<SockTech> = cells
                .iter()
                .filter(|c| c.socket_ttl == Some(ttl))
                .map(|c| c.tech)
                .collect();
            assert_eq!(techs, best, "ttl {ttl}");
        }
        assert!(
            cells
                .iter()
                .all(|c| c.dns == DnsMode::System && c.reps == 3)
        );
    }

    #[test]
    fn s1_and_s2u_keep_system_ttl() {
        assert!(s1_cells().iter().all(|c| c.socket_ttl.is_none()));
        assert!(s2u_cells().iter().all(|c| c.socket_ttl.is_none()));
    }

    #[test]
    fn run_cells_passes_socket_ttl_to_runner() {
        let env = FakeEnv::default();
        let runner = TtlRunner {
            seen: std::cell::RefCell::new(Vec::new()),
        };
        let mut c = cell(DnsMode::System, 1);
        c.socket_ttl = Some(128);
        let mut hooks = RecHooks::default();
        let out = run_cells(&env, &runner, &[c], &[target("a")], &mut hooks);
        assert_eq!(out.aborted, None);
        assert_eq!(*runner.seen.borrow(), vec![Some(128)]);
    }

    #[test]
    fn preflight_aborts_when_other_dpi_running() {
        for image in FOREIGN_DPI {
            let env = FakeEnv {
                foreign: vec![image.to_string()],
                ..FakeEnv::default()
            };
            assert_eq!(
                preflight(&env, false),
                Err(Abort::OtherDpiRunning(image.to_string()))
            );
        }
    }

    #[test]
    fn preflight_aborts_on_windivert_service() {
        let env = FakeEnv {
            windivert: Some("WinDivert".into()),
            ..FakeEnv::default()
        };
        assert_eq!(
            preflight(&env, false),
            Err(Abort::WinDivertPresent("WinDivert".into()))
        );
    }

    #[test]
    fn preflight_requires_elevation_when_needed() {
        let env = FakeEnv::default();
        assert_eq!(preflight(&env, true), Err(Abort::NotElevated));
        assert_eq!(preflight(&env, false), Ok(()));
        let admin = FakeEnv {
            elevated: true,
            ..FakeEnv::default()
        };
        assert_eq!(preflight(&admin, true), Ok(()));
    }

    #[test]
    fn preflight_aborts_on_stale_journal_and_no_internet() {
        let env = FakeEnv {
            stale: vec![PathBuf::from("eski.jsonl")],
            ..FakeEnv::default()
        };
        assert_eq!(preflight(&env, false), Err(Abort::StaleJournal));
        let offline = FakeEnv {
            controls: RefCell::new(vec![Outcome::TcpTimeout]),
            ..FakeEnv::default()
        };
        assert_eq!(preflight(&offline, false), Err(Abort::NoInternet));
    }

    #[test]
    fn cell_aborts_when_control_probe_fails() {
        // 1. hücre: kontrol geçer; 2. hücre: kontrol başarısız.
        let env = FakeEnv {
            controls: RefCell::new(vec![Outcome::Ok { http_status: 204 }, Outcome::DnsFail]),
            ..FakeEnv::default()
        };
        let runner = FakeRunner {
            calls: StdCell::new(0),
        };
        let targets = [target("a"), target("b")];
        let cells = [cell(DnsMode::System, 2), cell(DnsMode::Doh, 2)];
        let mut hooks = RecHooks::default();
        let out = run_cells(&env, &runner, &cells, &targets, &mut hooks);
        assert_eq!(out.aborted, Some(Abort::NoInternet));
        assert_eq!(
            out.rows, 4,
            "yalnızca 1. hücrenin satırları (2 tekrar × 2 hedef)"
        );
        assert_eq!(runner.calls.get(), 4, "2. hücrenin hedefleri hiç denenmez");
        assert_eq!(hooks.rows.len(), 4);
        assert_eq!(hooks.aborts, vec![Abort::NoInternet], "journal geri alınır");
        assert!(
            hooks.beats >= 4 + 2,
            "her hücre ve probdan önce kira yenilenir"
        );
    }

    #[test]
    fn cell_aborts_when_foreign_dpi_appears_mid_run() {
        struct Appearing(StdCell<usize>);
        impl LabEnv for Appearing {
            fn is_elevated(&self) -> bool {
                false
            }
            fn stale_journals(&self) -> Vec<PathBuf> {
                Vec::new()
            }
            fn foreign_process(&self, image: &str) -> bool {
                // İlk hücrede yok, ikinci hücrede beliriyor.
                if image == "winws.exe" {
                    self.0.set(self.0.get() + 1);
                    return self.0.get() > 1;
                }
                false
            }
            fn windivert_service(&self) -> Option<String> {
                None
            }
            fn spike_adapter_present(&self) -> bool {
                false
            }
            fn control_probe(&self) -> Outcome {
                Outcome::Ok { http_status: 204 }
            }
        }
        let env = Appearing(StdCell::new(0));
        let runner = FakeRunner {
            calls: StdCell::new(0),
        };
        let mut hooks = RecHooks::default();
        let cells = [cell(DnsMode::System, 1), cell(DnsMode::Doh, 1)];
        let out = run_cells(&env, &runner, &cells, &[target("a")], &mut hooks);
        assert_eq!(
            out.aborted,
            Some(Abort::OtherDpiRunning("winws.exe".into()))
        );
        assert_eq!(out.rows, 1);
        assert_eq!(hooks.aborts.len(), 1);
    }

    #[test]
    fn watchdog_in_job_blocks_only_system_changes() {
        assert_eq!(job_gate(false, true), Err(Abort::WatchdogInJob));
        assert_eq!(job_gate(false, false), Ok(()));
        assert_eq!(job_gate(true, true), Ok(()));
        assert_eq!(job_gate(true, false), Ok(()));
    }

    #[test]
    fn heartbeat_failure_stops_run() {
        struct Dead;
        impl CellHooks for Dead {
            fn heartbeat(&mut self) -> anyhow::Result<()> {
                anyhow::bail!("bekçi yok")
            }
            fn on_row(&mut self, _: &ProbeResult) -> anyhow::Result<()> {
                Ok(())
            }
            fn on_abort(&mut self, _: &Abort) {}
        }
        let runner = FakeRunner {
            calls: StdCell::new(0),
        };
        let out = run_cells(
            &FakeEnv::default(),
            &runner,
            &[cell(DnsMode::System, 1)],
            &[target("a")],
            &mut Dead,
        );
        assert!(matches!(out.aborted, Some(Abort::WatchdogLost(_))));
        assert_eq!(runner.calls.get(), 0);
    }

    #[test]
    fn lab_bin_cleanup_refused_while_spike_adapter_exists() {
        let dir = std::env::temp_dir().join(format!("alterdns-binclean-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("wintun")).unwrap();
        let env = FakeEnv {
            adapter: true,
            ..FakeEnv::default()
        };
        assert!(lab_bin_cleanup(&env, &dir).is_err());
        assert!(dir.exists(), "adaptör varken lab/bin silinmez");
        assert!(lab_bin_cleanup(&FakeEnv::default(), &dir).is_ok());
        assert!(!dir.exists());
        assert!(
            lab_bin_cleanup(&FakeEnv::default(), &dir).is_ok(),
            "dizin yoksa sorun değil"
        );
    }

    #[test]
    fn scenario_matrices_match_plan() {
        let s1 = s1_cells();
        assert_eq!(s1.len(), 3);
        assert!(s1.iter().all(|c| c.tech == SockTech::None && c.reps == 3));
        assert!(s1.iter().any(|c| c.dns == DnsMode::AltUdp1253));

        let s2 = s2u_cells();
        // None + 2 konum × (Split + TlsRecordSplit + Oob + Disorder ttl 1…8) = 23 teknik; × 2 DNS.
        assert_eq!(s2.len(), 46);
        assert!(
            s2.iter()
                .all(|c| c.reps == 3 && c.dns != DnsMode::AltUdp1253)
        );
        let ttls: Vec<u8> = s2
            .iter()
            .filter_map(|c| match c.tech {
                SockTech::Disorder { ttl, .. } => Some(ttl),
                _ => None,
            })
            .collect();
        assert_eq!(ttls.len(), 2 * 8 * 2);
        assert!(ttls.iter().all(|t| (1..=8).contains(t)));
    }

    #[test]
    fn bisect_mtu_finds_largest_passing_size() {
        let mut calls = 0;
        let found = bisect_mtu(576, 1500, &mut |m| {
            calls += 1;
            m <= 1420
        });
        assert_eq!(found, Some(1420));
        assert!(calls <= 12, "ikili arama: {calls} deneme");
        assert_eq!(bisect_mtu(576, 1500, &mut |_| false), None);
        assert_eq!(bisect_mtu(576, 1500, &mut |_| true), Some(1500));
    }

    #[test]
    fn estimate_bytes_matches_cells() {
        let cells = [
            cell(DnsMode::System, 3),
            cell(DnsMode::Doh, 3),
            Cell {
                tech: SockTech::Split {
                    at: SplitAt::SniMiddle,
                },
                dns: DnsMode::AltUdp1253,
                reps: 2,
                socket_ttl: None,
            },
        ];
        let targets = 5;
        let expected = 3 * 5 * PROBE_BYTES
            + 3 * 5 * (PROBE_BYTES + DOH_EXTRA_BYTES)
            + 2 * 5 * PROBE_BYTES
            + 3 * CONTROL_BYTES;
        assert_eq!(estimate_bytes(&cells, targets), expected);
        assert_eq!(estimate_bytes(&[], targets), 0);
    }

    #[test]
    fn confirm_requires_literal_evet() {
        let s = Summary {
            scenario: "S1".into(),
            changes: "yok".into(),
            revert: "gerekmez".into(),
            probes: 1,
            bytes: 1024,
        };
        let ask = |answer: &str| {
            let mut out = Vec::new();
            let ok = confirm_with(&s, &mut answer.as_bytes(), &mut out);
            let shown = String::from_utf8(out).unwrap();
            assert!(shown.contains("S1"), "özet gösterilir");
            ok
        };
        assert!(ask("evet\n"));
        assert!(ask("evet\r\n"));
        for no in [
            "",
            "\n",
            "e\n",
            "Evet\n",
            "EVET\n",
            "yes\n",
            " evet\n",
            "evet evet\n",
            "hayır\n",
        ] {
            assert!(!ask(no), "{no:?} onay sayılmamalı");
        }
    }

    /// Kaynakta kullanıcıya ait sistem değerlerini yazan hiçbir komut yoktur; istisna da yoktur
    /// (CLAUDE.md kural 2, ADR-010, ADR-014; D5 kapandı). Yalnızca kendi nesnelerimiz (adaptör,
    /// rota, adres) `ops.rs` üzerinden eklenip kaldırılır.
    #[test]
    fn no_user_owned_system_value_writes_in_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        // Kalıplar parçalı yazılır ki bu dosya kendini yakalamasın.
        let patterns = [
            ["set-netipv4", "protocol"].concat(),
            ["set-netipv6", "protocol"].concat(),
            ["defaultcur", "hoplimit"].concat(),
            ["set-netipinter", "face"].concat(),
            ["set-dnsclient", "serveraddress"].concat(),
            ["set-dnsclient", "dohserveraddress"].concat(),
            ["add-dnsclient", "dohserveraddress"].concat(),
            ["set-dnsclient", "globalsetting"].concat(),
            ["set-netfirewall", "profile"].concat(),
            ["set-netconnection", "profile"].concat(),
            ["set-nettcp", "setting"].concat(),
            ["set-netoffload", "globalsetting"].concat(),
            ["winhttp ", "set"].concat(),
            ["set-mp", "preference"].concat(),
            ["add-mp", "preference"].concat(),
            // 2026-11-12 güvenlik incelemesi: kayıt defteri, netsh, diğer cmdlet'ler, Win32.
            ["set-item", "property"].concat(),
            ["new-item", "property"].concat(),
            ["reg ", "add"].concat(),
            ["netsh int ipv4 ", "set"].concat(),
            ["netsh int ipv6 ", "set"].concat(),
            ["netsh interface ", "ip set"].concat(),
            ["winhttp ", "reset"].concat(),
            ["winhttp ", "import"].concat(),
            ["set-dns", "client "].concat(),
            ["add-dnsclient", "nrptrule"].concat(),
            ["set-dnsclient", "nrptrule"].concat(),
            ["remove-dnsclient", "nrptrule"].concat(),
            ["remove-dnsclient", "dohserveraddress"].concat(),
            ["set-net", "route"].concat(),
            ["set-net", "adapter"].concat(),
            ["set-net", "ipaddress"].concat(),
            ["new-netfirewall", "rule"].concat(),
            ["set-netfirewall", "rule"].concat(),
            ["set-netfirewall", "setting"].concat(),
            ["setipinterface", "entry"].concat(),
            ["setinterfacedns", "settings"].concat(),
            ["createipforward", "entry2"].concat(),
            ["winhttpsetdefault", "proxyconfiguration"].concat(),
        ];
        let mut hits = Vec::new();
        for dir in ["xtask", "lab", "spikes", "apps", "crates", "scripts"] {
            walk(&root.join(dir), &mut |p| {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
                if ![
                    "rs", "ps1", "psm1", "cmd", "bat", "toml", "reg", "vbs", "js", "py",
                ]
                .contains(&ext)
                {
                    return;
                }
                let rel = p.strip_prefix(&root).unwrap_or(p);
                let text = std::fs::read_to_string(p)
                    .unwrap_or_default()
                    .to_lowercase();
                for pat in &patterns {
                    if text.contains(pat.as_str()) {
                        hits.push(format!("{}: {pat}", rel.display()));
                    }
                }
            });
        }
        assert!(
            hits.is_empty(),
            "kullanıcıya ait sistem değeri yazımı bulundu: {hits:?}"
        );
    }

    fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if p.is_dir() {
                // Yalnızca derleme çıktıları ve lab'ın veri dizinleri atlanır; `src/bin` gibi
                // kaynak dizinleri taranır (2026-11-12 güvenlik incelemesi).
                let generated = ["target", "node_modules", ".git"].contains(&name)
                    || p.ends_with("lab/bin")
                    || p.ends_with("lab/results");
                if !generated {
                    walk(&p, f);
                }
            } else {
                f(&p);
            }
        }
    }

    /// S0 betiği yalnızca okur: her cmdlet `Get-*` veya izinli boru hattı yardımcısıdır.
    #[test]
    fn s0_script_is_read_only() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lab/scripts/s0-inventory.ps1");
        let raw = std::fs::read_to_string(&path).unwrap();
        let violations = ps_script_violations(&path).unwrap();
        assert!(
            violations.is_empty(),
            "S0 betiği salt okunur olmalı: {violations:?}"
        );
        assert!(raw.contains("Get-NetIPv4Protocol"), "betik boş olmamalı");
    }

    #[test]
    fn ps_checker_catches_writes() {
        let dir = std::env::temp_dir().join(format!("alterdns-pscheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let check = |i: usize, text: &str| {
            let p = dir.join(format!("{i}.ps1"));
            std::fs::write(&p, text).unwrap();
            ps_script_violations(&p).unwrap()
        };
        let hop = ["Set-NetIPv4", "Protocol -DefaultHopLimit 1"].concat();
        let bad = [
            hop.as_str(),
            "Stop-Service GoodbyeDPI",
            "Remove-Item C:\\x",
            "rm C:\\x",
            "netsh int ipv4 show glob",
            "Get-Process | Out-File x.txt",
            "Get-Process > x.txt",
            "sc.exe query",
            "Invoke-Expression $x",
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8",
            "[scriptblock]::Create('x').Invoke()",
            "$env:X = 1",
        ];
        for (i, b) in bad.iter().enumerate() {
            assert!(!check(i, b).is_empty(), "{b:?} yakalanmalı");
        }
        let ok = "# Set-Content yorumda sorun değil\nGet-Service -Name 'Set-Thing' | Select-Object Name | ConvertTo-Json\n$x = 1\n(Get-Date).ToString('o')\n";
        assert_eq!(check(99, ok), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
