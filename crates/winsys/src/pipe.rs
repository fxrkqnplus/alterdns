// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Named pipe sunucusu ve istemcisi (IPC; K10, K21).
//!
//! Bütün G/Ç örtüşmelidir. Okuma, yazma ve bağlantı bekleme zaman aşımı ya da durdurma olayıyla
//! kesilebilir. Kesilen işlemin tamamlanması her zaman beklenir: arabellek ve `OVERLAPPED` o ana
//! kadar yaşar.
//!
//! Sunucu bir istemci bağlanınca sonraki dinleyen örneği hemen açar. Böylece sunucu çalıştığı
//! sürece pipe adı boşa çıkmaz ve başka bir işlem aynı adı alamaz.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING,
    ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ,
    HANDLE, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
use windows::Win32::Security::{
    EqualSid, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, RevertToSelf,
    SECURITY_ATTRIBUTES, SID_AND_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_GROUPS, TOKEN_OWNER,
    TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenGroups, TokenOwner, TokenSessionId, TokenUser,
    WinInteractiveSid, WinLocalSystemSid,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE,
    FILE_WRITE_DATA, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, GetCurrentThread, INFINITE, OpenProcess, OpenThreadToken,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

use crate::acl::{LocalSd, sd_from_sddl, wide};
use crate::token::{Token, process_token, sid_string, token_info};

/// `SE_GROUP_ENABLED` ve `SE_GROUP_USE_FOR_DENY_ONLY` (winnt.h).
const SE_GROUP_ENABLED: u32 = 0x4;
const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;
const PIPE_BUFFER: u32 = 64 * 1024;

/// Sahip olunan çekirdek tutamacı; düşünce bir kez kapatılır.
struct OwnedHandle(HANDLE);

// SAFETY: Çekirdek tutamaçları her iş parçacığından kullanılabilir; tutamaç yalnızca `Drop`'ta
// bir kez kapatılır.
unsafe impl Send for OwnedHandle {}
// SAFETY: Paylaşılan kullanım yalnızca iş parçacığı güvenli Win32 çağrılarıyla yapılır.
unsafe impl Sync for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0.0.is_null() {
            // SAFETY: Tutamaç bu nesneye aittir ve başka yerde kapatılmaz.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn win32(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(code.0 as i32)
}

fn is(e: &windows::core::Error, code: WIN32_ERROR) -> bool {
    e.code() == code.to_hresult()
}

fn os_error(e: windows::core::Error) -> io::Error {
    // HRESULT_FROM_WIN32 ise özgün Win32 kodu korunur (ErrorKind eşlemesi için).
    let hr = e.code().0.cast_unsigned();
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
    } else {
        io::Error::other(e)
    }
}

fn new_event() -> io::Result<OwnedHandle> {
    // SAFETY: Adsız, elle sıfırlanan, başlangıçta işaretsiz bir olay; tutamaç sahiplenilir.
    let h = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(os_error)?;
    Ok(OwnedHandle(h))
}

fn millis(d: Duration) -> u32 {
    u32::try_from(d.as_millis()).unwrap_or(INFINITE - 1)
}

/// Bekleyen bir G/Ç işlemini bitirir. Zaman aşımı ya da durdurmada işlem iptal edilir ve
/// iptalin tamamlanması beklenir; `ov` ve arabellek ancak bundan sonra serbest kalabilir.
fn wait_io(
    h: HANDLE,
    ov: &OVERLAPPED,
    timeout: Option<Duration>,
    stop: Option<HANDLE>,
) -> io::Result<u32> {
    let mut handles = vec![ov.hEvent];
    if let Some(s) = stop {
        handles.push(s);
    }
    let ms = timeout.map_or(INFINITE, millis);
    // SAFETY: Tutamaçlar geçerli olaylardır ve çağrı boyunca yaşar.
    let w: WAIT_EVENT = unsafe { WaitForMultipleObjects(&handles, false, ms) };
    let completed = w == WAIT_OBJECT_0;
    if !completed {
        // SAFETY: `ov` bu tutamaçta bekleyen işlemindir.
        unsafe {
            let _ = CancelIoEx(h, Some(ov));
        }
    }
    let mut n = 0u32;
    // SAFETY: `ov` bu tutamaçtaki işlemindir; `bWait = true` iptal dahil tamamlanmayı bekler.
    let r = unsafe { GetOverlappedResult(h, ov, &mut n, true) };
    match (completed, r) {
        (_, Ok(())) => Ok(n),
        (true, Err(e)) => Err(os_error(e)),
        (false, Err(_)) if w == WAIT_TIMEOUT => {
            Err(io::Error::new(io::ErrorKind::TimedOut, "pipe zaman aşımı"))
        }
        (false, Err(_)) if handles.len() == 2 && w.0 == WAIT_OBJECT_0.0 + 1 => {
            Err(io::Error::new(io::ErrorKind::Interrupted, "durduruldu"))
        }
        (false, Err(_)) => Err(io::Error::last_os_error()),
    }
}

/// Dinleyiciyi durdurur; bekleyen `accept` `Ok(None)` döner.
#[derive(Clone)]
pub struct Stopper(Arc<OwnedHandle>);

impl Stopper {
    pub fn stop(&self) {
        // SAFETY: Olay tutamacı geçerlidir ve `Arc` ile yaşar.
        unsafe {
            let _ = SetEvent(self.0.0);
        }
    }
}

pub struct PipeListener {
    name: Vec<u16>,
    sddl: String,
    max_instances: u32,
    pending: Mutex<Option<OwnedHandle>>,
    stop: Arc<OwnedHandle>,
}

impl PipeListener {
    /// İlk örneği `FILE_FLAG_FIRST_PIPE_INSTANCE` ile açar. Ad başka bir işlemdeyse
    /// `ErrorKind::AddrInUse` döner (pipe squatting).
    pub fn bind(name: &str, sddl: &str, max_instances: u32) -> io::Result<PipeListener> {
        if !name.to_ascii_lowercase().starts_with(r"\\.\pipe\") || max_instances == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("geçersiz pipe: {name}"),
            ));
        }
        let mut l = PipeListener {
            name: wide(std::ffi::OsStr::new(name)),
            sddl: sddl.to_string(),
            max_instances,
            pending: Mutex::new(None),
            stop: Arc::new(new_event()?),
        };
        let first = l.create(true).map_err(|e| match e.raw_os_error() {
            Some(5 | 231) => io::Error::new(io::ErrorKind::AddrInUse, "pipe adı kullanımda"),
            _ => e,
        })?;
        l.pending = Mutex::new(Some(first));
        Ok(l)
    }

    pub fn stopper(&self) -> Stopper {
        Stopper(self.stop.clone())
    }

    fn create(&self, first: bool) -> io::Result<OwnedHandle> {
        let sd: LocalSd = sd_from_sddl(&self.sddl)?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: sd.0.0,
            bInheritHandle: false.into(),
        };
        let mut open = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if first {
            open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: Ad NUL ile biten UTF-16'dır; güvenlik öznitelikleri çağrı boyunca geçerlidir.
        let h = unsafe {
            CreateNamedPipeW(
                PCWSTR(self.name.as_ptr()),
                open,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                self.max_instances,
                PIPE_BUFFER,
                PIPE_BUFFER,
                0,
                Some(&sa),
            )
        };
        if h.is_invalid() {
            let e = io::Error::last_os_error();
            return Err(match e.raw_os_error() {
                Some(c) if !first && c == ERROR_PIPE_BUSY.0 as i32 => {
                    io::Error::new(io::ErrorKind::WouldBlock, "bütün örnekler kullanımda")
                }
                Some(c) if !first && c == ERROR_ACCESS_DENIED.0 as i32 => io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "yeni pipe örneği açma izni yok",
                ),
                _ => e,
            });
        }
        Ok(OwnedHandle(h))
    }

    fn stopped(&self, wait: Duration) -> bool {
        // SAFETY: Olay tutamacı geçerlidir.
        unsafe { WaitForSingleObject(self.stop.0, millis(wait)) == WAIT_OBJECT_0 }
    }

    /// Bir istemciyi bekler. Durdurulursa `Ok(None)`.
    pub fn accept(&self) -> io::Result<Option<PipeConn>> {
        loop {
            let taken = self
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let h = match taken {
                Some(h) => h,
                None => loop {
                    match self.create(false) {
                        Ok(h) => break h,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            if self.stopped(Duration::from_millis(50)) {
                                return Ok(None);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                },
            };
            let ev = new_event()?;
            let ov = OVERLAPPED {
                hEvent: ev.0,
                ..Default::default()
            };
            // SAFETY: `ov` ve olayı, bekleme tamamlanana kadar bu çerçevede yaşar.
            let r = unsafe { ConnectNamedPipe(h.0, Some(std::ptr::from_ref(&ov).cast_mut())) };
            let connected = match r {
                Ok(()) => true,
                Err(e) if is(&e, ERROR_PIPE_CONNECTED) => true,
                Err(e) if is(&e, ERROR_IO_PENDING) => {
                    match wait_io(h.0, &ov, None, Some(self.stop.0)) {
                        Ok(_) => true,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => return Ok(None),
                        // İstemci bağlanıp hemen ayrıldı: yeni örnekle sürdür.
                        Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA.0 as i32) => false,
                        Err(e) => return Err(e),
                    }
                }
                Err(e) if is(&e, ERROR_NO_DATA) => false,
                Err(e) => return Err(os_error(e)),
            };
            if !connected {
                continue;
            }
            // Ad boşa çıkmasın: sonraki dinleyen örnek bağlantı verilmeden açılır. Açılamazsa
            // (ör. bütün örnekler dolu) bir sonraki `accept` yeniden dener.
            if let Ok(next) = self.create(false) {
                *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(next);
            }
            return Ok(Some(PipeConn {
                h,
                server: true,
                process: Mutex::new(None),
            }));
        }
    }
}

/// Çağıranın belirtecinden okunan kimlik. `image_path` ek bir katmandır; tek başına güvenlik
/// sınırı değildir (K10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallerToken {
    pub sid: String,
    pub session_id: u32,
    pub elevated: bool,
    pub interactive: bool,
    pub image_path: Option<PathBuf>,
}

/// İstemcinin beklediği pipe sahibi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipeOwner {
    /// Ürün: sunucu LocalSystem olarak çalışan servistir.
    System,
    /// Yalnızca testler: bu işlemin belirtecindeki varsayılan sahip (yükseltilmişte
    /// Administrators).
    ProcessDefaultOwner,
}

pub struct PipeConn {
    h: OwnedHandle,
    server: bool,
    /// İstemci işleminin tutamacı; bağlantı boyunca tutulur (PID yeniden kullanımı, K10).
    process: Mutex<Option<OwnedHandle>>,
}

/// Kimliğe bürünmeyi her yolda geri alır.
struct RevertGuard;

impl Drop for RevertGuard {
    fn drop(&mut self) {
        // SAFETY: Bu iş parçacığı kimliğe bürünmüştür; geri dönüş her durumda güvenlidir.
        unsafe {
            let _ = RevertToSelf();
        }
    }
}

fn caller_token(token: HANDLE) -> io::Result<(String, u32, bool, bool)> {
    let user = token_info(token, TokenUser)?;
    // SAFETY: Arabellek `TOKEN_USER` ile başlar ve hizalıdır; SID arabelleğin içindedir.
    let sid = sid_string(unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid })?;
    let session = token_info(token, TokenSessionId)?;
    // SAFETY: `TokenSessionId` bir `u32` döndürür.
    let session_id = unsafe { *session.as_ptr().cast::<u32>() };
    let elevation = token_info(token, TokenElevation)?;
    // SAFETY: Arabellek bir `TOKEN_ELEVATION`'dır.
    let elevated = unsafe { (*elevation.as_ptr().cast::<TOKEN_ELEVATION>()).TokenIsElevated } != 0;
    let groups = token_info(token, TokenGroups)?;
    // SAFETY: Arabellek `TOKEN_GROUPS` ile başlar; `GroupCount` öğe, arabelleğin başından
    // türetilen işaretçiyle okunur ve hepsi arabelleğin içindedir.
    let interactive = unsafe {
        let base = groups.as_ptr().cast::<u8>();
        let count = (*base.cast::<TOKEN_GROUPS>()).GroupCount as usize;
        let first = base
            .add(std::mem::offset_of!(TOKEN_GROUPS, Groups))
            .cast::<SID_AND_ATTRIBUTES>();
        (0..count).any(|i| {
            let g = &*first.add(i);
            g.Attributes & SE_GROUP_ENABLED != 0
                && g.Attributes & SE_GROUP_USE_FOR_DENY_ONLY == 0
                && IsWellKnownSid(g.Sid, WinInteractiveSid).as_bool()
        })
    };
    Ok((sid, session_id, elevated, interactive))
}

fn image_path(process: HANDLE) -> Option<PathBuf> {
    let mut buf = vec![0u16; 32_768];
    let mut len = u32::try_from(buf.len()).unwrap_or(0);
    // SAFETY: Arabellek `len` öğe uzunluğunda ve yazılabilir; API en çok o kadar yazar.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    buf.truncate(len as usize);
    Some(PathBuf::from(OsString::from_wide(&buf)))
}

impl PipeConn {
    fn read_some(&self, buf: &mut [u8], deadline: Option<Instant>) -> io::Result<usize> {
        let ev = new_event()?;
        let ov = OVERLAPPED {
            hEvent: ev.0,
            ..Default::default()
        };
        // SAFETY: `buf` ve `ov`, `wait_io` tamamlanmayı (iptal dahil) bekleyene kadar yaşar.
        let r = unsafe {
            ReadFile(
                self.h.0,
                Some(&mut *buf),
                None,
                Some(std::ptr::from_ref(&ov).cast_mut()),
            )
        };
        let res = match r {
            Ok(()) => wait_io(self.h.0, &ov, Some(Duration::ZERO), None),
            Err(e) if is(&e, ERROR_IO_PENDING) => wait_io(
                self.h.0,
                &ov,
                deadline.map(|d| d.saturating_duration_since(Instant::now())),
                None,
            ),
            Err(e) => Err(os_error(e)),
        };
        match res {
            Ok(n) => Ok(n as usize),
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(c) if c == ERROR_BROKEN_PIPE.0 as i32 || c == ERROR_PIPE_NOT_CONNECTED.0 as i32
                ) =>
            {
                Ok(0)
            }
            Err(e) => Err(e),
        }
    }

    /// `buf`'u doldurur. Karşı taraf kapanırsa `UnexpectedEof`, süre dolarsa `TimedOut`.
    pub fn read_exact_timeout(&self, buf: &mut [u8], timeout: Option<Duration>) -> io::Result<()> {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut filled = 0;
        while filled < buf.len() {
            let n = self.read_some(&mut buf[filled..], deadline)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "pipe kapandı"));
            }
            filled += n;
        }
        Ok(())
    }

    /// Bütün baytları yazar; süre içinde bitmezse `TimedOut` (okumayan karşı taraf).
    pub fn write_all_timeout(&self, buf: &[u8], timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        let mut off = 0;
        while off < buf.len() {
            let ev = new_event()?;
            let ov = OVERLAPPED {
                hEvent: ev.0,
                ..Default::default()
            };
            // SAFETY: `buf` ve `ov`, `wait_io` tamamlanmayı bekleyene kadar yaşar.
            let r = unsafe {
                WriteFile(
                    self.h.0,
                    Some(&buf[off..]),
                    None,
                    Some(std::ptr::from_ref(&ov).cast_mut()),
                )
            };
            let n = match r {
                Ok(()) => wait_io(self.h.0, &ov, Some(Duration::ZERO), None)?,
                Err(e) if is(&e, ERROR_IO_PENDING) => wait_io(
                    self.h.0,
                    &ov,
                    Some(deadline.saturating_duration_since(Instant::now())),
                    None,
                )?,
                Err(e) => return Err(os_error(e)),
            };
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "pipe yazmadı"));
            }
            off += n as usize;
        }
        Ok(())
    }

    /// Çağıranın kimliği (yalnızca sunucu ucu). Pipe'tan en az bir bayt okunduktan sonra
    /// çağrılmalıdır: Windows kimliğe bürünmeyi son okunan mesaja göre yapar.
    pub fn caller(&self) -> io::Result<CallerToken> {
        if !self.server {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "yalnızca sunucu ucu",
            ));
        }
        // SAFETY: Tutamaç bağlı bir sunucu ucudur; dönüşü `RevertGuard` garanti eder.
        unsafe { ImpersonateNamedPipeClient(self.h.0) }.map_err(os_error)?;
        let guard = RevertGuard;
        let mut raw = HANDLE::default();
        // SAFETY: İş parçacığı kimliğe bürünmüştür. Tanımlama düzeyindeki belirteç ancak
        // `OpenAsSelf = true` ile (işlemin kendi bağlamıyla) açılabilir.
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut raw) };
        drop(guard);
        opened.map_err(os_error)?;
        let token = Token(raw);
        let (sid, session_id, elevated, interactive) = caller_token(token.0)?;
        let mut pid = 0u32;
        // SAFETY: Tutamaç bağlı bir sunucu ucudur; çıktı yerel değişkendir.
        let process = unsafe { GetNamedPipeClientProcessId(self.h.0, &mut pid) }
            .ok()
            // SAFETY: Yalnızca sorgu hakkıyla açılır; tutamaç sahiplenilir.
            .and_then(|()| {
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()
            })
            .map(OwnedHandle);
        let image_path = process.as_ref().and_then(|p| image_path(p.0));
        *self.process.lock().unwrap_or_else(PoisonError::into_inner) = process;
        Ok(CallerToken {
            sid,
            session_id,
            elevated,
            interactive,
            image_path,
        })
    }

    /// Bekleyen G/Ç'yi iptal eder ve (sunucuda) bağlantıyı keser. Tekrar çağrılabilir.
    pub fn close(&self) {
        // SAFETY: Tutamaç geçerlidir; bütün bekleyen işlemler iptal edilir, sahipleri
        // tamamlanmayı kendi `wait_io`'larında bekler.
        unsafe {
            let _ = CancelIoEx(self.h.0, None);
            if self.server {
                let _ = DisconnectNamedPipe(self.h.0);
            }
        }
    }
}

fn owner_matches(owner: PSID, expect: PipeOwner) -> io::Result<bool> {
    match expect {
        // SAFETY: `owner` güvenlik tanımlayıcısının içindeki geçerli bir SID'dir.
        PipeOwner::System => Ok(unsafe { IsWellKnownSid(owner, WinLocalSystemSid) }.as_bool()),
        PipeOwner::ProcessDefaultOwner => {
            let token = process_token()?;
            let info = token_info(token.0, TokenOwner)?;
            // SAFETY: Arabellek `TOKEN_OWNER` ile başlar; SID arabelleğin içindedir.
            let mine = unsafe { (*info.as_ptr().cast::<TOKEN_OWNER>()).Owner };
            // SAFETY: İki SID de geçerlidir.
            Ok(unsafe { EqualSid(owner, mine) }.is_ok())
        }
    }
}

/// İstemci: pipe'ı tanımlama düzeyinde açar (sunucu kimliğe bürünemez) ve sahibini doğrular.
/// Sahip beklenen değilse `PermissionDenied` (sahte sunucu, K21).
pub fn connect_pipe(
    name: &str,
    expect_owner: PipeOwner,
    timeout: Duration,
) -> io::Result<PipeConn> {
    let w = wide(std::ffi::OsStr::new(name));
    let deadline = Instant::now() + timeout;
    loop {
        // SAFETY: Ad NUL ile biten UTF-16'dır; tutamaç sahiplenilir.
        let r = unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                GENERIC_READ.0 | FILE_WRITE_DATA.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match r {
            Ok(h) => {
                let h = OwnedHandle(h);
                let mut owner = PSID::default();
                let mut sd = PSECURITY_DESCRIPTOR::default();
                // SAFETY: Tutamaç geçerlidir; tanımlayıcı `LocalSd` ile serbest bırakılır.
                let err = unsafe {
                    GetSecurityInfo(
                        h.0,
                        SE_KERNEL_OBJECT,
                        OWNER_SECURITY_INFORMATION,
                        Some(&mut owner),
                        None,
                        None,
                        None,
                        Some(&mut sd),
                    )
                };
                let _sd = LocalSd(sd);
                if err.0 != 0 {
                    return Err(win32(err));
                }
                if owner.0.is_null() || !owner_matches(owner, expect_owner)? {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "pipe sahibi beklenen değil",
                    ));
                }
                return Ok(PipeConn {
                    h,
                    server: false,
                    process: Mutex::new(None),
                });
            }
            Err(e) if is(&e, ERROR_PIPE_BUSY) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "pipe meşgul"));
                }
                // SAFETY: Ad geçerli bir UTF-16 dizesidir.
                let _ = unsafe { WaitNamedPipeW(PCWSTR(w.as_ptr()), millis(left).max(1)) };
            }
            Err(e) if is(&e, ERROR_FILE_NOT_FOUND) => {
                return Err(io::Error::new(io::ErrorKind::NotFound, "pipe yok"));
            }
            Err(e) => return Err(os_error(e)),
        }
    }
}

/// Bu işlemin oturum kimliği.
pub fn current_session_id() -> io::Result<u32> {
    let mut id = 0u32;
    // SAFETY: Çıktı yerel değişkendir.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut id) }.map_err(os_error)?;
    Ok(id)
}
