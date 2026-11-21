// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gizli ileti penceresi ve tepsi simgesi (`Shell_NotifyIcon`, sürüm 4).
//!
//! Pencere görünmeyen bir üst düzey penceredir (ileti penceresi değil): `TaskbarCreated` ve
//! `WM_SETTINGCHANGE` yalnızca üst düzey pencerelere yayınlanır, `FindWindowW` de yalnızca
//! onları bulur. Olaylar, pencereyi oluşturan iş parçacığında `on_event`'e verilir.

use std::cell::{Cell, RefCell};
use std::io;
use std::path::Path;
use std::rc::Rc;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::UI::Shell::{
    ExtractIconExW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO,
    NIIF_RESPECT_QUIET_TIME, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
    Shell_NotifyIconGetRect, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyWindow, FindWindowW, GWLP_USERDATA, GetSystemMetrics, GetWindowLongPtrW,
    HICON, LR_DEFAULTCOLOR, MSGFLT_ALLOW, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SM_CXSMICON, SPI_SETCLIENTAREAANIMATION, SetWindowLongPtrW, WM_APP, WM_CLOSE, WM_CONTEXTMENU,
    WM_DWMCOLORIZATIONCOLORCHANGED, WM_NCDESTROY, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP, WTS_SESSION_LOCK,
};
use windows::core::{PCWSTR, w};

use crate::acl::wide;
use crate::pipe::os_error;
use crate::screen::Rect;

const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;
/// `NIN_KEYSELECT = NIN_SELECT | NINF_KEY` (shellapi.h); `windows` crate tanımlamıyor.
const NIN_KEYSELECT: u32 = NIN_SELECT | 0x1;
/// Uygulama içi yayın iletisinin adı (`RegisterWindowMessageW`).
const BROADCAST_MESSAGE: PCWSTR = w!("AlterDNS.Broadcast.v1");
/// `CreateIconFromResourceEx` sürüm değeri (Windows 3.x ve sonrası biçim).
const ICON_VERSION: u32 = 0x0003_0000;

/// Pencereye gelen kabuk ve sistem olayları.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    /// Tepsi simgesine tıklandı (`NIN_SELECT`).
    Activate,
    /// Sağ tık ya da Shift+F10 (`WM_CONTEXTMENU`).
    ContextMenu,
    /// Klavyeyle seçildi (`NIN_KEYSELECT`).
    Keyboard,
    /// Explorer yeniden başladı; simge yeniden eklendi.
    TaskbarRecreated,
    /// Oturum kilitlendi.
    SessionLocked,
    /// Uygulamanın kayıtlı iletisi (`post_broadcast`).
    Broadcast(u32),
    /// Renk teması, vurgu rengi ya da animasyon ayarı değişti.
    SettingChanged,
}

type EventFn = Box<dyn FnMut(ShellEvent)>;

#[derive(Clone, Copy)]
struct TrayState {
    icon: HICON,
    tip: [u16; 128],
    shown: bool,
}

struct Inner {
    hwnd: Cell<HWND>,
    on_event: RefCell<Option<EventFn>>,
    tray: Cell<Option<TrayState>>,
    taskbar_msg: u32,
    broadcast_msg: u32,
}

impl Inner {
    fn dispatch(&self, event: ShellEvent) {
        // Geri çağrı dışarı alınarak çağrılır: içinden gelen yeniden giriş ödünç hatası vermez
        // (o sıradaki iç içe olaylar düşer).
        let taken = self
            .on_event
            .try_borrow_mut()
            .ok()
            .and_then(|mut s| s.take());
        if let Some(mut f) = taken {
            f(event);
            if let Ok(mut slot) = self.on_event.try_borrow_mut()
                && slot.is_none()
            {
                *slot = Some(f);
            }
        }
    }

    fn tray(&self) -> Option<TrayState> {
        self.tray.get()
    }

    fn set_shown(&self, shown: bool) {
        if let Some(mut s) = self.tray.get() {
            s.shown = shown;
            self.tray.set(Some(s));
        }
    }
}

/// Gizli üst düzey pencere. Oluşturan iş parçacığında kullanılır ve orada düşürülmelidir.
pub struct MessageWindow {
    inner: Rc<Inner>,
}

impl MessageWindow {
    /// `class` sınıfıyla gizli pencere oluşturur. Tepsi `"AlterDNS.Tray"`, ana pencere
    /// `"AlterDNS.Window"` kullanır (yayın hedefi).
    pub fn create(class: &str, on_event: Box<dyn FnMut(ShellEvent)>) -> io::Result<MessageWindow> {
        if class.is_empty() || class.len() > 200 || class.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "geçersiz pencere sınıfı",
            ));
        }
        let class_w = wide(class.as_ref());
        // SAFETY: Kayıtlı ileti adları sabit geniş dizeler.
        let (taskbar_msg, broadcast_msg) = unsafe {
            (
                RegisterWindowMessageW(w!("TaskbarCreated")),
                RegisterWindowMessageW(BROADCAST_MESSAGE),
            )
        };
        if taskbar_msg == 0 || broadcast_msg == 0 {
            return Err(io::Error::last_os_error());
        }
        let inner = Rc::new(Inner {
            hwnd: Cell::new(HWND::default()),
            on_event: RefCell::new(Some(on_event)),
            tray: Cell::new(None),
            taskbar_msg,
            broadcast_msg,
        });
        // SAFETY: Sınıf adı arabelleği çağrılar boyunca canlı; sistem adı kopyalar. Pencere
        // bu iş parçacığında oluşturulur; `GWLP_USERDATA`'ya yazılan işaretçi `inner` canlı
        // kaldıkça geçerlidir ve `WM_NCDESTROY`'da silinir.
        unsafe {
            let instance = GetModuleHandleW(None).map_err(os_error)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class_w.as_ptr()),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(io::Error::last_os_error());
            }
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                PCWSTR(class_w.as_ptr()),
                PCWSTR(class_w.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(os_error)?;
            inner.hwnd.set(hwnd);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&inner) as isize);
            // Yükseltilmiş çalışılırsa Explorer'ın TaskbarCreated iletisi UIPI'ye takılmasın.
            let _ = ChangeWindowMessageFilterEx(hwnd, taskbar_msg, MSGFLT_ALLOW, None);
            // Kilitlenme bildirimi alınamazsa yalnızca SessionLocked olayı gelmez.
            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        }
        Ok(MessageWindow { inner })
    }

    pub fn hwnd_raw(&self) -> isize {
        self.inner.hwnd.get().0 as isize
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        let hwnd = self.inner.hwnd.get();
        // SAFETY: Pencere bu nesneye ait; zaten yok edildiyse çağrılar hata döner ve yok sayılır.
        unsafe {
            let _ = WTSUnRegisterSessionNotification(hwnd);
            let _ = DestroyWindow(hwnd);
        }
    }
}

fn setting_name_is(lp: LPARAM, name: &str) -> bool {
    if lp.0 == 0 {
        return false;
    }
    let ptr = lp.0 as *const u16;
    let want: Vec<u16> = name.encode_utf16().collect();
    // SAFETY: WM_SETTINGCHANGE'in lParam'ı sıfır değilse sistemin sıralayıp ilettiği, sıfırla
    // biten bir dizedir. Okuma ilk farklı karakterde ya da sonlandırıcıda durur.
    unsafe {
        for (i, &c) in want.iter().enumerate() {
            if *ptr.add(i) != c {
                return false;
            }
        }
        *ptr.add(want.len()) == 0
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: Değer `create`'te yazılan `Rc<Inner>` işaretçisi ya da sıfırdır.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Inner;
    if ptr.is_null() {
        // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    if msg == WM_NCDESTROY {
        // SAFETY: Pencere yok ediliyor; işaretçi bir daha kullanılmasın.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
    }
    // Geri çağrı `MessageWindow`'u düşürebilir; işlem bitene kadar `Inner` canlı tutulur.
    // SAFETY: İşaretçi canlı bir `Rc<Inner>`'dan gelir (pencere varken `MessageWindow` vardır).
    let inner = unsafe {
        Rc::increment_strong_count(ptr);
        Rc::from_raw(ptr)
    };
    if msg == inner.broadcast_msg {
        inner.dispatch(ShellEvent::Broadcast(wp.0 as u32));
        return LRESULT(0);
    }
    if msg == inner.taskbar_msg {
        if inner.tray().is_some() {
            let shown = notify_add(&inner);
            inner.set_shown(shown);
            inner.dispatch(ShellEvent::TaskbarRecreated);
        }
        return LRESULT(0);
    }
    match msg {
        WM_TRAY => {
            match (lp.0 & 0xFFFF) as u32 {
                NIN_SELECT => inner.dispatch(ShellEvent::Activate),
                NIN_KEYSELECT => inner.dispatch(ShellEvent::Keyboard),
                WM_CONTEXTMENU => inner.dispatch(ShellEvent::ContextMenu),
                _ => {}
            }
            LRESULT(0)
        }
        WM_WTSSESSION_CHANGE => {
            if wp.0 == WTS_SESSION_LOCK as usize {
                inner.dispatch(ShellEvent::SessionLocked);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if wp.0 == SPI_SETCLIENTAREAANIMATION.0 as usize
                || setting_name_is(lp, "ImmersiveColorSet")
            {
                inner.dispatch(ShellEvent::SettingChanged);
            }
            LRESULT(0)
        }
        WM_DWMCOLORIZATIONCOLORCHANGED => {
            inner.dispatch(ShellEvent::SettingChanged);
            LRESULT(0)
        }
        // Gizli pencere dışarıdan kapatılamaz; ömrü `MessageWindow`'a bağlıdır.
        WM_CLOSE => LRESULT(0),
        // SAFETY: Varsayılan işleyiciye aynı parametreler aktarılır.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
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

fn shell_notify(message: NOTIFY_ICON_MESSAGE, nid: &NOTIFYICONDATAW) -> bool {
    // SAFETY: Yapı yerel ve doğru boyutlu.
    unsafe { Shell_NotifyIconW(message, nid) }.as_bool()
}

/// Simgeyi (yeniden) ekler. RefCell ödünç alınmadan çağrılır: `Shell_NotifyIconW` beklerken
/// gönderilen iletiler aynı pencere işleyicisine girebilir.
fn notify_add(inner: &Inner) -> bool {
    let Some(state) = inner.tray() else {
        return false;
    };
    let hwnd = inner.hwnd.get();
    let _ = shell_notify(NIM_DELETE, &notify_data(hwnd));
    let mut nid = notify_data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = state.icon;
    nid.szTip = state.tip;
    if !shell_notify(NIM_ADD, &nid) {
        return false;
    }
    nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    shell_notify(NIM_SETVERSION, &nid)
}

/// UTF-16'ya çevirip `N - 1` birime keser (vekil çifti bölünmez) ve sıfırla bitirir.
fn fixed<const N: usize>(text: &str) -> [u16; N] {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > N - 1 {
        units.truncate(N - 1);
        if units.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
            units.pop();
        }
    }
    let mut out = [0u16; N];
    out[..units.len()].copy_from_slice(&units);
    out
}

/// ICO kapsayıcısından istenen boyuta en uygun girdi: istenen boyuttan küçük olmayanların en
/// küçüğü, yoksa en büyüğü. Dönen: (konum, uzunluk). `LookupIconIdFromDirectoryEx` kaynak
/// biçimindeki dizini (GRPICONDIR) bekler; `.ico` dosya biçimi burada ayrıştırılır.
fn pick_entry(ico: &[u8], want: u32) -> Option<(usize, usize)> {
    if ico.get(..4)? != [0, 0, 1, 0] {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]));
    let mut best: Option<(u32, usize, usize)> = None;
    for i in 0..count {
        let e = ico.get(6 + i * 16..6 + i * 16 + 16)?;
        let size = if e[0] == 0 { 256 } else { u32::from(e[0]) };
        let len = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        if off.checked_add(len)? > ico.len() || len == 0 {
            return None;
        }
        let better = match best {
            None => true,
            Some((b, ..)) if b >= want => size >= want && size < b,
            Some((b, ..)) => size > b,
        };
        if better {
            best = Some((size, off, len));
        }
    }
    best.map(|(_, off, len)| (off, len))
}

fn load_icon(ico: &[u8]) -> io::Result<HICON> {
    // SAFETY: Parametresiz sistem ölçüsü okuması.
    let want = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16);
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "geçersiz ICO");
    let (off, len) = pick_entry(ico, want as u32).ok_or_else(invalid)?;
    let image = ico.get(off..off + len).ok_or_else(invalid)?;
    // SAFETY: Dilim ICO girdisinin görüntü verisidir (PNG ya da DIB); API yalnızca okur.
    unsafe { CreateIconFromResourceEx(image, true, ICON_VERSION, want, want, LR_DEFAULTCOLOR) }
        .map_err(|_| invalid())
}

fn destroy_icon(icon: HICON) {
    // SAFETY: Simge `load_icon` ile bu işlemde oluşturuldu ve artık kullanılmıyor.
    unsafe {
        let _ = DestroyIcon(icon);
    }
}

/// Bir `MessageWindow`'a bağlı tepsi simgesi (kimlik 1). Düşünce simge kaldırılır.
pub struct TrayIcon {
    inner: Rc<Inner>,
}

impl TrayIcon {
    /// Simgeyi ekler. Görev çubuğu henüz yoksa simge `TaskbarCreated` ile eklenir
    /// (`is_shown` yanlış döner). ICO geçersizse `InvalidData`.
    pub fn add(window: &MessageWindow, icon_ico: &[u8], tooltip: &str) -> io::Result<TrayIcon> {
        let inner = window.inner.clone();
        if inner.tray().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "pencerede zaten tepsi simgesi var",
            ));
        }
        let icon = load_icon(icon_ico)?;
        inner.tray.set(Some(TrayState {
            icon,
            tip: fixed(tooltip),
            shown: false,
        }));
        let shown = notify_add(&inner);
        inner.set_shown(shown);
        Ok(TrayIcon { inner })
    }

    /// Simge şu anda görev çubuğunda mı.
    pub fn is_shown(&self) -> bool {
        self.inner.tray().is_some_and(|s| s.shown)
    }

    fn modify(&self, fill: impl FnOnce(&mut NOTIFYICONDATAW)) -> io::Result<()> {
        if !self.is_shown() {
            // Görev çubuğu yeniden oluşunca güncel durumla eklenir.
            return Ok(());
        }
        let mut nid = notify_data(self.inner.hwnd.get());
        fill(&mut nid);
        if shell_notify(NIM_MODIFY, &nid) {
            Ok(())
        } else {
            Err(io::Error::other("Shell_NotifyIcon(NIM_MODIFY) başarısız"))
        }
    }

    pub fn set_icon(&self, icon_ico: &[u8]) -> io::Result<()> {
        let icon = load_icon(icon_ico)?;
        let Some(mut state) = self.inner.tray() else {
            destroy_icon(icon);
            return Err(io::Error::other("tepsi simgesi yok"));
        };
        let old = state.icon;
        state.icon = icon;
        self.inner.tray.set(Some(state));
        let result = self.modify(|nid| {
            nid.uFlags = NIF_ICON;
            nid.hIcon = icon;
        });
        destroy_icon(old);
        result
    }

    /// İpucu metni; 127 UTF-16 birimine kesilir.
    pub fn set_tooltip(&self, text: &str) -> io::Result<()> {
        let tip = fixed(text);
        let Some(mut state) = self.inner.tray() else {
            return Err(io::Error::other("tepsi simgesi yok"));
        };
        state.tip = tip;
        self.inner.tray.set(Some(state));
        self.modify(|nid| {
            nid.uFlags = NIF_TIP | NIF_SHOWTIP;
            nid.szTip = tip;
        })
    }

    /// Bildirim balonu (`NIF_INFO`); sessiz saatlere uyar. Simge görünmüyorsa hata.
    pub fn balloon(&self, title: &str, text: &str) -> io::Result<()> {
        if !self.is_shown() {
            return Err(io::Error::other("tepsi simgesi görünmüyor"));
        }
        self.modify(|nid| {
            nid.uFlags = NIF_INFO;
            nid.szInfoTitle = fixed(title);
            nid.szInfo = fixed(text);
            nid.dwInfoFlags = NIIF_INFO | NIIF_RESPECT_QUIET_TIME;
        })
    }

    /// Simgenin ekran dikdörtgeni (`Shell_NotifyIconGetRect`); alınamazsa `None`.
    pub fn anchor(&self) -> Option<Rect> {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.inner.hwnd.get(),
            uID: TRAY_ID,
            ..Default::default()
        };
        // SAFETY: Yapı yerel ve doğru boyutlu.
        let r = unsafe { Shell_NotifyIconGetRect(&id) }.ok()?;
        Some(Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        })
    }
}

impl std::fmt::Debug for TrayIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayIcon")
            .field("hwnd", &self.inner.hwnd.get().0)
            .field("shown", &self.is_shown())
            .finish()
    }
}

impl std::fmt::Debug for MessageWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageWindow")
            .field("hwnd", &self.inner.hwnd.get().0)
            .finish()
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        if let Some(state) = self.inner.tray.take() {
            let _ = shell_notify(NIM_DELETE, &notify_data(self.inner.hwnd.get()));
            destroy_icon(state.icon);
        }
    }
}

/// O sınıftaki pencereye uygulamanın kayıtlı iletisini gönderir. `FindWindowW` yalnızca
/// çağıranın masaüstünde arar; başka oturumlara yayın yapılmaz. Pencere yoksa `Ok(false)`.
pub fn post_broadcast(class: &str, code: u32) -> io::Result<bool> {
    let class_w = wide(class.as_ref());
    // SAFETY: Dizeler çağrı boyunca canlı; ileti kuyruğa alınır, işaretçi taşımaz.
    unsafe {
        let msg = RegisterWindowMessageW(BROADCAST_MESSAGE);
        if msg == 0 {
            return Err(io::Error::last_os_error());
        }
        let Ok(hwnd) = FindWindowW(PCWSTR(class_w.as_ptr()), PCWSTR::null()) else {
            return Ok(false);
        };
        PostMessageW(Some(hwnd), msg, WPARAM(code as usize), LPARAM(0)).map_err(os_error)?;
    }
    Ok(true)
}

/// Explorer'ın görev çubuğu bu masaüstünde var mı (`Shell_TrayWnd`).
pub fn taskbar_present() -> bool {
    // SAFETY: Sabit sınıf adıyla salt okuma araması.
    unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.is_ok()
}

/// Exe'deki simge sayısı (`ExtractIconExW`, -1); okunamazsa 0.
pub fn icon_count(exe: &Path) -> u32 {
    let path = wide(exe.as_os_str());
    // SAFETY: Yol arabelleği çağrı boyunca canlı; -1 ile yalnızca sayı döner, simge oluşturulmaz.
    unsafe { ExtractIconExW(PCWSTR(path.as_ptr()), -1, None, None, 0) }
}

#[cfg(test)]
mod tests {
    use super::{fixed, pick_entry};

    fn ico(sizes: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 1, 0, sizes.len() as u8, 0];
        let mut off = 6 + 16 * sizes.len();
        for &s in sizes {
            out.extend([s, s, 0, 0, 1, 0, 32, 0]);
            out.extend(4u32.to_le_bytes());
            out.extend((off as u32).to_le_bytes());
            off += 4;
        }
        for &s in sizes {
            out.extend([s; 4]);
        }
        out
    }

    #[test]
    fn pick_entry_prefers_smallest_not_below_wanted() {
        let data = ico(&[16, 20, 24, 32]);
        let size_at = |w| pick_entry(&data, w).map(|(off, _)| data[off]);
        assert_eq!(size_at(16), Some(16));
        assert_eq!(size_at(18), Some(20));
        assert_eq!(size_at(24), Some(24));
        assert_eq!(size_at(40), Some(32));
        assert_eq!(pick_entry(b"junk", 16), None);
        let mut truncated = data.clone();
        truncated.truncate(data.len() - 2);
        assert_eq!(pick_entry(&truncated, 16), None);
    }

    #[test]
    fn fixed_truncates_without_splitting_surrogates() {
        let t: [u16; 4] = fixed("ab😀");
        assert_eq!(t, [u16::from(b'a'), u16::from(b'b'), 0, 0]);
        let t: [u16; 4] = fixed("abc");
        assert_eq!(&t[..3], &[97, 98, 99]);
        assert_eq!(t[3], 0);
    }
}
