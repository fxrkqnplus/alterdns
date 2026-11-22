// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tepsi ve pencere kiplerinin ortak parçaları: log, yapılandırma, tema, durum metinleri ve
//! durum iş parçacığı.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use alterdns_core::{Lang, Paths, UserConfig, init_log, load_user};
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ui::slint::Color;
use alterdns_ui::{StatusKind, on_accent, rgb};
use alterdns_winsys::ThemeInfo;

use crate::status::{IpcSource, Notice, UiStatus, run};

pub use alterdns_core::ui::{
    MSG_ACTIVATE, MSG_PAGE, MSG_QUIT, MSG_SETTINGS_CHANGED, TRAY_CLASS, WINDOW_CLASS,
};

/// Kaynak kodu ve belgeler; arayüz yalnızca bu önekle başlayan adresleri açar.
pub const REPO_URL: &str = "https://github.com/fxrkqnplus/alterdns";

/// Arayüz başlatılamadı (ADNS-UI-001): ileti kutusuyla gösterilir, çıkış 1.
pub fn ui_error(e: impl std::error::Error + Send + Sync + 'static) -> AdnsError {
    AdnsError::new(AdnsErrorCode::UiStartFailed).with_source(e)
}

/// Tek örnek kilidi başka bir işlemdeyken, o örneğin gizli penceresini en çok 3 sn bekler (ilk
/// örnek kilidi alıp penceresini henüz oluşturmamış olabilir). Pencere hiç yoksa kilit başka
/// bir işlemce tutuluyordur: sessizce çıkılmaz, ADNS-UI-001 döner.
pub fn wait_for_owner(class: &str) -> Result<(), AdnsError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if alterdns_winsys::window_owner(class).is_some() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(ui_error(std::io::Error::other(format!(
                "{class} kilidi tutuluyor ama penceresi yok"
            ))));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Bildirim gösterilemedi (ADNS-UI-002): yalnızca loglanır, akış sürer.
pub fn notification_error(e: std::io::Error) -> AdnsError {
    AdnsError::new(AdnsErrorCode::UiNotificationsUnavailable).with_source(e)
}

/// Yollar, log (`%LOCALAPPDATA%\AlterDNS\logs\<name>.log`) ve kullanıcı yapılandırması.
/// Yapılandırma okunamazsa varsayılanlarla sürülür ve loglanır.
pub fn start(name: &str) -> Result<(Paths, UserConfig), AdnsError> {
    let paths = Paths::system()?;
    let logs = paths.user_logs();
    std::fs::create_dir_all(&logs).map_err(ui_error)?;
    init_log(&logs, name)?;
    let cfg = match load_user(&paths.user_config()) {
        Ok((cfg, _)) => cfg,
        Err(e) => {
            tracing::warn!(
                code = e.code().as_str(),
                "kullanıcı yapılandırması okunamadı"
            );
            UserConfig::default()
        }
    };
    tracing::info!(kip = name, "arayüz başladı");
    Ok((paths, cfg))
}

/// Slint'e verilecek tema değerleri: (koyu, vurgu, vurgu üzerindeki metin, animasyon).
pub fn theme_colors(t: &ThemeInfo) -> (bool, Color, Color, bool) {
    (
        !t.apps_light,
        rgb(t.accent),
        rgb(on_accent(t.accent)),
        t.animations,
    )
}

/// Durumun arayüz karşılığı: tür, katalog başlığı ve ileti (yalnızca hata için dolu).
pub fn status_view(status: &UiStatus, lang: Lang) -> (StatusKind, String, String) {
    match status {
        UiStatus::Connecting => (StatusKind::Connecting, String::new(), String::new()),
        UiStatus::Ready { .. } => (StatusKind::Ready, String::new(), String::new()),
        UiStatus::Unreachable(code) => {
            let r = AdnsError::new(*code).render(lang);
            (
                StatusKind::Unreachable,
                r.title,
                format!("{} ({})", r.message, code.as_str()),
            )
        }
    }
}

/// Durum döngüsünü ayrı iş parçacığında başlatır; `sink` o iş parçacığından çağrılır.
pub fn spawn_status(
    sink: impl FnMut(UiStatus, Option<Notice>) + Send + 'static,
    stop: Arc<AtomicBool>,
) -> Result<(), AdnsError> {
    std::thread::Builder::new()
        .name("status".into())
        .spawn(move || run(IpcSource::default(), sink, stop))
        .map(|_| ())
        .map_err(ui_error)
}
