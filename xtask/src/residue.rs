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

pub fn scan(paths: &Paths, service: &str, pipe: &str) -> Vec<Residue> {
    let mut out = Vec::new();
    match alterdns_winsys::query_service(service) {
        Ok(Some(_)) => out.push(Residue::Service(service.to_string())),
        Ok(None) => {}
        Err(e) => out.push(Residue::Unreadable(format!("servis {service}: {e}"))),
    }
    for dir in [&paths.program_files, &paths.machine, &paths.user] {
        if dir.exists() {
            out.push(Residue::Dir(dir.clone()));
        }
    }
    for key in [REGISTRY_KEY, UNINSTALL_KEY] {
        if LOCAL_MACHINE.open(key).is_ok() {
            out.push(Residue::RegistryKey(format!(r"HKLM\{key}")));
        }
    }
    if LOCAL_MACHINE
        .open(RUN_KEY)
        .and_then(|k| k.get_type(RUN_VALUE))
        .is_ok()
    {
        out.push(Residue::RunValue);
    }
    if pipe_exists(pipe) {
        out.push(Residue::Pipe(pipe.to_string()));
    }
    out
}
