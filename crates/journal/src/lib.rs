// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS journal'ı: her sistem değişikliği önce diske yazılır, sonra uygulanır, gerekirse
//! geri alınır (`docs/ARCHITECTURE.md` §6).

#![forbid(unsafe_code)]

pub mod file;
pub mod install;
pub mod model;
pub mod ops;
pub mod real;
pub mod recover;
pub mod txn;
pub mod watchdog;

pub use file::{DirLock, JournalFile, list, lock_dir, lock_dir_with};
pub use install::InstallOps;
pub use model::{
    ChangeRecord, Kind, Lifetime, Line, SCHEMA, Snapshot, State, new_txn_id, valid_txn_id,
};
pub use ops::{
    REGISTRY_ROOT, RUN_VALUE, SERVICE_NAME, SystemOps, TEST_REGISTRY_ROOT, Targets, UNINSTALL_KEY,
    is_own_object, target_allowed,
};
pub use real::{
    RealOps, product_install_root, sha256_file, validate_service_binary, validate_startup_command,
};
pub use recover::{RecoveryReport, recover_at_startup};
pub use txn::{RevertReport, Txn, revert_file};
pub use watchdog::{
    LeaseMsg, MAINT_EXE, MAX_LEASE_SECS, WatchdogHandle, WatchdogOutcome, WatchdogReport,
    WatchdogStatus, parse_line, report_path, run_watchdog,
};
