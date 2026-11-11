// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Journal'lı kurulum ve söküm: adaptör → adres → iletim → tek IP rotası. Her sistem değişikliği
//! önce journal'a yazılır; ayrı işlemdeki bekçi (kira ≤ 60 sn) ebeveyn ölürse veya kira dolarsa
//! geri alır (CLAUDE.md kural 1).

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use serde::Serialize;
use xtask::lab::cli::{RealEnv, journal_dir, load_targets, real_env, repo_root};
use xtask::lab::journal::{Journal, OpKind, SPIKE_ADAPTER, SystemOps};
use xtask::lab::ops::{RealOps, ps_query};
use xtask::lab::probe::{ProbeTarget, Role};
use xtask::lab::scenario::job_gate;
use xtask::lab::sockettech::SockTech;
use xtask::lab::watchdog::{MAX_LEASE_SECS, WatchdogHandle};

use crate::config::{self, TUN_CIDR, TUN_IP, TUN_PREFIX, WintunConfig};
use crate::ffi::{self, Adapter, Wintun};
use crate::forward::{self, Engine, TunDevice};

/// Sabit adaptör GUID'i: her koşuda aynı ağ profili kaydı kullanılır (wintun.h `RequestedGUID`).
const ADAPTER_GUID: u128 = 0x7c2e_4b61_9d3a_4f58_b0e7_5a1c_d2f4_8e93;
const TUNNEL_TYPE: &str = "AlterDNS";

/// Koşu bağlamı: yapılandırma, hedef, gerçek ortam ve ikili yolları.
pub struct Ctx {
    pub root: PathBuf,
    pub cfg: WintunConfig,
    pub target: ProbeTarget,
    /// Engelsiz kontrol hedefi (TUN dışından geçmeli; diğer trafik etkilenmiyor mu?).
    pub control: ProbeTarget,
    pub env: RealEnv,
    pub dll: PathBuf,
    pub xtask_exe: PathBuf,
}

impl Ctx {
    pub fn load() -> anyhow::Result<Ctx> {
        // `repo_root` "…/xtask/.." biçimindedir; DLL tam yoldan yükleneceği için sadeleştirilir.
        let root = std::path::absolute(repo_root())?;
        let cfg = config::load(&root)?;
        let targets = load_targets(&root)?;
        let target = targets
            .iter()
            .find(|t| t.name == cfg.test_target && t.role == Role::Target)
            .cloned()
            .ok_or_else(|| anyhow!("data/probes içinde hedef yok: {}", cfg.test_target))?;
        let control = targets
            .iter()
            .find(|t| t.role == Role::Control)
            .cloned()
            .ok_or_else(|| anyhow!("data/probes/control.toml içinde kontrol hedefi yok"))?;
        let env = real_env(&root, &targets)?;
        let dll = root
            .join("lab")
            .join("bin")
            .join("wintun-0.14.1")
            .join("wintun")
            .join("bin")
            .join("amd64")
            .join("wintun.dll");
        let xtask_exe = root
            .join("target")
            .join("xtask")
            .join("debug")
            .join("xtask.exe");
        if !xtask_exe.exists() {
            bail!(
                "bekçi ikilisi yok: {} (önce `cargo build -p xtask --target-dir target/xtask`)",
                xtask_exe.display()
            );
        }
        let exe_time = std::fs::metadata(&xtask_exe)?.modified()?;
        let src_time = newest_mtime(&root.join("xtask"))?;
        if binary_is_stale(exe_time, src_time) {
            bail!(
                "bekçi ikilisi xtask kaynaklarından eski: {} (yeniden derleyin: `cargo build -p xtask --target-dir target/xtask`)",
                xtask_exe.display()
            );
        }
        Ok(Ctx {
            root,
            cfg,
            target,
            control,
            env,
            dll,
            xtask_exe,
        })
    }

    pub fn test_ip(&self) -> Ipv4Addr {
        self.cfg.test_ip
    }
}

/// Dizindeki `.rs` ve `Cargo.toml` dosyalarının en yeni değişiklik zamanı (`target` hariç).
pub fn newest_mtime(dir: &Path) -> std::io::Result<std::time::SystemTime> {
    let mut newest = std::time::SystemTime::UNIX_EPOCH;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name != "target" {
                newest = newest.max(newest_mtime(&path)?);
            }
        } else if name.ends_with(".rs") || name == "Cargo.toml" {
            newest = newest.max(std::fs::metadata(&path)?.modified()?);
        }
    }
    Ok(newest)
}

/// İkili, kaynakların en yenisinden eskiyse yeniden derlenmemiştir.
pub fn binary_is_stale(exe: std::time::SystemTime, sources: std::time::SystemTime) -> bool {
    exe < sources
}

/// Başka yazılımlara ait Wintun adaptörleri (`ad:durum`), salt okunur. Spike bunlara dokunmaz;
/// DLL'in sürücüyü güncelleyip onları yeniden başlatıp başlatmadığını görmek için kaydedilir.
pub fn other_wintun_adapters() -> String {
    ps_query(&format!(
        "(Get-NetAdapter -IncludeHidden | Where-Object {{ $_.InterfaceDescription -like 'Wintun*' -and $_.Name -ne '{SPIKE_ADAPTER}' }} | ForEach-Object {{ $_.Name + ':' + $_.Status }}) -join ','"
    ))
    .unwrap_or_else(|e| format!("okunamadı: {e}"))
}

/// Tünel açıkken adaptörün IP yapılandırması (NLA/DNS/IPv6 yan etkileri için).
#[derive(Debug, Default, Serialize)]
pub struct AdapterSnapshot {
    pub addresses: String,
    pub routes: String,
    pub dns_servers: String,
    pub interface_metric: String,
    pub network_category: String,
}

pub fn adapter_snapshot() -> AdapterSnapshot {
    let q = |script: String| ps_query(&script).unwrap_or_else(|e| format!("okunamadı: {e}"));
    let a = SPIKE_ADAPTER;
    AdapterSnapshot {
        addresses: q(format!(
            "(Get-NetIPAddress -InterfaceAlias '{a}' -ErrorAction SilentlyContinue | ForEach-Object {{ $_.IPAddress + '/' + $_.PrefixLength }}) -join ','"
        )),
        routes: q(format!(
            "(Get-NetRoute -InterfaceAlias '{a}' -ErrorAction SilentlyContinue | ForEach-Object {{ $_.DestinationPrefix + '>' + $_.NextHop }}) -join ','"
        )),
        dns_servers: q(format!(
            "(Get-DnsClientServerAddress -InterfaceAlias '{a}' -ErrorAction SilentlyContinue | ForEach-Object {{ $_.ServerAddresses }}) -join ','"
        )),
        interface_metric: q(format!(
            "(Get-NetIPInterface -InterfaceAlias '{a}' -AddressFamily IPv4).InterfaceMetric"
        )),
        network_category: q(format!(
            "(Get-NetConnectionProfile -InterfaceAlias '{a}' -ErrorAction SilentlyContinue).NetworkCategory"
        )),
    }
}

pub fn tun_op() -> OpKind {
    OpKind::TunAdapter {
        name: SPIKE_ADAPTER.into(),
    }
}

fn addr_op() -> OpKind {
    OpKind::UnicastAddress {
        if_name: SPIKE_ADAPTER.into(),
        cidr: TUN_CIDR.into(),
    }
}

pub fn route_op(ip: Ipv4Addr) -> OpKind {
    OpKind::Route {
        prefix: format!("{ip}/32"),
        if_name: SPIKE_ADAPTER.into(),
    }
}

/// `f` doğru olana kadar (en çok `limit`) bekler.
pub fn wait_for(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if f() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn is_absent(op: &OpKind) -> bool {
    RealOps.read(op).is_ok_and(|v| v == "absent")
}

/// Kurulu tünel. Alanların düşme sırası önemlidir; söküm `down` ile yapılır.
pub struct Tunnel {
    pub engine: Arc<Engine>,
    pub phys_if: u32,
    pub tun_if: u32,
    pub mtu: u16,
    pub driver_version: Option<(u16, u16)>,
    wd: WatchdogHandle,
    journal: Journal,
    route: OpKind,
    serve_task: tokio::task::JoinHandle<()>,
    rt: tokio::runtime::Runtime,
    adapter: Arc<Adapter>,
}

/// Söküm sonucu.
#[derive(Debug, Default, Serialize)]
pub struct Teardown {
    pub reverted: Vec<u32>,
    pub conflicts: Vec<u32>,
    pub failed: Vec<(u32, String)>,
    pub adapter_absent: bool,
    pub route_absent: bool,
    pub notes: Vec<String>,
}

struct Parts {
    engine: Arc<Engine>,
    phys_if: u32,
    tun_if: u32,
    mtu: u16,
    driver_version: Option<(u16, u16)>,
    serve_task: tokio::task::JoinHandle<()>,
    rt: tokio::runtime::Runtime,
    adapter: Arc<Adapter>,
}

impl Tunnel {
    /// Journal ve bekçiyi başlatır, sonra adaptörü, adresi, iletimi ve rotayı kurar. Herhangi
    /// bir adım başarısız olursa o ana kadar yapılanlar geri alınır.
    pub fn up(
        ctx: &Ctx,
        lib: &Arc<Wintun>,
        out_dir: &Path,
        run: &str,
        tech: SockTech,
    ) -> anyhow::Result<Tunnel> {
        let ip = ctx.test_ip();
        // Fiziksel arayüz, rota eklenmeden önce belirlenir (sonra en iyi arayüz TUN olur).
        let phys_if = ffi::best_interface_v4(ip).context("fiziksel arayüz bulunamadı")?;
        let mut journal = Journal::create(&journal_dir(&ctx.root), run)?;
        let mut wd = WatchdogHandle::spawn_with(&ctx.xtask_exe, journal.path(), out_dir)?;
        if let Err(why) = job_gate(wd.outside_job(), true) {
            let _ = wd.done();
            bail!("{why}");
        }
        wd.renew(MAX_LEASE_SECS)?;
        match Self::build(ctx, lib, &mut journal, &mut wd, phys_if, tech) {
            Ok(p) => Ok(Tunnel {
                engine: p.engine,
                phys_if: p.phys_if,
                tun_if: p.tun_if,
                mtu: p.mtu,
                driver_version: p.driver_version,
                wd,
                journal,
                route: route_op(ip),
                serve_task: p.serve_task,
                rt: p.rt,
                adapter: p.adapter,
            }),
            Err(e) => {
                // `build` içindeki adaptör ve çalışma zamanı burada düşmüş durumda.
                eprintln!("Kurulum başarısız: {e:#}\nYapılanlar geri alınıyor…");
                wait_for(Duration::from_secs(15), || is_absent(&tun_op()));
                match journal.revert_all(&RealOps) {
                    Ok(r) => eprintln!(
                        "Geri alındı: {} kayıt, çakışma {:?}, hata {:?}",
                        r.reverted.len(),
                        r.conflicts,
                        r.failed
                    ),
                    Err(re) => {
                        eprintln!("Geri alma hatası: {re:#} (`xtask lab recover` çalıştırın)")
                    }
                }
                let _ = wd.done();
                Err(e)
            }
        }
    }

    fn build(
        ctx: &Ctx,
        lib: &Arc<Wintun>,
        journal: &mut Journal,
        wd: &mut WatchdogHandle,
        phys_if: u32,
        tech: SockTech,
    ) -> anyhow::Result<Parts> {
        let ip = ctx.test_ip();
        if !is_absent(&tun_op()) {
            bail!("{SPIKE_ADAPTER} zaten var; önce `xtask lab recover` çalıştırın");
        }
        // 1. Adaptör: değişikliği DLL yapar; önce journal'a `Pending` yazılır.
        let id = journal.record_external(&RealOps, tun_op(), "present")?;
        let adapter = lib
            .create_adapter(SPIKE_ADAPTER, TUNNEL_TYPE, ADAPTER_GUID)
            .context("gizli bir kalıntı adaptör olabilir: `pnputil /enum-devices /class Net` (lab/RUNBOOK.md)")?;
        journal.mark_applied(id)?;
        wd.renew(MAX_LEASE_SECS)?;
        if !wait_for(Duration::from_secs(20), || !is_absent(&tun_op())) {
            bail!("{SPIKE_ADAPTER} oluşturuldu ama Get-NetAdapter'da görünmedi");
        }
        // 2. Adres (198.18.0.1/30): Wintun örneğindeki gibi doğrudan `Preferred` (DAD yok).
        // Değişikliği Windows API'si yapar; önce journal'a `Pending` yazılır. Geri alma
        // (spike, bekçi, `lab recover`) Remove-NetIPAddress ile yapılır.
        let id = journal.record_external(&RealOps, addr_op(), "present")?;
        ffi::create_unicast_address(&ffi::address_row(adapter.luid(), TUN_IP, TUN_PREFIX))
            .context("adaptör adresi oluşturulamadı (CreateUnicastIpAddressEntry)")?;
        journal.mark_applied(id)?;
        wd.renew(MAX_LEASE_SECS)?;
        let mtu = ps_query(&format!(
            "(Get-NetIPInterface -InterfaceAlias '{SPIKE_ADAPTER}' -AddressFamily IPv4).NlMtu"
        ))
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(1500, |m| u16::try_from(m).unwrap_or(u16::MAX));
        // 3. Oturum ve iletim. Wintun adaptörü oturum açılana kadar "Disconnected" görünür ve
        // bağlantısız arayüzde adres "Deprecated" kalır (2026-11-11 ölçümü); bu yüzden oturum
        // adres beklemesinden önce başlar (Wintun example.c ile aynı sıra). Rota henüz yok:
        // TUN'a yalnızca sistemin sahipsiz paketleri düşer ve motor bunları düşürür.
        let session = adapter.start_session()?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let engine = Engine::new(tech);
        let stack = {
            let _guard = rt.enter();
            let mut cfg = ipstack::IpStackConfig::default();
            cfg.mtu_unchecked(mtu);
            ipstack::IpStack::new(cfg, TunDevice::new(session))
        };
        let serve_task = rt.spawn(forward::serve(stack, ip, Some(phys_if), engine.clone()));
        // Adres kullanılabilir (Preferred) ve adaptör bağlı olmalı; beklerken kira yenilenir.
        let preferred = format!(
            "(Get-NetIPAddress -InterfaceAlias '{SPIKE_ADAPTER}' -AddressFamily IPv4 -ErrorAction SilentlyContinue).AddressState"
        );
        let media = format!("(Get-NetAdapter -Name '{SPIKE_ADAPTER}').MediaConnectionState");
        if !wait_for(Duration::from_secs(20), || {
            let _ = wd.renew(MAX_LEASE_SECS);
            ps_query(&preferred).is_ok_and(|s| s == "Preferred")
        }) {
            let state = ps_query(&preferred).unwrap_or_else(|e| format!("okunamadı: {e}"));
            let link = ps_query(&media).unwrap_or_else(|e| format!("okunamadı: {e}"));
            bail!(
                "adaptör adresi kullanılabilir (Preferred) duruma geçmedi: adres durumu {state:?}, bağlantı {link:?}"
            );
        }
        // 4. Rota: yalnızca test IP'si (/32, ActiveStore).
        wd.renew(MAX_LEASE_SECS)?;
        journal.apply(&RealOps, route_op(ip), "present")?;
        wd.renew(MAX_LEASE_SECS)?;
        let tun_if = ffi::best_interface_v4(ip)?;
        if tun_if == phys_if {
            bail!("rota eklendi ama test IP'si hâlâ fiziksel arayüzden çıkıyor");
        }
        Ok(Parts {
            engine,
            phys_if,
            tun_if,
            mtu,
            driver_version: lib.running_driver_version(),
            serve_task,
            rt,
            adapter,
        })
    }

    /// Bekçi kirasını yeniler. İletim motoru durduysa (görev bitti ya da döngü saptandı) kira
    /// yenilenmez ve hata döner: rota bir motor olmadan test IP'sini karadeliğe çekmesin.
    pub fn beat(&mut self) -> anyhow::Result<()> {
        if self.serve_task.is_finished() {
            bail!("iletim motoru durdu");
        }
        if self.engine.halted() {
            bail!("döngü saptandı: IP_UNICAST_IF etkisiz; motor durduruldu");
        }
        self.wd.renew(MAX_LEASE_SECS)
    }

    /// Söküm: rota ve adres betikle kaldırılır, iletim durur, adaptör DLL ile kapatılır,
    /// journal kapatılır ve kalıntı olmadığı doğrulanır.
    pub fn down(mut self) -> Teardown {
        let mut t = Teardown::default();
        let _ = self.wd.renew(MAX_LEASE_SECS);
        for op in [self.route.clone(), addr_op()] {
            if let Err(e) = RealOps.write(&op, "absent") {
                t.notes.push(format!("{op:?} kaldırılamadı: {e:#}"));
            }
        }
        let _ = self.wd.renew(MAX_LEASE_SECS);
        self.serve_task.abort();
        self.rt.shutdown_timeout(Duration::from_secs(3));
        // Oturum (okuma iş parçacığı) bitince adaptörün tek sahibi biz kalırız. Beklerken kira
        // yenilenir: söküm bekçinin süresini aşıp onunla yarışmasın.
        let wd = &mut self.wd;
        let adapter = &self.adapter;
        if !wait_for(Duration::from_secs(15), || {
            let _ = wd.renew(MAX_LEASE_SECS);
            Arc::strong_count(adapter) == 1
        }) {
            t.notes
                .push("oturum 15 sn içinde kapanmadı; adaptör yine de bırakılıyor".into());
        }
        drop(self.adapter);
        let wd = &mut self.wd;
        t.adapter_absent = wait_for(Duration::from_secs(20), || {
            let _ = wd.renew(MAX_LEASE_SECS);
            is_absent(&tun_op())
        });
        let _ = self.wd.renew(MAX_LEASE_SECS);
        match self.journal.revert_all(&RealOps) {
            Ok(r) => {
                t.reverted = r.reverted;
                t.conflicts = r.conflicts;
                t.failed = r.failed;
            }
            Err(e) => t.notes.push(format!("journal geri alınamadı: {e:#}")),
        }
        t.adapter_absent = is_absent(&tun_op());
        t.route_absent = is_absent(&self.route);
        if let Err(e) = self.wd.done() {
            t.notes.push(format!("bekçiye DONE iletilemedi: {e:#}"));
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn stale_watchdog_binary_is_detected() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(binary_is_stale(t0, t0 + Duration::from_secs(1)));
        assert!(!binary_is_stale(t0 + Duration::from_secs(1), t0));
        assert!(!binary_is_stale(t0, t0));
    }

    #[test]
    fn newest_mtime_ignores_target_and_other_files() {
        let d = std::env::temp_dir().join(format!("alterdns-mtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::create_dir_all(d.join("target")).unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let new = old + Duration::from_secs(500);
        let newest = old + Duration::from_secs(9_000);
        let set = |p: PathBuf, t: SystemTime| {
            std::fs::write(&p, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        };
        set(d.join("src").join("a.rs"), old);
        set(d.join("Cargo.toml"), new);
        set(d.join("target").join("b.rs"), newest);
        set(d.join("notlar.md"), newest);
        assert_eq!(newest_mtime(&d).unwrap(), new);
        let _ = std::fs::remove_dir_all(&d);
    }
}
