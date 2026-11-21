// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ekran bilgisi ve pencere konumu. Hesap saf işlevlerdedir (`place_flyout`, `center_in`);
//! sistem okumaları (`taskbar_edge`, `monitor_at`, `cursor_pos`) yalnızca girdileri toplar.

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETTASKBARPOS, APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Flyout ile görev çubuğu ve çalışma alanı kenarı arasındaki boşluk (mantıksal px).
const MARGIN: i32 = 12;
const BASE_DPI: u32 = 96;

/// Fiziksel piksel dikdörtgeni (sağ ve alt dahil değil).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn width(self) -> i32 {
        self.right - self.left
    }

    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Görev çubuğunun bulunduğu kenar. `Unknown`: okunamadı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
    Unknown,
}

/// Fiziksel konum ve boyut; `scale` = DPI / 96.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

fn scale_of(dpi: u32) -> f64 {
    f64::from(dpi.max(1)) / f64::from(BASE_DPI)
}

fn px(v: i32, scale: f64) -> i32 {
    (f64::from(v) * scale).round() as i32
}

/// Bir eksende `[lo, hi)` aralığı; boşluk sığmıyorsa boşluksuz.
fn span(lo: i32, hi: i32, margin: i32) -> (i32, i32) {
    if hi - lo > 2 * margin {
        (lo + margin, hi - margin)
    } else {
        (lo, hi)
    }
}

/// `pos`'u, `size` uzunluğundaki nesne `[lo, hi)` içinde kalacak şekilde sıkıştırır.
/// `size <= hi - lo` olmalıdır.
fn clamp_in(pos: i32, size: i32, lo: i32, hi: i32) -> i32 {
    pos.min(hi - size).max(lo)
}

/// Kenar okunamadıysa simgenin çalışma alanına göre konumundan çıkarılır. Simge alanın
/// içindeyse (taşma penceresi) flyout simgenin üstüne yerleşir.
fn infer_edge(anchor: Rect, work: Rect) -> Edge {
    if anchor.top >= work.bottom {
        Edge::Bottom
    } else if anchor.bottom <= work.top {
        Edge::Top
    } else if anchor.right <= work.left {
        Edge::Left
    } else if anchor.left >= work.right {
        Edge::Right
    } else {
        Edge::Bottom
    }
}

/// Flyout'un fiziksel konumu. `anchor`: tepsi simgesinin (ya da imlecin) dikdörtgeni; `edge`:
/// görev çubuğunun kenarı (`ABM_GETTASKBARPOS`); `work`: monitörün çalışma alanı.
///
/// Flyout simgeden görev çubuğunun ters yönüne doğru açılır ve çalışma alanında 12 mantıksal px
/// boşlukla kalır. Otomatik gizli görev çubuğu çalışma alanından yer ayırmaz; bu yüzden kenar
/// çalışma alanı sınırına değil simgenin kendisine göre alınır. Çalışma alanı flyout'tan küçükse
/// flyout alana sığdırılır.
pub fn place_flyout(
    anchor: Rect,
    edge: Edge,
    work: Rect,
    dpi: u32,
    logical_w: i32,
    logical_h: i32,
) -> Placement {
    let scale = scale_of(dpi);
    let m = px(MARGIN, scale);
    let (x_lo, x_hi) = span(work.left, work.right, m);
    let (y_lo, y_hi) = span(work.top, work.bottom, m);
    let width = px(logical_w, scale).min(x_hi - x_lo).max(0);
    let height = px(logical_h, scale).min(y_hi - y_lo).max(0);
    let cx = anchor.left + anchor.width() / 2;
    let cy = anchor.top + anchor.height() / 2;
    let edge = match edge {
        Edge::Unknown => infer_edge(anchor, work),
        known => known,
    };
    let (x, y) = match edge {
        Edge::Top => (cx - width / 2, anchor.bottom.max(work.top) + m),
        Edge::Left => (anchor.right.max(work.left) + m, cy - height / 2),
        Edge::Right => (anchor.left.min(work.right) - width - m, cy - height / 2),
        Edge::Bottom | Edge::Unknown => (cx - width / 2, anchor.top.min(work.bottom) - height - m),
    };
    Placement {
        x: clamp_in(x, width, x_lo, x_hi),
        y: clamp_in(y, height, y_lo, y_hi),
        width,
        height,
        scale,
    }
}

/// Çalışma alanında ortalanmış pencere; alandan büyükse alana sığdırılır.
pub fn center_in(work: Rect, dpi: u32, logical_w: i32, logical_h: i32) -> Placement {
    let scale = scale_of(dpi);
    let width = px(logical_w, scale).min(work.width()).max(0);
    let height = px(logical_h, scale).min(work.height()).max(0);
    Placement {
        x: work.left + (work.width() - width) / 2,
        y: work.top + (work.height() - height) / 2,
        width,
        height,
        scale,
    }
}

/// Birincil görev çubuğunun kenarı.
pub fn taskbar_edge() -> Edge {
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // SAFETY: `data` yerel, `cbSize` doğru; ABM_GETTASKBARPOS yalnızca okur.
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) } == 0 {
        return Edge::Unknown;
    }
    match data.uEdge {
        ABE_BOTTOM => Edge::Bottom,
        ABE_TOP => Edge::Top,
        ABE_LEFT => Edge::Left,
        ABE_RIGHT => Edge::Right,
        _ => Edge::Unknown,
    }
}

/// Noktaya en yakın monitörün çalışma alanı ve etkin DPI'ı (okunamazsa 96).
pub fn monitor_at(point: (i32, i32)) -> (Rect, u32) {
    let pt = POINT {
        x: point.0,
        y: point.1,
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let (mut dx, mut dy) = (BASE_DPI, BASE_DPI);
    // SAFETY: MONITOR_DEFAULTTONEAREST her zaman geçerli bir tanıtıcı verir; çıktı yapıları
    // yerel ve doğru boyutlu.
    let ok = unsafe {
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            dx = BASE_DPI;
        }
        GetMonitorInfoW(monitor, &mut info).as_bool()
    };
    let work = if ok {
        Rect {
            left: info.rcWork.left,
            top: info.rcWork.top,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom,
        }
    } else {
        // Olmaması gereken durum: 1024 × 768'lik varsayılan alan.
        Rect {
            left: 0,
            top: 0,
            right: 1024,
            bottom: 768,
        }
    };
    (work, dx)
}

/// İmlecin ekran konumu; okunamazsa (0, 0).
pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    // SAFETY: Çıktı yerel değişkene yazılır.
    if unsafe { GetCursorPos(&mut p) }.is_err() {
        return (0, 0);
    }
    (p.x, p.y)
}

/// Ana pencere: imlecin bulunduğu monitörde ortalanmış fiziksel dikdörtgen.
pub fn centered_on_cursor_monitor(logical_w: i32, logical_h: i32) -> Placement {
    let (work, dpi) = monitor_at(cursor_pos());
    center_in(work, dpi, logical_w, logical_h)
}
