// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pencere sertleştirme (düz bir Win32 test penceresiyle), tema ve vurgu okuması. Görünür pencere
//! açan testler etkileşimli masaüstü yoksa atlanır.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Test penceresini oluşturmak ve iletileri göndermek için.
#![allow(unsafe_code)]

use std::cell::Cell;
use std::rc::Rc;

use alterdns_winsys::{
    WindowKind, accent_from_abgr, arm_harden, ex_style, harden, interactive_desktop, theme_info,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GWL_STYLE,
    GetWindowLongPtrW, GetWindowRect, HTCLIENT, HWND_NOTOPMOST, IsZoomed, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassW, SC_MAXIMIZE, SC_MOVE, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_KEYDOWN, WM_NCHITTEST, WM_SYSCOMMAND,
    WNDCLASSW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZEBOX,
    WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SYSMENU,
};
use windows::core::w;

unsafe extern "system" fn test_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Bu iş parçacığına ait test penceresi; düşünce yok edilir.
struct TestWindow(HWND);

impl TestWindow {
    fn new(style: WINDOW_STYLE, ex: WINDOW_EX_STYLE) -> TestWindow {
        // SAFETY: Sınıf adı sabit; pencere bu iş parçacığında oluşturulur.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(test_proc),
                hInstance: instance.into(),
                lpszClassName: w!("AlterDNS.Test.Window"),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                ex,
                w!("AlterDNS.Test.Window"),
                w!("AlterDNS test"),
                style,
                100,
                100,
                320,
                240,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            TestWindow(hwnd)
        }
    }

    fn raw(&self) -> isize {
        self.0.0 as isize
    }

    fn style(&self) -> isize {
        // SAFETY: Canlı pencere üzerinde salt okuma.
        unsafe { GetWindowLongPtrW(self.0, GWL_STYLE) }
    }

    fn show(&self, on: bool) {
        // SAFETY: Canlı pencere; etkinleştirmeden gösterilir.
        unsafe {
            let _ = ShowWindow(self.0, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }

    fn send(&self, msg: u32, wp: usize, lp: isize) -> isize {
        // SAFETY: Canlı pencereye eşzamanlı ileti.
        unsafe { SendMessageW(self.0, msg, Some(WPARAM(wp)), Some(LPARAM(lp))).0 }
    }

    fn rect(&self) -> RECT {
        let mut r = RECT::default();
        // SAFETY: Çıktı yerel değişkene yazılır.
        unsafe { GetWindowRect(self.0, &mut r).unwrap() };
        r
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: Pencere bu nesneye ait.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

fn pump() {
    let mut msg = MSG::default();
    // SAFETY: Bu iş parçacığının kuyruğu boşaltılır; `msg` yerel.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn skip_without_desktop() -> bool {
    if interactive_desktop() {
        return false;
    }
    eprintln!("etkileşimli masaüstü yok; atlandı");
    true
}

const TOOL: isize = WS_EX_TOOLWINDOW.0 as isize;
const TOP: isize = WS_EX_TOPMOST.0 as isize;
const APP: isize = WS_EX_APPWINDOW.0 as isize;

#[test]
fn flyout_style_survives_show_hide_cycles() {
    if skip_without_desktop() {
        return;
    }
    let win = TestWindow::new(WS_POPUP | WS_SYSMENU, WS_EX_APPWINDOW);
    harden(win.raw(), WindowKind::Flyout).unwrap();
    for _ in 0..3 {
        win.show(true);
        win.show(false);
        // Çatının stili geri yazmasını taklit et.
        // SAFETY: Canlı pencerenin genişletilmiş stili değiştirilmeye çalışılır.
        unsafe {
            let ex = GetWindowLongPtrW(win.0, GWL_EXSTYLE);
            SetWindowLongPtrW(win.0, GWL_EXSTYLE, (ex & !TOOL) | APP);
            let _ = SetWindowPos(
                win.0,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    win.show(true);
    let ex = ex_style(win.raw());
    assert_eq!(ex & TOOL, TOOL, "TOOLWINDOW yok: {ex:#x}");
    assert_eq!(ex & TOP, TOP, "TOPMOST yok: {ex:#x}");
    assert_eq!(ex & APP, 0, "APPWINDOW var: {ex:#x}");
    assert_eq!(win.style() & WS_SYSMENU.0 as isize, 0);
}

#[test]
fn flyout_hit_test_is_client_and_cannot_move() {
    if skip_without_desktop() {
        return;
    }
    let win = TestWindow::new(WS_POPUP, WINDOW_EX_STYLE(0));
    harden(win.raw(), WindowKind::Flyout).unwrap();
    win.show(true);
    let before = win.rect();
    let point = ((before.top + 5) << 16 | (before.left + 5)) as isize;
    assert_eq!(win.send(WM_NCHITTEST, 0, point), HTCLIENT as isize);
    // Engellenmezse SC_MOVE kalıcı taşıma döngüsüne girer; dönmesi engellendiğini gösterir.
    win.send(WM_SYSCOMMAND, SC_MOVE as usize, 0);
    let after = win.rect();
    assert_eq!(
        (before.left, before.top, before.right, before.bottom),
        (after.left, after.top, after.right, after.bottom)
    );
}

#[test]
fn main_window_cannot_maximize() {
    if skip_without_desktop() {
        return;
    }
    let win = TestWindow::new(WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE(0));
    harden(win.raw(), WindowKind::Main).unwrap();
    assert_eq!(win.style() & WS_MAXIMIZEBOX.0 as isize, 0);
    // SAFETY: Canlı pencerenin stili değiştirilmeye çalışılır.
    unsafe {
        SetWindowLongPtrW(win.0, GWL_STYLE, win.style() | WS_MAXIMIZEBOX.0 as isize);
    }
    assert_eq!(win.style() & WS_MAXIMIZEBOX.0 as isize, 0);
    win.show(true);
    win.send(WM_SYSCOMMAND, SC_MAXIMIZE as usize, 0);
    // SAFETY: Canlı pencere üzerinde salt okuma.
    assert!(!unsafe { IsZoomed(win.0) }.as_bool());
    // Ana pencere TOOLWINDOW olmaz (görev çubuğunda görünür).
    assert_eq!(ex_style(win.raw()) & TOOL, 0);
}

#[test]
fn armed_flyout_requests_close_on_escape() {
    if skip_without_desktop() {
        return;
    }
    let closes = Rc::new(Cell::new(0));
    let counter = closes.clone();
    arm_harden(
        WindowKind::Flyout,
        Box::new(move || counter.set(counter.get() + 1)),
    )
    .unwrap();
    let win = TestWindow::new(WS_POPUP, WS_EX_APPWINDOW);
    win.show(true);
    let ex = ex_style(win.raw());
    assert_eq!(
        ex & TOOL,
        TOOL,
        "kanca gösterimden önce sertleştirmedi: {ex:#x}"
    );
    assert_eq!(ex & APP, 0);
    win.send(WM_KEYDOWN, usize::from(VK_ESCAPE.0), 0);
    // Kapatma isteği ertelenir: ileti döngüsü dönmeden çağrılmaz.
    assert_eq!(closes.get(), 0);
    pump();
    assert_eq!(closes.get(), 1);
    // Kanca yalnızca bir pencereyi yakalar.
    let other = TestWindow::new(WS_POPUP, WS_EX_APPWINDOW);
    other.show(true);
    assert_eq!(ex_style(other.raw()) & TOOL, 0);
}

#[test]
fn accent_from_abgr_converts() {
    assert_eq!(accent_from_abgr(0xFF_D7_78_00), 0x00_00_78_D7);
    assert_eq!(accent_from_abgr(0x00_EB_63_25), 0x00_25_63_EB);
}

#[test]
fn theme_info_reads_without_error() {
    let t = theme_info();
    assert!(t.accent <= 0x00FF_FFFF, "{:#x}", t.accent);
}
