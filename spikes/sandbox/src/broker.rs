// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Broker: işçiyi AppContainer (yeteneksiz, Low IL) + Job Object içinde başlatır, girdi
//! dosyasını salt okunur ve devralınmış bir handle olarak verir, çıktıyı pipe'tan okur.

use std::ffi::c_void;
use std::io::Read;
use std::net::TcpListener;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GENERIC_READ, HANDLE, HANDLE_FLAG_INHERIT,
    SetHandleInformation, WAIT_OBJECT_0,
};
use windows::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows::Win32::Security::{FreeSid, PSID, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, OPEN_EXISTING,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME, JOB_OBJECT_UILIMIT_DESKTOP,
    JOB_OBJECT_UILIMIT_DISPLAYSETTINGS, JOB_OBJECT_UILIMIT_EXITWINDOWS,
    JOB_OBJECT_UILIMIT_GLOBALATOMS, JOB_OBJECT_UILIMIT_HANDLES, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS, JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateProcessW, DETACHED_PROCESS, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, ResumeThread,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const PROFILE: PCWSTR = w!("AlterDNS.Spike.Scan");
/// İşçi bellek sınırı (ARCHITECTURE §7.2 varsayılanı).
const MEMORY_LIMIT: usize = 256 << 20;
/// İşçi CPU süresi sınırı: 10 sn (100 ns birimi).
const CPU_LIMIT_100NS: i64 = 10 * 10_000_000;
/// Duvar saati sınırı: aşılırsa iş nesnesi sonlandırılır.
const WALL_LIMIT: Duration = Duration::from_secs(20);
/// "ALL APPLICATION PACKAGES" SID'i; %ProgramFiles% bu gruba RX verir.
const ALL_APP_PACKAGES: &str = "*S-1-15-2-1";
/// `PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY` (ProcThreadAttributeValue(14, FALSE, TRUE, FALSE)).
const PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY: usize = 0x0002_000E;
const PROCESS_CREATION_CHILD_PROCESS_RESTRICTED: u32 = 0x01;

/// Kapsam sonunda kapanan Win32 handle'ı.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0.0.is_null() {
            // SAFETY: Handle bu yapının tek sahibi olduğu bir Win32 çağrısından geldi.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

pub fn main(args: &[String]) -> anyhow::Result<()> {
    let file = crate::arg(args, "--file").context("--file yok")?;
    let probe = crate::arg(args, "--probe").context("--probe yok")?;
    let report = run(Path::new(file), probe)?;
    println!("{report}");
    Ok(())
}

/// Spike sonunda: AppContainer profilini ve kullanıcı dosyası dizinini kaldırır.
pub fn cleanup() -> anyhow::Result<()> {
    // SAFETY: Sabit, sıfırla biten geniş dize.
    let r = unsafe { DeleteAppContainerProfile(PROFILE) };
    if let Err(e) = r {
        eprintln!("profil silinemedi (zaten yok olabilir): {e}");
    }
    let dir = user_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    println!(
        "{}",
        json!({ "profile_deleted": true, "user_dir_removed": !dir.exists() })
    );
    Ok(())
}

fn user_dir() -> anyhow::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA yok")?;
    Ok(PathBuf::from(base).join("AlterDNS-spike"))
}

fn sha256_hex(path: &Path) -> anyhow::Result<String> {
    let data = std::fs::read(path)?;
    Ok(Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// AppContainer SID'i: profil yoksa oluşturur, varsa adından türetir.
fn app_container_sid() -> anyhow::Result<PSID> {
    // SAFETY: Sabit geniş dizeler; yetenek listesi boş. Dönen SID FreeSid ile bırakılır.
    let created = unsafe {
        CreateAppContainerProfile(
            PROFILE,
            w!("AlterDNS sandbox spike"),
            w!("Faz 0 T-SPK-003"),
            None,
        )
    };
    match created {
        Ok(sid) => Ok(sid),
        Err(e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult() => {
            // SAFETY: Sabit geniş dize; dönen SID FreeSid ile bırakılır.
            Ok(unsafe { DeriveAppContainerSidFromAppContainerName(PROFILE) }?)
        }
        Err(e) => Err(e.into()),
    }
}

/// İşçi exe'sine "ALL APPLICATION PACKAGES" için okuma/çalıştırma izni verir.
/// %ProgramFiles% bunu varsayılan olarak verir; spike exe'si target/ altında olduğu için gerekir.
fn grant_exe(exe: &Path) -> anyhow::Result<()> {
    let out = Command::new("icacls")
        .arg(exe)
        .args(["/grant", &format!("{ALL_APP_PACKAGES}:(RX)")])
        .output()
        .context("icacls çalıştırılamadı")?;
    if !out.status.success() {
        bail!("icacls başarısız: {}", String::from_utf8_lossy(&out.stdout));
    }
    Ok(())
}

fn inheritable_sa() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    }
}

fn open_readonly_inheritable(path: &Path) -> anyhow::Result<Owned> {
    let sa = inheritable_sa();
    // SAFETY: Yol HSTRING ile sıfırla biten geniş dizeye çevrildi; SA geçerli bir yerel.
    let h = unsafe {
        CreateFileW(
            &HSTRING::from(path.as_os_str()),
            GENERIC_READ.0,
            FILE_SHARE_READ,
            Some(&sa),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }?;
    Ok(Owned(h))
}

/// (okuma ucu, yazma ucu). Yalnızca yazma ucu devralınabilir.
fn stdout_pipe() -> anyhow::Result<(Owned, Owned)> {
    let sa = inheritable_sa();
    let mut r = HANDLE::default();
    let mut w = HANDLE::default();
    // SAFETY: Çıktı handle'ları yerel değişkenlere yazılır; SA geçerli.
    unsafe { CreatePipe(&mut r, &mut w, Some(&sa), 0) }?;
    let (r, w) = (Owned(r), Owned(w));
    // SAFETY: `r.0` az önce oluşturulmuş geçerli bir handle.
    unsafe { SetHandleInformation(r.0, HANDLE_FLAG_INHERIT.0, Default::default()) }?;
    Ok((r, w))
}

fn make_job() -> anyhow::Result<Owned> {
    // SAFETY: Adsız iş nesnesi; varsayılan güvenlik.
    let job = Owned(unsafe { CreateJobObjectW(None, PCWSTR::null()) }?);
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_PROCESS_TIME
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    info.BasicLimitInformation.ActiveProcessLimit = 1;
    info.BasicLimitInformation.PerProcessUserTimeLimit = CPU_LIMIT_100NS;
    info.ProcessMemoryLimit = MEMORY_LIMIT;
    // SAFETY: Yapı doğru türde ve boyut bildiriliyor.
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            std::mem::size_of_val(&info) as u32,
        )
    }?;
    let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
        UIRestrictionsClass: JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
            | JOB_OBJECT_UILIMIT_EXITWINDOWS
            | JOB_OBJECT_UILIMIT_GLOBALATOMS
            | JOB_OBJECT_UILIMIT_HANDLES
            | JOB_OBJECT_UILIMIT_READCLIPBOARD
            | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
            | JOB_OBJECT_UILIMIT_WRITECLIPBOARD,
    };
    // SAFETY: Yapı doğru türde ve boyut bildiriliyor.
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectBasicUIRestrictions,
            &ui as *const _ as *const c_void,
            std::mem::size_of_val(&ui) as u32,
        )
    }?;
    Ok(job)
}

/// İşçiyi askıda, AppContainer içinde ve yalnızca `inherit` handle'larını devralarak başlatır.
fn spawn_suspended(
    exe: &Path,
    cmdline: &str,
    sid: PSID,
    inherit: &[HANDLE],
    stdout: HANDLE,
) -> anyhow::Result<PROCESS_INFORMATION> {
    let mut size = 0usize;
    // SAFETY: İlk çağrı yalnızca gereken boyutu ister; hata beklenir.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 3, None, &mut size) };
    let mut storage = vec![0u64; size.div_ceil(8)];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr() as *mut c_void);
    // SAFETY: `storage` en az `size` bayt ve 8 bayt hizalı.
    unsafe { InitializeProcThreadAttributeList(Some(list), 3, None, &mut size) }?;

    let caps = SECURITY_CAPABILITIES {
        AppContainerSid: sid,
        Capabilities: std::ptr::null_mut(),
        CapabilityCount: 0,
        Reserved: 0,
    };
    let policy: u32 = PROCESS_CREATION_CHILD_PROCESS_RESTRICTED;
    // SAFETY: Değerler (caps, inherit, policy) CreateProcessW dönene kadar yaşar; boyutlar doğru.
    let result = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            Some(&caps as *const _ as *const c_void),
            std::mem::size_of_val(&caps),
            None,
            None,
        )
        .and_then(|()| {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(inherit.as_ptr() as *const c_void),
                std::mem::size_of_val(inherit),
                None,
                None,
            )
        })
        .and_then(|()| {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY,
                Some(&policy as *const _ as *const c_void),
                std::mem::size_of_val(&policy),
                None,
                None,
            )
        })
        .and_then(|()| {
            let mut si = STARTUPINFOEXW::default();
            si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdOutput = stdout;
            si.StartupInfo.hStdError = stdout;
            si.lpAttributeList = list;
            let exe_w = wide(&exe.display().to_string());
            let mut cmd_w = wide(cmdline);
            let mut pi = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR(exe_w.as_ptr()),
                Some(PWSTR(cmd_w.as_mut_ptr())),
                None,
                None,
                true,
                // DETACHED_PROCESS: konsol (conhost) oluşturulmaz. CREATE_NO_WINDOW ile
                // conhost iş nesnesinin süreç sınırına takılıyor, işçi 0xC0000142 ile ölüyordu.
                EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | DETACHED_PROCESS,
                None,
                w!(r"C:\Windows\System32"),
                &si.StartupInfo,
                &mut pi,
            )
            .map(|()| pi)
        })
    };
    // SAFETY: Liste yukarıda başarıyla başlatıldı.
    unsafe { DeleteProcThreadAttributeList(list) };
    Ok(result?)
}

/// Bir prob için işçiyi çalıştırır ve işçinin JSON çıktısını broker alanlarıyla döndürür.
pub fn run(file: &Path, probe: &str) -> anyhow::Result<Value> {
    let exe = std::env::current_exe()?;
    grant_exe(&exe)?;
    let user_dir = user_dir()?;
    std::fs::create_dir_all(&user_dir)?;
    let user_file = user_dir.join("secret.txt");
    std::fs::write(&user_file, b"kullanici verisi")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let expected_sha256 = sha256_hex(file)?;

    let sid = app_container_sid()?;
    let input = open_readonly_inheritable(file)?;
    let (out_r, out_w) = stdout_pipe()?;
    let job = make_job()?;
    let cmdline = format!(
        "\"{}\" worker --probe {probe} --handle {} --user-file \"{}\" --user-temp \"{}\" --port {port}",
        exe.display(),
        input.0.0 as usize,
        user_file.display(),
        // Sondaki `\` kapanan tırnağı kaçırır (`\"`), sonraki argümanlar bozulur.
        std::env::temp_dir()
            .display()
            .to_string()
            .trim_end_matches('\\')
    );

    let started = Instant::now();
    let spawned = spawn_suspended(&exe, &cmdline, sid, &[input.0, out_w.0], out_w.0);
    // SAFETY: SID CreateAppContainerProfile/Derive'dan geldi ve artık kullanılmıyor.
    unsafe {
        let _ = FreeSid(sid);
    }
    let pi = spawned?;
    let process = Owned(pi.hProcess);
    let thread = Owned(pi.hThread);
    drop(out_w); // Bizdeki yazma ucu kapanmazsa okuma EOF görmez.

    // SAFETY: İşlem askıda; iş nesnesine atanır, sonra iş parçacığı sürdürülür.
    unsafe { AssignProcessToJobObject(job.0, process.0) }?;
    // SAFETY: `thread` askıda başlatılan işlemin ana iş parçacığı.
    unsafe { ResumeThread(thread.0) };

    // SAFETY: Okuma ucunun tek sahibi artık File'dır.
    let mut reader = unsafe { std::fs::File::from_raw_handle(out_r.0.0) };
    std::mem::forget(out_r);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = reader.read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let output = match rx.recv_timeout(WALL_LIMIT) {
        Ok(s) => s,
        Err(_) => {
            // SAFETY: Geçerli iş nesnesi; içindeki işlemleri sonlandırır.
            unsafe {
                let _ = TerminateJobObject(job.0, 1);
            }
            String::new()
        }
    };
    // SAFETY: Geçerli işlem handle'ı.
    let waited = unsafe { WaitForSingleObject(process.0, 5_000) };
    let mut code = 0u32;
    // SAFETY: Geçerli işlem handle'ı; çıktı yerel değişkene yazılır.
    unsafe { GetExitCodeProcess(process.0, &mut code) }?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut report: Value = output
        .lines()
        .find_map(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| {
            json!({ "probe": probe, "ok": false, "error": null, "detail": { "no_output": true } })
        });
    if let Some(obj) = report.as_object_mut() {
        obj.insert("expected_sha256".into(), json!(expected_sha256));
        obj.insert("exit_code".into(), json!(code));
        obj.insert("exited".into(), json!(waited == WAIT_OBJECT_0));
        obj.insert("elapsed_ms".into(), json!(elapsed_ms));
    }
    drop(listener);
    Ok(report)
}
