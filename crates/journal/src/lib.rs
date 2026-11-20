// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS journal'ı: her sistem değişikliği önce diske yazılır, sonra uygulanır, gerekirse
//! geri alınır (`docs/ARCHITECTURE.md` §6).

#![forbid(unsafe_code)]

pub mod file;
pub mod model;

pub use file::{DirLock, JournalFile, list, lock_dir, lock_dir_with};
pub use model::{
    ChangeRecord, Kind, Lifetime, Line, SCHEMA, Snapshot, State, new_txn_id, valid_txn_id,
};
