// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Geçerli işlemin belirteci.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, PSID, TOKEN_ELEVATION, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
    TokenElevation,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PWSTR;

/// Belirteç tutamacı; düşünce kapatılır.
pub(crate) struct Token(pub(crate) HANDLE);

/// Geçerli işlemin belirteci (`TOKEN_QUERY`).
pub(crate) fn process_token() -> std::io::Result<Token> {
    let mut handle = HANDLE::default();
    // SAFETY: Sözde işlem tutamacı kapatılmaz; çıktı geçerli bir yerel değişkene yazılır.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) }
        .map_err(std::io::Error::other)?;
    Ok(Token(handle))
}

/// Belirteç bilgisi. Arabellek `usize` dizisidir: içindeki yapılar işaretçi hizası ister.
pub(crate) fn token_info(
    token: HANDLE,
    class: TOKEN_INFORMATION_CLASS,
) -> std::io::Result<Vec<usize>> {
    let mut len = 0u32;
    // SAFETY: Yalnızca boyut sorulur; arabellek verilmez, çıktı yerel değişkendir.
    let _ = unsafe { GetTokenInformation(token, class, None, 0, &mut len) };
    if len == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut buf = vec![0usize; (len as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: Arabellek en az `len` bayttır, hizalıdır ve yazılabilir.
    unsafe { GetTokenInformation(token, class, Some(buf.as_mut_ptr().cast()), len, &mut len) }
        .map_err(std::io::Error::other)?;
    Ok(buf)
}

/// SID'in metin biçimi (`S-1-5-…`).
pub(crate) fn sid_string(sid: PSID) -> std::io::Result<String> {
    let mut text = PWSTR::null();
    // SAFETY: `sid` geçerli bir SID'e işaret eder; çıktı `LocalFree` ile serbest bırakılır.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(std::io::Error::other)?;
    // SAFETY: Başarılı çağrı NUL ile biten bir UTF-16 dizesi döndürür; kopyalanır, sonra bir kez
    // serbest bırakılır.
    let s = unsafe { text.to_string() };
    // SAFETY: Bellek API'nin `LocalAlloc` ile ayırdığı dizedir.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    s.map_err(std::io::Error::other)
}

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: Tutamaç `OpenProcessToken`'dan geldi ve yalnızca burada bir kez kapatılır.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Geçerli işlem yükseltilmiş (yönetici belirteciyle) mi çalışıyor? Servis (SYSTEM) için de
/// `true` döner.
pub fn is_elevated() -> std::io::Result<bool> {
    let token = process_token()?;
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
