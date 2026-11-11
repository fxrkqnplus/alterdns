// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wintun DLL'i ve Windows API'leri için ince FFI katmanı (spike; `unsafe` yalnızca burada).
//! Bildirimler `wintun.h` (GPL-2.0 OR MIT; MIT seçeneği) dosyasından alınmıştır.

use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};

/// `lab/bin/wintun-0.14.1/wintun/bin/amd64/wintun.dll` için sabit SHA-256 (2026-11-10, imza
/// WireGuard LLC, `lab/pins.toml`).
pub const WINTUN_DLL_SHA256: &str =
    "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce";

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// DLL'in SHA-256'sı beklenenle aynı değilse yüklemeyi reddeder (`Wintun::load` aynı denetimi
/// dosyayı kilitli tutarak yapar; bu işlev testler için).
#[cfg(test)]
pub fn verify_dll(path: &Path, expected: &str) -> anyhow::Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("{} okunamadı", path.display()))?;
    let got = sha256_hex(&bytes);
    if got != expected {
        bail!("{}: SHA-256 beklenenle aynı değil ({got})", path.display());
    }
    Ok(())
}

/// Giden IPv4 soketini fiziksel arayüze bağlar (`IP_UNICAST_IF`). TUN'a giden /32 rota varken
/// bile soket bu arayüzden çıkar; böylece iletim TUN'a geri dönmez.
pub fn set_unicast_if_v4(sock: &socket2::Socket, if_index: u32) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows::Win32::Networking::WinSock::{IP_UNICAST_IF, IPPROTO_IP, SOCKET, setsockopt};
    // IPv4 için arayüz dizini ağ bayt sırasında verilir (IP_UNICAST_IF belgesi).
    let value = if_index.to_be().to_ne_bytes();
    let raw = usize::try_from(sock.as_raw_socket())
        .map_err(|_| std::io::Error::other("soket tutamacı usize'a sığmıyor"))?;
    // SAFETY: `sock` bu çağrı boyunca canlı ve geçerli bir soket tutamacıdır; `value` 4 baytlık
    // yerel bir dizidir ve setsockopt onu yalnızca okur.
    let rc = unsafe { setsockopt(SOCKET(raw), IPPROTO_IP.0, IP_UNICAST_IF, Some(&value)) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// `dest` için en iyi çıkış arayüzünün dizini. TUN rotası eklenmeden önce çağrılmalıdır.
pub fn best_interface_v4(dest: Ipv4Addr) -> std::io::Result<u32> {
    use windows::Win32::NetworkManagement::IpHelper::GetBestInterface;
    let mut index = 0u32;
    // SAFETY: `index` geçerli bir çıkış işaretçisidir; adres ağ bayt sırasında u32 olarak verilir.
    let rc = unsafe { GetBestInterface(u32::from_ne_bytes(dest.octets()), &mut index) };
    if rc == 0 {
        Ok(index)
    } else {
        Err(std::io::Error::from_raw_os_error(rc as i32))
    }
}

fn filetime_to_duration(ft: windows::Win32::Foundation::FILETIME) -> Duration {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    Duration::from_nanos(ticks.saturating_mul(100))
}

/// Bu işlemin toplam CPU süresi (çekirdek + kullanıcı) ve özel bellek kullanımı (bayt).
pub fn process_metrics() -> std::io::Result<(Duration, u64)> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    let mut mem = PROCESS_MEMORY_COUNTERS_EX::default();
    let cb = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>())
        .map_err(std::io::Error::other)?;
    // SAFETY: GetCurrentProcess sözde tutamaç döndürür (kapatılmaz). Bütün işaretçiler bu
    // çağrı boyunca canlı yerel değişkenleri gösterir; `cb` yapının gerçek boyutudur.
    unsafe {
        let me = GetCurrentProcess();
        GetProcessTimes(me, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(std::io::Error::other)?;
        let counters =
            (&mut mem as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>();
        if !K32GetProcessMemoryInfo(me, counters, cb).as_bool() {
            return Err(std::io::Error::last_os_error());
        }
    }
    let cpu = filetime_to_duration(kernel) + filetime_to_duration(user);
    Ok((cpu, mem.PrivateUsage as u64))
}

/// Gerçek Windows dizini (`GetSystemWindowsDirectoryW`; ortam değişkenine bakmaz).
pub fn windows_dir() -> std::io::Result<std::path::PathBuf> {
    use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
    let mut buf = vec![0u16; 512];
    // SAFETY: `buf` yazılabilir yerel bir ara bellektir; dönen uzunluk ona sığar ya da 0'dır.
    let n = unsafe { GetSystemWindowsDirectoryW(Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(
        buf.get(..n).unwrap_or_default(),
    )))
}

/// Bu işlemdeki sonraki bütün DLL yüklemeleri yalnızca System32 ve tam yolu verilen dosyayla
/// sınırlanır (uygulama dizini ve çalışma dizini aranmaz).
pub fn restrict_dll_search() -> std::io::Result<()> {
    use windows::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
    };
    // SAFETY: Bayrak sabiti geçerli; işlem genelinde arama yolunu daraltmaktan başka etkisi yok.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) }.map_err(std::io::Error::other)
}

/// Wintun örneğindeki gibi (example.c) doğrudan `Preferred` durumda başlayan IPv4 adres satırı:
/// DAD atlanır. `New-NetIPAddress` bu durumu veremez; ARP'siz L3 tünelde DAD bitmeyebilir.
pub fn address_row(
    luid: windows::Win32::NetworkManagement::Ndis::NET_LUID_LH,
    ip: Ipv4Addr,
    prefix: u8,
) -> windows::Win32::NetworkManagement::IpHelper::MIB_UNICASTIPADDRESS_ROW {
    use windows::Win32::NetworkManagement::IpHelper::{
        InitializeUnicastIpAddressEntry, MIB_UNICASTIPADDRESS_ROW,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, IN_ADDR, IN_ADDR_0, IpDadStatePreferred};
    let mut row = MIB_UNICASTIPADDRESS_ROW::default();
    // SAFETY: `row` geçerli, yazılabilir yerel bir yapıdır; işlev yalnızca varsayılanları doldurur.
    unsafe { InitializeUnicastIpAddressEntry(&mut row) };
    row.Address.Ipv4.sin_family = AF_INET;
    row.Address.Ipv4.sin_addr = IN_ADDR {
        S_un: IN_ADDR_0 {
            S_addr: u32::from_ne_bytes(ip.octets()),
        },
    };
    row.InterfaceLuid = luid;
    row.OnLinkPrefixLength = prefix;
    row.DadState = IpDadStatePreferred;
    row
}

/// Adres satırını sisteme ekler (yönetici). `Preferred` durumu satırdan gelir.
pub fn create_unicast_address(
    row: &windows::Win32::NetworkManagement::IpHelper::MIB_UNICASTIPADDRESS_ROW,
) -> std::io::Result<()> {
    use windows::Win32::NetworkManagement::IpHelper::CreateUnicastIpAddressEntry;
    // SAFETY: `row` çağrı boyunca canlı, `address_row` ile tam doldurulmuş bir yapıdır.
    let rc = unsafe { CreateUnicastIpAddressEntry(row) };
    if rc.0 == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(rc.0 as i32))
    }
}

type AdapterHandle = *mut std::ffi::c_void;
type SessionHandle = *mut std::ffi::c_void;
type RawProc = unsafe extern "system" fn() -> isize;

/// Halka kapasitesi (4 MiB; `WINTUN_MIN_RING_CAPACITY`…`WINTUN_MAX_RING_CAPACITY`, ikinin kuvveti).
const RING_CAPACITY: u32 = 0x40_0000;
/// `WINTUN_MAX_IP_PACKET_SIZE`.
const MAX_IP_PACKET: usize = 0xFFFF;

/// Yüklenmiş Wintun DLL'i (işlev işaretçileri). DLL işlem ömrü boyunca yüklü kalır.
// wintun.h işlev imzaları (WINAPI = "system").
type CreateAdapterFn = unsafe extern "system" fn(
    windows::core::PCWSTR,
    windows::core::PCWSTR,
    *const windows::core::GUID,
) -> AdapterHandle;
type CloseAdapterFn = unsafe extern "system" fn(AdapterHandle);
type DriverVersionFn = unsafe extern "system" fn() -> u32;
type StartSessionFn = unsafe extern "system" fn(AdapterHandle, u32) -> SessionHandle;
type EndSessionFn = unsafe extern "system" fn(SessionHandle);
type ReadWaitEventFn =
    unsafe extern "system" fn(SessionHandle) -> windows::Win32::Foundation::HANDLE;
type ReceivePacketFn = unsafe extern "system" fn(SessionHandle, *mut u32) -> *mut u8;
type ReleaseReceivePacketFn = unsafe extern "system" fn(SessionHandle, *const u8);
type AllocateSendPacketFn = unsafe extern "system" fn(SessionHandle, u32) -> *mut u8;
type SendPacketFn = unsafe extern "system" fn(SessionHandle, *const u8);
type GetAdapterLuidFn = unsafe extern "system" fn(
    AdapterHandle,
    *mut windows::Win32::NetworkManagement::Ndis::NET_LUID_LH,
);

pub struct Wintun {
    create_adapter: CreateAdapterFn,
    close_adapter: CloseAdapterFn,
    running_driver_version: DriverVersionFn,
    start_session: StartSessionFn,
    end_session: EndSessionFn,
    read_wait_event: ReadWaitEventFn,
    receive_packet: ReceivePacketFn,
    release_receive_packet: ReleaseReceivePacketFn,
    allocate_send_packet: AllocateSendPacketFn,
    send_packet: SendPacketFn,
    get_adapter_luid: GetAdapterLuidFn,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Wintun {
    /// DLL'i doğrular (SHA-256) ve tam yoldan yükler. Dosya, doğrulamadan yükleme bitene kadar
    /// yazmaya ve silmeye karşı paylaşımsız açık tutulur (doğrulama ile yükleme arasında
    /// değiştirilemesin).
    pub fn load(dll: &Path, expected_sha256: &str) -> anyhow::Result<Arc<Wintun>> {
        use std::ffi::CStr;
        use std::io::Read;
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW,
        };
        use windows::Win32::System::LibraryLoader::{
            GetModuleFileNameW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
        };
        use windows::core::{PCSTR, PCWSTR};

        if !dll.is_absolute() {
            bail!("DLL yolu tam yol olmalı: {}", dll.display());
        }
        const FILE_SHARE_READ: u32 = 1;
        let mut guard = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(dll)
            .with_context(|| format!("{} açılamadı", dll.display()))?;
        let mut bytes = Vec::new();
        guard.read_to_end(&mut bytes)?;
        let got = sha256_hex(&bytes);
        if got != expected_sha256 {
            bail!("{}: SHA-256 beklenenle aynı değil ({got})", dll.display());
        }
        // Doğrulanan dosyanın gerçek yolu tutamaçtan alınır: yol üzerindeki bir dizin bağlantısı
        // doğrulamadan sonra değiştirilse bile yüklenen dosya bu dosya olur.
        let mut buf = vec![0u16; 1024];
        // SAFETY: `guard` bu çağrı boyunca açık, geçerli bir dosya tutamacıdır; `buf` yazılabilir.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(guard.as_raw_handle()),
                &mut buf,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
        if n == 0 || n >= buf.len() {
            return Err(std::io::Error::last_os_error()).context("DLL gerçek yolu alınamadı");
        }
        let final_path = String::from_utf16(buf.get(..n).unwrap_or_default())?;
        let final_path = crate::harden::strip_verbatim(&final_path)
            .ok_or_else(|| anyhow::anyhow!("DLL yerel bir sürücüde değil: {final_path}"))?;
        let wide_path: Vec<u16> = final_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide_path` NUL ile biten, çağrı boyunca canlı bir UTF-16 dizisidir ve tam yoldur;
        // içerik az önce sabit hash ile doğrulandı ve `guard` dosyayı değişikliğe karşı tutuyor.
        // LOAD_LIBRARY_SEARCH_SYSTEM32: DLL bağımlılıkları yalnızca System32'den çözülür.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(wide_path.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .with_context(|| format!("{final_path} yüklenemedi"))?;
        let mut loaded = vec![0u16; 1024];
        // SAFETY: `module` az önce yüklenen geçerli modüldür; `loaded` yazılabilir.
        let m = unsafe { GetModuleFileNameW(Some(module), &mut loaded) } as usize;
        let loaded = String::from_utf16_lossy(loaded.get(..m).unwrap_or_default());
        drop(guard);
        let loaded = crate::harden::strip_verbatim(&loaded).unwrap_or(loaded);
        if !loaded.eq_ignore_ascii_case(&final_path) {
            bail!("yüklenen modül beklenen dosya değil: {loaded} ≠ {final_path}");
        }
        let sym = |name: &'static CStr| -> anyhow::Result<RawProc> {
            // SAFETY: `module` geçerli bir modül tutamacıdır; `name` NUL ile biten sabit bir dizedir.
            unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }
                .ok_or_else(|| anyhow::anyhow!("wintun.dll içinde {name:?} yok"))
        };
        // SAFETY: Her işaretçi, wintun.h bildirimiyle aynı çağrı kuralına (WINAPI = system) ve
        // imzaya sahip dışa aktarılmış işleve dönüştürülüyor (Wintun 0.14.1 üstbilgisi).
        let lib = unsafe {
            Wintun {
                create_adapter: std::mem::transmute::<RawProc, CreateAdapterFn>(sym(
                    c"WintunCreateAdapter",
                )?),
                close_adapter: std::mem::transmute::<RawProc, CloseAdapterFn>(sym(
                    c"WintunCloseAdapter",
                )?),
                running_driver_version: std::mem::transmute::<RawProc, DriverVersionFn>(sym(
                    c"WintunGetRunningDriverVersion",
                )?),
                start_session: std::mem::transmute::<RawProc, StartSessionFn>(sym(
                    c"WintunStartSession",
                )?),
                end_session: std::mem::transmute::<RawProc, EndSessionFn>(sym(
                    c"WintunEndSession",
                )?),
                read_wait_event: std::mem::transmute::<RawProc, ReadWaitEventFn>(sym(
                    c"WintunGetReadWaitEvent",
                )?),
                receive_packet: std::mem::transmute::<RawProc, ReceivePacketFn>(sym(
                    c"WintunReceivePacket",
                )?),
                release_receive_packet: std::mem::transmute::<RawProc, ReleaseReceivePacketFn>(
                    sym(c"WintunReleaseReceivePacket")?,
                ),
                allocate_send_packet: std::mem::transmute::<RawProc, AllocateSendPacketFn>(sym(
                    c"WintunAllocateSendPacket",
                )?),
                send_packet: std::mem::transmute::<RawProc, SendPacketFn>(sym(
                    c"WintunSendPacket",
                )?),
                get_adapter_luid: std::mem::transmute::<RawProc, GetAdapterLuidFn>(sym(
                    c"WintunGetAdapterLUID",
                )?),
            }
        };
        Ok(Arc::new(lib))
    }

    /// Çalışan Wintun sürücüsünün sürümü (büyük, küçük); sürücü yüklü değilse `None`.
    pub fn running_driver_version(&self) -> Option<(u16, u16)> {
        // SAFETY: Parametresiz, yan etkisiz sorgu; DLL yüklü.
        let v = unsafe { (self.running_driver_version)() };
        (v != 0).then_some(((v >> 16) as u16, (v & 0xFFFF) as u16))
    }

    /// Adaptörü oluşturur (yönetici gerekir; sürücü paketi yoksa DLL kurar).
    /// Sabit GUID her koşuda aynı ağ profili kaydının kullanılmasını sağlar.
    pub fn create_adapter(
        self: &Arc<Self>,
        name: &str,
        tunnel_type: &str,
        guid: u128,
    ) -> anyhow::Result<Arc<Adapter>> {
        let (name_w, type_w) = (wide(name), wide(tunnel_type));
        let guid = windows::core::GUID::from_u128(guid);
        // SAFETY: İki dize de NUL ile biten, çağrı boyunca canlı UTF-16 dizileridir; `guid`
        // çağrı boyunca canlı bir yerel değişkendir.
        let handle = unsafe {
            (self.create_adapter)(
                windows::core::PCWSTR(name_w.as_ptr()),
                windows::core::PCWSTR(type_w.as_ptr()),
                &guid,
            )
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("WintunCreateAdapter({name}) başarısız"));
        }
        Ok(Arc::new(Adapter {
            lib: self.clone(),
            handle,
        }))
    }
}

/// Wintun adaptörü. Düşünce `WintunCloseAdapter` çağrılır; `WintunCreateAdapter` ile açıldığı
/// için adaptör sistemden de kalkar.
pub struct Adapter {
    lib: Arc<Wintun>,
    handle: AdapterHandle,
}

// SAFETY: Tutamaç DLL'e ait opak bir işaretçidir; bu tür onu yalnızca `StartSession` (oluşturan
// iş parçacığından) ve `Drop` içinde `CloseAdapter` için kullanır, eşzamanlı çağrı yapmaz.
// wintun.h hiçbir işlev için iş parçacığı bağlılığı belirtmez. Kapatma sırası `Session`'ın
// tuttuğu `Arc<Adapter>` ile güvence altındadır (önce oturum biter).
unsafe impl Send for Adapter {}
// SAFETY: Yukarıdaki gerekçe; paylaşılan başvurudan yalnızca `start_session` çağrılır.
unsafe impl Sync for Adapter {}

impl Adapter {
    /// Adaptörün ağ arabirimi LUID'i.
    pub fn luid(&self) -> windows::Win32::NetworkManagement::Ndis::NET_LUID_LH {
        let mut luid = windows::Win32::NetworkManagement::Ndis::NET_LUID_LH::default();
        // SAFETY: `handle` bu adaptör yaşadıkça geçerlidir; `luid` yazılabilir yerel bir değerdir.
        unsafe { (self.lib.get_adapter_luid)(self.handle, &mut luid) };
        luid
    }

    pub fn start_session(self: &Arc<Self>) -> anyhow::Result<Arc<Session>> {
        // SAFETY: `handle` bu adaptör yaşadıkça geçerlidir; kapasite izin verilen aralıkta.
        let handle = unsafe { (self.lib.start_session)(self.handle, RING_CAPACITY) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error()).context("WintunStartSession başarısız");
        }
        // SAFETY: `handle` az önce başarıyla açılan oturumdur.
        let read_event = unsafe { (self.lib.read_wait_event)(handle) };
        Ok(Arc::new(Session {
            lib: self.lib.clone(),
            _adapter: self.clone(),
            handle,
            read_event,
        }))
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        // SAFETY: `handle` geçerli ve yalnızca burada, bir kez kapatılır; bütün oturumlar
        // (`Arc<Adapter>` tuttukları için) bundan önce bitmiştir.
        unsafe { (self.lib.close_adapter)(self.handle) }
    }
}

/// Wintun oturumu (paket halkaları).
pub struct Session {
    lib: Arc<Wintun>,
    _adapter: Arc<Adapter>,
    handle: SessionHandle,
    /// Oturumun yönettiği olay; `CloseHandle` çağrılmaz (wintun.h).
    read_event: windows::Win32::Foundation::HANDLE,
}

// SAFETY: wintun.h, ReceivePacket, ReleaseReceivePacket, AllocateSendPacket ve SendPacket için
// "This function is thread-safe" der. Okuma olayı bir çekirdek olay tutamacıdır ve her iş
// parçacığından beklenebilir. Oturum yalnızca `Drop`'ta, tek sahip kaldığında sonlandırılır.
unsafe impl Send for Session {}
// SAFETY: Yukarıdaki gerekçe.
unsafe impl Sync for Session {}

impl Session {
    /// Bir IP paketi; halka boşsa `None`. Adaptör kapanıyorsa hata (`ERROR_HANDLE_EOF`).
    pub fn recv(&self) -> std::io::Result<Option<Vec<u8>>> {
        use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, GetLastError};
        let mut size = 0u32;
        // SAFETY: `handle` oturum yaşadıkça geçerlidir; `size` geçerli bir çıkış işaretçisidir.
        let p = unsafe { (self.lib.receive_packet)(self.handle, &mut size) };
        if p.is_null() {
            // SAFETY: Ön koşulu yok; başarısız çağrının hemen ardından okunur.
            let e = unsafe { GetLastError() };
            return if e == ERROR_NO_MORE_ITEMS {
                Ok(None)
            } else {
                Err(std::io::Error::from_raw_os_error(e.0 as i32))
            };
        }
        // SAFETY: Wintun, `size` bayt okunabilir belleği gösteren bir işaretçi döndürdü; bellek
        // ReleaseReceivePacket çağrılana kadar geçerlidir ve içerik önce kopyalanır.
        let data = unsafe { std::slice::from_raw_parts(p, size as usize) }.to_vec();
        // SAFETY: `p` bu oturumun ReceivePacket'inden döndü ve bir kez serbest bırakılıyor.
        unsafe { (self.lib.release_receive_packet)(self.handle, p) };
        Ok(Some(data))
    }

    /// Okunacak veri olana kadar (en çok `timeout`) bekler.
    pub fn wait_readable(&self, timeout: Duration) -> bool {
        use windows::Win32::Foundation::WAIT_OBJECT_0;
        use windows::Win32::System::Threading::WaitForSingleObject;
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: `read_event` oturum yaşadıkça geçerli bir olay tutamacıdır.
        unsafe { WaitForSingleObject(self.read_event, ms) == WAIT_OBJECT_0 }
    }

    /// Bir IP paketini adaptöre yazar. Halka doluysa paket sessizce düşer (wintun.h örneği).
    pub fn send(&self, packet: &[u8]) -> std::io::Result<()> {
        use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, GetLastError};
        if packet.is_empty() || packet.len() > MAX_IP_PACKET {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "paket boyutu geçersiz",
            ));
        }
        // SAFETY: `handle` geçerli; boyut WINTUN_MAX_IP_PACKET_SIZE sınırının içinde.
        let p = unsafe { (self.lib.allocate_send_packet)(self.handle, packet.len() as u32) };
        if p.is_null() {
            // SAFETY: Ön koşulu yok; başarısız çağrının hemen ardından okunur.
            let e = unsafe { GetLastError() };
            return if e == ERROR_BUFFER_OVERFLOW {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(e.0 as i32))
            };
        }
        // SAFETY: Wintun `packet.len()` bayt yazılabilir bellek ayırdı; kaynak ve hedef
        // çakışmaz. SendPacket aynı işaretçiyi bir kez tüketir.
        unsafe {
            std::ptr::copy_nonoverlapping(packet.as_ptr(), p, packet.len());
            (self.lib.send_packet)(self.handle, p);
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: `handle` geçerli ve yalnızca burada, bir kez sonlandırılır.
        unsafe { (self.lib.end_session)(self.handle) }
    }
}

impl crate::forward::PacketIo for Session {
    fn recv(&self) -> std::io::Result<Option<Vec<u8>>> {
        Session::recv(self)
    }

    fn wait_readable(&self, timeout: Duration) -> bool {
        Session::wait_readable(self, timeout)
    }

    fn send(&self, packet: &[u8]) -> std::io::Result<()> {
        Session::send(self, packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_hash_mismatch_refuses_load() {
        let dir = std::env::temp_dir().join(format!("alterdns-wintun-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("wintun.dll");
        std::fs::write(&fake, b"sahte").unwrap();
        assert!(verify_dll(&fake, WINTUN_DLL_SHA256).is_err());
        assert!(verify_dll(&dir.join("yok.dll"), WINTUN_DLL_SHA256).is_err());
        assert!(verify_dll(&fake, &sha256_hex(b"sahte")).is_ok());
        // Yükleyici de aynı denetimden geçer: hash tutmayan dosya yüklenmez.
        assert!(Wintun::load(&fake, WINTUN_DLL_SHA256).is_err());
        // Hash tutsa bile göreli yol reddedilir (arama yolu ele geçirilmesin).
        assert!(Wintun::load(Path::new("wintun.dll"), &sha256_hex(b"sahte")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn address_row_is_preferred_ipv4_on_given_luid() {
        use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
        use windows::Win32::Networking::WinSock::{AF_INET, IpDadStatePreferred};
        let luid = NET_LUID_LH { Value: 0x1234_5678 };
        let row = address_row(luid, Ipv4Addr::new(198, 18, 0, 1), 30);
        // SAFETY (test): birlik alanları, address_row'un yazdığı IPv4 kolundan okunuyor.
        unsafe {
            assert_eq!(row.Address.si_family, AF_INET);
            assert_eq!(
                row.Address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
                [198, 18, 0, 1]
            );
            assert_eq!(row.InterfaceLuid.Value, 0x1234_5678);
        }
        assert_eq!(row.OnLinkPrefixLength, 30);
        assert_eq!(row.DadState, IpDadStatePreferred);
        assert!(!row.SkipAsSource, "kaynak adres olarak kullanılabilmeli");
    }

    #[test]
    fn unicast_if_accepts_loopback_index() {
        let sock =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        // Döngü arayüzünün dizini Windows'ta 1'dir.
        set_unicast_if_v4(&sock, 1).unwrap();
    }

    #[test]
    fn best_interface_for_loopback_is_loopback() {
        assert_eq!(best_interface_v4(Ipv4Addr::LOCALHOST).unwrap(), 1);
    }

    #[test]
    fn process_metrics_are_plausible() {
        let (cpu, private) = process_metrics().unwrap();
        assert!(private > 100 * 1024, "özel bellek: {private}");
        assert!(cpu < Duration::from_secs(3600));
    }

    /// Gerçek DLL gerekir (`lab/bin`, git dışı): `cargo test -- --ignored real_dll`.
    /// Yalnızca yükler ve sembolleri çözer; adaptör oluşturmaz, sistemi değiştirmez.
    #[test]
    #[ignore]
    fn real_dll_loads_and_resolves_symbols() {
        let dll = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../lab/bin/wintun-0.14.1/wintun/bin/amd64/wintun.dll");
        let lib = Wintun::load(&dll, WINTUN_DLL_SHA256).unwrap();
        // Sürücü yüklü değilse None; yüklüyse 0.14 beklenir.
        if let Some(v) = lib.running_driver_version() {
            assert_eq!(v, (0, 14));
        }
    }
}
