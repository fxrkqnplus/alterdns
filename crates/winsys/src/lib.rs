// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS Win32 sarmalayıcıları; projedeki `unsafe` kodun tek yeri.
//!
//! Her `unsafe` blok `// SAFETY:` açıklamasıyla gelir. Dışarıya yalnızca güvenli API açılır.

#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod acl;
mod dirs;
mod dll;
mod instance;
mod pipe;
mod screen;
mod service;
mod theme;
mod token;
mod tray;
mod window;

pub use acl::{
    DirAccess, DirPin, DirProblem, create_protected_dir, evaluate_security, is_reparse_point,
    pin_dir, verify_protected_chain, verify_protected_dir,
};
pub use dirs::{KnownFolder, known_folder, system_dir, windows_dir};
pub use dll::harden_dll_search;
pub use instance::{SingleInstance, single_instance};
pub use pipe::{
    CallerToken, PipeConn, PipeListener, PipeOwner, Stopper, connect_pipe, current_session_id,
};
pub use screen::{
    Edge, Placement, Rect, center_in, centered_on_cursor_monitor, cursor_pos, monitor_at,
    place_flyout, taskbar_edge,
};
pub use service::{
    REQUIRED_PRIVILEGES, RESET_PERIOD_SECS, RESTART_DELAYS_SECS, ServiceConfigReport, ServiceSpec,
    ServiceState, ServiceStatusReporter, create_service, delete_service, dispatch, query_service,
    register_stop_handler, service_config_report, service_create_access, start_service,
};
pub use theme::{DEFAULT_ACCENT, ThemeInfo, accent_from_abgr, theme_info};
pub use token::is_elevated;
pub use tray::{MessageWindow, ShellEvent, TrayIcon, icon_count, post_broadcast, taskbar_present};
pub use window::{
    WindowKind, allow_foreground_any, arm_harden, bring_to_front, ex_style, harden,
    interactive_desktop,
};
