// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows servisi (K11): SCM işlemleri ve servis ana döngüsü için ince sarmalayıcı.
//!
//! Kurulan servis: otomatik başlangıç, LocalSystem, tırnaklı yol, hizmet SID'i `Unrestricted`,
//! 5 / 30 / 60 sn sonra yeniden başlatma (sayaç 86 400 sn), hata koduyla duranda da kurtarma,
//! yalnızca gereken ayrıcalıklar.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::System::Services::{
    ChangeServiceConfig2W, QueryServiceConfig2W, SC_HANDLE,
    SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO, SERVICE_REQUIRED_PRIVILEGES_INFOW,
};
use windows::core::PWSTR;
use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
    ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
    ServiceInfo, ServiceSidType, ServiceStartType, ServiceState as WsState, ServiceStatus,
    ServiceType,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

/// Servis işleminin belirtecinde kalan ayrıcalıklar. Faz 1 işleri (pipe, tanımlama düzeyinde
/// belirteç okuma, kendi kayıt defteri anahtarı ve dosyaları) başka ayrıcalık istemez; servisin
/// başlattığı `alterdns-maint` de bu belirteci devralır. Faz 2'de motorla genişler.
///
/// `SeImpersonatePrivilege` bilerek yoktur: istemci tanımlama düzeyinde bağlanır, kimlik okumak
/// için yeterlidir. Bu ayrıcalık yerel yetki yükseltme saldırılarında sık kullanılır.
pub const REQUIRED_PRIVILEGES: [&str; 1] = ["SeChangeNotifyPrivilege"];
pub const RESTART_DELAYS_SECS: [u64; 3] = [5, 30, 60];
pub const RESET_PERIOD_SECS: u64 = 86_400;

/// `ERROR_SERVICE_DOES_NOT_EXIST` ve `ERROR_SERVICE_MARKED_FOR_DELETE`.
const SERVICE_DOES_NOT_EXIST: i32 = 1060;
const SERVICE_MARKED_FOR_DELETE: i32 = 1072;

fn io_err(e: windows_service::Error) -> io::Error {
    match e {
        windows_service::Error::Winapi(inner) => inner,
        other => io::Error::other(other),
    }
}

fn missing(e: &io::Error) -> bool {
    e.raw_os_error() == Some(SERVICE_DOES_NOT_EXIST)
}

fn manager(extra: ServiceManagerAccess) -> io::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | extra)
        .map_err(io_err)
}

pub struct ServiceSpec<'a> {
    pub name: &'a str,
    pub display_name: &'a str,
    pub binary: &'a Path,
}

fn failure_actions() -> ServiceFailureActions {
    ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(RESET_PERIOD_SECS)),
        reboot_msg: None,
        command: None,
        actions: Some(
            RESTART_DELAYS_SECS
                .iter()
                .map(|s| ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(*s),
                })
                .collect(),
        ),
    }
}

fn raw(svc: &Service) -> SC_HANDLE {
    SC_HANDLE(svc.raw_handle())
}

fn set_required_privileges(svc: &Service, privileges: &[&str]) -> io::Result<()> {
    // REG_MULTI_SZ: NUL ile ayrılmış, çift NUL ile biten liste.
    let mut multi: Vec<u16> = privileges
        .iter()
        .flat_map(|p| p.encode_utf16().chain(std::iter::once(0)))
        .chain(std::iter::once(0))
        .collect();
    let info = SERVICE_REQUIRED_PRIVILEGES_INFOW {
        pmszRequiredPrivileges: PWSTR(multi.as_mut_ptr()),
    };
    // SAFETY: Tutamaç `CHANGE_CONFIG` hakkıyla açık bir servistir; yapı ve liste çağrı boyunca
    // yaşar.
    unsafe {
        ChangeServiceConfig2W(
            raw(svc),
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            Some(std::ptr::from_ref(&info).cast()),
        )
    }
    .map_err(io::Error::other)
}

fn required_privileges(svc: &Service) -> io::Result<Vec<String>> {
    let mut needed = 0u32;
    // SAFETY: Yalnızca boyut sorulur.
    let _ = unsafe {
        QueryServiceConfig2W(
            raw(svc),
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            None,
            &mut needed,
        )
    };
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    // `usize` dizisi: yapı işaretçi hizası ister.
    let mut buf = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: Arabellek en az `needed` bayttır ve yazılabilir.
    let bytes =
        unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), needed as usize) };
    // SAFETY: Tutamaç `QUERY_CONFIG` hakkıyla açıktır; arabellek yeterince büyüktür.
    unsafe {
        QueryServiceConfig2W(
            raw(svc),
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            Some(bytes),
            &mut needed,
        )
    }
    .map_err(io::Error::other)?;
    // SAFETY: Arabellek bir `SERVICE_REQUIRED_PRIVILEGES_INFOW` ile başlar; liste işaretçisi
    // aynı arabelleğin içindeki çift NUL ile biten UTF-16 dizesine işaret eder.
    let list = unsafe {
        (*buf.as_ptr().cast::<SERVICE_REQUIRED_PRIVILEGES_INFOW>()).pmszRequiredPrivileges
    };
    if list.0.is_null() {
        return Ok(Vec::new());
    }
    // Liste arabelleğin içinde olmalı; okuma arabelleğin sonuyla sınırlıdır.
    let base = buf.as_ptr() as usize;
    let start = (list.0 as usize)
        .checked_sub(base)
        .filter(|s| *s < needed as usize && s % 2 == 0)
        .ok_or_else(|| io::Error::other("ayrıcalık listesi arabellek dışında"))?;
    // SAFETY: `start` arabelleğin içinde ve 2'ye hizalıdır; uzunluk arabelleğin sonunu aşmaz.
    let words = unsafe {
        std::slice::from_raw_parts(
            buf.as_ptr().cast::<u8>().add(start).cast::<u16>(),
            (needed as usize - start) / 2,
        )
    };
    Ok(words
        .split(|w| *w == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect())
}

/// Servisi K11 ayarlarıyla oluşturur. Ayarlardan biri uygulanamazsa servis silinir.
pub fn create_service(spec: &ServiceSpec<'_>) -> io::Result<()> {
    let mgr = manager(ServiceManagerAccess::CREATE_SERVICE)?;
    let info = ServiceInfo {
        name: spec.name.into(),
        display_name: spec.display_name.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        // Boşluk içeren yol kütüphanece tırnaklanır.
        executable_path: spec.binary.to_path_buf(),
        launch_arguments: Vec::new(),
        dependencies: Vec::new(),
        account_name: None,
        account_password: None,
    };
    let svc = mgr
        .create_service(
            &info,
            ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_CONFIG | ServiceAccess::DELETE,
        )
        .map_err(io_err)?;
    let configured = (|| -> io::Result<()> {
        svc.set_config_service_sid_info(ServiceSidType::Unrestricted)
            .map_err(io_err)?;
        svc.update_failure_actions(failure_actions())
            .map_err(io_err)?;
        svc.set_failure_actions_on_non_crash_failures(true)
            .map_err(io_err)?;
        set_required_privileges(&svc, &REQUIRED_PRIVILEGES)
    })();
    if let Err(e) = configured {
        let _ = svc.delete();
        return Err(e);
    }
    Ok(())
}

fn wait_state(svc: &Service, want: WsState, wait: Duration) -> io::Result<ServiceStatus> {
    let deadline = Instant::now() + wait;
    loop {
        let st = svc.query_status().map_err(io_err)?;
        if st.current_state == want || Instant::now() >= deadline {
            return Ok(st);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Servisi başlatır ve çalışır duruma gelmesini bekler.
pub fn start_service(name: &str, wait: Duration) -> io::Result<()> {
    let svc = manager(ServiceManagerAccess::empty())?
        .open_service(name, ServiceAccess::START | ServiceAccess::QUERY_STATUS)
        .map_err(io_err)?;
    svc.start::<&str>(&[]).map_err(io_err)?;
    let st = wait_state(&svc, WsState::Running, wait)?;
    if st.current_state == WsState::Running {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "servis çalışmadı: {:?}, çıkış {:?}",
            st.current_state, st.exit_code
        )))
    }
}

/// Servisi durdurur ve siler; SCM'den kaybolmasını bekler. Servis yoksa `Ok`.
pub fn delete_service(name: &str, stop_wait: Duration) -> io::Result<()> {
    let mgr = manager(ServiceManagerAccess::empty())?;
    let svc = match mgr.open_service(
        name,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    ) {
        Ok(s) => s,
        Err(e) => {
            let e = io_err(e);
            return if missing(&e) { Ok(()) } else { Err(e) };
        }
    };
    if svc.query_status().map_err(io_err)?.current_state != WsState::Stopped {
        let _ = svc.stop();
        if wait_state(&svc, WsState::Stopped, stop_wait)?.current_state != WsState::Stopped {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "servis durmadı"));
        }
    }
    svc.delete().map_err(io_err)?;
    drop(svc);
    let deadline = Instant::now() + stop_wait;
    loop {
        if let Err(e) = mgr.open_service(name, ServiceAccess::QUERY_STATUS) {
            let e = io_err(e);
            if missing(&e) {
                return Ok(());
            }
            if e.raw_os_error() != Some(SERVICE_MARKED_FOR_DELETE) {
                return Err(e);
            }
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "servis silinmek üzere işaretli kaldı",
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceState {
    /// SCM'nin sakladığı komut satırı (tırnaklarıyla).
    pub binary_path: String,
    pub running: bool,
}

/// Servis yoksa `None`. Yönetici gerekmez.
pub fn query_service(name: &str) -> io::Result<Option<ServiceState>> {
    let svc = match manager(ServiceManagerAccess::empty())?.open_service(
        name,
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(s) => s,
        Err(e) => {
            let e = io_err(e);
            return if missing(&e) { Ok(None) } else { Err(e) };
        }
    };
    let cfg = svc.query_config().map_err(io_err)?;
    let st = svc.query_status().map_err(io_err)?;
    Ok(Some(ServiceState {
        binary_path: cfg.executable_path.to_string_lossy().into_owned(),
        running: st.current_state == WsState::Running,
    }))
}

/// T-SVC-003 için kurulu servisin ayarları.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceConfigReport {
    pub binary_path_raw: String,
    pub auto_start: bool,
    pub local_system: bool,
    pub sid_unrestricted: bool,
    pub restart_delays_secs: Vec<u64>,
    pub reset_period_secs: Option<u64>,
    pub non_crash_failures: bool,
    pub required_privileges: Vec<String>,
}

pub fn service_config_report(name: &str) -> io::Result<ServiceConfigReport> {
    let svc = manager(ServiceManagerAccess::empty())?
        .open_service(name, ServiceAccess::QUERY_CONFIG)
        .map_err(io_err)?;
    let cfg = svc.query_config().map_err(io_err)?;
    let actions = svc.get_failure_actions().map_err(io_err)?;
    Ok(ServiceConfigReport {
        binary_path_raw: cfg.executable_path.to_string_lossy().into_owned(),
        auto_start: cfg.start_type == ServiceStartType::AutoStart,
        local_system: cfg
            .account_name
            .as_ref()
            .is_some_and(|a| a.to_string_lossy().eq_ignore_ascii_case("LocalSystem")),
        sid_unrestricted: svc.get_config_service_sid_info().map_err(io_err)?
            == ServiceSidType::Unrestricted,
        restart_delays_secs: actions
            .actions
            .unwrap_or_default()
            .iter()
            .filter(|a| a.action_type == ServiceActionType::Restart)
            .map(|a| a.delay.as_secs())
            .collect(),
        reset_period_secs: match actions.reset_period {
            ServiceFailureResetPeriod::After(d) => Some(d.as_secs()),
            ServiceFailureResetPeriod::Never => None,
        },
        non_crash_failures: svc
            .get_failure_actions_on_non_crash_failures()
            .map_err(io_err)?,
        required_privileges: required_privileges(&svc)?,
    })
}

/// Servis tarafı: SCM'ye durum bildirir.
pub struct ServiceStatusReporter(ServiceStatusHandle);

impl ServiceStatusReporter {
    fn set(
        &self,
        state: WsState,
        accept: ServiceControlAccept,
        exit: ServiceExitCode,
        wait: Duration,
    ) -> io::Result<()> {
        self.0
            .set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: state,
                controls_accepted: accept,
                exit_code: exit,
                checkpoint: 0,
                wait_hint: wait,
                process_id: None,
            })
            .map_err(io_err)
    }

    pub fn running(&self) -> io::Result<()> {
        self.set(
            WsState::Running,
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            ServiceExitCode::NO_ERROR,
            Duration::ZERO,
        )
    }

    pub fn stop_pending(&self) -> io::Result<()> {
        self.set(
            WsState::StopPending,
            ServiceControlAccept::empty(),
            ServiceExitCode::NO_ERROR,
            Duration::from_secs(10),
        )
    }

    /// `service_specific`: servise özgü çıkış kodu (hata); `None` başarılı durma.
    pub fn stopped(&self, service_specific: Option<u32>) -> io::Result<()> {
        self.set(
            WsState::Stopped,
            ServiceControlAccept::empty(),
            service_specific.map_or(ServiceExitCode::NO_ERROR, ServiceExitCode::ServiceSpecific),
            Duration::ZERO,
        )
    }
}

/// Durdurma ve kapanma denetimlerini `on_stop`'a bağlar.
pub fn register_stop_handler(
    name: &str,
    on_stop: impl Fn() + Send + 'static,
) -> io::Result<ServiceStatusReporter> {
    let handle = service_control_handler::register(name, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            on_stop();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })
    .map_err(io_err)?;
    Ok(ServiceStatusReporter(handle))
}

static SERVICE_MAIN: OnceLock<fn(Vec<OsString>)> = OnceLock::new();

fn service_main_trampoline(arguments: Vec<OsString>) {
    if let Some(main) = SERVICE_MAIN.get() {
        main(arguments);
    }
}

windows_service::define_windows_service!(ffi_service_main, service_main_trampoline);

/// Servis dağıtıcısını başlatır; servis durana kadar döner. SCM dışından çalıştırılırsa hemen
/// hata verir.
pub fn dispatch(name: &str, main: fn(Vec<OsString>)) -> io::Result<()> {
    let _ = SERVICE_MAIN.set(main);
    windows_service::service_dispatcher::start(name, ffi_service_main).map_err(io_err)
}
