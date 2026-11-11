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

use crate::config::{self, TUN_CIDR, WintunConfig};
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
        let adapter = lib.create_adapter(SPIKE_ADAPTER, TUNNEL_TYPE, ADAPTER_GUID)?;
        journal.mark_applied(id)?;
        wd.renew(MAX_LEASE_SECS)?;
        if !wait_for(Duration::from_secs(20), || !is_absent(&tun_op())) {
            bail!("{SPIKE_ADAPTER} oluşturuldu ama Get-NetAdapter'da görünmedi");
        }
        // 2. Adres (198.18.0.1/30, ActiveStore).
        journal.apply(&RealOps, addr_op(), "present")?;
        wd.renew(MAX_LEASE_SECS)?;
        let preferred = format!(
            "(Get-NetIPAddress -InterfaceAlias '{SPIKE_ADAPTER}' -AddressFamily IPv4 -ErrorAction SilentlyContinue).AddressState"
        );
        if !wait_for(Duration::from_secs(15), || {
            ps_query(&preferred).is_ok_and(|s| s == "Preferred")
        }) {
            bail!("adaptör adresi kullanılabilir (Preferred) duruma geçmedi");
        }
        let mtu = ps_query(&format!(
            "(Get-NetIPInterface -InterfaceAlias '{SPIKE_ADAPTER}' -AddressFamily IPv4).NlMtu"
        ))
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(1500, |m| u16::try_from(m).unwrap_or(u16::MAX));
        // 3. İletim: rota eklenmeden önce çalışır durumda olmalı.
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

    /// Bekçi kirasını yeniler; bekçi öldüyse hata verir.
    pub fn beat(&mut self) -> anyhow::Result<()> {
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
        // Oturum (okuma iş parçacığı) bitince adaptörün tek sahibi biz kalırız.
        if !wait_for(Duration::from_secs(5), || {
            Arc::strong_count(&self.adapter) == 1
        }) {
            t.notes
                .push("oturum 5 sn içinde kapanmadı; adaptör yine de bırakılıyor".into());
        }
        drop(self.adapter);
        t.adapter_absent = wait_for(Duration::from_secs(20), || is_absent(&tun_op()));
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
