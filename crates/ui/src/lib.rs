// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS arayüz bileşenleri (Slint, yazılım çizici, `fluent` stili): `FlyoutWindow` ve
//! `MainWindow`. Metinler `.slint` içinde `@tr("İngilizce anahtar")` ile yazılır; Türkçe ve
//! İngilizce çeviriler ikiliye gömülüdür (`lang/`).

// `forbid` yerine `deny`: Slint'in ürettiği kod vtable statikleri için kendi üzerinde
// `allow(unsafe_code)` taşır ve `forbid` bunu derleme hatasına çevirir. Elle yazılan kodda
// `unsafe` yoktur ve olmayacaktır; istisna yalnızca `generated` modülüdür.
#![deny(unsafe_code)]

// Slint derleyicisinin ürettiği kod; panik ve lint kuralları elle yazılan koda uygulanır.
#[allow(clippy::all, clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod generated {
    slint::include_modules!();
}

pub use generated::*;
pub use slint;

use alterdns_errors::{AdnsError, AdnsErrorCode, Lang};

/// Gömülü çevirilerden birini seçer. Bileşen oluşturulduktan sonra çağrılmalıdır; açık
/// pencerelerin metni hemen değişir. Seçim yapılmazsa kaynak metin (İngilizce) görünür.
pub fn select_language(lang: Lang) -> Result<(), AdnsError> {
    slint::select_bundled_translation(lang.code())
        .map_err(|e| AdnsError::new(AdnsErrorCode::UiStartFailed).with_source(e))
}

/// 0xRRGGBB → Slint rengi.
pub fn rgb(value: u32) -> slint::Color {
    slint::Color::from_rgb_u8(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

fn channel(c: u32) -> f64 {
    let c = f64::from(c & 0xFF) / 255.0;
    if c <= 0.039_28 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Vurgu rengi üzerinde okunacak metin: beyaz ya da koyu (#1F1F1F), hangisi daha yüksek
/// kontrast veriyorsa (WCAG göreli parlaklık).
pub fn on_accent(accent: u32) -> u32 {
    let l =
        0.2126 * channel(accent >> 16) + 0.7152 * channel(accent >> 8) + 0.0722 * channel(accent);
    let dark = 0.2126 * channel(0x1F) + 0.7152 * channel(0x1F) + 0.0722 * channel(0x1F);
    let with_white = 1.05 / (l + 0.05);
    let with_dark = (l + 0.05) / (dark + 0.05);
    if with_white >= with_dark {
        0xFF_FF_FF
    } else {
        0x1F_1F_1F
    }
}
