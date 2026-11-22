// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Marka varlıkları (`docs/UI_SPEC.md` §1). Kaynaklar: onaylı tepsi glifi
//! `assets/brand/tray/tray.svg` (Aday C, 2026-11-20) ve `assets/brand/alterdns.png`. Özgün
//! dosyalar değiştirilmez; çıktılar `assets/brand/generated/` altına yazılır ve depoya girer.
//! `check`, çıktıların kaynaklardan yeniden üretilenle aynı olduğunu (piksel düzeyinde, küçük
//! toleransla) denetler.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use resvg::tiny_skia::{
    Color, ColorU8, FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform,
};
use resvg::usvg;

pub const TRAY_SIZES: [u32; 4] = [16, 20, 24, 32];
/// Uygulama simgesi boyutları (100–200 % ölçekte küçük ve büyük simge, Explorer görünümleri).
pub const APP_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];
const OUT_DIR: &str = r"assets\brand\generated";
const TRAY_SVG: &str = r"assets\brand\tray\tray.svg";
const LOGO_PNG: &str = r"assets\brand\alterdns.png";
/// Piksel karşılaştırmasında kanal başına izin verilen fark.
const TOLERANCE: u8 = 2;

/// Görev çubuğunun teması: açık görev çubuğunda glif koyu, koyuda beyazdır.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayTheme {
    Light,
    Dark,
}

impl TrayTheme {
    pub const ALL: [TrayTheme; 2] = [TrayTheme::Light, TrayTheme::Dark];

    fn glyph(self) -> &'static str {
        match self {
            TrayTheme::Light => "#1F1F1F",
            TrayTheme::Dark => "#FFFFFF",
        }
    }

    fn taskbar(self) -> &'static str {
        match self {
            TrayTheme::Light => "#F3F3F3",
            TrayTheme::Dark => "#202020",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TrayTheme::Light => "light",
            TrayTheme::Dark => "dark",
        }
    }
}

/// Durum rozeti (UI_SPEC §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    None,
    Active,
    Paused,
    Warning,
    Error,
}

impl Badge {
    pub const ALL: [Badge; 5] = [
        Badge::None,
        Badge::Active,
        Badge::Paused,
        Badge::Warning,
        Badge::Error,
    ];

    fn color(self) -> Option<&'static str> {
        match self {
            Badge::None => None,
            Badge::Active => Some("#16A34A"),
            Badge::Paused => Some("#9CA3AF"),
            Badge::Warning => Some("#F59E0B"),
            Badge::Error => Some("#DC2626"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Badge::None => "none",
            Badge::Active => "active",
            Badge::Paused => "paused",
            Badge::Warning => "warning",
            Badge::Error => "error",
        }
    }
}

/// Glif rengini temaya göre verir ve rozeti ekler: 16'lık ızgarada sağ altta r = 2,7, görev
/// çubuğu renginde 1,1 px kenarlık (tray.svg'deki not).
fn tray_svg(svg: &str, theme: TrayTheme, badge: Badge) -> anyhow::Result<String> {
    let colored = svg.replacen(
        r##"color="#000000""##,
        &format!(r#"color="{}""#, theme.glyph()),
        1,
    );
    if colored == svg {
        bail!("tray.svg'de color=\"#000000\" yok");
    }
    let end = colored.rfind("</svg>").context("tray.svg kapanmıyor")?;
    let badge_el = badge.color().map_or_else(String::new, |c| {
        format!(
            r#"<circle cx="12.75" cy="12.75" r="2.7" fill="{c}" stroke="{}" stroke-width="1.1"/>"#,
            theme.taskbar()
        )
    });
    Ok(format!("{}{badge_el}{}", &colored[..end], &colored[end..]))
}

/// Tepsi glifi, `size` × `size` PNG.
pub fn render_tray(
    svg: &str,
    size: u32,
    theme: TrayTheme,
    badge: Badge,
) -> anyhow::Result<Vec<u8>> {
    let text = tray_svg(svg, theme, badge)?;
    let tree = usvg::Tree::from_str(&text, &usvg::Options::default())?;
    let mut pix = Pixmap::new(size, size).context("pixmap")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pix.as_mut(),
    );
    Ok(pix.encode_png()?)
}

/// PNG gömülü ICO kapsayıcısı (Vista ve sonrası).
pub fn ico(pngs: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0, 0, 1, 0];
    out.extend((pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len();
    for (size, png) in pngs {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend([side, side, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((png.len() as u32).to_le_bytes());
        out.extend((offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in pngs {
        out.extend(png);
    }
    out
}

/// Parlaklık → alfa: siyah zemin saydam, beyaz monogram opak; renk `rgb`. Kare alana sığdırılır.
fn logo(src: &Pixmap, rgb: [u8; 3], size: u32) -> anyhow::Result<Vec<u8>> {
    let mut tinted = Pixmap::new(src.width(), src.height()).context("pixmap")?;
    for (d, s) in tinted.pixels_mut().iter_mut().zip(src.pixels()) {
        let c = s.demultiply();
        let lum =
            (u32::from(c.red()) * 299 + u32::from(c.green()) * 587 + u32::from(c.blue()) * 114)
                / 1000;
        let alpha = (lum * u32::from(c.alpha()) / 255) as u8;
        *d = ColorU8::from_rgba(rgb[0], rgb[1], rgb[2], alpha).premultiply();
    }
    fit(&tinted, size, None)
}

/// `src`'yi kırpmadan `size` × `size` alana ortalar; `background` verilirse önce onu boyar.
fn fit(src: &Pixmap, size: u32, background: Option<Color>) -> anyhow::Result<Vec<u8>> {
    let mut out = Pixmap::new(size, size).context("pixmap")?;
    if let Some(bg) = background {
        out.fill(bg);
    }
    let scale = size as f32 / src.width().max(src.height()) as f32;
    let tx = (size as f32 - src.width() as f32 * scale) / 2.0;
    let ty = (size as f32 - src.height() as f32 * scale) / 2.0;
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    out.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &paint,
        Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
        None,
    );
    Ok(out.encode_png()?)
}

/// Üretilecek bütün dosyalar: (depo içi yol, içerik).
fn outputs(root: &Path) -> anyhow::Result<Vec<(PathBuf, Vec<u8>)>> {
    let svg = std::fs::read_to_string(root.join(TRAY_SVG)).context(TRAY_SVG)?;
    let out = PathBuf::from(OUT_DIR);
    let mut files = Vec::new();
    for theme in TrayTheme::ALL {
        for badge in Badge::ALL {
            let pngs = TRAY_SIZES
                .iter()
                .map(|s| Ok((*s, render_tray(&svg, *s, theme, badge)?)))
                .collect::<anyhow::Result<Vec<_>>>()?;
            files.push((
                out.join(format!("tray-{}-{}.ico", theme.name(), badge.name())),
                ico(&pngs),
            ));
        }
    }
    let src = Pixmap::load_png(root.join(LOGO_PNG)).context(LOGO_PNG)?;
    files.push((
        out.join("logo-light-512.png"),
        logo(&src, [0x1F, 0x1F, 0x1F], 512)?,
    ));
    files.push((
        out.join("logo-dark-512.png"),
        logo(&src, [0xFF, 0xFF, 0xFF], 512)?,
    ));
    files.push((
        out.join("square-1024.png"),
        fit(&src, 1024, Some(Color::BLACK))?,
    ));
    // Exe simgesi (Explorer, görev çubuğu, Alt+Tab) ve ana pencere simgesi: siyah kare zemin
    // açık ve koyu temada okunur.
    let app = APP_SIZES
        .iter()
        .map(|s| Ok((*s, fit(&src, *s, Some(Color::BLACK))?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    files.push((out.join("app.ico"), ico(&app)));
    files.push((out.join("app-256.png"), fit(&src, 256, Some(Color::BLACK))?));
    Ok(files)
}

pub fn generate(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(root.join(OUT_DIR))?;
    let mut written = Vec::new();
    for (rel, data) in outputs(root)? {
        std::fs::write(root.join(&rel), data).with_context(|| rel.display().to_string())?;
        written.push(rel);
    }
    Ok(written)
}

/// ICO'nun PNG girdileri; tek PNG ise kendisi.
fn images(data: &[u8]) -> anyhow::Result<Vec<Pixmap>> {
    if data.starts_with(&[0, 0, 1, 0]) {
        let count = usize::from(u16::from_le_bytes([data[4], data[5]]));
        (0..count)
            .map(|i| {
                let e = 6 + i * 16;
                let field = |o: usize| -> anyhow::Result<usize> {
                    let b = data.get(e + o..e + o + 4).context("ICO girdisi kısa")?;
                    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
                };
                let (len, off) = (field(8)?, field(12)?);
                let png = data.get(off..off + len).context("ICO girdisi dışarıda")?;
                Ok(Pixmap::decode_png(png)?)
            })
            .collect()
    } else {
        Ok(vec![Pixmap::decode_png(data)?])
    }
}

fn same_pixels(a: &[PremultipliedColorU8], b: &[PremultipliedColorU8]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.red().abs_diff(y.red()) <= TOLERANCE
                && x.green().abs_diff(y.green()) <= TOLERANCE
                && x.blue().abs_diff(y.blue()) <= TOLERANCE
                && x.alpha().abs_diff(y.alpha()) <= TOLERANCE
        })
}

pub fn check(root: &Path) -> anyhow::Result<()> {
    for (rel, want) in outputs(root)? {
        let got = std::fs::read(root.join(&rel))
            .with_context(|| format!("{} yok: cargo xtask assets", rel.display()))?;
        let (w, g) = (images(&want)?, images(&got)?);
        let same = w.len() == g.len()
            && w.iter().zip(&g).all(|(a, b)| {
                a.width() == b.width()
                    && a.height() == b.height()
                    && same_pixels(a.pixels(), b.pixels())
            });
        if !same {
            bail!("{} güncel değil: cargo xtask assets", rel.display());
        }
    }
    Ok(())
}
