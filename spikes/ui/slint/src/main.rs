// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI spike: Slint flyout (ADR-002 seçenek 1, yalnızca GPL-3.0 seçeneği).
//! Kapanış kipi: `--close=hide` (varsayılan; bileşen yaşar) veya `--close=drop`
//! (bileşen bırakılır, açılışta yeniden oluşturulur).

use std::cell::{Cell, RefCell};

use ui_common::{emit, placement};

slint::slint! {
    export component Flyout inherits Window {
        no-frame: true;
        always-on-top: true;
        title: "AlterDNS";
        background: #202020;
        default-font-family: "Segoe UI Variable Text";
        preferred-width: 360px;
        preferred-height: 420px;

        VerticalLayout {
            padding: 16px;
            spacing: 12px;
            Text {
                text: "AlterDNS — Korunuyorsunuz";
                font-size: 20px;
                font-weight: 600;
                color: white;
            }
            for card in [
                { title: "Bağlantı koruması", sub: "Bağlı • Wi-Fi • Profil: Uyumlu TCP" },
                { title: "Dosya güvenliği", sub: "Son tarama: Güvenli • 2 dk önce" },
                { title: "Tarayıcı koruması", sub: "Bugün 3 adres engellendi • Uzantı bağlı" },
            ] : Rectangle {
                height: 72px;
                background: #2b2b2b;
                border-radius: 8px;
                accessible-role: groupbox;
                accessible-label: card.title;
                VerticalLayout {
                    padding: 12px;
                    spacing: 4px;
                    Text { text: card.title; font-size: 14px; font-weight: 600; color: white; }
                    Text { text: card.sub; font-size: 12px; color: #c8c8c8; }
                }
            }
            Text { text: "ğĞüÜşŞıİöÖçÇ"; font-size: 14px; color: white; }
            Rectangle {
                height: 40px;
                background: #3a6fd8;
                border-radius: 6px;
                accessible-role: button;
                accessible-label: "Ana pencereyi aç";
                Text { text: "Ana pencereyi aç"; font-size: 14px; font-weight: 600; color: white; }
                TouchArea {}
            }
        }
    }
}

thread_local! {
    static FLYOUT: RefCell<Option<Flyout>> = const { RefCell::new(None) };
    static OPEN: Cell<bool> = const { Cell::new(false) };
    static DROP_ON_CLOSE: Cell<bool> = const { Cell::new(false) };
}

fn main() -> anyhow::Result<()> {
    let drop_on_close = std::env::args().any(|a| a == "--close=drop");
    DROP_ON_CLOSE.with(|c| c.set(drop_on_close));
    ui_common::create_tray(
        Box::new(|| defer(toggle)),
        Box::new(|| defer(open)),
        Box::new(|| defer(close)),
    )?;
    ui_common::install_paint_probe()?;
    if !drop_on_close {
        let f = Flyout::new()?;
        FLYOUT.with(|c| *c.borrow_mut() = Some(f));
    }
    emit("TRAY_READY");
    slint::run_event_loop_until_quit()?;
    ui_common::remove_tray();
    Ok(())
}

/// Tepsi penceresi yordamından Slint çağrısını olay döngüsüne erteler.
fn defer(f: fn()) {
    if let Err(e) = slint::invoke_from_event_loop(f) {
        eprintln!("olay döngüsüne erteleme başarısız: {e}");
    }
}

fn toggle() {
    if OPEN.with(Cell::get) {
        close();
    } else {
        open();
    }
}

fn open() {
    if OPEN.with(Cell::get) {
        return;
    }
    ui_common::arm_ready();
    ui_common::arm_harden();
    let p = placement();
    FLYOUT.with(|c| {
        let mut slot = c.borrow_mut();
        if slot.is_none() {
            match Flyout::new() {
                Ok(f) => *slot = Some(f),
                Err(e) => {
                    eprintln!("flyout oluşturulamadı: {e}");
                    return;
                }
            }
        }
        if let Some(f) = slot.as_ref() {
            let window = f.window();
            window.set_position(slint::PhysicalPosition::new(p.x, p.y));
            window.set_size(slint::PhysicalSize::new(p.width as u32, p.height as u32));
            if let Err(e) = f.show() {
                eprintln!("gösterilemedi: {e}");
                return;
            }
            OPEN.with(|o| o.set(true));
        }
    });
}

fn close() {
    if !OPEN.with(Cell::get) {
        return;
    }
    OPEN.with(|o| o.set(false));
    FLYOUT.with(|c| {
        let mut slot = c.borrow_mut();
        if let Some(f) = slot.as_ref() {
            let _ = f.hide();
        }
        if DROP_ON_CLOSE.with(Cell::get) {
            *slot = None;
        }
    });
    emit("HIDDEN");
}
