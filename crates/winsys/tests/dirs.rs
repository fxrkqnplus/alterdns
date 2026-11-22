// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sistem dizinleri, bilinen klasörler ve DLL arama yolu.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_winsys::{KnownFolder, harden_dll_search, known_folder, system_dir, windows_dir};

#[test]
fn system_dir_is_absolute_and_under_windows_dir() {
    let sys = system_dir().unwrap();
    let win = windows_dir().unwrap();
    assert!(sys.is_absolute(), "{}", sys.display());
    assert!(
        sys.starts_with(&win),
        "{} / {}",
        sys.display(),
        win.display()
    );
    assert!(sys.join("kernel32.dll").is_file());
}

const MARKER: &str = "ALTERDNS_WINSYS_PRINT_WINDOWS_DIR";

/// Alt işlemde çalışır: işaret varsa Windows dizinini basar.
#[test]
fn print_windows_dir_helper() {
    if std::env::var_os(MARKER).is_some() {
        println!("WINDOWS_DIR={}", windows_dir().unwrap().display());
    }
}

/// Faz 0 S-2: yol `SystemRoot` ortam değişkeninden değil, Win32 API'sinden gelir.
#[test]
fn windows_dir_ignores_systemroot_env() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "print_windows_dir_helper", "--nocapture"])
        .env(MARKER, "1")
        .env("SystemRoot", r"C:\sahte")
        .env("windir", r"C:\sahte")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find_map(|l| l.strip_prefix("WINDOWS_DIR="))
        .unwrap_or_else(|| panic!("çıktı yok: {stdout}"));
    assert_ne!(line, r"C:\sahte");
    assert_eq!(line, windows_dir().unwrap().display().to_string());
}

#[test]
fn known_folders_are_absolute_and_exist() {
    for f in [
        KnownFolder::ProgramData,
        KnownFolder::LocalAppData,
        KnownFolder::ProgramFiles,
    ] {
        let p = known_folder(f).unwrap();
        assert!(p.is_absolute() && p.is_dir(), "{f:?}: {}", p.display());
    }
}

#[test]
fn harden_dll_search_succeeds() {
    harden_dll_search(false).unwrap();
    harden_dll_search(true).unwrap();
}
