// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI spike ortak kodu (UI_SPEC §3–§4):
//! - tepsi simgesi + gizli ileti penceresi (ölçüm aracının aç/kapa iletisi buraya gelir),
//! - flyout konumu (simgeye hizalı, çalışma alanı içinde, PerMonitor DPI),
//! - HWND sertleştirmesi (alt sınıf: HTCLIENT, SC_MOVE/SC_SIZE/SC_KEYMENU yok, odak kaybı ve Esc),
//! - ilk kare zaman damgası: iş parçacığına özel `WH_CALLWNDPROCRET` kancası, açılıştan
//!   sonraki ilk `WM_PAINT` işlendikten sonra `DwmFlush` ve `READY <qpc>` yazar.
//!
//! Spike kodudur; her `unsafe` blok `// SAFETY:` açıklaması taşır.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::thread::LocalKey;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmFlush,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::Shell::{
    DefSubclassProc, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
    NIN_SELECT, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    RemoveWindowSubclass, SetWindowSubclass, Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CWPRETSTRUCT, CallNextHookEx, CreateWindowExW, DefWindowProcW, GA_ROOT, GWL_EXSTYLE,
    GWL_STYLE, GetAncestor, GetCursorPos, GetWindowLongPtrW, HTCLIENT, HWND_TOPMOST,
    IDI_APPLICATION, IsWindowVisible, LoadIconW, MSG, PM_REMOVE, PostMessageW, RegisterClassW,
    RegisterWindowMessageW, SC_KEYMENU, SC_MOVE, SC_SIZE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, WA_INACTIVE,
    WH_CALLWNDPROCRET, WH_GETMESSAGE, WINDOW_EX_STYLE, WM_ACTIVATE, WM_APP, WM_CONTEXTMENU, WM_KEYDOWN,
    WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_SHOWWINDOW, WM_SYSCOMMAND, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
};
use windows::core::{PCWSTR, w};

/// Gizli tepsi penceresinin sınıf adı; ölçüm aracı `FindWindowW` ile bulur.
pub const TRAY_CLASS: PCWSTR = w!("AlterDNSSpikeTray");
/// Ölçüm aracının gönderdiği aç/kapa iletisinin adı (`RegisterWindowMessageW`).
pub const TOGGLE_MESSAGE: PCWSTR = w!("AlterDNS.Spike.Toggle");
/// Aç/kapa iletisinin `wParam` değerleri: 0 = aç/kapa (tepsi), 1 = aç, 2 = kapat.
pub const CMD_TOGGLE: usize = 0;
pub const CMD_OPEN: usize = 1;
pub const CMD_CLOSE: usize = 2;
/// Ölçüm kipi: tanımlıysa odak kaybında kapanma devre dışıdır (ölçüm aracı açık/kapa komutlarıyla sürer).
pub const MEASURE_ENV: &str = "ALTERDNS_SPIKE_MEASURE";
/// Flyout mantıksal boyutu (UI_SPEC §4: 360 × en fazla 520).
pub const FLYOUT_W: i32 = 360;
pub const FLYOUT_H: i32 = 420;
/// Ekran kenarına uzaklık (mantıksal px).
const MARGIN: i32 = 12;

const WM_TRAY: u32 = WM_APP + 1;
const WM_CLOSE_FLYOUT: u32 = WM_APP + 2;
const WM_AFTER_PAINT: u32 = WM_APP + 3;
const TRAY_ID: u32 = 1;
const SUBCLASS_ID: usize = 1;
/// `NIN_KEYSELECT = NIN_SELECT | NINF_KEY` (shellapi.h); `windows` crate tanımlamıyor.
const NIN_KEYSELECT: u32 = NIN_SELECT | 0x1;

type Callback = RefCell<Option<Box<dyn FnMut()>>>;

thread_local! {
    static ON_TOGGLE: Callback = const { RefCell::new(None) };
    static ON_OPEN: Callback = const { RefCell::new(None) };
    static ON_CLOSE: Callback = const { RefCell::new(None) };
    static TRAY_HWND: Cell<HWND> = Cell::new(HWND::default());
    static TOGGLE_MSG: Cell<u32> = const { Cell::new(0) };
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static HARDEN_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Fiziksel piksel cinsinden flyout dikdörtgeni ve ölçek.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

/// Sistem genelinde tutarlı QPC değeri (işlemler arası karşılaştırılabilir).
pub fn qpc() -> i64 {
    let mut v = 0i64;
    // SAFETY: Çıktı yerel değişkene yazılır.
    unsafe {
        let _ = QueryPerformanceCounter(&mut v);
    }
    v
}

/// QPC frekansı (tik/sn).
pub fn qpc_frequency() -> i64 {
    let mut v = 0i64;
    // SAFETY: Çıktı yerel değişkene yazılır.
    unsafe {
        let _ = QueryPerformanceFrequency(&mut v);
    }
    v
}

/// Ölçüm aracının okuduğu satırı yazar ve hemen boşaltır.
pub fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn call(slot: &'static LocalKey<Callback>) {
    // Geri çağrıyı dışarı alarak yeniden girişte RefCell ödünç hatasını önle.
    let taken = slot.with(|c| c.borrow_mut().take());
    if let Some(mut f) = taken {
        f();
        slot.with(|c| {
            let mut b = c.borrow_mut();
            if b.is_none() {
                *b = Some(f);
            }
        });
    }
}

/// Tepsi simgesini ve gizli ileti penceresini oluşturur (çağıran iş parçacığında).
pub fn create_tray(
    on_toggle: Box<dyn FnMut()>,
    on_open: Box<dyn FnMut()>,
    on_close: Box<dyn FnMut()>,
) -> anyhow::Result<HWND> {
    ON_TOGGLE.with(|c| *c.borrow_mut() = Some(on_toggle));
    ON_OPEN.with(|c| *c.borrow_mut() = Some(on_open));
    ON_CLOSE.with(|c| *c.borrow_mut() = Some(on_close));
    // SAFETY: Sınıf ve pencere bu iş parçacığında oluşturulur; sabit geniş dizeler kullanılır.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(tray_proc),
            hInstance: instance.into(),
            lpszClassName: TRAY_CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            TRAY_CLASS,
            w!("AlterDNS spike tray"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        TRAY_HWND.with(|c| c.set(hwnd));
        TOGGLE_MSG.with(|c| c.set(RegisterWindowMessageW(TOGGLE_MESSAGE)));

        let mut nid = notify_data(hwnd);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = LoadIconW(None, IDI_APPLICATION)?;
        for (dst, src) in nid.szTip.iter_mut().zip("AlterDNS spike".encode_utf16()) {
            *dst = src;
        }
        Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid).ok()?;
        Ok(hwnd)
    }
}

/// Tepsi simgesini kaldırır.
pub fn remove_tray() {
    let hwnd = TRAY_HWND.with(Cell::get);
    let nid = notify_data(hwnd);
    // SAFETY: Aynı kimlikle eklenen simge silinir; yapı yerel.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    }
}

/// Flyout'u kapatma isteğini tepsi penceresine erteler (yeniden girişi önler).
pub fn request_close() {
    let hwnd = TRAY_HWND.with(Cell::get);
    // SAFETY: Geçerli tepsi penceresine iletiler kuyruğa alınır.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE_FLYOUT, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let toggle = TOGGLE_MSG.with(Cell::get);
    if toggle != 0 && msg == toggle {
        match wp.0 {
            CMD_OPEN => call(&ON_OPEN),
            CMD_CLOSE => call(&ON_CLOSE),
            _ => call(&ON_TOGGLE),
        }
        return LRESULT(0);
    }
    match msg {
        WM_TRAY => {
            let event = (lp.0 & 0xFFFF) as u32;
            if event == NIN_SELECT || event == NIN_KEYSELECT || event == WM_CONTEXTMENU {
                call(&ON_TOGGLE);
            }
            LRESULT(0)
        }
        WM_CLOSE_FLYOUT => {
            call(&ON_CLOSE);
            LRESULT(0)
        }
        WM_AFTER_PAINT => {
            // Boyama işlendi; bir sonraki DWM birleştirmesini bekle ve zaman damgasını yaz.
            // SAFETY: Parametresiz DWM çağrısı.
            unsafe {
                let _ = DwmFlush();
            }
            // Biçim: `READY <qpc> <flyout HWND>`; denetim betiği pencereyi buradan bulur.
            emit(&format!("READY {} {}", qpc(), wp.0));
            LRESULT(0)
        }
        // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// İlk kare ve sertleştirme kancalarını bu iş parçacığına kurar.
///
/// `WM_PAINT` kuyruktan `GetMessage` ile alınıp `DispatchMessage` ile işlenir; `SendMessage`
/// ile gönderilmediği için `WH_CALLWNDPROCRET` onu görmez. Bu yüzden boyama
/// `WH_GETMESSAGE` ile kuyruktan çıkarken yakalanır ve tepsi penceresine bir ileti
/// kuyruklanır; bu ileti boyama işlendikten sonra işlenir (`WM_AFTER_PAINT`).
/// Sertleştirme için gönderilen `WM_SHOWWINDOW` iletisi `WH_CALLWNDPROCRET` ile görülür.
pub fn install_paint_probe() -> anyhow::Result<()> {
    // SAFETY: Yalnızca bu iş parçacığının iletileri için kancalar; modül tanıtıcısı gerekmez.
    unsafe {
        SetWindowsHookExW(WH_CALLWNDPROCRET, Some(paint_hook), None, GetCurrentThreadId())?;
        SetWindowsHookExW(WH_GETMESSAGE, Some(getmsg_hook), None, GetCurrentThreadId())?;
    }
    Ok(())
}

unsafe extern "system" fn getmsg_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && wp.0 == PM_REMOVE.0 as usize && ARMED.with(Cell::get) {
        // SAFETY: WH_GETMESSAGE için lParam bir MSG işaretçisidir.
        let msg = unsafe { &*(lp.0 as *const MSG) };
        let tray = TRAY_HWND.with(Cell::get);
        if msg.message == WM_PAINT && msg.hwnd != tray {
            // SAFETY: Bu iş parçacığına ait HWND üzerinde salt okunur sorgular.
            let ok = unsafe {
                GetAncestor(msg.hwnd, GA_ROOT) == msg.hwnd && IsWindowVisible(msg.hwnd).as_bool()
            };
            if ok {
                ARMED.with(|c| c.set(false));
                // SAFETY: Geçerli tepsi penceresine ileti kuyruklanır.
                unsafe {
                    let _ = PostMessageW(Some(tray), WM_AFTER_PAINT, WPARAM(msg.hwnd.0 as usize), LPARAM(0));
                }
            }
        }
    }
    // SAFETY: Zincirdeki sonraki kancaya aynı parametreler aktarılır.
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// Sonraki görünür üst düzey pencerenin ilk `WM_PAINT`'inden sonra `READY` yazdırır.
pub fn arm_ready() {
    ARMED.with(|c| c.set(true));
}

/// Sonraki görünür üst düzey pencereyi sertleştirir (Slint penceresi için).
pub fn arm_harden() {
    HARDEN_PENDING.with(|c| c.set(true));
}

unsafe extern "system" fn paint_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: WH_CALLWNDPROCRET için lParam bir CWPRETSTRUCT işaretçisidir.
        let cwp = unsafe { &*(lp.0 as *const CWPRETSTRUCT) };
        let tray = TRAY_HWND.with(Cell::get);
        // SAFETY: Bu iş parçacığına ait bir HWND üzerinde salt okunur sorgular.
        let top_level = unsafe { GetAncestor(cwp.hwnd, GA_ROOT) } == cwp.hwnd;
        if cwp.hwnd != tray && top_level {
            if HARDEN_PENDING.with(Cell::get) && cwp.message == WM_SHOWWINDOW && cwp.wParam.0 != 0 {
                HARDEN_PENDING.with(|c| c.set(false));
                if let Err(e) = harden(cwp.hwnd) {
                    eprintln!("sertleştirme başarısız: {e}");
                }
            }
        }
    }
    // SAFETY: Zincirdeki sonraki kancaya aynı parametreler aktarılır.
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// UI_SPEC §4 pencere davranışını uygular: sürüklenemez, sistem menüsü yok, odak kaybında ve
/// Esc ile kapanır, görev çubuğunda görünmez, en üstte, yuvarlak köşe ve geçici arka plan.
pub fn harden(hwnd: HWND) -> anyhow::Result<()> {
    // SAFETY: Bu iş parçacığına ait HWND; yapılar doğru boyutla geçirilir.
    unsafe {
        SetWindowSubclass(hwnd, Some(harden_proc), SUBCLASS_ID, 0).ok()?;
        let corner: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        )?;
        let backdrop: DWM_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const _,
            std::mem::size_of_val(&backdrop) as u32,
        )?;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | WS_EX_TOOLWINDOW.0 as isize | WS_EX_TOPMOST.0 as isize,
        );
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_SYSMENU.0 as isize));
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(())
}

unsafe extern "system" fn harden_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => return LRESULT(HTCLIENT as isize),
        WM_SYSCOMMAND => {
            let cmd = (wp.0 & 0xFFF0) as u32;
            if cmd == SC_MOVE || cmd == SC_SIZE || cmd == SC_KEYMENU {
                return LRESULT(0);
            }
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) as u32 == WA_INACTIVE && std::env::var_os(MEASURE_ENV).is_none() {
                request_close();
            }
        }
        WM_KEYDOWN => {
            if wp.0 == usize::from(VK_ESCAPE.0) {
                request_close();
                return LRESULT(0);
            }
        }
        WM_NCDESTROY => {
            // SAFETY: Kurulan alt sınıf aynı kimlikle kaldırılır.
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(harden_proc), SUBCLASS_ID);
            }
        }
        _ => {}
    }
    // SAFETY: Alt sınıf zincirinde sonraki işleyiciye aynı parametreler aktarılır.
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Flyout'un fiziksel konumu ve boyutu: simgeye hizalı, görev çubuğu kenarına göre,
/// çalışma alanı içinde. Simge dikdörtgeni alınamazsa imleç konumu kullanılır.
pub fn placement() -> Placement {
    let tray = TRAY_HWND.with(Cell::get);
    let id = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: tray,
        uID: TRAY_ID,
        ..Default::default()
    };
    // SAFETY: Yapılar yerel ve doğru boyutlu.
    let anchor = unsafe { Shell_NotifyIconGetRect(&id) }.unwrap_or_else(|_| {
        let mut p = POINT::default();
        // SAFETY: Çıktı yerel değişkene yazılır.
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        RECT { left: p.x, top: p.y, right: p.x, bottom: p.y }
    });
    let center = POINT {
        x: (anchor.left + anchor.right) / 2,
        y: (anchor.top + anchor.bottom) / 2,
    };
    // SAFETY: Monitör tanıtıcısı ve çıktı yapıları yerel.
    let (work, dpi) = unsafe {
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (info.rcWork, dx)
    };
    let scale = f64::from(dpi) / 96.0;
    let px = |v: i32| (f64::from(v) * scale).round() as i32;
    let (w, h, m) = (px(FLYOUT_W), px(FLYOUT_H), px(MARGIN));
    let clamp_x = |x: i32| x.clamp(work.left + m, (work.right - w - m).max(work.left + m));
    let clamp_y = |y: i32| y.clamp(work.top + m, (work.bottom - h - m).max(work.top + m));
    let (x, y) = if anchor.top >= work.bottom {
        (clamp_x(center.x - w / 2), work.bottom - h - m) // görev çubuğu altta
    } else if anchor.bottom <= work.top {
        (clamp_x(center.x - w / 2), work.top + m) // üstte
    } else if anchor.right <= work.left {
        (work.left + m, clamp_y(center.y - h / 2)) // solda
    } else if anchor.left >= work.right {
        (work.right - w - m, clamp_y(center.y - h / 2)) // sağda
    } else {
        (clamp_x(center.x - w / 2), clamp_y(center.y - h - m)) // taşma alanı / bilinmiyor
    };
    Placement { x, y, width: w, height: h, scale }
}
