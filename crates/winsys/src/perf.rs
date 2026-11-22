// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ölçüm ilkelleri (`xtask perf`, TESTING §5): işlem belleği (`PrivateUsage`), CPU zamanı,
//! iş parçacığı döngü sayıları, ileti pompası ve işlem yaşı. Yalnızca okur.

use std::io;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    THREAD_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::WindowsProgramming::QueryThreadCycleTime;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    TranslateMessage,
};

use crate::pipe::os_error;

/// Bir işlemin anlık ölçümü.
#[derive(Clone, Debug)]
pub struct ProcessSample {
    /// `PROCESS_MEMORY_COUNTERS_EX.PrivateUsage`.
    pub private_bytes: u64,
    /// Çekirdek + kullanıcı CPU zamanı.
    pub cpu_time: Duration,
    /// (iş parçacığı kimliği, döngü sayısı); açılamayan iş parçacıkları atlanır.
    pub thread_cycles: Vec<(u32, u64)>,
}

fn filetime_100ns(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// Tutamaç düşünce kapanır.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: Tutamaç bu nesneye ait ve bir kez kapatılır.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn times(process: HANDLE) -> io::Result<(FILETIME, Duration)> {
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: Çıktılar yerel değişkenlere yazılır; tutamaç çağrı boyunca geçerli.
    unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(os_error)?;
    let cpu = filetime_100ns(kernel) + filetime_100ns(user);
    Ok((creation, Duration::from_nanos(cpu.saturating_mul(100))))
}

fn thread_cycles(pid: u32) -> io::Result<Vec<(u32, u64)>> {
    // SAFETY: Anlık görüntü tutamacı `Owned` ile kapanır; yapı yerel ve boyutu doğru.
    let snapshot =
        Owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(os_error)?);
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: Aynı yerel yapı her adımda yeniden doldurulur.
    let mut more = unsafe { Thread32First(snapshot.0, &mut entry) }.is_ok();
    while more {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: İş parçacığı yalnızca sorgu hakkıyla açılır ve kapatılır; çıktı yerel.
            if let Ok(h) =
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, entry.th32ThreadID) }
            {
                let h = Owned(h);
                let mut cycles = 0u64;
                if unsafe { QueryThreadCycleTime(h.0, &mut cycles) }.is_ok() {
                    out.push((entry.th32ThreadID, cycles));
                }
            }
        }
        // SAFETY: Bkz. yukarı.
        more = unsafe { Thread32Next(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(out)
}

/// İşlemin belleği, CPU zamanı ve iş parçacığı döngüleri (yalnızca sorgu hakkıyla açılır).
pub fn sample_process(pid: u32) -> io::Result<ProcessSample> {
    // SAFETY: Tutamaç yalnızca sorgu hakkıyla açılır ve `Owned` ile kapanır.
    let process = Owned(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map_err(os_error)?,
    );
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: Genişletilmiş yapı temel yapının başını içerir; boyut `cb` ile bildirilir.
    unsafe {
        GetProcessMemoryInfo(
            process.0,
            std::ptr::from_mut(&mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    }
    .map_err(os_error)?;
    let (_, cpu_time) = times(process.0)?;
    Ok(ProcessSample {
        private_bytes: counters.PrivateUsage as u64,
        cpu_time,
        thread_cycles: thread_cycles(pid)?,
    })
}

/// Bu iş parçacığının iletilerini `timeout` boyunca işler.
pub fn pump_messages(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let ms = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
        // SAFETY: Bekleme tutamaçsızdır; iletiler yerel yapıya alınıp bu iş parçacığında
        // işlenir.
        unsafe {
            let _ = MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT);
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

/// Geçerli işlemin oluşturulmasından bu yana geçen süre.
pub fn process_age() -> Duration {
    // SAFETY: Sözde tutamaç kapatılmaz; çıktı yerel.
    let created = unsafe { times(GetCurrentProcess()) }.map_or(0, |(c, _)| filetime_100ns(c));
    // SAFETY: Parametresiz okuma.
    let now = filetime_100ns(unsafe { GetSystemTimeAsFileTime() });
    Duration::from_nanos(now.saturating_sub(created).saturating_mul(100))
}

/// Bu işlemin kullanabildiği mantıksal işlemci sayısı.
pub fn logical_cpus() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}
