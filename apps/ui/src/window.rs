// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ana pencere kipi (UI_SPEC §5): ayrı işlem, kapanınca biter. Oturumda zaten açıksa var olan
//! pencere öne getirilir (istenen sayfayla) ve bu işlem çıkar.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alterdns_core::{Lang, LeftClick, UserConfig, save_user, version::VERSION};
use alterdns_errors::AdnsError;
use alterdns_ui::slint::{self, ComponentHandle};
use alterdns_ui::{MainWindow, Page, select_language};
use alterdns_winsys::{
    MessageWindow, ShellEvent, WindowKind, allow_foreground_any, arm_harden, bring_to_front,
    centered_on_cursor_monitor, hardened_window, open_url, post_broadcast, single_instance,
    theme_info,
};

use crate::args::PageName;
use crate::common::{
    MSG_ACTIVATE, MSG_PAGE, MSG_SETTINGS_CHANGED, REPO_URL, spawn_status, start, status_view,
    theme_colors, ui_error,
};

const WIDTH: i32 = 960;
const HEIGHT: i32 = 640;

fn slint_page(page: PageName) -> Page {
    match page {
        PageName::Overview => Page::Overview,
        PageName::File => Page::File,
        PageName::Browser => Page::Browser,
        PageName::Connection => Page::Connection,
        PageName::Components => Page::Components,
        PageName::Updates => Page::Updates,
        PageName::Settings => Page::Settings,
        PageName::About => Page::About,
    }
}

fn load_settings(main: &MainWindow, cfg: &UserConfig) {
    main.set_language(cfg.language.code().into());
    main.set_run_at_logon(cfg.run_at_logon);
    main.set_notifications(cfg.notifications);
    main.set_left_click_opens_main(cfg.left_click == LeftClick::MainWindow);
    main.set_online_lookups(cfg.online_lookups);
}

fn read_settings(main: &MainWindow, base: &UserConfig) -> UserConfig {
    UserConfig {
        language: Lang::from_code(&main.get_language()).unwrap_or(base.language),
        run_at_logon: main.get_run_at_logon(),
        notifications: main.get_notifications(),
        left_click: if main.get_left_click_opens_main() {
            LeftClick::MainWindow
        } else {
            LeftClick::Flyout
        },
        online_lookups: main.get_online_lookups(),
    }
}

/// Yalnızca bu deponun adresleri açılır (Hakkında sayfası bağlantıları).
fn allowed_link(url: &str) -> bool {
    url == REPO_URL || url.starts_with(&format!("{REPO_URL}/"))
}

pub fn run(page: Option<PageName>) -> Result<(), AdnsError> {
    let Some(_lock) = single_instance("AlterDNS.Window").map_err(ui_error)? else {
        // Var olan pencere öne gelir; ön plana geçebilmesi için izin verilir.
        allow_foreground_any();
        let code = page.map_or(MSG_ACTIVATE, |p| MSG_PAGE | p.code());
        post_broadcast("AlterDNS.Window", code).map_err(ui_error)?;
        return Ok(());
    };
    let (paths, cfg) = start("window")?;
    let main = MainWindow::new().map_err(ui_error)?;
    select_language(cfg.language)?;
    let theme = theme_info();
    let (dark, accent, on_accent, animations) = theme_colors(&theme);
    main.set_dark(dark);
    main.set_accent(accent);
    main.set_on_accent(on_accent);
    main.set_animations(animations);
    main.set_version(VERSION.into());
    load_settings(&main, &cfg);
    if let Some(page) = page {
        main.set_page(slint_page(page));
    }

    let config_path = paths.user_config();
    let weak = main.as_weak();
    let mut current = cfg.clone();
    main.on_settings_changed(move || {
        let Some(main) = weak.upgrade() else {
            return;
        };
        let next = read_settings(&main, &current);
        if let Err(e) = save_user(&config_path, &next) {
            tracing::error!(code = e.code().as_str(), "ayarlar kaydedilemedi");
            load_settings(&main, &current);
            return;
        }
        if next.language != current.language
            && let Err(e) = select_language(next.language)
        {
            tracing::warn!(code = e.code().as_str(), "dil seçilemedi");
        }
        current = next;
        // Tepsi açıksa ayarları yeniden okur (dil, sol tık, bildirimler).
        if let Err(e) = post_broadcast("AlterDNS.Tray", MSG_SETTINGS_CHANGED) {
            tracing::warn!(error = %e, "tepsiye ayar iletisi gönderilemedi");
        }
    });
    main.on_open_link(|url| {
        if !allowed_link(&url) {
            tracing::warn!("izin verilmeyen bağlantı açılmadı");
            return;
        }
        if let Err(e) = open_url(&url) {
            tracing::warn!(error = %e, "bağlantı açılamadı");
        }
    });

    // İkinci örnekten gelen ileti: öne gel, istenirse sayfayı aç.
    let weak = main.as_weak();
    let _window = MessageWindow::create(
        "AlterDNS.Window",
        Box::new(move |event| {
            let ShellEvent::Broadcast(code) = event else {
                return;
            };
            if let Some(main) = weak.upgrade()
                && let Some(page) = (code & MSG_PAGE != 0)
                    .then(|| PageName::from_code(code & !MSG_PAGE))
                    .flatten()
            {
                main.set_page(slint_page(page));
            }
            if let Some(hwnd) = hardened_window() {
                bring_to_front(hwnd);
            }
        }),
    )
    .map_err(ui_error)?;

    let lang = cfg.language;
    let weak = main.as_weak();
    let stop = Arc::new(AtomicBool::new(false));
    spawn_status(
        move |status, _notice| {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(main) = weak.upgrade() {
                    let (kind, title, detail) = status_view(&status, lang);
                    main.set_status_kind(kind);
                    main.set_service_title(title.into());
                    main.set_service_detail(detail.into());
                }
            });
        },
        stop.clone(),
    )?;

    let p = centered_on_cursor_monitor(WIDTH, HEIGHT);
    main.window()
        .set_position(slint::PhysicalPosition::new(p.x, p.y));
    main.window().set_size(slint::PhysicalSize::new(
        p.width.max(1) as u32,
        p.height.max(1) as u32,
    ));
    arm_harden(WindowKind::Main, Box::new(|| {})).map_err(ui_error)?;
    main.show().map_err(ui_error)?;
    if let Some(hwnd) = hardened_window() {
        bring_to_front(hwnd);
    }
    let result = slint::run_event_loop().map_err(ui_error);
    stop.store(true, Ordering::SeqCst);
    tracing::info!("ana pencere kapandı");
    result
}
