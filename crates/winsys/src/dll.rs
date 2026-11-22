// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! DLL arama yolu (K18). Yalnızca bu işlemin kendi arama yolunu daraltır; sistem ayarı değildir.

use windows::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};

/// Sonraki bütün DLL yüklemelerini System32 ile (ve istenirse uygulama dizini ile) sınırlar.
/// Her exe `main`'in ilk satırında çağırır; servis `include_app_dir = true` verir.
pub fn harden_dll_search(include_app_dir: bool) -> std::io::Result<()> {
    let mut flags = LOAD_LIBRARY_SEARCH_SYSTEM32;
    if include_app_dir {
        flags |= LOAD_LIBRARY_SEARCH_APPLICATION_DIR;
    }
    // SAFETY: Bayraklar belgelenmiş sabitlerdir; çağrı yalnızca bu işlemin arama yolunu değiştirir.
    unsafe { SetDefaultDllDirectories(flags) }.map_err(std::io::Error::other)
}
