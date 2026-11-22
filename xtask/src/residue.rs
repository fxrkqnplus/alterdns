// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kalıntı taraması (`docs/ARCHITECTURE.md` §8): kaldırmadan sonra geride kalan AlterDNS
//! nesneleri. Yalnızca okur.

use std::path::PathBuf;

use alterdns_core::Paths;
use windows_registry::LOCAL_MACHINE;

pub const REGISTRY_KEY: &str = r"SOFTWARE\AlterDNS";
pub const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "AlterDNS";
pub const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\AlterDNS";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Residue {
    Service(String),
    Dir(PathBuf),
    RegistryKey(String),
    RunValue,
    Pipe(String),
    Unreadable(String),
}

fn pipe_exists(pipe: &str) -> bool {
    let Some(name) = pipe.strip_prefix(r"\\.\pipe\") else {
        return false;
    };
    std::fs::read_dir(r"\\.\pipe\").is_ok_and(|rd| {
        rd.filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
    })
}

/// Taranacak sistem nesneleri. Ürün sabit adları kullanır; testler makinedeki gerçek kuruluma
/// takılmamak için kendi adlarını verir.
#[derive(Clone, Copy, Debug)]
pub struct ResidueTargets<'a> {
    pub service: &'a str,
    pub pipe: &'a str,
    /// `HKLM` altındaki ürün anahtarı.
    pub registry_key: &'a str,
    /// `HKLM` altındaki kaldırma kaydı anahtarı.
    pub uninstall_key: &'a str,
    /// `HKLM\…\Run` altındaki değer adı.
    pub run_value: &'a str,
}

impl ResidueTargets<'static> {
    pub fn product() -> ResidueTargets<'static> {
        ResidueTargets {
            service: alterdns_journal::SERVICE_NAME,
            pipe: alterdns_ipc::PIPE_NAME,
            registry_key: REGISTRY_KEY,
            uninstall_key: UNINSTALL_KEY,
            run_value: RUN_VALUE,
        }
    }
}

pub fn scan(paths: &Paths, t: &ResidueTargets<'_>) -> Vec<Residue> {
    let mut out = Vec::new();
    match alterdns_winsys::query_service(t.service) {
        Ok(Some(_)) => out.push(Residue::Service(t.service.to_string())),
        Ok(None) => {}
        Err(e) => out.push(Residue::Unreadable(format!("servis {}: {e}", t.service))),
    }
    for dir in [&paths.program_files, &paths.machine, &paths.user] {
        if dir.exists() {
            out.push(Residue::Dir(dir.clone()));
        }
    }
    for key in [t.registry_key, t.uninstall_key] {
        if LOCAL_MACHINE.open(key).is_ok() {
            out.push(Residue::RegistryKey(format!(r"HKLM\{key}")));
        }
    }
    if LOCAL_MACHINE
        .open(RUN_KEY)
        .and_then(|k| k.get_type(t.run_value))
        .is_ok()
    {
        out.push(Residue::RunValue);
    }
    if pipe_exists(t.pipe) {
        out.push(Residue::Pipe(t.pipe.to_string()));
    }
    out
}
