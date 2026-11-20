// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gerçek sistem işlemleri. Her çağrı, `Targets`'tan bağımsız olarak sabit kökleri yeniden
//! denetler: yanlış kurulmuş bir txn bile kullanıcıya ait bir değere dokunamaz (ADR-014).
//!
//! - `RegistryValue`: `HKLM\SOFTWARE\AlterDNS\...\<değer>` (yalnızca `test-ops` ile
//!   `HKCU\Software\AlterDNS-Test\...`). Yalnızca REG_SZ; başka türde bir değer hatadır, "yok"
//!   sayılmaz.
//! - `FileValue`: yalnızca `test-ops`, `%TEMP%\alterdns-*` altı.
//! - Servis, kaldırma kaydı ve kurulum dosyaları 1C'de, başlangıç kaydı 1D'de gelir.

use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

use crate::model::{Kind, Snapshot};
use crate::ops::{SystemOps, registry_target_fixed};

/// Gerçek Windows işlemleri.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealOps;

impl SystemOps for RealOps {
    fn read(&self, kind: Kind, target: &str) -> Result<Snapshot, String> {
        match kind {
            Kind::RegistryValue => read_registry(target),
            #[cfg(any(test, feature = "test-ops"))]
            Kind::FileValue => read_file_value(target),
            other => Err(unsupported(other)),
        }
    }

    fn write(&self, kind: Kind, target: &str, value: &Snapshot) -> Result<(), String> {
        match kind {
            Kind::RegistryValue => write_registry(target, value),
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
        other => format!("{other:?} 1C'de gelir"),
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
