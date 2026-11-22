// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Marka varlıkları: tepsi ICO'ları (tema × durum), saydam logolar ve güncellik denetimi.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use xtask::assets::{Badge, TrayTheme, check, ico, render_tray};

fn svg() -> String {
    let root = xtask::check::repo_root().unwrap();
    std::fs::read_to_string(root.join(r"assets\brand\tray\tray.svg")).unwrap()
}

fn rgba(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let p = resvg::tiny_skia::Pixmap::decode_png(png).unwrap();
    (p.width(), p.height(), p.data().to_vec())
}

fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

#[test]
fn ico_container_is_valid() {
    let pngs: Vec<(u32, Vec<u8>)> = [16, 20, 24, 32]
        .iter()
        .map(|s| {
            (
                *s,
                render_tray(&svg(), *s, TrayTheme::Dark, Badge::None).unwrap(),
            )
        })
        .collect();
    let data = ico(&pngs);
    assert_eq!(&data[..4], &[0, 0, 1, 0]);
    assert_eq!(u16::from_le_bytes([data[4], data[5]]), 4);
    for (i, (size, _)) in pngs.iter().enumerate() {
        let e = 6 + i * 16;
        assert_eq!(u32::from(data[e]), *size, "genişlik");
        let off = u32::from_le_bytes(data[e + 12..e + 16].try_into().unwrap()) as usize;
        assert_eq!(&data[off..off + 8], b"\x89PNG\r\n\x1a\n");
    }
}

#[test]
fn tray_render_sizes_and_alpha() {
    let (w, h, data) = rgba(&render_tray(&svg(), 16, TrayTheme::Dark, Badge::None).unwrap());
    assert_eq!((w, h), (16, 16));
    // Köşe saydam (yuvarlatılmış kare), "A" boşluğunun dışında kalan gövde opak.
    assert_eq!(pixel(&data, w, 0, 0)[3], 0);
    assert_eq!(pixel(&data, w, 2, 8)[3], 255);
    // Koyu görev çubuğunda glif beyaz.
    assert_eq!(&pixel(&data, w, 2, 8)[..3], &[255, 255, 255]);
}

/// 16 px biçimde yatay kenarlar tam pikseldedir: "A"nın tepesinin hemen üstündeki satır tam
/// opak, tepe satırı tam saydam (yarı saydam bulanık kenar yok).
#[test]
fn hinted_16px_glyph_has_crisp_edges() {
    let root = xtask::check::repo_root().unwrap();
    let svg = std::fs::read_to_string(root.join(r"assets\brand\tray\tray-16.svg")).unwrap();
    let (w, _, data) = rgba(&render_tray(&svg, 16, TrayTheme::Dark, Badge::None).unwrap());
    for x in 7..=9 {
        assert_eq!(pixel(&data, w, x, 3)[3], 255, "x={x} üst kenar");
        assert_eq!(pixel(&data, w, x, 4)[3], 0, "x={x} tepe");
    }
    // "A" boşluk olarak çizilir: yatay çizgi (y 9) saydam, ayakların arası (y 10–11) gövde.
    assert_eq!(pixel(&data, w, 8, 9)[3], 0);
    assert_eq!(pixel(&data, w, 8, 10)[3], 255);
    assert_eq!(pixel(&data, w, 9, 11)[3], 255);
}

#[test]
fn badge_pixel_has_badge_color() {
    let (w, _, data) = rgba(&render_tray(&svg(), 32, TrayTheme::Light, Badge::Warning).unwrap());
    let p = pixel(&data, w, 25, 25);
    // #F59E0B, kenar yumuşatması için küçük tolerans.
    for (got, want) in p[..3].iter().zip([0xF5u8, 0x9E, 0x0B]) {
        assert!(got.abs_diff(want) <= 6, "{p:?}");
    }
    assert_eq!(p[3], 255);
}

#[test]
fn generated_assets_are_up_to_date() {
    check(&xtask::check::repo_root().unwrap()).unwrap();
}
