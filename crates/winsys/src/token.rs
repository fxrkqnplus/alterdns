// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geçerli işlemin belirteci.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Belirteç tutamacı; düşünce kapatılır.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: Tutamaç `OpenProcessToken`'dan geldi ve yalnızca burada bir kez kapatılır.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Geçerli işlem yükseltilmiş (yönetici belirteciyle) mi çalışıyor? Servis (SYSTEM) için de
/// `true` döner.
pub fn is_elevated() -> std::io::Result<bool> {
    let mut handle = HANDLE::default();
    // SAFETY: `GetCurrentProcess` kapatılması gerekmeyen sözde bir tutamaç döndürür; çıktı
    // tutamacı geçerli bir yerel değişkene yazılır.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) }
        .map_err(std::io::Error::other)?;
    let token = Token(handle);
    let mut info = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    let size =
        u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>()).map_err(std::io::Error::other)?;
    // SAFETY: Arabellek `TOKEN_ELEVATION` boyutunda ve yazılabilir; boyut doğru verilir.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(std::ptr::from_mut(&mut info).cast()),
            size,
            &mut len,
        )
    }
    .map_err(std::io::Error::other)?;
    Ok(info.TokenIsElevated != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_elevated_answers() {
        // Testler yönetici olmadan da çalışır; burada yalnızca çağrının başarılı olduğu sınanır.
        assert!(is_elevated().is_ok());
    }
}
