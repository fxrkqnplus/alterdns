// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `alterdns-ui.exe` ikilisi: komut satırı, yükleme zamanı DLL'leri, sürüm bilgisi ve simge,
//! ana pencerenin tek örnek davranışı.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use alterdns_core::ui::{
    MSG_PERF_FLYOUT_CLOSE, MSG_PERF_FLYOUT_OPEN, MSG_QUIT, PERF_CLASS, TRAY_CLASS, WINDOW_CLASS,
};
use alterdns_winsys::{
    MessageWindow, ShellEvent, icon_count, interactive_desktop, post_broadcast, pump_messages,
    system_dir, taskbar_present, window_owner,
};

/// Ana pencere tek örneklidir: onu başlatan testler sırayla çalışır.
static WINDOW: Mutex<()> = Mutex::new(());

const EXE: &str = env!("CARGO_BIN_EXE_alterdns-ui");

#[test]
fn exe_rejects_unknown_args() {
    for args in [&["--x"][..], &["--logon"], &["--page", "nope"]] {
        let code = Command::new(EXE)
            .args(args)
            .stdin(Stdio::null())
            .status()
            .unwrap()
            .code();
        assert_eq!(code, Some(2), "{args:?}");
    }
}

#[test]
fn exe_restricts_load_time_imports_to_system32() {
    let pe = std::fs::read(EXE).unwrap();
    assert_eq!(alterdns_buildinfo::dependent_load_flags(&pe), Some(0x0800));
}

/// Statik içe aktarmaların hepsi System32'de bulunan sistem DLL'leri ya da API kümeleridir;
/// `DEPENDENTLOADFLAG` yüklemeyi System32 ile sınırlar ve CRT statik bağlıdır (vcruntime yok).
#[test]
fn static_imports_are_system_dlls() {
    let pe = std::fs::read(EXE).unwrap();
    let imports = alterdns_buildinfo::imported_dlls(&pe).unwrap();
    assert!(imports.contains(&"kernel32.dll".to_string()), "{imports:?}");
    let system = system_dir().unwrap();
    for dll in &imports {
        let ok =
            dll.starts_with("api-ms-") || dll.starts_with("ext-ms-") || system.join(dll).is_file();
        assert!(
            ok,
            "System32'de olmayan statik içe aktarma: {dll} ({imports:?})"
        );
        assert!(
            !dll.starts_with("vcruntime") && !dll.starts_with("msvcp"),
            "{dll}"
        );
    }
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn exe_version_info_and_icon() {
    let pe = std::fs::read(EXE).unwrap();
    for text in ["Furkan Okan", "AlterDNS", "alterdns-ui.exe", "PerMonitorV2"] {
        let needle = if text == "PerMonitorV2" {
            text.as_bytes().to_vec()
        } else {
            utf16(text)
        };
        assert!(
            pe.windows(needle.len()).any(|w| w == needle.as_slice()),
            "{text} yok"
        );
    }
    assert!(icon_count(std::path::Path::new(EXE)) >= 1);
}

/// İkinci ana pencere işlemi ilkini öne getirir ve 0 ile çıkar; ilki açık kalır.
#[test]
fn second_window_activates_first() {
    let _guard = WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    if !interactive_desktop() {
        eprintln!("etkileşimli masaüstü yok; atlandı");
        return;
    }
    let mut first = Command::new(EXE).spawn().unwrap();
    let ready = Instant::now();
    // İlk örneğin ileti penceresi oluşunca yayın hedefi bulunur.
    while !post_broadcast("AlterDNS.Window", 2).unwrap() {
        assert!(
            ready.elapsed() < Duration::from_secs(20),
            "ilk pencere açılmadı"
        );
        assert!(first.try_wait().unwrap().is_none(), "ilk örnek erken çıktı");
        std::thread::sleep(Duration::from_millis(100));
    }
    let second = Command::new(EXE)
        .args(["--page", "about"])
        .status()
        .unwrap();
    assert_eq!(second.code(), Some(0));
    assert!(first.try_wait().unwrap().is_none(), "ilk örnek kapanmamalı");
    let _ = first.kill();
    let _ = first.wait();
}

/// `--perf`: ana pencere ilk çizimde işlem yaşını (mikro saniye) `AlterDNS.Perf` penceresine
/// bildirir; kapanma iletisiyle işlem 5 sn içinde biter (T-PERF-010).
#[test]
fn perf_window_reports_first_paint_and_exits() {
    let _guard = WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    if !interactive_desktop() {
        eprintln!("etkileşimli masaüstü yok; atlandı");
        return;
    }
    let got: Rc<RefCell<Vec<u32>>> = Rc::default();
    let sink = got.clone();
    let _perf = MessageWindow::create(
        PERF_CLASS,
        Box::new(move |e| {
            if let ShellEvent::Broadcast(v) = e {
                sink.borrow_mut().push(v);
            }
        }),
    )
    .unwrap();
    let mut child = Command::new(EXE).arg("--perf").spawn().unwrap();
    let start = Instant::now();
    while got.borrow().is_empty() && start.elapsed() < Duration::from_secs(10) {
        pump_messages(Duration::from_millis(100));
    }
    let value = got.borrow().first().copied();
    let quit_at = Instant::now();
    while post_broadcast(WINDOW_CLASS, MSG_QUIT).unwrap_or(false)
        && quit_at.elapsed() < Duration::from_secs(5)
    {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let exited = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if quit_at.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let us = value.expect("ilk çizim bildirilmedi");
    assert!(us > 0 && us < 10_000_000, "{us}");
    assert!(
        exited.is_some(),
        "kapanma iletisinden sonra işlem 5 sn içinde bitmedi"
    );
}

fn perf_receiver() -> (MessageWindow, Rc<RefCell<Vec<u32>>>) {
    let got: Rc<RefCell<Vec<u32>>> = Rc::default();
    let sink = got.clone();
    let window = MessageWindow::create(
        PERF_CLASS,
        Box::new(move |e| {
            if let ShellEvent::Broadcast(v) = e {
                sink.borrow_mut().push(v);
            }
        }),
    )
    .unwrap();
    (window, got)
}

fn wait_for(got: &Rc<RefCell<Vec<u32>>>, n: usize, timeout: Duration) -> bool {
    let start = Instant::now();
    while got.borrow().len() < n && start.elapsed() < timeout {
        pump_messages(Duration::from_millis(50));
    }
    got.borrow().len() >= n
}

/// Tepsi, ölçüm iletisiyle flyout'u açar ve ilk çizimde süreyi bildirir (soğuk ve sıcak açılış).
#[test]
fn tray_reports_flyout_open_time() {
    if !interactive_desktop() || !taskbar_present() {
        eprintln!("etkileşimli masaüstü ya da görev çubuğu yok; atlandı");
        return;
    }
    if window_owner(TRAY_CLASS).is_some() {
        eprintln!("bu oturumda AlterDNS tepsisi zaten açık; atlandı");
        return;
    }
    let (_perf, got) = perf_receiver();
    let mut child = Command::new(EXE).arg("--tray").spawn().unwrap();
    let start = Instant::now();
    while window_owner(TRAY_CLASS).is_none() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut ok = true;
    for i in 1..=2 {
        ok &= post_broadcast(TRAY_CLASS, MSG_PERF_FLYOUT_OPEN).unwrap();
        ok &= wait_for(&got, i, Duration::from_secs(5));
        let _ = post_broadcast(TRAY_CLASS, MSG_PERF_FLYOUT_CLOSE);
        pump_messages(Duration::from_millis(400));
    }
    let _ = post_broadcast(TRAY_CLASS, MSG_QUIT);
    let quit_at = Instant::now();
    while child.try_wait().unwrap().is_none() && quit_at.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let exited = child.try_wait().unwrap().is_some();
    if !exited {
        let _ = child.kill();
    }
    let values = got.borrow().clone();
    assert!(ok, "ölçüm değeri gelmedi: {values:?}");
    assert!(
        values.iter().all(|us| *us > 0 && *us < 5_000_000),
        "{values:?}"
    );
    assert!(
        exited,
        "kapanma iletisinden sonra tepsi 5 sn içinde bitmedi"
    );
}
