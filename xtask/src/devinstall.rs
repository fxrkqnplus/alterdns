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
use alterdns_core::ui::{MSG_QUIT, TRAY_CLASS, WINDOW_CLASS};
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{Client, ConnectOptions, PIPE_NAME};
use alterdns_journal::{
    InstallOps, JournalFile, Kind, Lifetime, RUN_VALUE, RealOps, SERVICE_NAME, Snapshot, SystemOps,
    Targets, Txn, list, recover_at_startup, revert_file, sha256_file,
};
use alterdns_winsys::{
    DirAccess, create_protected_dir, is_elevated, post_broadcast, query_service, start_service,
    verify_protected_chain,
};
use anyhow::{Context, bail};

use crate::residue;

pub const COMPONENT: &str = "dev-install";
pub const BINARIES: [&str; 3] = ["alterdns-svc.exe", "alterdns-maint.exe", "alterdns-ui.exe"];
const BUILD_HINT: &str =
    "cargo build --release -p alterdns-svc -p alterdns-maint -p alterdns-ui-app";

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

/// Kurulum adımları: önce dosyalar, sonra servis, en son oturum açılışı kaydı. Yalnızca
/// kaynakları okur (hash).
pub fn plan(paths: &Paths, version: &str, target_dir: &Path) -> anyhow::Result<Vec<PlannedChange>> {
    let dir = paths.install_dir(version);
    let mut out = Vec::new();
    for bin in BINARIES {
        let source = target_dir.join(bin);
        if !source.is_file() {
            bail!(
                "derlenmiş ikili yok: {} (önce: {BUILD_HINT})",
                source.display()
            );
        }
        let bytes = std::fs::read(&source).with_context(|| source.display().to_string())?;
        if is_test_build(&bytes) {
            bail!(
                "{} bir test derlemesi (test-ops); kurulmaz. Önce: {BUILD_HINT}",
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
    // Oturum açılışında tepsi (K16); kullanıcı kapattıysa `--logon` ile açılan tepsi çıkar.
    out.push(PlannedChange {
        kind: Kind::StartupEntry,
        target: RUN_VALUE.to_string(),
        after: Snapshot::Present(format!(
            "\"{}\" --tray --logon",
            dir.join("alterdns-ui.exe").display()
        )),
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
    // Önceden var olan (başka kaynaklı) bir Run değeri journal'a "önceki değer" olarak girer ama
    // geri almada doğrulamadan geçemez; kurulum baştan reddedilir.
    if RealOps
        .read(Kind::StartupEntry, RUN_VALUE)
        .map_err(|e| anyhow::anyhow!("başlangıç kaydı okunamadı: {e}"))?
        != Snapshot::Absent
    {
        bail!(
            "{RUN_VALUE} zaten var; AlterDNS ona dokunmaz. Değeri kendiniz kaldırıp yeniden deneyin"
        );
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

/// Çalışan arayüz işlemlerini (tepsi, ana pencere) kapatır: kurulu `alterdns-ui.exe` açıkken
/// silinemez. Gizli pencereleri kaybolana kadar (en çok 10 sn) bekler. Yalnızca çağıranın
/// masaüstündeki pencerelere iletilir.
fn close_ui(install_root: &Path) -> anyhow::Result<()> {
    // Yalnızca sahibi kurulu `alterdns-ui.exe` olan pencerelere iletilir; aynı sınıf adlı başka
    // bir pencere (ör. target\ altından çalışan geliştirme sürümü) kurulu dosyayı kilitlemez.
    let installed = |class: &str| {
        alterdns_winsys::window_owner(class)
            .and_then(|pid| alterdns_winsys::process_image(pid).ok())
            .is_some_and(|image| image.starts_with(install_root))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut asked = false;
    loop {
        let tray = installed(TRAY_CLASS) && post_broadcast(TRAY_CLASS, MSG_QUIT)?;
        let window = installed(WINDOW_CLASS) && post_broadcast(WINDOW_CLASS, MSG_QUIT)?;
        if !tray && !window {
            break;
        }
        if !asked {
            eprintln!("arayüz kapatılıyor…");
            asked = true;
        }
        if Instant::now() >= deadline {
            bail!(
                "AlterDNS arayüzü kapanmadı: tepsi menüsünden Çıkış'ı seçin, ana pencereyi kapatın ve yeniden deneyin (başka bir kullanıcı oturumunda açıksa orada kapatın)"
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if asked {
        // Pencere yok edildikten sonra işlemin çıkıp dosyayı bırakması için.
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Arayüzün kullanıcı verisi (`%LOCALAPPDATA%\AlterDNS`: log ve ayarlar). Bağlantıysa silinmez.
fn remove_user_data(dir: &Path) -> anyhow::Result<()> {
    use std::os::windows::fs::MetadataExt;
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| dir.display().to_string()),
    };
    if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
        bail!("{} düz bir dizin değil; silinmedi", dir.display());
    }
    std::fs::remove_dir_all(dir).with_context(|| dir.display().to_string())?;
    eprintln!("kullanıcı verisi silindi: {}", dir.display());
    Ok(())
}

pub fn dev_uninstall() -> anyhow::Result<()> {
    require_elevated(is_elevated()?)?;
    let paths = Paths::system()?;
    close_ui(&paths.program_files)?;
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
        remove_user_data(&paths.user)?;
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
