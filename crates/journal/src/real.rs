// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gerçek sistem işlemleri. Her çağrı, `Targets`'tan bağımsız olarak sabit kökleri yeniden
//! denetler: yanlış kurulmuş bir txn bile kullanıcıya ait bir değere dokunamaz (ADR-014).
//!
//! - `RegistryValue`: `HKLM\SOFTWARE\AlterDNS\...\<değer>` (yalnızca `test-ops` ile
//!   `HKCU\Software\AlterDNS-Test\...`). Yalnızca REG_SZ; başka türde bir değer hatadır, "yok"
//!   sayılmaz.
//! - `FileValue`: yalnızca `test-ops`, `%TEMP%\alterdns-*` altı.
//! - `FileInstall`: `%ProgramFiles%\AlterDNS\…` (yalnızca `test-ops` ile `%TEMP%\alterdns-*`).
//!   Değer dosyanın SHA-256'sıdır. Okuma ve silme burada; kopyalama `InstallOps`'ta.
//! - `ServiceCreate`: yalnızca `AlterDNS` servisi; değer ikilinin yoludur (tırnaksız).
//! - Servis ayarı ve kaldırma kaydı Faz 5'te, başlangıç kaydı 1D'de gelir.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use alterdns_winsys::{
    KnownFolder, ServiceSpec, create_service, delete_service, known_folder, query_service,
    verify_protected_chain,
};
use sha2::{Digest, Sha256};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

use crate::model::{Kind, Snapshot};
use crate::ops::{SERVICE_NAME, SystemOps, registry_target_fixed};

/// Servisin durması ve silinmesi için beklenen en uzun süre.
const SERVICE_STOP_WAIT: Duration = Duration::from_secs(30);
/// `FILE_FLAG_OPEN_REPARSE_POINT`: bağlantı izlenmez, kendisi açılır.
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
/// `FILE_ATTRIBUTE_REPARSE_POINT`.
const ATTR_REPARSE_POINT: u32 = 0x400;

/// Gerçek Windows işlemleri.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealOps;

impl SystemOps for RealOps {
    fn read(&self, kind: Kind, target: &str) -> Result<Snapshot, String> {
        match kind {
            Kind::RegistryValue => read_registry(target),
            Kind::FileInstall => read_file_install(target),
            Kind::ServiceCreate => read_service(target),
            #[cfg(any(test, feature = "test-ops"))]
            Kind::FileValue => read_file_value(target),
            other => Err(unsupported(other)),
        }
    }

    fn write(&self, kind: Kind, target: &str, value: &Snapshot) -> Result<(), String> {
        match kind {
            Kind::RegistryValue => write_registry(target, value),
            Kind::FileInstall => match value {
                Snapshot::Absent => remove_file_install(target),
                Snapshot::Present(_) => {
                    Err("kurulum dosyası kaynaktan kopyalanır (InstallOps)".into())
                }
            },
            Kind::ServiceCreate => write_service(target, value),
            #[cfg(any(test, feature = "test-ops"))]
            Kind::FileValue => write_file_value(target, value),
            other => Err(unsupported(other)),
        }
    }
}

fn unsupported(kind: Kind) -> String {
    match kind {
        Kind::StartupEntry => "başlangıç kaydı 1D'de gelir".into(),
        Kind::Unknown => "bilinmeyen değişiklik türü".into(),
        other => format!("{other:?} Faz 5'te gelir"),
    }
}

/// `%ProgramFiles%\AlterDNS` (bilinen klasörden).
pub fn product_install_root() -> Result<PathBuf, String> {
    known_folder(KnownFolder::ProgramFiles)
        .map(|p| p.join("AlterDNS"))
        .map_err(|e| e.to_string())
}

fn plain_absolute(p: &Path) -> bool {
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}

/// Kurulum dosyası hedefi: sabit kök altında, `..` yok, göreli kısımda `:` (alternatif veri
/// akışı) yok. Ürün kökünde üst dizinler korumalı olmalıdır (zincir denetimi çağıranda).
/// Dönen ikinci değer: zincir denetiminin kökü (`test-ops` geçici hedeflerinde `None`).
fn file_target(target: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let p = Path::new(target);
    if !plain_absolute(p) {
        return Err(format!("geçersiz dosya hedefi: {target}"));
    }
    let root = product_install_root()?;
    if let Ok(rest) = p.strip_prefix(&root) {
        if rest.as_os_str().is_empty() || rest.to_string_lossy().contains(':') {
            return Err(format!("geçersiz dosya hedefi: {target}"));
        }
        return Ok((p.to_path_buf(), Some(root)));
    }
    #[cfg(any(test, feature = "test-ops"))]
    if crate::ops::test_temp(target) {
        let rest = p
            .strip_prefix(std::env::temp_dir())
            .map_err(|e| e.to_string())?;
        if rest.to_string_lossy().contains(':') {
            return Err(format!("geçersiz dosya hedefi: {target}"));
        }
        return Ok((p.to_path_buf(), None));
    }
    Err(format!("izin dışı dosya hedefi: {target}"))
}

/// Hedef var mı? Varsa üst dizin zinciri denetlenir ve hedef düz bir dosya olmalıdır.
fn existing_file(target: &str) -> Result<Option<PathBuf>, String> {
    let (path, root) = file_target(target)?;
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if let (Some(root), Some(parent)) = (root, path.parent()) {
        verify_protected_chain(&root, parent).map_err(|p| format!("kurulum dizini: {p:?}"))?;
    }
    use std::os::windows::fs::MetadataExt;
    if meta.file_attributes() & ATTR_REPARSE_POINT != 0 || !meta.is_file() {
        return Err(format!("düz bir dosya değil: {target}"));
    }
    Ok(Some(path))
}

/// SHA-256, küçük harf onaltılık.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn read_file_install(target: &str) -> Result<Snapshot, String> {
    match existing_file(target)? {
        None => Ok(Snapshot::Absent),
        Some(path) => sha256_file(&path)
            .map(Snapshot::Present)
            .map_err(|e| e.to_string()),
    }
}

fn remove_file_install(target: &str) -> Result<(), String> {
    match existing_file(target)? {
        None => Ok(()),
        Some(path) => match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other.map_err(|e| e.to_string()),
        },
    }
}

/// Hedefi denetler ve kurulum için yolunu verir (yalnızca `InstallOps`).
pub(crate) fn install_target(target: &str) -> Result<PathBuf, String> {
    let (path, root) = file_target(target)?;
    if let (Some(root), Some(parent)) = (root, path.parent()) {
        verify_protected_chain(&root, parent).map_err(|p| format!("kurulum dizini: {p:?}"))?;
    }
    Ok(path)
}

/// SCM'nin sakladığı komut satırı tırnaklıdır; journal değeri tırnaksız yoldur.
fn normalize_binary(raw: &str) -> String {
    raw.trim().trim_matches('"').to_string()
}

fn read_service(target: &str) -> Result<Snapshot, String> {
    match query_service(target).map_err(|e| e.to_string())? {
        Some(s) => Ok(Snapshot::Present(normalize_binary(&s.binary_path))),
        None => Ok(Snapshot::Absent),
    }
}

/// Servis ikilisi: `%ProgramFiles%\AlterDNS\<sürüm>\alterdns-svc.exe`, var olan bir dosya.
pub fn validate_service_binary(p: &str) -> Result<PathBuf, String> {
    let path = Path::new(p);
    let root = product_install_root()?;
    let ok_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("alterdns-svc.exe"));
    let ok_depth = path.parent().and_then(Path::parent) == Some(root.as_path());
    if !plain_absolute(path) || !ok_name || !ok_depth {
        return Err(format!("izin dışı servis ikilisi: {p}"));
    }
    let real = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let real_root = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
    if !real.starts_with(&real_root) {
        return Err(format!("servis ikilisi kurulum dizininde değil: {p}"));
    }
    Ok(path.to_path_buf())
}

fn write_service(target: &str, value: &Snapshot) -> Result<(), String> {
    if !target.eq_ignore_ascii_case(SERVICE_NAME) {
        return Err(format!("izin dışı servis: {target}"));
    }
    match value {
        Snapshot::Present(p) => {
            let binary = validate_service_binary(p)?;
            create_service(&ServiceSpec {
                name: SERVICE_NAME,
                display_name: SERVICE_NAME,
                binary: &binary,
            })
            .map_err(|e| e.to_string())
        }
        Snapshot::Absent => {
            delete_service(SERVICE_NAME, SERVICE_STOP_WAIT).map_err(|e| e.to_string())
        }
    }
}

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)` ve `(ERROR_PATH_NOT_FOUND)`.
const NOT_FOUND: [u32; 2] = [0x8007_0002, 0x8007_0003];

fn is_not_found<T>(r: &windows_registry::Result<T>) -> bool {
    matches!(r, Err(e) if NOT_FOUND.contains(&e.code().0.cast_unsigned()))
}

struct RegTarget<'a> {
    hive: &'static Key,
    key: &'a str,
    value: &'a str,
}

fn parse_registry(target: &str) -> Result<RegTarget<'_>, String> {
    if !registry_target_fixed(target) {
        return Err(format!("izin dışı kayıt defteri hedefi: {target}"));
    }
    let (hive, rest) = target
        .split_once('\\')
        .ok_or_else(|| format!("kök anahtar yok: {target}"))?;
    let (key, value) = rest
        .rsplit_once('\\')
        .ok_or_else(|| format!("değer adı yok: {target}"))?;
    if key.is_empty() || value.is_empty() {
        return Err(format!("anahtar ya da değer adı boş: {target}"));
    }
    let hive = match hive.to_ascii_uppercase().as_str() {
        "HKLM" => LOCAL_MACHINE,
        "HKCU" => CURRENT_USER,
        _ => return Err(format!("desteklenmeyen kök anahtar: {target}")),
    };
    Ok(RegTarget { hive, key, value })
}

fn read_registry(target: &str) -> Result<Snapshot, String> {
    let t = parse_registry(target)?;
    let key = t.hive.open(t.key);
    if is_not_found(&key) {
        return Ok(Snapshot::Absent);
    }
    let key = key.map_err(|e| format!("anahtar açılamadı: {e}"))?;
    let ty = key.get_type(t.value);
    if is_not_found(&ty) {
        return Ok(Snapshot::Absent);
    }
    match ty.map_err(|e| format!("değer okunamadı: {e}"))? {
        Type::String => key
            .get_string(t.value)
            .map(Snapshot::Present)
            .map_err(|e| format!("değer okunamadı: {e}")),
        other => Err(format!("beklenmeyen değer türü: {other:?}")),
    }
}

fn write_registry(target: &str, value: &Snapshot) -> Result<(), String> {
    let t = parse_registry(target)?;
    match value {
        Snapshot::Present(s) => {
            if s.contains('\0') {
                return Err("değer NUL karakteri içeremez".into());
            }
            t.hive
                .create(t.key)
                .and_then(|k| k.set_string(t.value, s))
                .map_err(|e| format!("değer yazılamadı: {e}"))
        }
        Snapshot::Absent => {
            let key = t.hive.options().write().open(t.key);
            if is_not_found(&key) {
                return Ok(());
            }
            let key = key.map_err(|e| format!("anahtar açılamadı: {e}"))?;
            let removed = key.remove_value(t.value);
            if is_not_found(&removed) {
                return Ok(());
            }
            removed.map_err(|e| format!("değer silinemedi: {e}"))
        }
    }
}

#[cfg(any(test, feature = "test-ops"))]
fn read_file_value(target: &str) -> Result<Snapshot, String> {
    use crate::file::MAX_VALUE_BYTES;
    if !crate::ops::test_temp(target) {
        return Err(format!("geçici dizin dışında: {target}"));
    }
    match std::fs::symlink_metadata(target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Snapshot::Absent),
        Err(e) => Err(format!("dosya okunamadı: {e}")),
        Ok(m) if !m.is_file() => Err(format!("normal dosya değil: {target}")),
        Ok(m) if m.len() > MAX_VALUE_BYTES as u64 => Err(format!("dosya çok büyük: {target}")),
        Ok(_) => std::fs::read_to_string(target)
            .map(Snapshot::Present)
            .map_err(|e| format!("dosya okunamadı: {e}")),
    }
}

#[cfg(any(test, feature = "test-ops"))]
fn write_file_value(target: &str, value: &Snapshot) -> Result<(), String> {
    if !crate::ops::test_temp(target) {
        return Err(format!("geçici dizin dışında: {target}"));
    }
    match value {
        Snapshot::Present(s) => std::fs::write(target, s).map_err(|e| e.to_string()),
        Snapshot::Absent => match std::fs::remove_file(target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other.map_err(|e| e.to_string()),
        },
    }
}
