// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS çekirdeği: yollar, yapılandırma (şema v1), sürüm ve log maskeleme.

#![forbid(unsafe_code)]

pub mod config;
pub mod paths;
pub mod redact;

pub use alterdns_errors::Lang;
pub use config::{LeftClick, LoadOutcome, MachineConfig, UserConfig, load_user, save_user};
pub use paths::Paths;
pub use redact::redact_user_paths;

pub mod version {
    /// Bütün ikililerde aynı olan ürün sürümü (workspace sürümü).
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
}
