// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS Win32 sarmalayıcıları; projedeki `unsafe` kodun tek yeri.
//!
//! Her `unsafe` blok `// SAFETY:` açıklamasıyla gelir. Dışarıya yalnızca güvenli API açılır.

#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod dirs;
mod dll;

pub use dirs::{KnownFolder, known_folder, system_dir, windows_dir};
pub use dll::harden_dll_search;
