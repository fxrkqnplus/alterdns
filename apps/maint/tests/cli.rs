// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-maint` komut satırı ve exe sürüm bilgisi.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_alterdns-maint");

fn run(args: &[&str]) -> Option<i32> {
    Command::new(EXE)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .code()
}

#[test]
fn unknown_command_exits_2() {
    assert_eq!(run(&[]), Some(2));
    assert_eq!(run(&["bilinmeyen"]), Some(2));
    assert_eq!(run(&["watchdog"]), Some(2));
    assert_eq!(run(&["watchdog", "--txn"]), Some(2));
    assert_eq!(run(&["watchdog", "--txn", "../x"]), Some(2));
    assert_eq!(run(&["watchdog", "--txn", "a", "fazla"]), Some(2));
    // Komut satırından yol alınmaz.
    assert_eq!(
        run(&["watchdog", "--journal", r"C:\x", "--txn", "a"]),
        Some(2)
    );
}

/// Ürün dizini yoksa (ya da korumalı değilse) bekçi hiçbir şeye dokunmadan 1 ile çıkar.
#[test]
fn missing_product_journal_dir_is_refused() {
    let dir = alterdns_winsys::known_folder(alterdns_winsys::KnownFolder::ProgramData)
        .unwrap()
        .join(r"AlterDNS\journal");
    if dir.exists() {
        // Geliştirme kurulumundan sonra bu makinede sınanmaz: ürün dizinine yazılmaz.
        return;
    }
    let status = Command::new(EXE)
        .args(["watchdog", "--txn", "a"])
        .env_remove("ALTERDNS_TEST_JOURNAL_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(!dir.exists());
}

/// `CONTRIBUTING.md` sürüm bilgisi kuralları, derlenmiş exe'den okunarak.
#[test]
fn exe_version_info_matches_rules() {
    let ps = alterdns_winsys::system_dir()
        .unwrap()
        .join(r"WindowsPowerShell\v1.0\powershell.exe");
    let exe = EXE.replace('\'', "''");
    let script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         (Get-Item -LiteralPath '{exe}').VersionInfo | Select-Object CompanyName, ProductName, \
         LegalCopyright, FileVersion, ProductVersion, OriginalFilename, FileDescription \
         | ConvertTo-Json -Compress"
    );
    let out = Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}'))
        .unwrap_or_else(|e| panic!("{e}: {text}"));
    let version = env!("CARGO_PKG_VERSION");
    for (key, expected) in [
        ("CompanyName", "Furkan Okan"),
        ("ProductName", "AlterDNS"),
        ("LegalCopyright", "Copyright © 2026 Furkan Okan"),
        ("FileVersion", version),
        ("ProductVersion", version),
        ("OriginalFilename", "alterdns-maint.exe"),
        ("FileDescription", "AlterDNS bakım"),
    ] {
        assert_eq!(v[key], expected, "{key}: {v}");
    }
}

/// K18: `/DEPENDENTLOADFLAG:0x800` (`LOAD_LIBRARY_SEARCH_SYSTEM32`), exe'nin PE yükleme
/// yapılandırmasından okunarak. Yükleme zamanı içe aktarımları `main`'den önce çözülür.
#[test]
fn exe_restricts_load_time_imports_to_system32() {
    let pe = std::fs::read(EXE).unwrap();
    let u16_at = |o: usize| u16::from_le_bytes([pe[o], pe[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([pe[o], pe[o + 1], pe[o + 2], pe[o + 3]]);
    let nt = u32_at(0x3C) as usize;
    assert_eq!(&pe[nt..nt + 4], b"PE\0\0");
    let coff = nt + 4;
    let sections = u16_at(coff + 2) as usize;
    let opt = coff + 20;
    let opt_size = u16_at(coff + 16) as usize;
    assert_eq!(u16_at(opt), 0x20B, "PE32+");
    // Veri dizinleri PE32+'da isteğe bağlı başlığın 112. baytından başlar; 10 = yükleme yapılandırması.
    let rva = u32_at(opt + 112 + 10 * 8) as usize;
    assert_ne!(rva, 0, "yükleme yapılandırması yok");
    let table = opt + opt_size;
    let offset = (0..sections)
        .map(|i| table + i * 40)
        .find_map(|s| {
            let va = u32_at(s + 12) as usize;
            let size = u32_at(s + 8).max(u32_at(s + 16)) as usize;
            let raw = u32_at(s + 20) as usize;
            (va..va + size).contains(&rva).then(|| rva - va + raw)
        })
        .unwrap();
    // IMAGE_LOAD_CONFIG_DIRECTORY64.DependentLoadFlags: 0x4E.
    assert!(
        u32_at(offset) as usize >= 0x50,
        "yapı DependentLoadFlags'i içermiyor"
    );
    assert_eq!(u16_at(offset + 0x4E), 0x0800);
}
