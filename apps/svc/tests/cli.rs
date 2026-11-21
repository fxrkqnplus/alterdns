// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-svc` ikilisi: komut satırı, SCM dışı çalıştırma ve yükleme zamanı DLL'leri (K18).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_alterdns-svc");

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
fn exe_rejects_unknown_args_and_runs_only_under_scm() {
    assert_eq!(run(&["fazla"]), Some(2));
    // SCM dışından çalıştırılınca dağıtıcı bağlanamaz; hiçbir şey yapmadan 1 ile çıkar.
    assert_eq!(run(&[]), Some(1));
}

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
