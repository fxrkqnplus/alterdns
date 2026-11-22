// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kabuk yardımcıları: varsayılan tarayıcıda adres açma ve başlatma hatası iletisi.

use std::io;

use std::os::windows::ffi::OsStringExt;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW, SW_SHOWNORMAL,
};

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::acl::wide;
use crate::pipe::os_error;

/// `https://` adresini varsayılan tarayıcıda açar. Başka şema reddedilir (`InvalidInput`);
/// hangi adreslerin açılabileceğine çağıran karar verir.
pub fn open_url(url: &str) -> io::Result<()> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_control() || c == '"') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "geçersiz adres",
        ));
    }
    let target = wide(url.as_ref());
    // SAFETY: Dizeler çağrı boyunca canlı; "open" fiili yalnızca https adresini kabuğa verir.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 32'den büyük bir değer döndürürse başarılıdır.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(io::Error::other("ShellExecuteW başarısız"))
    }
}

/// İşlemin ikili yolu (`QueryFullProcessImageNameW`, Win32 biçimi).
pub fn process_image(pid: u32) -> io::Result<std::path::PathBuf> {
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: Tutamaç yalnızca sorgu hakkıyla açılır ve kapatılır; arabellek `len` öğe tutar ve
    // API en çok o kadar yazar.
    unsafe {
        let handle =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(os_error)?;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.map_err(os_error)?;
    }
    buf.truncate(len as usize);
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(
        &buf,
    )))
}

/// Hata simgeli, tek düğmeli ileti kutusu (arayüz başlatılamadığında).
pub fn error_box(title: &str, text: &str) {
    let (title, text) = (wide(title.as_ref()), wide(text.as_ref()));
    // SAFETY: Dizeler çağrı boyunca canlı; sahip pencere yok.
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}
