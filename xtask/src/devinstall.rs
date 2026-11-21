// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geliştirme kurulumu (K12). Yönetici ister ve yalnızca kullanıcının yönetici terminalinde,
//! onayıyla çalıştırılır (`CONTRIBUTING.md` kural 1). Ağ ayarlarına dokunmaz.
//!
//! - `dev-install`: ikilileri `%ProgramFiles%\AlterDNS\<sürüm>\` altına kopyalar ve servisi
//!   kurar. Hepsi tek bir `Persistent` txn'dir; bir adım başarısız olursa txn geri alınır.
//! - `dev-uninstall`: journal'daki txn'leri en yeniden başlayarak geri alır, dizinleri siler ve
//!   kalıntı taraması yapar.
//!
//! Testler bu iki işlevi çağırmaz: yönetici terminalinde ya da CI'da (yükseltilmiş) gerçek
//! kuruluma dönüşürlerdi. Yükseltme kapısı `require_elevated`, adımlar `plan` ile sınanır.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alterdns_core::Paths;
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{Client, ConnectOptions, PIPE_NAME};
use alterdns_journal::{
    InstallOps, JournalFile, Kind, Lifetime, RealOps, SERVICE_NAME, Snapshot, Targets, Txn, list,
    recover_at_startup, revert_file, sha256_file,
};
use alterdns_winsys::{
    DirAccess, create_protected_dir, is_elevated, query_service, start_service,
    verify_protected_chain,
};
use anyhow::{Context, bail};

use crate::residue;

pub const COMPONENT: &str = "dev-install";
pub const BINARIES: [&str; 2] = ["alterdns-svc.exe", "alterdns-maint.exe"];

/// Yalnızca `test-ops` derlemelerinde bulunan dizeler. `cargo xtask check` test derlemelerini
/// aynı `target\debug`'a yazar; onların kuruluma girmesi engellenir.
const TEST_BUILD_MARKERS: [&[u8]; 2] = [b"ALTERDNS_TEST_JOURNAL_DIR", b"(A;;GA;;;OW)"];

/// İkili bir `test-ops` derlemesi mi?
pub fn is_test_build(binary: &[u8]) -> bool {
    TEST_BUILD_MARKERS
        .iter()
        .any(|m| binary.windows(m.len()).any(|w| w == *m))
}

/// Yönetici değilse `SysElevationDenied` (`ADNS-SYS-004`).
pub fn require_elevated(elevated: bool) -> Result<(), AdnsError> {
    if elevated {
        Ok(())
    } else {
        Err(AdnsError::new(AdnsErrorCode::SysElevationDenied))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedChange {
    pub kind: Kind,
    pub target: String,
    pub after: Snapshot,
    pub source: Option<PathBuf>,
}

/// Kurulum adımları: önce dosyalar, sonra servis. Yalnızca kaynakları okur (hash).
pub fn plan(paths: &Paths, version: &str, target_dir: &Path) -> anyhow::Result<Vec<PlannedChange>> {
    let dir = paths.install_dir(version);
    let mut out = Vec::new();
    for bin in BINARIES {
        let source = target_dir.join(bin);
        if !source.is_file() {
            bail!(
                "derlenmiş ikili yok: {} (önce: cargo build --release -p alterdns-svc -p alterdns-maint)",
                source.display()
            );
        }
        let bytes = std::fs::read(&source).with_context(|| source.display().to_string())?;
        if is_test_build(&bytes) {
            bail!(
                "{} bir test derlemesi (test-ops); kurulmaz. Önce: cargo build --release -p alterdns-svc -p alterdns-maint",
                source.display()
            );
        }
        let hash = sha256_file(&source).with_context(|| source.display().to_string())?;
        out.push(PlannedChange {
            kind: Kind::FileInstall,
            target: dir.join(bin).to_string_lossy().into_owned(),
            after: Snapshot::Present(hash),
            source: Some(source),
        });
    }
    out.push(PlannedChange {
        kind: Kind::ServiceCreate,
        target: SERVICE_NAME.to_string(),
        after: Snapshot::Present(dir.join("alterdns-svc.exe").to_string_lossy().into_owned()),
        source: None,
    });
    Ok(out)
}

/// Korumalı veri ve kurulum dizinleri (K20). Var olan dizin ancak korumalıysa kabul edilir.
fn prepare_dirs(paths: &Paths, version: &str) -> anyhow::Result<()> {
    for (dir, access) in [
        (paths.machine.clone(), DirAccess::AdminOnly),
        (paths.journal_dir(), DirAccess::AdminOnly),
        (paths.machine_logs(), DirAccess::AdminOnly),
        (
            paths
                .machine_config()
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            DirAccess::AdminWriteUsersRead,
        ),
        (paths.program_files.clone(), DirAccess::AdminWriteUsersRead),
        (paths.install_dir(version), DirAccess::AdminWriteUsersRead),
    ] {
        create_protected_dir(&dir, access).with_context(|| dir.display().to_string())?;
    }
    Ok(())
}

fn wait_for_ping(limit: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + limit;
    loop {
        let attempt = Client::connect(&ConnectOptions::product(PIPE_NAME))
            .and_then(|(mut c, _)| c.call("svc.ping", serde_json::json!({})));
        match attempt {
            Ok(_) => return Ok(()),
            Err(e) if Instant::now() >= deadline => bail!("servis yanıt vermedi: {e}"),
            Err(_) => std::thread::sleep(Duration::from_millis(300)),
        }
    }
}

pub fn dev_install(profile: &str) -> anyhow::Result<()> {
    require_elevated(is_elevated()?)?;
    let target_dir = crate::check::repo_root()?.join("target").join(profile);
    let paths = Paths::system()?;
    let version = alterdns_core::version::VERSION;
    if query_service(SERVICE_NAME)?.is_some() || paths.program_files.exists() {
        bail!("AlterDNS zaten kurulu ya da kalıntısı var: önce cargo xtask dev-uninstall");
    }
    let changes = plan(&paths, version, &target_dir)?;
    prepare_dirs(&paths, version)?;
    let journal = paths.journal_dir();
    let targets = Targets::product(paths.program_files.clone());
    let leftover = recover_at_startup(&journal, &RealOps, &targets);
    if !leftover.txns.is_empty() || !leftover.errors.is_empty() {
        eprintln!("önceki yarım kalmış txn'ler: {leftover:?}");
    }
    let sources: BTreeMap<String, PathBuf> = changes
        .iter()
        .filter_map(|c| c.source.clone().map(|s| (c.target.clone(), s)))
        .collect();
    let ops = InstallOps::new(sources);
    let mut txn = Txn::begin(&journal, COMPONENT, Lifetime::Persistent, targets)?;
    let result = (|| -> anyhow::Result<()> {
        for c in &changes {
            txn.apply(&ops, c.kind, &c.target, c.after.clone())
                .with_context(|| format!("{:?} {}", c.kind, c.target))?;
            eprintln!("uygulandı: {:?} {}", c.kind, c.target);
        }
        start_service(SERVICE_NAME, Duration::from_secs(30))?;
        wait_for_ping(Duration::from_secs(10))
    })();
    match result {
        Ok(()) => {
            txn.commit()?;
            eprintln!("kuruldu; servis çalışıyor ve IPC ping yanıtlandı (sürüm {version})");
            Ok(())
        }
        Err(e) => {
            let report = txn.rollback(&RealOps)?;
            eprintln!("geri alındı: {report:?}");
            Err(e)
        }
    }
}

pub fn dev_uninstall() -> anyhow::Result<()> {
    require_elevated(is_elevated()?)?;
    let paths = Paths::system()?;
    let journal = paths.journal_dir();
    let targets = Targets::product(paths.program_files.clone());
    let mut clean = true;
    if journal.exists() {
        // Korumasız bir journal'a göre yönetici yetkisiyle geri alma yapılmaz (K20).
        verify_protected_chain(&paths.machine, &journal)
            .map_err(|p| anyhow::anyhow!("journal dizini korumalı değil: {p:?}"))?;
        // Adlar `<bileşen>-<unix_ms>-<pid>`: ters ad sırası aynı bileşende en yeniyi öne alır.
        let mut files = list(&journal)?;
        files.reverse();
        for path in files {
            let Some(mut file) = JournalFile::open(&path)? else {
                eprintln!("kullanımda, atlandı: {}", path.display());
                clean = false;
                continue;
            };
            let report = revert_file(&mut file, &RealOps, &targets)?;
            eprintln!("geri alındı: {} {report:?}", path.display());
            clean &= report.clean();
        }
    }
    if clean {
        if paths.program_files.exists() {
            for entry in std::fs::read_dir(&paths.program_files)?.flatten() {
                let _ = std::fs::remove_dir(entry.path());
            }
            std::fs::remove_dir(&paths.program_files)
                .with_context(|| paths.program_files.display().to_string())?;
        }
        if paths.machine.exists() {
            verify_protected_chain(&paths.machine, &paths.machine)
                .map_err(|p| anyhow::anyhow!("veri dizini korumalı değil: {p:?}"))?;
            std::fs::remove_dir_all(&paths.machine)
                .with_context(|| paths.machine.display().to_string())?;
        }
    } else {
        eprintln!("geri alma temiz değil; dizinler silinmedi");
    }
    let residue = residue::scan(&paths, SERVICE_NAME, PIPE_NAME);
    if residue.is_empty() {
        eprintln!("kalıntı taraması temiz");
        Ok(())
    } else {
        bail!("kalıntı var: {residue:?}")
    }
}
