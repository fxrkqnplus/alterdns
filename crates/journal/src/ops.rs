// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sistem okuma/yazma arayüzü ve hedef izin listesi (K2, ADR-014). Journal yalnızca AlterDNS'in
//! kendi nesnelerine dokunur; izin listesi dışındaki hedef uygulanmadan reddedilir.

use std::path::{Component, Path, PathBuf};

use crate::model::{Kind, Snapshot};

/// Gerçek ya da sahte sistem işlemleri. Hata metni loglanır, kullanıcıya katalog kodu gider.
pub trait SystemOps {
    fn read(&self, kind: Kind, target: &str) -> Result<Snapshot, String>;
    fn write(&self, kind: Kind, target: &str, value: &Snapshot) -> Result<(), String>;
}

pub const SERVICE_NAME: &str = "AlterDNS";
pub const REGISTRY_ROOT: &str = r"HKLM\SOFTWARE\AlterDNS";
pub const RUN_VALUE: &str = r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\AlterDNS";
pub const UNINSTALL_KEY: &str =
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\AlterDNS";
/// Yalnızca `test-ops`: kullanıcının kendi test anahtarı.
pub const TEST_REGISTRY_ROOT: &str = r"HKCU\Software\AlterDNS-Test";

/// Bir txn'in dokunabileceği hedefler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Targets {
    pub service: String,
    pub registry_root: String,
    pub run_value: String,
    pub uninstall_key: String,
    /// `%ProgramFiles%\AlterDNS`
    pub install_root: PathBuf,
}

impl Targets {
    pub fn product(install_root: PathBuf) -> Targets {
        Targets {
            service: SERVICE_NAME.into(),
            registry_root: REGISTRY_ROOT.into(),
            run_value: RUN_VALUE.into(),
            uninstall_key: UNINSTALL_KEY.into(),
            install_root,
        }
    }
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

fn no_parent_segments(s: &str) -> bool {
    !s.split(['\\', '/']).any(|seg| seg == ".." || seg == ".")
}

fn under_registry(target: &str, root: &str) -> bool {
    let t = lower(target);
    let r = format!("{}\\", lower(root).trim_end_matches('\\'));
    t.starts_with(&r) && t.len() > r.len() && no_parent_segments(target)
}

fn under_dir(target: &str, root: &Path) -> bool {
    let p = Path::new(target);
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        && lower(target).starts_with(&lower(&root.to_string_lossy()))
        && p.starts_with(root)
}

/// Geçici dizindeki `alterdns-*` alt dizini (yalnızca testler).
#[cfg(any(test, feature = "test-ops"))]
fn test_temp(target: &str) -> bool {
    let p = Path::new(target);
    let temp = std::env::temp_dir();
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        && p.strip_prefix(&temp).ok().is_some_and(|rest| {
            rest.components()
                .next()
                .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with("alterdns-"))
                && rest.components().count() >= 2
        })
}

/// Hedef bu txn'in izin listesinde mi? Kayıt defteri hedefleri ayrıca sabit ürün köküyle (ya da
/// `test-ops` ile kullanıcının test anahtarıyla) sınırlıdır; `Targets` yanlış kurulsa bile
/// başka bir anahtara dokunulmaz.
pub fn target_allowed(kind: Kind, target: &str, t: &Targets) -> bool {
    let registry_ok = |root: &str| {
        let fixed = under_registry(root, REGISTRY_ROOT) || lower(root) == lower(REGISTRY_ROOT);
        #[cfg(any(test, feature = "test-ops"))]
        let fixed = fixed
            || under_registry(root, TEST_REGISTRY_ROOT)
            || lower(root) == lower(TEST_REGISTRY_ROOT);
        fixed && under_registry(target, root)
    };
    match kind {
        Kind::ServiceCreate | Kind::ServiceConfig => {
            t.service == SERVICE_NAME && target.eq_ignore_ascii_case(SERVICE_NAME)
        }
        Kind::RegistryValue => registry_ok(&t.registry_root),
        Kind::StartupEntry => {
            target.eq_ignore_ascii_case(&t.run_value) && t.run_value.eq_ignore_ascii_case(RUN_VALUE)
        }
        Kind::UninstallEntry => {
            target.eq_ignore_ascii_case(&t.uninstall_key)
                && t.uninstall_key.eq_ignore_ascii_case(UNINSTALL_KEY)
        }
        Kind::FileInstall => under_dir(target, &t.install_root),
        #[cfg(any(test, feature = "test-ops"))]
        Kind::FileValue => test_temp(target),
        Kind::Unknown => false,
    }
}

/// AlterDNS'in oluşturduğu nesneler geri almada yalnızca kaldırılır, yeniden eklenmez.
pub fn is_own_object(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::ServiceCreate | Kind::FileInstall | Kind::UninstallEntry | Kind::StartupEntry
    )
}
