// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pencere sertleştirme (UI_SPEC §4, §5). Arayüz çatısının penceresine alt sınıf eklenir:
//! - Flyout: sürüklenemez ve boyutlanamaz, sistem menüsü yok, görev çubuğunda ve Alt+Tab'da
//!   görünmez, en üstte; odak kaybında ve Esc ile kapatma isteği (ertelenmiş).
//! - Ana pencere: büyütülemez.
//!
//! Çatı stili sonradan geri yazabildiği için kurallar `WM_STYLECHANGING`'de her değişiklikte
//! yeniden uygulanır. Alt sınıf `GWLP_WNDPROC` zinciriyle kurulur: `SetWindowSubclass`
//! comctl32 v5.82'de yalnızca sıra numarasıyla dışa aktarılır ve bildirimi olmayan ikililer
//! (testler, servis) adla bağlanamaz.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::sync::OnceLock;

use windows::Win32::Foundation::{
    GetLastError, HANDLE, HWND, LPARAM, LRESULT, SetLastError, WIN32_ERROR, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::StationsAndDesktops::{
    GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, CWPRETSTRUCT, CallNextHookEx, CallWindowProcW,
    DefWindowProcW, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC, GetAncestor, GetWindowLongPtrW,
    GetWindowThreadProcessId, HHOOK, HTCAPTION, HTCLIENT, HWND_TOPMOST, IsIconic, IsWindow,
    PostMessageW, RegisterWindowMessageW, SC_KEYMENU, SC_MAXIMIZE, SC_MOVE, SC_SIZE, STYLESTRUCT,
    SW_RESTORE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, ShowWindow,
    UnhookWindowsHookEx, WA_INACTIVE, WH_CALLWNDPROCRET, WINDOWPOS, WM_ACTIVATE, WM_KEYDOWN,
    WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_SHOWWINDOW, WM_STYLECHANGING, WM_SYSCOMMAND,
    WM_WINDOWPOSCHANGING, WNDPROC, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_MAXIMIZEBOX, WS_SYSMENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON,
    SendMessageW, WM_SETICON,
};
use windows::core::{PCWSTR, w};

use crate::pipe::os_error;

const WSF_VISIBLE: u32 = 1;

/// Sertleştirilen pencerenin türü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Flyout,
    Main,
}

type CloseFn = Box<dyn FnMut()>;

struct Hardened {
    kind: Cell<WindowKind>,
    /// Zincirdeki önceki pencere işleyicisi.
    prev: WNDPROC,
    on_close: RefCell<Option<CloseFn>>,
    close_posted: Cell<bool>,
}

thread_local! {
    static PENDING: RefCell<Option<(WindowKind, CloseFn)>> = const { RefCell::new(None) };
    static HOOK: Cell<Option<HHOOK>> = const { Cell::new(None) };
    /// `arm_harden` ile en son yakalanıp sertleştirilen pencere.
    static LAST_HARDENED: Cell<Option<isize>> = const { Cell::new(None) };
    /// Bu iş parçacığında sertleştirilmiş pencereler (HWND → durum).
    static HARDENED: RefCell<HashMap<isize, Rc<Hardened>>> = RefCell::new(HashMap::new());
}

/// Ertelenmiş kapatma isteğinin kayıtlı iletisi (çatının kendi WM_APP iletileriyle çakışmaz).
fn close_message() -> u32 {
    static MSG: OnceLock<u32> = OnceLock::new();
    // SAFETY: Sabit adla kayıtlı ileti.
    *MSG.get_or_init(|| unsafe { RegisterWindowMessageW(w!("AlterDNS.Harden.Close.v1")) })
}

fn hwnd_of(raw: isize) -> HWND {
    HWND(raw as *mut core::ffi::c_void)
}

fn state_of(hwnd: HWND) -> Option<Rc<Hardened>> {
    HARDENED.with(|m| m.try_borrow().ok()?.get(&(hwnd.0 as isize)).cloned())
}

/// Sonraki görünür üst düzey pencereyi (bu iş parçacığında) gösterilmeden önce yakalar ve
/// sertleştirir: `WH_CALLWNDPROCRET` ile `WM_SHOWWINDOW`. Yakalanınca kanca kaldırılır. Yeniden
/// çağrılırsa bekleyen istek değişir.
pub fn arm_harden(kind: WindowKind, on_close_request: Box<dyn FnMut()>) -> io::Result<()> {
    PENDING.with(|p| *p.borrow_mut() = Some((kind, on_close_request)));
    if HOOK.with(Cell::get).is_none() {
        // SAFETY: Yalnızca bu iş parçacığının iletileri için kanca; modül tanıtıcısı gerekmez.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CALLWNDPROCRET,
                Some(show_hook),
                None,
                GetCurrentThreadId(),
            )
        };
        match hook {
            Ok(h) => HOOK.with(|c| c.set(Some(h))),
            Err(e) => {
                PENDING.with(|p| p.borrow_mut().take());
                return Err(os_error(e));
            }
        }
    }
    Ok(())
}

unsafe extern "system" fn show_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut captured = false;
    if code >= 0 && lp.0 != 0 {
        // SAFETY: WH_CALLWNDPROCRET için lParam bir CWPRETSTRUCT işaretçisidir.
        let cwp = unsafe { &*(lp.0 as *const CWPRETSTRUCT) };
        // SAFETY: Bu iş parçacığına ait HWND üzerinde salt okunur sorgu.
        let top_level = unsafe { GetAncestor(cwp.hwnd, GA_ROOT) } == cwp.hwnd;
        if cwp.message == WM_SHOWWINDOW && cwp.wParam.0 != 0 && top_level {
            let pending = PENDING.with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.take()));
            if let Some((kind, on_close)) = pending {
                captured = true;
                // Başarısızsa pencere sertleşmez; çağıran `ex_style` ile denetleyebilir.
                if harden_with(cwp.hwnd, kind, Some(on_close)).is_ok() {
                    LAST_HARDENED.with(|c| c.set(Some(cwp.hwnd.0 as isize)));
                }
            }
        }
    }
    // SAFETY: Zincirdeki sonraki kancaya aynı parametreler aktarılır.
    let result = unsafe { CallNextHookEx(None, code, wp, lp) };
    if captured && let Some(hook) = HOOK.with(Cell::take) {
        // SAFETY: Bu iş parçacığında kurulan kanca bir kez kaldırılır.
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
    result
}

/// Bu iş parçacığında `arm_harden` ile en son sertleştirilen ve hâlâ var olan pencere.
pub fn hardened_window() -> Option<isize> {
    let raw = LAST_HARDENED.with(Cell::get)?;
    // SAFETY: Salt okunur geçerlilik sorgusu.
    unsafe { IsWindow(Some(hwnd_of(raw))) }
        .as_bool()
        .then_some(raw)
}

/// Pencereyi doğrudan sertleştirir (kapatma isteği geri çağrısı olmadan).
pub fn harden(hwnd: isize, kind: WindowKind) -> io::Result<()> {
    harden_with(hwnd_of(hwnd), kind, None)
}

fn set_dwm<T>(hwnd: HWND, attr: DWMWINDOWATTRIBUTE, value: &T) {
    // SAFETY: Değer çağrı boyunca canlı ve boyutu doğru. Eski Windows'ta desteklenmeyen öznitelik
    // hatası yok sayılır (yalnızca görünüm).
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attr,
            std::ptr::from_ref(value).cast(),
            std::mem::size_of::<T>() as u32,
        );
    }
}

/// Alt sınıfı kurar ya da (zaten kuruluysa) türü ve geri çağrıyı günceller.
fn subclass(hwnd: HWND, kind: WindowKind, on_close: Option<CloseFn>) -> io::Result<()> {
    if let Some(state) = state_of(hwnd) {
        state.kind.set(kind);
        *state.on_close.borrow_mut() = on_close;
        return Ok(());
    }
    let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = harden_proc;
    // SAFETY: Pencere bu iş parçacığına ait (çağıran denetler). Önceki işleyici değeri bir
    // WNDPROC'tur; sıfır dönüşü son hatayla ayırt edilir.
    let prev = unsafe {
        SetLastError(WIN32_ERROR(0));
        let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as usize as isize);
        if prev == 0 && GetLastError() != WIN32_ERROR(0) {
            return Err(io::Error::last_os_error());
        }
        std::mem::transmute::<isize, WNDPROC>(prev)
    };
    let state = Rc::new(Hardened {
        kind: Cell::new(kind),
        prev,
        on_close: RefCell::new(on_close),
        close_posted: Cell::new(false),
    });
    HARDENED.with(|m| m.borrow_mut().insert(hwnd.0 as isize, state));
    Ok(())
}

fn harden_with(hwnd: HWND, kind: WindowKind, on_close: Option<CloseFn>) -> io::Result<()> {
    // SAFETY: Salt okunur sorgular.
    let (exists, owner) = unsafe {
        (
            IsWindow(Some(hwnd)).as_bool(),
            GetWindowThreadProcessId(hwnd, None),
        )
    };
    if !exists {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "pencere yok"));
    }
    // SAFETY: Parametresiz sorgu.
    if owner != unsafe { GetCurrentThreadId() } {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pencere bu iş parçacığına ait değil",
        ));
    }
    subclass(hwnd, kind, on_close)?;
    // Stil yeniden yazılır; değişiklik alt sınıftaki WM_STYLECHANGING kurallarından geçer.
    // SAFETY: Bu iş parçacığının penceresinde stil okuma/yazma ve konum güncellemesi.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        let (insert_after, zorder) = match kind {
            WindowKind::Flyout => (Some(HWND_TOPMOST), Default::default()),
            WindowKind::Main => (None, SWP_NOZORDER),
        };
        SetWindowPos(
            hwnd,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED | zorder,
        )
        .map_err(os_error)?;
    }
    match kind {
        WindowKind::Flyout => {
            set_dwm(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND);
            set_dwm(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_TRANSIENTWINDOW);
        }
        WindowKind::Main => set_exe_icons(hwnd),
    }
    Ok(())
}

/// Başlık çubuğu, görev çubuğu ve Alt+Tab simgeleri: exe'nin çok boyutlu simgesinden (kaynak
/// kimliği 1) pencerenin DPI'ına uygun boyutta yüklenir; böylece Windows tek bir büyük
/// görseli küçültmez. Simge yoksa (ör. test ikilisi) bir şey yapılmaz. Simgeler işlem
/// ömrünce kullanılır ve bırakılmaz.
fn set_exe_icons(hwnd: HWND) {
    // SAFETY: Modül tanıtıcısı bu işleme ait; kaynak kimliği tamsayı olarak verilir
    // (MAKEINTRESOURCE); ileti bu iş parçacığının penceresine gönderilir.
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let dpi = GetDpiForWindow(hwnd);
        for (which, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let size = GetSystemMetricsForDpi(metric, dpi);
            let icon = LoadImageW(
                Some(module.into()),
                // MAKEINTRESOURCE(1): işaretçi değil, kaynak kimliği.
                PCWSTR(std::ptr::without_provenance(1)),
                IMAGE_ICON,
                size,
                size,
                LR_DEFAULTCOLOR,
            );
            if let Ok(icon) = icon {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(which as usize)),
                    Some(LPARAM(icon.0 as isize)),
                );
            }
        }
    }
}

/// Türün stil kuralları: `index` GWL_STYLE ya da GWL_EXSTYLE.
fn enforce(kind: WindowKind, index: i32, style: u32) -> u32 {
    match (kind, index) {
        (WindowKind::Flyout, i) if i == GWL_EXSTYLE.0 => {
            (style | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) & !WS_EX_APPWINDOW.0
        }
        (WindowKind::Flyout, i) if i == GWL_STYLE.0 => style & !WS_SYSMENU.0,
        (WindowKind::Main, i) if i == GWL_STYLE.0 => style & !WS_MAXIMIZEBOX.0,
        (WindowKind::Main, i) if i == GWL_EXSTYLE.0 => style & !WS_EX_TOOLWINDOW.0,
        _ => style,
    }
}

fn post_close(hwnd: HWND, state: &Hardened) {
    if state.close_posted.replace(true) {
        return;
    }
    // SAFETY: Kendi penceremize ileti kuyruklanır; işaretçi taşımaz.
    unsafe {
        let _ = PostMessageW(Some(hwnd), close_message(), WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn harden_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Durum, geri çağrı pencereyi yok etse bile işlem bitene kadar canlı kalır (Rc kopyası).
    let Some(state) = state_of(hwnd) else {
        // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    };
    // SAFETY: `prev`, alt sınıf kurulurken pencerenin işleyicisiydi; aynı parametreler aktarılır.
    let next = || unsafe { CallWindowProcW(state.prev, hwnd, msg, wp, lp) };
    let kind = state.kind.get();
    let flyout = kind == WindowKind::Flyout;
    match msg {
        WM_NCDESTROY => {
            let r = next();
            HARDENED.with(|m| {
                if let Ok(mut m) = m.try_borrow_mut() {
                    m.remove(&(hwnd.0 as isize));
                }
            });
            return r;
        }
        WM_STYLECHANGING if lp.0 != 0 => {
            // Önce çatı işlesin; kurallar en son uygulanır.
            let r = next();
            // SAFETY: WM_STYLECHANGING'in lParam'ı yazılabilir bir STYLESTRUCT işaretçisidir.
            let ss = unsafe { &mut *(lp.0 as *mut STYLESTRUCT) };
            ss.styleNew = enforce(kind, wp.0 as i32, ss.styleNew);
            return r;
        }
        WM_WINDOWPOSCHANGING if flyout && lp.0 != 0 => {
            // SAFETY: WM_WINDOWPOSCHANGING'in lParam'ı yazılabilir bir WINDOWPOS işaretçisidir.
            let pos = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
            // Sistem HWND_NOTOPMOST'u burada belirli bir pencereye çevirmiş olabilir; her z-sırası
            // değişikliği en üste yönlendirilir.
            if !pos.flags.contains(SWP_NOZORDER) {
                pos.hwndInsertAfter = HWND_TOPMOST;
            }
        }
        WM_NCHITTEST if flyout => return LRESULT(HTCLIENT as isize),
        WM_SYSCOMMAND => {
            let cmd = (wp.0 & 0xFFF0) as u32;
            let blocked = if flyout {
                cmd == SC_MOVE || cmd == SC_SIZE || cmd == SC_KEYMENU
            } else {
                cmd == SC_MAXIMIZE
            };
            if blocked {
                return LRESULT(0);
            }
        }
        WM_NCLBUTTONDBLCLK if !flyout && wp.0 == HTCAPTION as usize => return LRESULT(0),
        WM_ACTIVATE if flyout && (wp.0 & 0xFFFF) as u32 == WA_INACTIVE => post_close(hwnd, &state),
        WM_KEYDOWN if flyout && wp.0 == usize::from(VK_ESCAPE.0) => {
            post_close(hwnd, &state);
            return LRESULT(0);
        }
        m if m == close_message() => {
            state.close_posted.set(false);
            let taken = state
                .on_close
                .try_borrow_mut()
                .ok()
                .and_then(|mut s| s.take());
            if let Some(mut f) = taken {
                f();
                if let Ok(mut slot) = state.on_close.try_borrow_mut()
                    && slot.is_none()
                {
                    *slot = Some(f);
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    next()
}

/// Pencerenin genişletilmiş stili (`GWL_EXSTYLE`).
pub fn ex_style(hwnd: isize) -> isize {
    // SAFETY: Salt okunur stil sorgusu; geçersiz pencerede 0 döner.
    unsafe { GetWindowLongPtrW(hwnd_of(hwnd), GWL_EXSTYLE) }
}

/// Pencereyi gösterir (simge durumundaysa geri getirir) ve öne almayı dener.
pub fn bring_to_front(hwnd: isize) {
    let hwnd = hwnd_of(hwnd);
    // SAFETY: Pencere tanıtıcısıyla gösterme ve öne alma; geçersizse çağrılar hata döner.
    unsafe {
        let cmd = if IsIconic(hwnd).as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        let _ = ShowWindow(hwnd, cmd);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Başka bir işlemin (ör. öne getirilecek ilk örnek) ön plana geçmesine izin verir.
pub fn allow_foreground_any() {
    // SAFETY: Parametresi sabit sistem çağrısı.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// İşlem görünür (etkileşimli) pencere istasyonunda mı. Görünür pencere açan testler değilse
/// atlanır; CI oturumu masaüstüsüz olabilir.
pub fn interactive_desktop() -> bool {
    let mut flags = USEROBJECTFLAGS::default();
    // SAFETY: İstasyon tanıtıcısı işleme aittir ve kapatılmaz; çıktı yerel yapıya yazılır.
    unsafe {
        let Ok(station) = GetProcessWindowStation() else {
            return false;
        };
        if GetUserObjectInformationW(
            HANDLE(station.0),
            UOI_FLAGS,
            Some(std::ptr::from_mut(&mut flags).cast()),
            std::mem::size_of::<USEROBJECTFLAGS>() as u32,
            None,
        )
        .is_err()
        {
            return false;
        }
    }
    flags.dwFlags & WSF_VISIBLE != 0
}
