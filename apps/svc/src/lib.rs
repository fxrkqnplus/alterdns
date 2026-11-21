// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS servisi. Açılış sırası:
//! 1. Veri dizini zinciri (`AlterDNS` → `journal`, `AlterDNS` → `logs`) korumalı olmalı (K20);
//!    değilse `ADNS-SVC-001`.
//! 2. Journal dizini servis çalıştığı sürece sabitlenir; log açılır.
//! 3. `recover_at_startup`: commit edilmemiş ve `Session` txn'leri geri alınır.
//! 4. IPC sunucusu; pipe başka bir işlemdeyse `ADNS-IPC-005`.

#![forbid(unsafe_code)]

mod handler;

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use alterdns_core::Paths;
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{Server, ServerConfig};
use alterdns_journal::{RealOps, Targets, recover_at_startup};
use alterdns_winsys::{DirPin, pin_dir, verify_protected_chain};

use crate::handler::{SvcHandler, outcome_name};

pub struct SvcConfig {
    pub paths: Paths,
    pub pipe_name: String,
    require_protected_dirs: bool,
    log: bool,
    #[cfg(feature = "test-ops")]
    test_access: bool,
}

impl SvcConfig {
    pub fn product() -> Result<SvcConfig, AdnsError> {
        Ok(SvcConfig {
            paths: Paths::system()?,
            pipe_name: alterdns_ipc::PIPE_NAME.to_string(),
            require_protected_dirs: true,
            log: true,
            #[cfg(feature = "test-ops")]
            test_access: false,
        })
    }

    /// Yalnızca testler: kökler ve pipe adı dışarıdan, test DACL'i, log kapalı (log tek bir
    /// genel abonedir). `require_protected_dirs = false` dizinleri oluşturur ve denetlemez.
    #[cfg(feature = "test-ops")]
    pub fn test(paths: Paths, pipe_name: &str, require_protected_dirs: bool) -> SvcConfig {
        SvcConfig {
            paths,
            pipe_name: pipe_name.to_string(),
            require_protected_dirs,
            log: false,
            test_access: true,
        }
    }
}

pub struct Running {
    server: Server,
    _journal_pin: DirPin,
}

impl Running {
    pub fn stop(self) {
        tracing::info!("servis duruyor");
        self.server.stop();
    }
}

fn start_failed(detail: String) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SvcStartFailed).with_source(std::io::Error::other(detail))
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

pub fn start(cfg: SvcConfig) -> Result<Running, AdnsError> {
    let started = Instant::now();
    let paths = &cfg.paths;
    let journal = paths.journal_dir();
    let logs = paths.machine_logs();
    if cfg.require_protected_dirs {
        for leaf in [&journal, &logs] {
            verify_protected_chain(&paths.machine, leaf)
                .map_err(|p| start_failed(format!("{}: {p:?}", leaf.display())))?;
        }
    } else {
        for dir in [&journal, &logs] {
            std::fs::create_dir_all(dir).map_err(|e| start_failed(e.to_string()))?;
        }
    }
    let journal_pin = pin_dir(&journal).map_err(|e| start_failed(e.to_string()))?;
    if cfg.log {
        // Log açılamazsa servis yine çalışır; hata servis çıkışına değil, olmayan loga düşer.
        let _ = alterdns_core::init_log(&logs, "svc");
    }
    tracing::info!(version = alterdns_core::version::VERSION, "servis başlıyor");

    let (_, outcome) = alterdns_core::load_machine(&paths.machine_config())?;
    tracing::info!(outcome = outcome_name(&outcome), "makine yapılandırması");

    let targets = Targets::product(paths.program_files.clone());
    let report = recover_at_startup(&journal, &RealOps, &targets);
    tracing::info!(
        reverted = report.txns.len(),
        kept = report.kept.len(),
        in_use = report.in_use.len(),
        errors = report.errors.len(),
        clean = report.clean(),
        "açılış kurtarması"
    );
    for (name, e) in &report.errors {
        tracing::warn!(
            txn = name.as_str(),
            code = e.code().as_str(),
            "kurtarma hatası"
        );
    }

    let handler = Arc::new(SvcHandler {
        started,
        machine_config: paths.machine_config(),
        events: OnceLock::new(),
    });
    let mut server_cfg = ServerConfig::product(
        paths.program_files.clone(),
        alterdns_core::version::VERSION,
        unix_ms(),
    );
    server_cfg.pipe_name = cfg.pipe_name.clone();
    #[cfg(feature = "test-ops")]
    if cfg.test_access {
        server_cfg = server_cfg.test_access();
    }
    let server = Server::start(server_cfg, handler.clone()).inspect_err(|e| {
        tracing::error!(code = e.code().as_str(), "IPC sunucusu başlatılamadı");
    })?;
    let _ = handler.events.set(server.events());
    tracing::info!("servis hazır");
    Ok(Running {
        server,
        _journal_pin: journal_pin,
    })
}

/// SCM'ye bildirilen servise özgü çıkış kodu; tam kod loga yazılır.
pub fn exit_code(e: &AdnsError) -> u32 {
    match e.code() {
        AdnsErrorCode::SvcStartFailed => 1,
        AdnsErrorCode::IpcPipeInUse => 5,
        _ => 900,
    }
}
