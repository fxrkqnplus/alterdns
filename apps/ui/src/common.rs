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

/// Tepsiye: kullanıcı ayarları değişti (yeniden oku).
pub const MSG_SETTINGS_CHANGED: u32 = 1;
/// Ana pencereye: öne gel.
pub const MSG_ACTIVATE: u32 = 2;
/// Ana pencereye: öne gel ve sayfayı aç (`MSG_PAGE | PageName::code`).
pub const MSG_PAGE: u32 = 0x100;
/// Kaynak kodu ve belgeler; arayüz yalnızca bu önekle başlayan adresleri açar.
pub const REPO_URL: &str = "https://github.com/fxrkqnplus/alterdns";

pub fn ui_error(e: impl std::error::Error + Send + Sync + 'static) -> AdnsError {
    AdnsError::new(AdnsErrorCode::UiStartFailed).with_source(e)
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
