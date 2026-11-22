// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Flyout ve ana pencere konumu (saf işlevler): dört kenar, otomatik gizli görev çubuğu, taşma
//! alanı, DPI ölçeği ve çalışma alanı sınırı.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_winsys::{Edge, Placement, Rect, center_in, place_flyout};
use proptest::prelude::*;

const W: i32 = 360;
const H: i32 = 400;

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

fn inside(p: &Placement, work: Rect) -> bool {
    p.width > 0
        && p.height > 0
        && p.x >= work.left
        && p.y >= work.top
        && p.x + p.width <= work.right
        && p.y + p.height <= work.bottom
}

#[test]
fn placement_bottom_taskbar() {
    let work = rect(0, 0, 1920, 1040);
    let p = place_flyout(rect(1000, 1048, 1024, 1072), Edge::Bottom, work, 96, W, H);
    assert_eq!((p.x, p.y, p.width, p.height), (832, 628, 360, 400));
    assert!((p.scale - 1.0).abs() < f64::EPSILON);
}

#[test]
fn placement_bottom_taskbar_icon_near_corner_is_clamped() {
    let work = rect(0, 0, 1920, 1040);
    let p = place_flyout(rect(1800, 1048, 1824, 1072), Edge::Bottom, work, 96, W, H);
    assert_eq!((p.x, p.y), (1920 - 360 - 12, 628));
}

#[test]
fn placement_top_taskbar() {
    let work = rect(0, 40, 1920, 1080);
    let p = place_flyout(rect(1000, 8, 1024, 32), Edge::Top, work, 96, W, H);
    assert_eq!((p.x, p.y), (832, 52));
}

#[test]
fn placement_left_taskbar() {
    let work = rect(48, 0, 1920, 1080);
    let p = place_flyout(rect(8, 500, 32, 524), Edge::Left, work, 96, W, H);
    assert_eq!((p.x, p.y), (60, 312));
}

#[test]
fn placement_right_taskbar() {
    let work = rect(0, 0, 1872, 1080);
    let p = place_flyout(rect(1880, 500, 1904, 524), Edge::Right, work, 96, W, H);
    assert_eq!((p.x, p.y), (1500, 312));
}

#[test]
fn placement_overflow_icon_is_clamped() {
    // Taşma penceresindeki simge çalışma alanının içindedir: flyout simgenin üstünde, sağdan
    // sıkıştırılmış.
    let work = rect(0, 0, 1920, 1040);
    let p = place_flyout(rect(1900, 980, 1916, 996), Edge::Bottom, work, 96, W, H);
    assert_eq!((p.x, p.y), (1548, 568));
    assert!(inside(&p, work));
}

#[test]
fn placement_autohide_bottom_stays_above_icon() {
    // Otomatik gizli görev çubuğu çalışma alanından yer ayırmaz; simge alanın içindedir.
    let work = rect(0, 0, 1920, 1080);
    let p = place_flyout(rect(1000, 1048, 1024, 1072), Edge::Bottom, work, 96, W, H);
    assert_eq!((p.x, p.y), (832, 1048 - 400 - 12));
}

#[test]
fn placement_autohide_top_goes_below_icon() {
    // Konum karşılaştırmasıyla kenar bulmak burada "taşma" sanıp simgenin üstüne yerleştirirdi.
    let work = rect(0, 0, 1920, 1080);
    let p = place_flyout(rect(1000, 8, 1024, 32), Edge::Top, work, 96, W, H);
    assert_eq!((p.x, p.y), (832, 44));
}

#[test]
fn placement_unknown_edge_infers_from_geometry() {
    let work = rect(0, 40, 1920, 1080);
    let p = place_flyout(rect(1000, 8, 1024, 32), Edge::Unknown, work, 96, W, H);
    assert_eq!((p.x, p.y), (832, 52));
    let work = rect(0, 0, 1920, 1040);
    let p = place_flyout(rect(1000, 1048, 1024, 1072), Edge::Unknown, work, 96, W, H);
    assert_eq!((p.x, p.y), (832, 628));
}

#[test]
fn placement_scales_with_dpi() {
    for (dpi, w) in [(96, 360), (120, 450), (144, 540), (192, 720)] {
        let work = rect(0, 0, 3840, 2080);
        let p = place_flyout(rect(2000, 2096, 2048, 2144), Edge::Bottom, work, dpi, W, H);
        assert_eq!(p.width, w, "dpi {dpi}");
        assert!(inside(&p, work), "dpi {dpi}: {p:?}");
    }
    let work = rect(0, 0, 3840, 2080);
    let p = place_flyout(rect(2000, 2096, 2048, 2144), Edge::Bottom, work, 192, W, H);
    assert_eq!((p.x, p.y, p.height), (2024 - 360, 2080 - 800 - 24, 800));
    assert!((p.scale - 2.0).abs() < f64::EPSILON);
}

#[test]
fn centered_window_is_inside_work_area() {
    let work = rect(0, 0, 1920, 1040);
    let p = center_in(work, 96, 960, 640);
    assert_eq!((p.x, p.y, p.width, p.height), (480, 200, 960, 640));
    // İkinci monitör, negatif koordinatlar, 150 %.
    let work = rect(-2560, -200, 0, 1240);
    let p = center_in(work, 144, 960, 640);
    assert_eq!((p.width, p.height), (1440, 960));
    assert!(inside(&p, work), "{p:?}");
    // Pencere çalışma alanından büyükse alana sığdırılır.
    let work = rect(0, 0, 1280, 720);
    let p = center_in(work, 192, 960, 640);
    assert!(inside(&p, work), "{p:?}");
}

#[test]
fn system_reads_are_sane() {
    let (work, dpi) = alterdns_winsys::monitor_at(alterdns_winsys::cursor_pos());
    assert!(work.right > work.left && work.bottom > work.top, "{work:?}");
    assert!(dpi >= 96, "{dpi}");
    // Kenar ortama göre değişir; yalnızca çağrının çalıştığı denetlenir.
    let _ = alterdns_winsys::taskbar_edge();
    let p = alterdns_winsys::centered_on_cursor_monitor(960, 640);
    assert!(inside(&p, work), "{p:?}");
}

fn edge() -> impl Strategy<Value = Edge> {
    prop_oneof![
        Just(Edge::Bottom),
        Just(Edge::Top),
        Just(Edge::Left),
        Just(Edge::Right),
        Just(Edge::Unknown),
    ]
}

proptest! {
    #[test]
    fn placement_never_leaves_work_area(
        left in -4000i32..4000,
        top in -3000i32..3000,
        width in 320i32..6000,
        height in 240i32..4000,
        ax in -5000i32..11000,
        ay in -4000i32..8000,
        aw in 0i32..64,
        edge in edge(),
        dpi in prop::sample::select(vec![96u32, 120, 144, 168, 192, 240, 288]),
        logical_h in 120i32..=520,
    ) {
        let work = rect(left, top, left + width, top + height);
        let anchor = rect(ax, ay, ax + aw, ay + aw);
        let p = place_flyout(anchor, edge, work, dpi, W, logical_h);
        prop_assert!(inside(&p, work), "{p:?} {work:?}");
        let c = center_in(work, dpi, 960, 640);
        prop_assert!(inside(&c, work), "{c:?} {work:?}");
    }
}
