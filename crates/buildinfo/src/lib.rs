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
