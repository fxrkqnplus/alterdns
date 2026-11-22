// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Oturum başına tek örnek kilidi (`Local\` adlı mutex).

use std::io;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, SetLastError, WIN32_ERROR,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use crate::acl::wide;
use crate::pipe::os_error;

/// Tutulduğu sürece kilit bu işlemdedir; düşünce tutamaç kapanır.
pub struct SingleInstance {
    handle: HANDLE,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: Tutamaç bu nesneye ait ve bir kez kapatılır.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

/// `Local\<name>` kilidini alır; aynı oturumda başka bir tutamaç tutuyorsa `None`.
/// `name` boş olamaz ve ters eğik çizgi içeremez (ad alanı her zaman `Local\`).
pub fn single_instance(name: &str) -> io::Result<Option<SingleInstance>> {
    if name.is_empty() || name.len() > 200 || name.contains(['\\', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "geçersiz kilit adı",
        ));
    }
    let full = wide(format!(r"Local\{name}").as_ref());
    // SAFETY: Ad arabelleği çağrı boyunca canlı. Son hata önce sıfırlanır: var olan nesnede
    // CreateMutexW başarılı döner ve ERROR_ALREADY_EXISTS bırakır.
    let (handle, existed) = unsafe {
        SetLastError(WIN32_ERROR(0));
        let handle = CreateMutexW(None, false, PCWSTR(full.as_ptr())).map_err(os_error)?;
        (handle, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    let lock = SingleInstance { handle };
    Ok((!existed).then_some(lock))
}
