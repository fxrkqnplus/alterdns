// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-ui.exe` ikilisi: komut satırı, yükleme zamanı DLL'leri, sürüm bilgisi ve simge,
//! ana pencerenin tek örnek davranışı.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use alterdns_winsys::{icon_count, interactive_desktop, post_broadcast, system_dir};

const EXE: &str = env!("CARGO_BIN_EXE_alterdns-ui");

#[test]
fn exe_rejects_unknown_args() {
    for args in [&["--x"][..], &["--logon"], &["--page", "nope"]] {
        let code = Command::new(EXE)
            .args(args)
            .stdin(Stdio::null())
            .status()
            .unwrap()
            .code();
        assert_eq!(code, Some(2), "{args:?}");
    }
}

#[test]
fn exe_restricts_load_time_imports_to_system32() {
    let pe = std::fs::read(EXE).unwrap();
    assert_eq!(alterdns_buildinfo::dependent_load_flags(&pe), Some(0x0800));
}

/// Statik içe aktarmaların hepsi System32'de bulunan sistem DLL'leri ya da API kümeleridir;
/// `DEPENDENTLOADFLAG` yüklemeyi System32 ile sınırlar ve CRT statik bağlıdır (vcruntime yok).
#[test]
fn static_imports_are_system_dlls() {
    let pe = std::fs::read(EXE).unwrap();
    let imports = alterdns_buildinfo::imported_dlls(&pe).unwrap();
    assert!(imports.contains(&"kernel32.dll".to_string()), "{imports:?}");
    let system = system_dir().unwrap();
    for dll in &imports {
        let ok =
            dll.starts_with("api-ms-") || dll.starts_with("ext-ms-") || system.join(dll).is_file();
        assert!(
            ok,
            "System32'de olmayan statik içe aktarma: {dll} ({imports:?})"
        );
        assert!(
            !dll.starts_with("vcruntime") && !dll.starts_with("msvcp"),
            "{dll}"
        );
    }
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn exe_version_info_and_icon() {
    let pe = std::fs::read(EXE).unwrap();
    for text in ["Furkan Okan", "AlterDNS", "alterdns-ui.exe", "PerMonitorV2"] {
        let needle = if text == "PerMonitorV2" {
            text.as_bytes().to_vec()
        } else {
            utf16(text)
        };
        assert!(
            pe.windows(needle.len()).any(|w| w == needle.as_slice()),
            "{text} yok"
        );
    }
    assert!(icon_count(std::path::Path::new(EXE)) >= 1);
}

/// İkinci ana pencere işlemi ilkini öne getirir ve 0 ile çıkar; ilki açık kalır.
#[test]
fn second_window_activates_first() {
    if !interactive_desktop() {
        eprintln!("etkileşimli masaüstü yok; atlandı");
        return;
    }
    let mut first = Command::new(EXE).spawn().unwrap();
    let ready = Instant::now();
    // İlk örneğin ileti penceresi oluşunca yayın hedefi bulunur.
    while !post_broadcast("AlterDNS.Window", 2).unwrap() {
        assert!(
            ready.elapsed() < Duration::from_secs(20),
            "ilk pencere açılmadı"
        );
        assert!(first.try_wait().unwrap().is_none(), "ilk örnek erken çıktı");
        std::thread::sleep(Duration::from_millis(100));
    }
    let second = Command::new(EXE)
        .args(["--page", "about"])
        .status()
        .unwrap();
    assert_eq!(second.code(), Some(0));
    assert!(first.try_wait().unwrap().is_none(), "ilk örnek kapanmamalı");
    let _ = first.kill();
    let _ = first.wait();
}
