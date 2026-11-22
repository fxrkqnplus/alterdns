// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ölçüm ilkelleri: işlem belleği, CPU zamanı ve iş parçacığı döngüleri, servis işlemi, ileti
//! pompası, işlem yaşı ve sertleştirilmiş pencerenin ilk çizim sondası.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Test penceresini oluşturmak ve geçersizleştirmek için.
#![allow(unsafe_code)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alterdns_winsys::{
    WindowKind, arm_harden, hardened_window, interactive_desktop, logical_cpus, on_next_paint,
    process_age, pump_messages, sample_process, service_pid,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SW_SHOWNOACTIVATE, ShowWindow,
    WINDOW_EX_STYLE, WNDCLASSW, WS_POPUP,
};
use windows::core::w;

#[test]
fn sample_own_process() {
    let s = sample_process(std::process::id()).unwrap();
    assert!(s.private_bytes > 1024 * 1024, "{}", s.private_bytes);
    assert!(!s.thread_cycles.is_empty());
    // Test ikilisi en az ana iş parçacığında iş yaptı.
    assert!(s.thread_cycles.iter().any(|(_, c)| *c > 0));
    assert!(logical_cpus() >= 1);
}

#[test]
fn cpu_time_grows_with_work() {
    let before = sample_process(std::process::id()).unwrap().cpu_time;
    let start = Instant::now();
    let mut x = 0u64;
    while start.elapsed() < Duration::from_millis(200) {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
    }
    std::hint::black_box(x);
    let after = sample_process(std::process::id()).unwrap().cpu_time;
    assert!(after > before, "{before:?} → {after:?}");
}

#[test]
fn service_pid_absent() {
    let name = format!("AlterDNS-yok-{}", std::process::id());
    assert_eq!(service_pid(&name).unwrap(), None);
}

#[test]
fn pump_returns_after_timeout() {
    let start = Instant::now();
    pump_messages(Duration::from_millis(200));
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(190) && took < Duration::from_secs(1),
        "{took:?}"
    );
}

#[test]
fn process_age_is_positive() {
    let age = process_age();
    assert!(
        age > Duration::ZERO && age < Duration::from_secs(3600),
        "{age:?}"
    );
}

unsafe extern "system" fn test_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn test_window() -> HWND {
    // SAFETY: Sabit sınıf adı; pencere bu iş parçacığında oluşturulur.
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(test_proc),
            hInstance: instance.into(),
            lpszClassName: w!("AlterDNS.Test.Paint"),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("AlterDNS.Test.Paint"),
            w!("AlterDNS test"),
            WS_POPUP,
            100,
            100,
            200,
            120,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap()
    }
}

fn invalidate(hwnd: HWND) {
    // SAFETY: Canlı pencerenin tamamı geçersizleştirilir.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

fn destroy(hwnd: HWND) {
    // SAFETY: Pencere bu testte oluşturuldu.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
fn paint_probe_fires_once() {
    if !interactive_desktop() {
        eprintln!("etkileşimli masaüstü yok; atlandı");
        return;
    }
    arm_harden(WindowKind::Flyout, Box::new(|| {})).unwrap();
    let hwnd = test_window();
    // SAFETY: Canlı pencere; etkinleştirmeden gösterilir.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    assert_eq!(hardened_window(), Some(hwnd.0 as isize));
    let fired = Rc::new(Cell::new(0));
    let counter = fired.clone();
    on_next_paint(
        hwnd.0 as isize,
        Box::new(move || counter.set(counter.get() + 1)),
    )
    .unwrap();
    invalidate(hwnd);
    pump_messages(Duration::from_millis(500));
    assert_eq!(fired.get(), 1);
    invalidate(hwnd);
    pump_messages(Duration::from_millis(300));
    assert_eq!(fired.get(), 1, "sonda bir kez çalışır");
    destroy(hwnd);
}

#[test]
fn paint_probe_requires_hardened_window() {
    let hwnd = test_window();
    let err = on_next_paint(hwnd.0 as isize, Box::new(|| {})).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    destroy(hwnd);
}
