// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS çekirdeği: yollar, yapılandırma (şema v1), sürüm ve log maskeleme.

#![forbid(unsafe_code)]

pub mod config;
pub mod log;
pub mod paths;
pub mod redact;

pub use alterdns_errors::Lang;
pub use config::{
    LeftClick, LoadOutcome, MachineConfig, UserConfig, load_machine, load_user, save_user,
};
pub use log::{CappedWriter, LOG_FILE_MAX, init_log, log_subscriber};
pub use paths::Paths;
pub use redact::redact_user_paths;

/// Arayüz işlemleri arasındaki pencere iletileri (`alterdns-ui.exe` ve geliştirme araçları).
pub mod ui {
    /// Tepsi kipinin gizli penceresinin sınıfı.
    pub const TRAY_CLASS: &str = "AlterDNS.Tray";
    /// Ana pencere kipinin gizli penceresinin sınıfı.
    pub const WINDOW_CLASS: &str = "AlterDNS.Window";
    /// Tepsiye: kullanıcı ayarları değişti (yeniden oku).
    pub const MSG_SETTINGS_CHANGED: u32 = 1;
    /// Ana pencereye: öne gel.
    pub const MSG_ACTIVATE: u32 = 2;
    /// Tepsiye ve ana pencereye: kapan (kaldırma ve güncelleme öncesi).
    pub const MSG_QUIT: u32 = 3;
    /// Ana pencereye: öne gel ve sayfayı aç (`MSG_PAGE | sayfa kodu`).
    pub const MSG_PAGE: u32 = 0x100;
}

pub mod version {
    /// Bütün ikililerde aynı olan ürün sürümü (workspace sürümü).
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
}
