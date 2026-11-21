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
mod pipe;
mod service;
mod token;

pub use acl::{
    DirAccess, DirPin, DirProblem, create_protected_dir, evaluate_security, is_reparse_point,
    pin_dir, verify_protected_chain, verify_protected_dir,
};
pub use dirs::{KnownFolder, known_folder, system_dir, windows_dir};
pub use dll::harden_dll_search;
pub use pipe::{
    CallerToken, PipeConn, PipeListener, PipeOwner, Stopper, connect_pipe, current_session_id,
};
pub use service::{
    REQUIRED_PRIVILEGES, RESET_PERIOD_SECS, RESTART_DELAYS_SECS, ServiceConfigReport, ServiceSpec,
    ServiceState, ServiceStatusReporter, create_service, delete_service, dispatch, query_service,
    register_stop_handler, service_config_report, start_service,
};
pub use token::is_elevated;
