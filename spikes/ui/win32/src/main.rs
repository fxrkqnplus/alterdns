// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI spike: Win32 + Direct2D/DirectWrite flyout (ADR-002 seçenek 2).
//! Açılışta pencere ve çizim hedefi oluşturulur, kapanışta yok edilir.
//! `--render=software`: Direct2D yazılım çizim hedefi (GPU sürücüsü yüklenmez); varsayılan donanım.

use std::cell::{Cell, RefCell};

use ui_common::{FLYOUT_H, FLYOUT_W, emit, harden, placement};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_ROUNDED_RECT,
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    MSG, RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos,
    ShowWindow, TranslateMessage, WM_DESTROY, WM_DPICHANGED, WM_PAINT, WM_SIZE, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const FLYOUT_CLASS: PCWSTR = w!("AlterDNSSpikeFlyoutWin32");
const GLYPHS: &str = "ğĞüÜşŞıİöÖçÇ";

struct Graphics {
    d2d: ID2D1Factory,
    title: IDWriteTextFormat,
    body: IDWriteTextFormat,
    small: IDWriteTextFormat,
}

thread_local! {
    static FLYOUT: Cell<HWND> = Cell::new(HWND::default());
    static SOFTWARE: Cell<bool> = const { Cell::new(false) };
    static GFX: RefCell<Option<Graphics>> = const { RefCell::new(None) };
    static TARGET: RefCell<Option<ID2D1HwndRenderTarget>> = const { RefCell::new(None) };
}

fn main() -> anyhow::Result<()> {
    // SAFETY: İşlem başında, pencere oluşturulmadan önce bir kez çağrılır.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // SAFETY: Sınıf bu iş parçacığında kaydedilir; sabit geniş dize.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(flyout_proc),
            hInstance: instance.into(),
            lpszClassName: FLYOUT_CLASS,
            ..Default::default()
        });
    }
    SOFTWARE.with(|c| c.set(std::env::args().any(|a| a == "--render=software")));
    ui_common::create_tray(Box::new(toggle), Box::new(open), Box::new(close))?;
    ui_common::install_paint_probe()?;
    emit("TRAY_READY");

    let mut msg = MSG::default();
    // SAFETY: Standart ileti döngüsü; `msg` yerel.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    ui_common::remove_tray();
    Ok(())
}

fn toggle() {
    if FLYOUT.with(Cell::get).is_invalid() {
        open();
    } else {
        close();
    }
}

fn open() {
    if !FLYOUT.with(Cell::get).is_invalid() {
        return;
    }
    ui_common::arm_ready();
    let p = placement();
    // SAFETY: Sınıf kayıtlı; pencere bu iş parçacığında oluşturulur.
    let created = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            FLYOUT_CLASS,
            w!("AlterDNS"),
            WS_POPUP,
            p.x,
            p.y,
            p.width,
            p.height,
            None,
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
    };
    let hwnd = match created {
        Ok(h) => h,
        Err(e) => {
            eprintln!("flyout oluşturulamadı: {e}");
            return;
        }
    };
    if let Err(e) = harden(hwnd) {
        eprintln!("sertleştirme başarısız: {e}");
    }
    FLYOUT.with(|c| c.set(hwnd));
    // SAFETY: Geçerli pencere.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn close() {
    let hwnd = FLYOUT.with(Cell::get);
    if hwnd.is_invalid() {
        return;
    }
    FLYOUT.with(|c| c.set(HWND::default()));
    // SAFETY: Geçerli pencere; WM_DESTROY çizim hedefini bırakır.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    emit("HIDDEN");
}

fn graphics() -> windows::core::Result<()> {
    if GFX.with(|g| g.borrow().is_some()) {
        return Ok(());
    }
    // SAFETY: Fabrikalar tek iş parçacığında kullanılır; sabit geniş dizeler.
    let g = unsafe {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fmt = |size: f32, weight: DWRITE_FONT_WEIGHT| {
            dw.CreateTextFormat(
                w!("Segoe UI Variable Text"),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("tr-TR"),
            )
        };
        Graphics {
            title: fmt(20.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            body: fmt(14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            small: fmt(12.0, DWRITE_FONT_WEIGHT_NORMAL)?,
            d2d,
        }
    };
    GFX.with(|c| *c.borrow_mut() = Some(g));
    Ok(())
}

fn target_properties() -> D2D1_RENDER_TARGET_PROPERTIES {
    D2D1_RENDER_TARGET_PROPERTIES {
        r#type: if SOFTWARE.with(Cell::get) {
            D2D1_RENDER_TARGET_TYPE_SOFTWARE
        } else {
            D2D1_RENDER_TARGET_TYPE_DEFAULT
        },
        ..Default::default()
    }
}

fn color(rgb: u32) -> D2D1_COLOR_F {
    let c = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    D2D1_COLOR_F { r: c(16), g: c(8), b: c(0), a: 1.0 }
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn paint(hwnd: HWND) -> windows::core::Result<()> {
    graphics()?;
    GFX.with(|g| {
        let g = g.borrow();
        let Some(g) = g.as_ref() else { return Ok(()) };
        TARGET.with(|t| {
            let mut t = t.borrow_mut();
            if t.is_none() {
                let mut rc = RECT::default();
                // SAFETY: Geçerli pencere; çıktı yerel.
                unsafe { GetClientRect(hwnd, &mut rc) }?;
                let props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: D2D_SIZE_U {
                        width: (rc.right - rc.left) as u32,
                        height: (rc.bottom - rc.top) as u32,
                    },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                };
                // SAFETY: Özellik yapıları yerel; fabrika geçerli.
                let rt = unsafe {
                    g.d2d
                        .CreateHwndRenderTarget(&target_properties(), &props)
                }?;
                // SAFETY: Geçerli pencere.
                let dpi = unsafe { GetDpiForWindow(hwnd) } as f32;
                // SAFETY: Geçerli çizim hedefi.
                unsafe { rt.SetDpi(dpi, dpi) };
                *t = Some(rt);
            }
            let Some(rt) = t.as_ref() else { return Ok(()) };
            // SAFETY: BeginDraw/EndDraw arasında yalnızca bu hedefin kaynakları kullanılır.
            let result = unsafe {
                let white = rt.CreateSolidColorBrush(&color(0xFFFFFF), None)?;
                let muted = rt.CreateSolidColorBrush(&color(0xC8C8C8), None)?;
                let card = rt.CreateSolidColorBrush(&color(0x2B2B2B), None)?;
                let accent = rt.CreateSolidColorBrush(&color(0x3A6FD8), None)?;
                rt.BeginDraw();
                rt.Clear(Some(&color(0x202020)));
                let w = FLYOUT_W as f32;
                let text = |s: &str, f: &IDWriteTextFormat, r: D2D_RECT_F, b| {
                    rt.DrawText(
                        &utf16(s),
                        f,
                        &r,
                        b,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                };
                text("AlterDNS — Korunuyorsunuz", &g.title, rect(16.0, 14.0, w - 16.0, 44.0), &white);
                let cards = [
                    ("Bağlantı koruması", "Bağlı • Wi-Fi • Profil: Uyumlu TCP"),
                    ("Dosya güvenliği", "Son tarama: Güvenli • 2 dk önce"),
                    ("Tarayıcı koruması", "Bugün 3 adres engellendi • Uzantı bağlı"),
                ];
                for (i, (title, sub)) in cards.iter().enumerate() {
                    let top = 56.0 + i as f32 * 84.0;
                    rt.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(16.0, top, w - 16.0, top + 72.0),
                            radiusX: 8.0,
                            radiusY: 8.0,
                        },
                        &card,
                    );
                    text(title, &g.body, rect(28.0, top + 12.0, w - 28.0, top + 34.0), &white);
                    text(sub, &g.small, rect(28.0, top + 38.0, w - 28.0, top + 60.0), &muted);
                }
                text(GLYPHS, &g.body, rect(16.0, 312.0, w - 16.0, 336.0), &white);
                let btn = 352.0;
                rt.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: rect(16.0, btn, w - 16.0, btn + 40.0),
                        radiusX: 6.0,
                        radiusY: 6.0,
                    },
                    &accent,
                );
                text("Ana pencereyi aç", &g.body, rect(120.0, btn + 9.0, w - 16.0, btn + 32.0), &white);
                let _ = FLYOUT_H;
                rt.EndDraw(None, None)
            };
            if result.is_err() {
                // Aygıt kaybı vb.: hedef sonraki boyamada yeniden oluşturulur.
                *t = None;
            }
            result
        })
    })
}

unsafe extern "system" fn flyout_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if let Err(e) = paint(hwnd) {
                eprintln!("çizim hatası: {e}");
            }
            // SAFETY: Geçerli pencere; boyama alanı doğrulanır.
            unsafe {
                let _ = ValidateRect(Some(hwnd), None);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            TARGET.with(|t| {
                if let Some(rt) = t.borrow().as_ref() {
                    let size = D2D_SIZE_U {
                        width: (lp.0 & 0xFFFF) as u32,
                        height: ((lp.0 >> 16) & 0xFFFF) as u32,
                    };
                    // SAFETY: Geçerli çizim hedefi.
                    let _ = unsafe { rt.Resize(&size) };
                }
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            // SAFETY: lParam, önerilen pencere dikdörtgenini gösteren bir RECT işaretçisidir.
            unsafe {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            TARGET.with(|t| *t.borrow_mut() = None);
            // SAFETY: Geçerli pencere.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            TARGET.with(|t| *t.borrow_mut() = None);
            LRESULT(0)
        }
        // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
