// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arayüz bileşenleri, başsız Slint test arka ucuyla: durum metinleri, modül kartları, dil
//! değişimi, gezinme etiketleri ve Hakkında sayfası.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Mutex;

use alterdns_errors::Lang;
use alterdns_ui::slint::ComponentHandle;
use alterdns_ui::{FlyoutWindow, MainWindow, Page, StatusKind, on_accent, select_language};
use i_slint_backend_testing::ElementHandle;

/// Çeviri seçimi işlem genelinde olabilir; dil bağımlı testler sırayla çalışır.
static LANG: Mutex<()> = Mutex::new(());

fn count(component: &impl ComponentHandle, label: &str) -> usize {
    ElementHandle::find_by_accessible_label(component, label).count()
}

#[test]
fn flyout_shows_unreachable_title_and_three_cards() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let flyout = FlyoutWindow::new().unwrap();
    flyout.show().unwrap();
    select_language(Lang::En).unwrap();
    flyout.set_status_kind(StatusKind::Unreachable);
    flyout.set_status_title("Cannot connect to the AlterDNS service".into());
    flyout.set_status_detail("ADNS-IPC-001".into());
    assert_eq!(count(&flyout, "Cannot connect to the AlterDNS service"), 1);
    assert_eq!(count(&flyout, "ADNS-IPC-001"), 1);
    assert_eq!(count(&flyout, "Not installed"), 3);
    // Faz 1'de "korunuyorsunuz" türünde bir metin yok; bağlıyken bile modül yüklü değil denir.
    flyout.set_status_kind(StatusKind::Ready);
    assert_eq!(count(&flyout, "Protection module not installed"), 1);
    assert_eq!(count(&flyout, "ADNS-IPC-001"), 0);
}

#[test]
fn flyout_buttons_raise_callbacks() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let flyout = FlyoutWindow::new().unwrap();
    flyout.show().unwrap();
    select_language(Lang::En).unwrap();
    let hits = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    for (name, label) in [
        ("main", "Open main window"),
        ("settings", "Settings"),
        ("quit", "Quit"),
    ] {
        let h = hits.clone();
        let push = move || h.borrow_mut().push(name);
        match name {
            "main" => flyout.on_open_main(push),
            "settings" => flyout.on_open_settings(push),
            _ => flyout.on_quit(push),
        }
        let buttons: Vec<_> = ElementHandle::find_by_accessible_label(&flyout, label).collect();
        assert!(!buttons.is_empty(), "{label}");
        buttons[0].invoke_accessible_default_action();
    }
    assert_eq!(*hits.borrow(), vec!["main", "settings", "quit"]);
}

#[test]
fn language_switch_changes_text() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let flyout = FlyoutWindow::new().unwrap();
    flyout.show().unwrap();
    select_language(Lang::En).unwrap();
    assert!(count(&flyout, "Open main window") >= 1);
    select_language(Lang::Tr).unwrap();
    assert!(count(&flyout, "Ana pencereyi aç") >= 1);
    assert_eq!(count(&flyout, "Open main window"), 0);
    assert_eq!(count(&flyout, "Yüklü değil"), 3);
    select_language(Lang::En).unwrap();
}

#[test]
fn main_window_pages_have_accessible_labels() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let main = MainWindow::new().unwrap();
    main.show().unwrap();
    select_language(Lang::En).unwrap();
    let pages = [
        ("Overview", Page::Overview),
        ("File security", Page::File),
        ("Browser protection", Page::Browser),
        ("Connection", Page::Connection),
        ("Components", Page::Components),
        ("Updates", Page::Updates),
        ("Settings", Page::Settings),
        ("About", Page::About),
    ];
    for (label, page) in pages {
        let items: Vec<_> = ElementHandle::find_by_accessible_label(&main, label)
            .filter(|e| e.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Tab))
            .collect();
        assert_eq!(items.len(), 1, "gezinme öğesi: {label}");
        items[0].invoke_accessible_default_action();
        assert_eq!(main.get_page(), page, "{label}");
    }
}

#[test]
fn about_page_has_author_license_and_source() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let main = MainWindow::new().unwrap();
    main.show().unwrap();
    select_language(Lang::En).unwrap();
    main.set_page(Page::About);
    main.set_version("0.0.0".into());
    for label in [
        "Developer: Furkan Okan",
        "Copyright © 2026 Furkan Okan",
        "License: GPL-3.0-or-later",
        "Source code: github.com/fxrkqnplus/alterdns",
        "Version 0.0.0",
    ] {
        assert_eq!(count(&main, label), 1, "{label}");
    }
    let links = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = links.clone();
    main.on_open_link(move |url| sink.borrow_mut().push(url.to_string()));
    ElementHandle::find_by_accessible_label(&main, "View source code")
        .next()
        .unwrap()
        .invoke_accessible_default_action();
    assert_eq!(
        *links.borrow(),
        vec!["https://github.com/fxrkqnplus/alterdns"]
    );
}

#[test]
fn settings_changes_are_reported() {
    let _guard = LANG.lock().unwrap_or_else(|e| e.into_inner());
    i_slint_backend_testing::init_no_event_loop();
    let main = MainWindow::new().unwrap();
    main.show().unwrap();
    select_language(Lang::En).unwrap();
    main.set_page(Page::Settings);
    main.set_notifications(true);
    let changed = std::rc::Rc::new(std::cell::Cell::new(0));
    let c = changed.clone();
    main.on_settings_changed(move || c.set(c.get() + 1));
    ElementHandle::find_by_accessible_label(&main, "Show notifications")
        .next()
        .unwrap()
        .invoke_accessible_default_action();
    assert!(!main.get_notifications());
    assert_eq!(changed.get(), 1);
}

#[test]
fn on_accent_picks_readable_text() {
    assert_eq!(on_accent(0x00_78_D7), 0xFF_FF_FF);
    assert_eq!(on_accent(0x25_63_EB), 0xFF_FF_FF);
    assert_eq!(on_accent(0xFF_D7_00), 0x1F_1F_1F);
    assert_eq!(on_accent(0xFF_FF_FF), 0x1F_1F_1F);
}
