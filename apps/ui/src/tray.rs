// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tepsi kipi (`--tray`): simge, flyout ve servis durumu (UI_SPEC §3, §4).
//!
//! Bütün olaylar (kabuk, Slint geri çağrıları, durum iş parçacığı) `Cmd` olarak Slint olay
//! döngüsüne kuyruklanır ve orada sırayla işlenir; böylece bir işleyici çalışırken iç içe
//! çağrılar durumu ödünç almaya çalışmaz.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alterdns_core::{Lang, LeftClick, Paths, UserConfig, load_user};
use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ui::slint::{self, ComponentHandle};
use alterdns_ui::{FlyoutWindow, TrayText, select_language};
use alterdns_winsys::{
    MessageWindow, Rect, ShellEvent, TrayIcon, WindowKind, allow_foreground, arm_harden,
    bring_to_front, cursor_pos, hardened_window, monitor_at, place_flyout, single_instance,
    taskbar_edge, theme_info,
};

use crate::args::PageName;
use crate::common::{
    MSG_QUIT, MSG_SETTINGS_CHANGED, TRAY_CLASS, notification_error, spawn_status, start,
    status_view, theme_colors, ui_error, wait_for_owner,
};
use crate::status::{Notice, UiStatus};

/// Flyout mantıksal genişliği (UI_SPEC §4).
const FLYOUT_WIDTH: i32 = 360;
/// Simgeye tıklama flyout'u odak kaybıyla kapattıysa aynı tıklamanın yeniden açmaması için.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

#[derive(Debug)]
enum Cmd {
    Shell(ShellEvent),
    Status(UiStatus, Option<Notice>),
    CloseFlyout,
    FinishClose,
    OpenMain(Option<PageName>),
    Quit,
}

fn post(cmd: Cmd) {
    if slint::invoke_from_event_loop(move || handle(cmd)).is_err() {
        tracing::debug!("olay döngüsü kapalı; komut düştü");
    }
}

struct Tray {
    // Bırakma sırası: önce simge (NIM_DELETE), sonra ileti penceresi.
    icon: TrayIcon,
    _window: MessageWindow,
    flyout: FlyoutWindow,
    paths: Paths,
    cfg: UserConfig,
    status: UiStatus,
    taskbar_light: bool,
    animations: bool,
    open: bool,
    last_closed: Option<Instant>,
    stop: Arc<AtomicBool>,
}

thread_local! {
    static APP: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

fn handle(cmd: Cmd) {
    if matches!(cmd, Cmd::Quit) {
        // Bırakma, ödünç bitince yapılır: simge kaldırılır, pencereler kapanır.
        let app = APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.take()));
        if let Some(app) = app {
            app.stop.store(true, Ordering::SeqCst);
            drop(app);
        }
        let _ = slint::quit_event_loop();
        return;
    }
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(app) = guard.as_mut() {
                app.handle(cmd);
            }
        }
        // Bir işleyici çalışırken gelen iç içe çağrı: sonra işlenir.
        Err(_) => post(cmd),
    });
}

fn tray_ico(taskbar_light: bool, warning: bool) -> &'static [u8] {
    match (taskbar_light, warning) {
        (true, false) => include_bytes!("../../../assets/brand/generated/tray-light-none.ico"),
        (true, true) => include_bytes!("../../../assets/brand/generated/tray-light-warning.ico"),
        (false, false) => include_bytes!("../../../assets/brand/generated/tray-dark-none.ico"),
        (false, true) => include_bytes!("../../../assets/brand/generated/tray-dark-warning.ico"),
    }
}

impl Tray {
    fn lang(&self) -> Lang {
        self.cfg.language
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Shell(ShellEvent::Activate) => match self.cfg.left_click {
                LeftClick::MainWindow => self.open_main(None),
                LeftClick::Flyout => self.toggle(),
            },
            Cmd::Shell(ShellEvent::ContextMenu | ShellEvent::Keyboard) => self.toggle(),
            Cmd::Shell(ShellEvent::TaskbarRecreated) => self.refresh_icon(),
            Cmd::Shell(ShellEvent::SessionLocked) => self.close_flyout(),
            Cmd::Shell(ShellEvent::SettingChanged) => self.refresh_theme(),
            Cmd::Shell(ShellEvent::Broadcast(MSG_SETTINGS_CHANGED)) => self.reload_settings(),
            Cmd::Shell(ShellEvent::Broadcast(_)) => {}
            Cmd::Status(status, notice) => {
                self.status = status;
                self.apply_status();
                if let Some(notice) = notice {
                    self.notify(notice.code());
                }
            }
            Cmd::CloseFlyout => self.close_flyout(),
            Cmd::FinishClose => {
                if !self.open {
                    let _ = self.flyout.hide();
                }
            }
            Cmd::OpenMain(page) => self.open_main(page),
            Cmd::Quit => {}
        }
    }

    fn tooltip(&self) -> String {
        let text = self.flyout.global::<TrayText>();
        let detail = match &self.status {
            UiStatus::Connecting => text.get_connecting().to_string(),
            UiStatus::Ready { .. } => text.get_ready().to_string(),
            UiStatus::Unreachable(_) => status_view(&self.status, self.lang()).1,
        };
        text.invoke_tooltip(detail.into()).to_string()
    }

    fn refresh_icon(&self) {
        let warning = matches!(self.status, UiStatus::Unreachable(_));
        if let Err(e) = self.icon.set_icon(tray_ico(self.taskbar_light, warning)) {
            tracing::warn!(error = %e, "tepsi simgesi güncellenemedi");
        }
        if let Err(e) = self.icon.set_tooltip(&self.tooltip()) {
            tracing::warn!(error = %e, "tepsi ipucu güncellenemedi");
        }
    }

    fn apply_status(&self) {
        let (kind, title, detail) = status_view(&self.status, self.lang());
        self.flyout.set_status_kind(kind);
        self.flyout.set_status_title(title.into());
        self.flyout.set_status_detail(detail.into());
        self.refresh_icon();
    }

    fn notify(&self, code: AdnsErrorCode) {
        if !self.cfg.notifications {
            return;
        }
        let r = AdnsError::new(code).render(self.lang());
        if let Err(e) = self.icon.balloon(&r.title, &r.message) {
            let err = notification_error(e);
            tracing::warn!(code = err.code().as_str(), error = ?err, "bildirim gösterilemedi");
        }
    }

    fn refresh_theme(&mut self) {
        let theme = theme_info();
        let (dark, accent, on_accent, animations) = theme_colors(&theme);
        self.flyout.set_dark(dark);
        self.flyout.set_accent(accent);
        self.flyout.set_on_accent(on_accent);
        self.flyout.set_animations(animations);
        self.animations = animations;
        self.taskbar_light = theme.taskbar_light;
        self.refresh_icon();
    }

    fn reload_settings(&mut self) {
        match load_user(&self.paths.user_config()) {
            Ok((cfg, _)) => self.cfg = cfg,
            Err(e) => tracing::warn!(code = e.code().as_str(), "ayarlar yeniden okunamadı"),
        }
        if let Err(e) = select_language(self.lang()) {
            tracing::warn!(code = e.code().as_str(), "dil seçilemedi");
        }
        self.apply_status();
    }

    fn toggle(&mut self) {
        if self.open {
            self.close_flyout();
        } else if self.last_closed.is_none_or(|t| t.elapsed() >= REOPEN_GUARD) {
            self.open_flyout();
        }
    }

    fn open_flyout(&mut self) {
        let height = self.flyout.get_content_height().ceil() as i32;
        let anchor = self.icon.anchor().unwrap_or_else(|| {
            let (x, y) = cursor_pos();
            Rect {
                left: x,
                top: y,
                right: x,
                bottom: y,
            }
        });
        let center = (
            anchor.left + (anchor.right - anchor.left) / 2,
            anchor.top + (anchor.bottom - anchor.top) / 2,
        );
        let (work, dpi) = monitor_at(center);
        let p = place_flyout(anchor, taskbar_edge(), work, dpi, FLYOUT_WIDTH, height);
        self.flyout.set_shown(false);
        self.flyout
            .window()
            .set_position(slint::PhysicalPosition::new(p.x, p.y));
        if let Err(e) = arm_harden(WindowKind::Flyout, Box::new(|| post(Cmd::CloseFlyout))) {
            tracing::warn!(error = %e, "flyout sertleştirmesi kurulamadı");
        }
        if let Err(e) = self.flyout.show() {
            tracing::error!(error = %e, "flyout gösterilemedi");
            return;
        }
        self.open = true;
        if let Some(hwnd) = hardened_window() {
            bring_to_front(hwnd);
        }
        self.flyout.set_shown(true);
    }

    fn close_flyout(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.last_closed = Some(Instant::now());
        self.flyout.set_shown(false);
        let delay = if self.animations {
            Duration::from_millis(100)
        } else {
            Duration::ZERO
        };
        slint::Timer::single_shot(delay, || post(Cmd::FinishClose));
    }

    fn open_main(&mut self, page: Option<PageName>) {
        self.close_flyout();
        let result = std::env::current_exe().and_then(|exe| {
            let mut cmd = std::process::Command::new(exe);
            if let Some(page) = page {
                cmd.args(["--page", page.as_str()]);
            }
            // Ön plana geçme izni yalnızca başlatılan pencere işlemine verilir.
            cmd.spawn().map(|child| allow_foreground(child.id()))
        });
        if let Err(e) = result {
            tracing::error!(error = %e, "ana pencere başlatılamadı");
        }
    }
}

/// Tepsi kipini çalıştırır. Aynı oturumda tepsi zaten çalışıyorsa ya da oturum açılışında
/// kullanıcı başlatmayı kapattıysa hemen döner.
pub fn run(logon: bool) -> Result<(), AdnsError> {
    let Some(_lock) = single_instance(TRAY_CLASS).map_err(ui_error)? else {
        // Tepsi zaten çalışıyor; kilit tutulduğu hâlde tepsi penceresi yoksa sessizce çıkılmaz.
        return wait_for_owner(TRAY_CLASS);
    };
    let (paths, cfg) = start("tray")?;
    if crate::args::exit_at_logon(&crate::args::Mode::Tray { logon }, &cfg) {
        tracing::info!("oturum açılışında başlatma kapalı; çıkılıyor");
        return Ok(());
    }
    let flyout = FlyoutWindow::new().map_err(ui_error)?;
    select_language(cfg.language)?;
    let theme = theme_info();
    let (dark, accent, on_accent, animations) = theme_colors(&theme);
    flyout.set_dark(dark);
    flyout.set_accent(accent);
    flyout.set_on_accent(on_accent);
    flyout.set_animations(animations);
    flyout.on_open_main(|| post(Cmd::OpenMain(None)));
    flyout.on_open_settings(|| post(Cmd::OpenMain(Some(PageName::Settings))));
    flyout.on_quit(|| post(Cmd::Quit));
    flyout.on_close_requested(|| post(Cmd::CloseFlyout));

    let window = MessageWindow::create(
        TRAY_CLASS,
        Box::new(|e| match e {
            // Kaldırma ve güncelleme öncesi kapanma isteği.
            ShellEvent::Broadcast(MSG_QUIT) => post(Cmd::Quit),
            e => post(Cmd::Shell(e)),
        }),
    )
    .map_err(ui_error)?;
    let tooltip = flyout
        .global::<TrayText>()
        .invoke_tooltip(flyout.global::<TrayText>().get_connecting());
    let icon =
        TrayIcon::add(&window, tray_ico(theme.taskbar_light, false), &tooltip).map_err(ui_error)?;
    if !icon.is_shown() {
        tracing::warn!("görev çubuğu yok; simge Explorer başlayınca eklenecek");
    }
    let stop = Arc::new(AtomicBool::new(false));
    APP.with(|a| {
        *a.borrow_mut() = Some(Tray {
            icon,
            _window: window,
            flyout,
            paths,
            cfg,
            status: UiStatus::Connecting,
            taskbar_light: theme.taskbar_light,
            animations,
            open: false,
            last_closed: None,
            stop: stop.clone(),
        });
    });
    spawn_status(move |s, n| post(Cmd::Status(s, n)), stop.clone())?;
    let result = slint::run_event_loop_until_quit().map_err(ui_error);
    stop.store(true, Ordering::SeqCst);
    APP.with(|a| drop(a.borrow_mut().take()));
    tracing::info!("tepsi kapandı");
    result
}
