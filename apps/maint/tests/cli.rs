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
    assert_eq!(alterdns_buildinfo::dependent_load_flags(&pe), Some(0x0800));
}

/// Statik içe aktarmaların hepsi KnownDLL, `ntdll` ya da API kümesidir: bırakılan bir DLL
/// yüklenemez ve Windows'ta olmayan bir çalışma zamanına (ör. `vcruntime140.dll`) bağımlılık
/// yoktur (CRT statik bağlanır).
#[test]
fn static_imports_are_system_dlls() {
    let pe = std::fs::read(EXE).unwrap();
    let imports = alterdns_buildinfo::imported_dlls(&pe).unwrap();
    assert!(imports.contains(&"kernel32.dll".to_string()), "{imports:?}");
    let known: Vec<String> = windows_registry::LOCAL_MACHINE
        .open(r"SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs")
        .unwrap()
        .values()
        .unwrap()
        .filter_map(|(_, v)| String::try_from(v).ok())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    for dll in &imports {
        let ok = dll == "ntdll.dll"
            || dll.starts_with("api-ms-")
            || dll.starts_with("ext-ms-")
            || known.contains(dll);
        assert!(
            ok,
            "KnownDLL olmayan statik içe aktarma: {dll} ({imports:?})"
        );
    }
}
