// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sistem dizinleri ve bilinen klasörler. Ortam değişkenlerine (`SystemRoot`, `ProgramData`…)
//! güvenilmez; yollar Win32 API'sinden alınır (Faz 0 S-2).

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetSystemWindowsDirectoryW};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramData, FOLDERID_ProgramFiles, KF_FLAG_DEFAULT,
    SHGetKnownFolderPath,
};

/// Windows yol uzunluğunun üst sınırı (UTF-16 birimi); bunu aşan değer hata sayılır.
const MAX_PATH_UNITS: usize = 32_768;

/// Uzunluk döndüren Win32 yol işlevleri için: arabellek yetmezse büyütüp yeniden çağırır.
/// Sınır, beklenmedik bir API davranışında sonsuz büyümeyi önler.
fn read_path(call: impl Fn(&mut [u16]) -> u32) -> std::io::Result<PathBuf> {
    let mut buf = vec![0u16; 260];
    loop {
        let n = call(&mut buf) as usize;
        if n == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n < buf.len() {
            buf.truncate(n);
            return Ok(PathBuf::from(OsString::from_wide(&buf)));
        }
        // Dönen değer gereken boyuttur (sondaki NUL dahil).
        if n > MAX_PATH_UNITS {
            return Err(std::io::Error::other(format!(
                "yol çok uzun: {n} birim (en çok {MAX_PATH_UNITS})"
            )));
        }
        buf.resize(n + 1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_path_grows_buffer_then_stops_at_limit() {
        // Önce 300 birim ister, sonra sığar.
        let p = read_path(|buf| {
            if buf.len() < 300 {
                300
            } else {
                buf[..3].copy_from_slice(&[u16::from(b'C'), u16::from(b':'), u16::from(b'\\')]);
                3
            }
        })
        .unwrap();
        assert_eq!(p, PathBuf::from("C:\\"));
        // Hep daha büyüğünü isteyen API sınırda durur.
        let err = read_path(|buf| u32::try_from(buf.len() + 10).unwrap()).unwrap_err();
        assert!(err.to_string().contains("çok uzun"), "{err}");
    }
}

/// Paylaşılan Windows dizini (`GetSystemWindowsDirectoryW`); `SystemRoot` ortam değişkeninden
/// bağımsızdır.
pub fn windows_dir() -> std::io::Result<PathBuf> {
    // SAFETY: Arabellek geçerli ve yazılabilir; API en çok `buf.len()` öğe yazar.
    read_path(|buf| unsafe { GetSystemWindowsDirectoryW(Some(buf)) })
}

/// System32 dizini (`GetSystemDirectoryW`).
pub fn system_dir() -> std::io::Result<PathBuf> {
    // SAFETY: Arabellek geçerli ve yazılabilir; API en çok `buf.len()` öğe yazar.
    read_path(|buf| unsafe { GetSystemDirectoryW(Some(buf)) })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownFolder {
    ProgramData,
    LocalAppData,
    ProgramFiles,
}

/// Bilinen klasörün yolu (`SHGetKnownFolderPath`); geçerli kullanıcının belirteciyle.
pub fn known_folder(folder: KnownFolder) -> std::io::Result<PathBuf> {
    let id = match folder {
        KnownFolder::ProgramData => &FOLDERID_ProgramData,
        KnownFolder::LocalAppData => &FOLDERID_LocalAppData,
        KnownFolder::ProgramFiles => &FOLDERID_ProgramFiles,
    };
    // SAFETY: `id` statik bir GUID'e işaret eder; belirteç verilmez (geçerli kullanıcı).
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
        .map_err(std::io::Error::other)?;
    // SAFETY: Başarılı çağrı NUL ile biten geçerli bir UTF-16 dizesi döndürür; kopyalandıktan
    // sonra bellek bir kez `CoTaskMemFree` ile serbest bırakılır.
    let path = unsafe {
        let wide = raw.as_wide().to_vec();
        CoTaskMemFree(Some(raw.0 as *const core::ffi::c_void));
        wide
    };
    Ok(PathBuf::from(OsString::from_wide(&path)))
}
