// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tepsi simgesi, gizli ileti penceresi ve tek örnek kilidi. Görev çubuğu varsa testler kısa
//! süreli bir test simgesi ekleyip kaldırır (AlterDNS'in kendi nesnesi); yoksa o kısım atlanır.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// İleti döngüsünü elle döndürmek ve sistemin TaskbarCreated iletisini taklit etmek için.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

use alterdns_winsys::{
    MessageWindow, ShellEvent, TrayIcon, icon_count, post_broadcast, single_instance,
    taskbar_present, windows_dir,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, PostMessageW, RegisterWindowMessageW,
    TranslateMessage,
};
use windows::core::w;

type Events = Rc<RefCell<Vec<ShellEvent>>>;

fn recorder() -> (Events, Box<dyn FnMut(ShellEvent)>) {
    let events: Events = Rc::default();
    let sink = events.clone();
    (events, Box::new(move |e| sink.borrow_mut().push(e)))
}

fn pump() {
    let mut msg = MSG::default();
    // SAFETY: Bu iş parçacığının kuyruğundaki iletiler alınıp işlenir; `msg` yerel.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn class(tag: &str) -> String {
    format!("AlterDNS.Test.{tag}.{}", std::process::id())
}

fn ico(name: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(r"..\..");
    std::fs::read(root.join(r"assets\brand\generated").join(name)).unwrap()
}

#[test]
fn single_instance_second_acquire_is_none() {
    let name = class("Instance");
    let first = single_instance(&name).unwrap().expect("ilk kilit");
    assert!(single_instance(&name).unwrap().is_none());
    drop(first);
    assert!(single_instance(&name).unwrap().is_some());
}

#[test]
fn single_instance_rejects_bad_names() {
    assert!(single_instance("").is_err());
    assert!(single_instance(r"Global\AlterDNS").is_err());
}

#[test]
fn broadcast_reaches_hidden_window() {
    let name = class("Broadcast");
    let (events, on_event) = recorder();
    let window = MessageWindow::create(&name, on_event).unwrap();
    assert!(post_broadcast(&name, 7).unwrap());
    pump();
    assert_eq!(*events.borrow(), vec![ShellEvent::Broadcast(7)]);
    drop(window);
    assert!(!post_broadcast(&name, 7).unwrap());
}

#[test]
fn icon_from_generated_ico_loads() {
    let (_events, on_event) = recorder();
    let window = MessageWindow::create(&class("Icon"), on_event).unwrap();
    let tray = TrayIcon::add(&window, &ico("tray-dark-warning.ico"), "AlterDNS test").unwrap();
    tray.set_icon(&ico("tray-light-none.ico")).unwrap();
    drop(tray);
    let err = TrayIcon::add(&window, b"not an icon", "AlterDNS test").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn second_tray_icon_on_same_window_is_refused() {
    let (_events, on_event) = recorder();
    let window = MessageWindow::create(&class("Twice"), on_event).unwrap();
    let _tray = TrayIcon::add(&window, &ico("tray-dark-none.ico"), "AlterDNS test").unwrap();
    let err = TrayIcon::add(&window, &ico("tray-dark-none.ico"), "AlterDNS test").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
}

fn post_taskbar_created(window: &MessageWindow) {
    // SAFETY: Kayıtlı ileti adı sabit; pencere bu iş parçacığına ait ve canlı.
    unsafe {
        let msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        assert_ne!(msg, 0);
        PostMessageW(
            Some(HWND(window.hwnd_raw() as *mut _)),
            msg,
            WPARAM(0),
            LPARAM(0),
        )
        .unwrap();
    }
}

#[test]
fn taskbar_created_readds_icon() {
    if !taskbar_present() {
        eprintln!("görev çubuğu yok (etkileşimsiz oturum); atlandı");
        return;
    }
    let (events, on_event) = recorder();
    let window = MessageWindow::create(&class("Taskbar"), on_event).unwrap();
    let tray = TrayIcon::add(&window, &ico("tray-light-none.ico"), "AlterDNS test").unwrap();
    assert!(tray.is_shown());
    post_taskbar_created(&window);
    pump();
    assert_eq!(*events.borrow(), vec![ShellEvent::TaskbarRecreated]);
    assert!(tray.is_shown());
    // NIM_MODIFY yalnızca simge varsa başarılı olur.
    tray.set_tooltip("AlterDNS test (yeniden eklendi)").unwrap();
}

#[test]
fn taskbar_created_without_icon_is_ignored() {
    let (events, on_event) = recorder();
    let window = MessageWindow::create(&class("NoIcon"), on_event).unwrap();
    post_taskbar_created(&window);
    pump();
    assert!(events.borrow().is_empty());
}

#[test]
fn icon_count_reads_exe_icons() {
    assert!(icon_count(&windows_dir().unwrap().join("explorer.exe")) > 0);
    assert_eq!(icon_count(std::path::Path::new(r"C:\nonexistent\x.exe")), 0);
}
