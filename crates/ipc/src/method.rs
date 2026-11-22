// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Faz 1 yöntemleri ve yetki sınıfları (K10).

use alterdns_errors::{AdnsError, AdnsErrorCode};

use crate::policy::CallerPolicy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Hello,
    Ping,
    Status,
    Subscribe,
    ReloadConfig,
}

const NAMES: [(Method, &str); 5] = [
    (Method::Hello, "svc.hello"),
    (Method::Ping, "svc.ping"),
    (Method::Status, "svc.status"),
    (Method::Subscribe, "events.subscribe"),
    (Method::ReloadConfig, "svc.reload_config"),
];

impl Method {
    /// Bilinmeyen yöntem `IpcBadRequest` (`ADNS-IPC-002`).
    pub fn parse(name: &str) -> Result<Method, AdnsError> {
        NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(m, _)| *m)
            .ok_or_else(|| {
                AdnsError::new(AdnsErrorCode::IpcBadRequest).with_source(std::io::Error::other(
                    format!("bilinmeyen yöntem: {name:?}"),
                ))
            })
    }

    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(m, _)| *m == self)
            .map_or("", |(_, n)| n)
    }

    pub fn policy(self) -> CallerPolicy {
        match self {
            Method::Hello | Method::Ping => CallerPolicy::Open,
            Method::Status | Method::Subscribe => CallerPolicy::InteractiveUser,
            Method::ReloadConfig => CallerPolicy::Elevated,
        }
    }
}
