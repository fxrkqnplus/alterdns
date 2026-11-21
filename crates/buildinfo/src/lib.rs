// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS exe sürüm bilgisi ve bağlayıcı bayrakları; her `apps/*/build.rs`'ten çağrılır.
//!
//! Sürüm bilgisi alanları `CONTRIBUTING.md` kod kurallarındadır. Bağlayıcı bayrağı
//! `/DEPENDENTLOADFLAG:0x800` (`LOAD_LIBRARY_SEARCH_SYSTEM32`), `main`'den önce çözülen
//! yükleme zamanı içe aktarımlarını da System32 ile sınırlar.

#![forbid(unsafe_code)]

pub struct ExeInfo {
    pub file_description: &'static str,
    pub original_filename: &'static str,
}

/// Exe'ye yazılacak sürüm bilgisi alanları; sürüm bütün ikililerde aynıdır.
pub fn version_fields(info: &ExeInfo, version: &str) -> Vec<(&'static str, String)> {
    vec![
        ("CompanyName", "Furkan Okan".to_string()),
        ("LegalCopyright", "Copyright © 2026 Furkan Okan".to_string()),
        ("ProductName", "AlterDNS".to_string()),
        ("FileVersion", version.to_string()),
        ("ProductVersion", version.to_string()),
        ("OriginalFilename", info.original_filename.to_string()),
        ("InternalName", info.original_filename.to_string()),
        ("FileDescription", info.file_description.to_string()),
    ]
}

/// Her exe'ye eklenen bağlayıcı bayrakları.
pub fn link_args() -> &'static [&'static str] {
    &["/DEPENDENTLOADFLAG:0x800"]
}

/// `build.rs` içinden: sürüm bilgisini gömer ve bağlayıcı bayraklarını ekler. Windows dışı
/// hedefte bir şey yapmaz.
pub fn embed(info: &ExeInfo) -> std::io::Result<()> {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let version = std::env::var("CARGO_PKG_VERSION").map_err(std::io::Error::other)?;
    let mut res = winresource::WindowsResource::new();
    for (key, value) in version_fields(info, &version) {
        res.set(key, &value);
    }
    res.compile()?;
    for arg in link_args() {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
    Ok(())
}

fn u16_at(pe: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*pe.get(o)?, *pe.get(o + 1)?]))
}

fn u32_at(pe: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *pe.get(o)?,
        *pe.get(o + 1)?,
        *pe.get(o + 2)?,
        *pe.get(o + 3)?,
    ]))
}

/// PE32+ yükleme yapılandırmasındaki `DependentLoadFlags` alanının dosyadaki konumu
/// (`IMAGE_LOAD_CONFIG_DIRECTORY64` + 0x4E). Testler ve DLL bırakma karşı denemesi için.
pub fn dependent_load_flags_offset(pe: &[u8]) -> Option<usize> {
    let nt = u32_at(pe, 0x3C)? as usize;
    if pe.get(nt..nt + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = nt + 4;
    let sections = usize::from(u16_at(pe, coff + 2)?);
    let opt = coff + 20;
    let opt_size = usize::from(u16_at(pe, coff + 16)?);
    if u16_at(pe, opt)? != 0x20B {
        return None;
    }
    // Veri dizinleri PE32+'da isteğe bağlı başlığın 112. baytından başlar; 10 = yükleme
    // yapılandırması.
    let rva = u32_at(pe, opt + 112 + 10 * 8)? as usize;
    if rva == 0 {
        return None;
    }
    let table = opt + opt_size;
    let offset = (0..sections).find_map(|i| {
        let s = table + i * 40;
        let size = u32_at(pe, s + 8)?.max(u32_at(pe, s + 16)?) as usize;
        let va = u32_at(pe, s + 12)? as usize;
        let raw = u32_at(pe, s + 20)? as usize;
        (va..va + size).contains(&rva).then(|| rva - va + raw)
    })?;
    (u32_at(pe, offset)? as usize >= 0x50).then_some(offset + 0x4E)
}

/// Exe'nin `DependentLoadFlags` değeri (beklenen `0x0800`).
pub fn dependent_load_flags(pe: &[u8]) -> Option<u16> {
    u16_at(pe, dependent_load_flags_offset(pe)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_match_contributing_rules() {
        let f = version_fields(
            &ExeInfo {
                file_description: "AlterDNS servisi",
                original_filename: "alterdns-svc.exe",
            },
            "0.1.0",
        );
        for expected in [
            ("CompanyName", "Furkan Okan"),
            ("LegalCopyright", "Copyright © 2026 Furkan Okan"),
            ("ProductName", "AlterDNS"),
            ("FileVersion", "0.1.0"),
            ("ProductVersion", "0.1.0"),
            ("OriginalFilename", "alterdns-svc.exe"),
            ("FileDescription", "AlterDNS servisi"),
        ] {
            assert!(
                f.iter().any(|(k, v)| *k == expected.0 && v == expected.1),
                "{expected:?}"
            );
        }
    }

    #[test]
    fn link_args_restrict_load_time_imports() {
        assert_eq!(link_args(), ["/DEPENDENTLOADFLAG:0x800"]);
    }
}
