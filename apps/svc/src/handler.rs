// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Servisin IPC yöntemleri. Servis kullanıcıya metin üretmez; yalnızca değer ve hata kodu
//! döner (K22).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use alterdns_core::{LoadOutcome, load_machine};
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{Caller, EventBus, Handler, Method};
use serde_json::{Value, json};

pub(crate) struct SvcHandler {
    pub(crate) started: Instant,
    pub(crate) machine_config: PathBuf,
    pub(crate) events: OnceLock<EventBus>,
}

pub(crate) fn outcome_name(o: &LoadOutcome) -> &'static str {
    match o {
        LoadOutcome::Loaded => "loaded",
        LoadOutcome::Defaulted { .. } => "defaulted",
        LoadOutcome::NewerSchema => "newer_schema",
    }
}

impl Handler for SvcHandler {
    fn call(&self, _caller: &Caller, method: Method, _params: &Value) -> Result<Value, AdnsError> {
        match method {
            Method::Status => Ok(json!({
                "version": alterdns_core::version::VERSION,
                "uptime_s": self.started.elapsed().as_secs(),
                "modules": {
                    "connection": "not_installed",
                    "file": "not_installed",
                    "browser": "not_installed",
                },
            })),
            Method::ReloadConfig => {
                let (_, outcome) = load_machine(&self.machine_config)?;
                let name = outcome_name(&outcome);
                tracing::info!(outcome = name, "makine yapılandırması yeniden okundu");
                if let Some(bus) = self.events.get() {
                    bus.publish("config.reloaded", json!({ "outcome": name }), None);
                }
                Ok(json!({ "outcome": name }))
            }
            other => Err(AdnsError::new(AdnsErrorCode::IpcBadRequest).with_source(
                std::io::Error::other(format!("işlenmeyen yöntem: {other:?}")),
            )),
        }
    }
}
