// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows teması, vurgu rengi ve animasyon tercihi. Yalnızca okunur (kullanıcı ayarıdır).

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::{BOOL, PCWSTR, w};

/// Okunamayan vurgu yerine kullanılan renk (UI_SPEC).
pub const DEFAULT_ACCENT: u32 = 0x25_63_EB;

/// Tema bilgisi. Okunamayanlar: açık tema, `DEFAULT_ACCENT`, animasyon açık.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeInfo {
    /// Uygulama pencereleri açık temada (`AppsUseLightTheme`).
    pub apps_light: bool,
    /// Görev çubuğu açık temada (`SystemUsesLightTheme`); tepsi glifinin rengini belirler.
    pub taskbar_light: bool,
    /// Vurgu rengi, 0xRRGGBB.
    pub accent: u32,
    /// İstemci alanı animasyonları açık (azaltılmış hareket kapalı).
    pub animations: bool,
}

/// DWM'nin ABGR DWORD'ünü 0xRRGGBB'ye çevirir.
pub fn accent_from_abgr(dword: u32) -> u32 {
    let r = dword & 0xFF;
    let g = (dword >> 8) & 0xFF;
    let b = (dword >> 16) & 0xFF;
    (r << 16) | (g << 8) | b
}

fn hkcu_dword(subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: Alt anahtar ve değer adları sabit; çıktı yerel değişkene en çok `size` bayt yazılır.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

pub fn theme_info() -> ThemeInfo {
    let personalize = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let apps_light = hkcu_dword(personalize, w!("AppsUseLightTheme")).is_none_or(|v| v != 0);
    let taskbar_light = hkcu_dword(personalize, w!("SystemUsesLightTheme")).is_none_or(|v| v != 0);
    let accent = hkcu_dword(w!(r"Software\Microsoft\Windows\DWM"), w!("AccentColor"))
        .map_or(DEFAULT_ACCENT, accent_from_abgr);
    let mut on = BOOL(1);
    // SAFETY: Çıktı yerel BOOL'a yazılır; ayar değiştirilmez.
    let animations = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_or(true, |()| on.as_bool());
    ThemeInfo {
        apps_light,
        taskbar_light,
        accent,
        animations,
    }
}
