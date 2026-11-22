// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gerçek sistem katmanı (`SystemOps`). Faz 0 Görev 8'de yalnızca okuma sorguları ve
//! geçici dizindeki test dosyası (`FileValue`) yazılabilir. Adaptör, adres ve rota yazımı
//! Görev 7'de (Wintun spike'ı) eklenir; sistem hop limiti yazılmaz (ADR-010).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};

use super::journal::{IpFamily, OpKind, SPIKE_ADAPTER, SystemOps, lab_temp_path};

pub struct RealOps;

/// PowerShell sorgusunun üst süresi (bekçi kirasının altında kalır).
const PS_TIMEOUT: Duration = Duration::from_secs(15);

/// System32 altındaki bir sistem aracı. Çalışma dizinindeki aynı adlı bir dosyanın araya
/// girmemesi için mutlak yol kullanılır. Dizin `SystemRoot` ortam değişkeninden değil, Win32
/// API'sinden alınır (Faz 0 S-2); API başarısız olursa sabit varsayılan kullanılır.
pub fn system32(exe: &str) -> PathBuf {
    alterdns_winsys::system_dir()
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\System32"))
        .join(exe)
}

pub(crate) fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
}

/// PowerShell komutu; modüller yalnızca sistem dizininden yüklenir. Devralınan `PSModulePath`
/// kullanıcının yazabildiği `Documents\WindowsPowerShell\Modules` dizinini içerir; yükseltilmiş
/// işlemde orada aynı adlı bir komut dışa aktaran modül yüklenmesin (2026-11-15 güvenlik incelemesi).
pub(crate) fn ps_command() -> Command {
    let mut cmd = Command::new(powershell());
    cmd.env(
        "PSModulePath",
        system32(r"WindowsPowerShell\v1.0\Modules").as_os_str(),
    );
    cmd
}

/// Komut dizesine gömülebilecek değer: ASCII harf, rakam ve `._:/-`; en çok 64 karakter.
/// (PowerShell tipografik tırnakları da dize sınırlayıcı sayar; izin listesi bunları dışarıda tutar.)
pub(crate) fn valid_token(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
}

/// PowerShell'e gömülecek değer, tek tırnak içinde.
pub(crate) fn quoted(v: &str) -> anyhow::Result<String> {
    if !valid_token(v) {
        bail!("geçersiz değer: {v:?}");
    }
    Ok(format!("'{v}'"))
}

/// Nesne var mı? Yalnızca "bulunamadı" (`ObjectNotFound`) hatası `absent` sayılır. Geçici bir
/// CIM hatası "yok" diye okunup kaldırılmamış bir rota journal'da geri alınmış sayılmasın
/// (2026-11-15 güvenlik incelemesi). `cmd` yalnızca `quoted` değerler içerir.
fn presence_script(cmd: &str) -> String {
    format!(
        "try {{ $null = {cmd} -ErrorAction Stop; 'present' }} catch {{ if ($_.CategoryInfo.Category -eq 'ObjectNotFound') {{ 'absent' }} else {{ 'hata: ' + $_.Exception.Message }} }}"
    )
}

fn parse_presence(out: &str) -> anyhow::Result<String> {
    match out.trim() {
        v @ ("present" | "absent") => Ok(v.to_string()),
        other => bail!("nesne durumu okunamadı: {other}"),
    }
}

fn presence(cmd: &str) -> anyhow::Result<String> {
    parse_presence(&ps_query(&presence_script(cmd))?)
}

/// Salt okunur PowerShell sorgusu; çıktının ilk satırını döndürür. 15 sn'de bitmezse sonlandırılır.
pub fn ps_query(script: &str) -> anyhow::Result<String> {
    let mut child = ps_command()
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("powershell.exe çalıştırılamadı")?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + PS_TIMEOUT;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "PowerShell sorgusu {} sn içinde bitmedi",
                PS_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out.join().map_err(|_| anyhow!("çıktı okunamadı"))?;
    let stderr = err.join().map_err(|_| anyhow!("hata çıktısı okunamadı"))?;
    if !status.success() {
        bail!(
            "sorgu başarısız: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string())
}

/// `FileValue` yolu: sözcüksel kural + üst dizinin gerçek yolu geçici dizinde + dosya bağlantı değil.
fn ensure_lab_temp(path: &Path) -> anyhow::Result<()> {
    if !lab_temp_path(path) {
        bail!(
            "FileValue yalnızca %TEMP%\\alterdns-* altında kullanılabilir: {}",
            path.display()
        );
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("üst dizin yok: {}", path.display()))?;
    let real_parent = std::fs::canonicalize(parent)?;
    let real_temp = std::fs::canonicalize(std::env::temp_dir())?;
    if !real_parent.starts_with(&real_temp) {
        bail!(
            "FileValue dizini bağlantıyla geçici dizin dışına çıkıyor: {}",
            path.display()
        );
    }
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("FileValue bir bağlantı olamaz: {}", path.display());
    }
    Ok(())
}

/// Adres/rota yazma betiği. Kurallar (sahiplik kuralına ek, savunma derinliği):
/// - Yalnızca `AlterDNS-Spike` arayüzü; yalnızca `ActiveStore` (yeniden başlatmada kalkar).
/// - Rota yalnızca tek hedef IP'dir (IPv4 /32 veya IPv6 /128); geniş önek ve varsayılan rota yok.
/// - Adres yalnızca RFC 2544 test aralığındadır (198.18.0.0/15) ve önek /24…/31'dir.
/// - Değer yalnızca `present` (ekle) veya `absent` (kaldır).
pub(crate) fn write_script(op: &OpKind, value: &str) -> anyhow::Result<String> {
    use std::net::IpAddr;
    let add = match value {
        "present" => true,
        "absent" => false,
        other => bail!("geçersiz hedef değer: {other:?}"),
    };
    match op {
        OpKind::Route { prefix, if_name } => {
            if if_name != SPIKE_ADAPTER {
                bail!("rota yalnızca {SPIKE_ADAPTER} arayüzüne yazılır: {if_name}");
            }
            let (ip, len) = prefix
                .split_once('/')
                .ok_or_else(|| anyhow!("önek yok: {prefix}"))?;
            let parsed = ip
                .parse::<IpAddr>()
                .map_err(|_| anyhow!("geçersiz rota hedefi: {prefix}"))?;
            if !routable_global(parsed) {
                bail!(
                    "rota hedefi genel bir adres olmalı (yerel ağ, DNS ya da ağ geçidi TUN'a çekilmez): {prefix}"
                );
            }
            route_prefix_ok(parsed, len)
                .map_err(|why| anyhow!("rota reddedildi ({why}): {prefix}"))?;
            let (p, i) = (quoted(prefix)?, quoted(if_name)?);
            Ok(if add {
                format!(
                    "New-NetRoute -DestinationPrefix {p} -InterfaceAlias {i} -PolicyStore ActiveStore -ErrorAction Stop | Out-Null"
                )
            } else {
                format!(
                    "Remove-NetRoute -DestinationPrefix {p} -InterfaceAlias {i} -PolicyStore ActiveStore -Confirm:$false -ErrorAction Stop"
                )
            })
        }
        OpKind::UnicastAddress { if_name, cidr } => {
            if if_name != SPIKE_ADAPTER {
                bail!("adres yalnızca {SPIKE_ADAPTER} arayüzüne yazılır: {if_name}");
            }
            let (ip, len) = cidr
                .split_once('/')
                .ok_or_else(|| anyhow!("önek yok: {cidr}"))?;
            let v4: std::net::Ipv4Addr =
                ip.parse().map_err(|_| anyhow!("geçersiz adres: {cidr}"))?;
            let len: u8 = len.parse().map_err(|_| anyhow!("geçersiz önek: {cidr}"))?;
            let [a, b, ..] = v4.octets();
            if !(a == 198 && (b == 18 || b == 19)) || !(24..=31).contains(&len) {
                bail!("adres yalnızca 198.18.0.0/15 test aralığında, /24…/31: {cidr}");
            }
            let (i, q) = (quoted(if_name)?, quoted(ip)?);
            Ok(if add {
                format!(
                    "New-NetIPAddress -InterfaceAlias {i} -IPAddress {q} -PrefixLength {len} -PolicyStore ActiveStore -ErrorAction Stop | Out-Null"
                )
            } else {
                format!(
                    "Remove-NetIPAddress -InterfaceAlias {i} -IPAddress {q} -Confirm:$false -ErrorAction Stop"
                )
            })
        }
        OpKind::TunAdapter { .. } => {
            bail!("adaptörü Wintun DLL'i oluşturur (record_external); kaldırma pnputil ile yapılır")
        }
        OpKind::HopLimit { .. } => bail!("sistem hop limiti yazılmaz (ADR-010)"),
        OpKind::FileValue { .. } => bail!("FileValue betikle yazılmaz"),
    }
}

/// Rotalanabilir genel bir adres mi? Özel, döngü, bağlantı yerel, CGNAT, test, çoklu yayın,
/// yayın ve ayrılmış aralıklar `false`.
/// Rota öneki denetimi. IPv6 yalnızca tek IP (/128). IPv4 /16…/32 olabilir (sağlayıcının
/// kendi bloğu, ör. oyun sunucuları): ağ biçiminde olmalı, ilk ve son adresi genel olmalı.
/// Ayrılmış blokların hepsi (özel, CGNAT, bağlantı yerel, 198.18.0.0/15, çoklu yayın…) /16'ya
/// hizalı ya da daha geniş olduğundan, /16 veya daha dar bir önek bunları ya tamamen içerir
/// (ilk adres yakalar) ya da hiç içermez: yerel ağ, ağ geçidi ve DNS TUN'a çekilemez.
fn route_prefix_ok(ip: std::net::IpAddr, len: &str) -> Result<(), &'static str> {
    let len: u8 = len.parse().map_err(|_| "önek uzunluğu sayı değil")?;
    match ip {
        std::net::IpAddr::V6(_) => (len == 128).then_some(()).ok_or("IPv6'da yalnızca /128"),
        std::net::IpAddr::V4(v4) => {
            if !(16..=32).contains(&len) {
                return Err("IPv4 öneki /16…/32 olmalı");
            }
            let bits = u32::from(v4);
            let host_mask = u32::MAX.checked_shr(u32::from(len)).unwrap_or(0);
            if bits & host_mask != 0 {
                return Err("önek ağ biçiminde değil");
            }
            let last = std::net::IpAddr::V4(std::net::Ipv4Addr::from(bits | host_mask));
            if routable_global(ip) && routable_global(last) {
                Ok(())
            } else {
                Err("önek genel olmayan adres içeriyor")
            }
        }
    }
}

pub(crate) fn routable_global(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || (a == 192 && b == 0 && c == 0))
        }
        std::net::IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xffc0) == 0xfe80
                || (first & 0xfe00) == 0xfc00
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// Wintun yazılım aygıtının PnP örnek kimliği: `SWD\WINTUN\{GUID}`.
pub(crate) fn valid_wintun_instance_id(id: &str) -> bool {
    // Windows kimliği karışık harfle bildirebilir (`SWD\Wintun\{…}`).
    let prefix = r"SWD\WINTUN\{";
    let Some(head) = id.get(..prefix.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(prefix) {
        return false;
    }
    let Some(guid) = id.get(prefix.len()..).and_then(|r| r.strip_suffix('}')) else {
        return false;
    };
    let parts: Vec<&str> = guid.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8usize, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Spike adaptörü mü? Açıklama, Wintun'un tünel türünden ürettiği "AlterDNS Tunnel" (yinelenen
/// adlarda " #2" eki) olmalı ve PnP kimliği Wintun yazılım aygıtı olmalı.
pub(crate) fn spike_device_ok(desc: &str, id: &str) -> bool {
    desc.starts_with(SPIKE_ADAPTER_DESCRIPTION) && valid_wintun_instance_id(id)
}

/// Spike'ın Wintun tünel türü "AlterDNS"; Windows açıklamaya " Tunnel" ekler.
pub const SPIKE_ADAPTER_DESCRIPTION: &str = "AlterDNS Tunnel";

/// Adaptör, işlem çöktüğü için kalakaldıysa kaldırır: yalnızca adı `AlterDNS-Spike`, açıklaması
/// Wintun ve PnP kimliği `SWD\WINTUN\{GUID}` olan aygıt, `pnputil /remove-device` ile.
fn remove_spike_adapter(name: &str) -> anyhow::Result<()> {
    if name != SPIKE_ADAPTER {
        bail!("yalnızca {SPIKE_ADAPTER} kaldırılır: {name}");
    }
    let info = ps_query(&format!(
        "$a = Get-NetAdapter -Name {} -ErrorAction Stop; $a.InterfaceDescription + '|' + $a.PnPDeviceID",
        quoted(name)?
    ))?;
    let (desc, id) = info
        .split_once('|')
        .ok_or_else(|| anyhow!("adaptör bilgisi okunamadı"))?;
    if !spike_device_ok(desc, id) {
        bail!("{SPIKE_ADAPTER} bir Wintun aygıtı değil ({desc}, {id}); dokunulmadı");
    }
    let status = Command::new(system32("pnputil.exe"))
        .args(["/remove-device", id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("pnputil çalıştırılamadı")?;
    if !status.success() {
        bail!("pnputil /remove-device başarısız: {status}");
    }
    Ok(())
}

impl SystemOps for RealOps {
    fn read(&self, op: &OpKind) -> anyhow::Result<String> {
        match op {
            OpKind::FileValue { path } => {
                ensure_lab_temp(path)?;
                match std::fs::read_to_string(path) {
                    Ok(v) => Ok(v),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("absent".into()),
                    Err(e) => Err(e.into()),
                }
            }
            OpKind::HopLimit { family } => ps_query(match family {
                IpFamily::V4 => "(Get-NetIPv4Protocol).DefaultHopLimit",
                IpFamily::V6 => "(Get-NetIPv6Protocol).DefaultHopLimit",
            }),
            OpKind::TunAdapter { name } => {
                presence(&format!("Get-NetAdapter -Name {}", quoted(name)?))
            }
            OpKind::UnicastAddress { if_name, cidr } => {
                let ip = cidr.split('/').next().unwrap_or(cidr);
                presence(&format!(
                    "Get-NetIPAddress -InterfaceAlias {} -IPAddress {}",
                    quoted(if_name)?,
                    quoted(ip)?
                ))
            }
            OpKind::Route { prefix, if_name } => presence(&format!(
                "Get-NetRoute -DestinationPrefix {} -InterfaceAlias {}",
                quoted(prefix)?,
                quoted(if_name)?
            )),
        }
    }

    fn write(&self, op: &OpKind, value: &str) -> anyhow::Result<()> {
        match op {
            OpKind::FileValue { path } => {
                ensure_lab_temp(path)?;
                if value == "absent" {
                    match std::fs::remove_file(path) {
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                        _ => {}
                    }
                } else {
                    std::fs::write(path, value)?;
                }
                Ok(())
            }
            op if !super::journal::owned(op) => {
                bail!("lab bu öğeye dokunamaz (sahiplik kuralı): {op:?}")
            }
            OpKind::TunAdapter { name } if value == "absent" => remove_spike_adapter(name),
            op => {
                ps_query(&write_script(op, value)?)?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_value_outside_temp_is_rejected() {
        let op = OpKind::FileValue {
            path: PathBuf::from(r"C:\Windows\win.ini"),
        };
        assert!(RealOps.read(&op).is_err());
        assert!(RealOps.write(&op, "x").is_err());
        let traversal = OpKind::FileValue {
            path: std::env::temp_dir()
                .join("alterdns-x")
                .join("..")
                .join("..")
                .join("x.txt"),
        };
        assert!(RealOps.read(&traversal).is_err());
        assert!(RealOps.write(&traversal, "x").is_err());
    }

    #[test]
    fn quoted_rejects_single_quote() {
        for bad in [
            "a'b",
            "a\u{2018}b",
            "a\u{2019}b",
            "a\u{201A}b",
            "a\u{201B}b",
            "a b",
            "a;b",
            "$x",
            "",
        ] {
            assert!(quoted(bad).is_err(), "{bad:?}");
        }
        assert_eq!(quoted("AlterDNS-Spike").unwrap(), "'AlterDNS-Spike'");
        assert!(quoted("203.0.113.7/32").is_ok());
        assert!(quoted("2001:db8::1/128").is_ok());
    }

    #[test]
    fn presence_distinguishes_not_found_from_errors() {
        let s = presence_script("Get-NetRoute -DestinationPrefix '203.0.113.7/32'");
        assert!(s.contains("-ErrorAction Stop"));
        assert!(s.contains("'ObjectNotFound'"));
        assert!(!s.contains("SilentlyContinue"));
        assert_eq!(parse_presence("present\r\n").unwrap(), "present");
        assert_eq!(parse_presence("absent").unwrap(), "absent");
        assert!(parse_presence("hata: CIM sunucusu yanıt vermedi").is_err());
        assert!(parse_presence("").is_err());
    }

    #[test]
    fn powershell_loads_modules_only_from_system_dir() {
        let cmd = ps_command();
        let module_path = cmd
            .get_envs()
            .find(|(k, _)| *k == "PSModulePath")
            .and_then(|(_, v)| v)
            .map(PathBuf::from)
            .unwrap();
        assert!(module_path.is_absolute());
        // Windows yolları büyük/küçük harf duyarsızdır; API `system32` döndürebilir.
        let lower = module_path.display().to_string().to_lowercase();
        assert!(
            lower.ends_with(r"system32\windowspowershell\v1.0\modules"),
            "{lower}"
        );
    }

    #[test]
    fn system_tools_use_absolute_paths() {
        assert!(powershell().is_absolute());
        let taskkill = system32("taskkill.exe");
        assert!(taskkill.is_file(), "{}", taskkill.display());
        let lower = taskkill.display().to_string().to_lowercase();
        assert!(lower.ends_with(r"system32\taskkill.exe"), "{lower}");
    }

    fn route(prefix: &str, if_name: &str) -> OpKind {
        OpKind::Route {
            prefix: prefix.into(),
            if_name: if_name.into(),
        }
    }

    fn addr(cidr: &str, if_name: &str) -> OpKind {
        OpKind::UnicastAddress {
            if_name: if_name.into(),
            cidr: cidr.into(),
        }
    }

    #[test]
    fn route_script_accepts_provider_prefixes() {
        // Sağlayıcının kendi bloğu (ör. Roblox AS22697): /16…/32, ağ biçiminde.
        for ok in [
            "128.116.0.0/17",
            "103.140.28.0/23",
            "128.116.5.0/24",
            "1.1.1.1/32",
        ] {
            let add = write_script(&route(ok, SPIKE_ADAPTER), "present").unwrap();
            assert!(add.contains(&format!("-DestinationPrefix '{ok}'")), "{add}");
        }
    }

    #[test]
    fn route_script_is_host_route_in_active_store_only() {
        let add = write_script(&route("1.1.1.1/32", SPIKE_ADAPTER), "present").unwrap();
        assert!(add.starts_with("New-NetRoute "), "{add}");
        assert!(add.contains("-DestinationPrefix '1.1.1.1/32'"), "{add}");
        assert!(add.contains("-InterfaceAlias 'AlterDNS-Spike'"), "{add}");
        assert!(add.contains("-PolicyStore ActiveStore"), "{add}");
        let del = write_script(&route("1.1.1.1/32", SPIKE_ADAPTER), "absent").unwrap();
        assert!(del.starts_with("Remove-NetRoute "), "{del}");
        assert!(del.contains("-Confirm:$false") && del.contains("-PolicyStore ActiveStore"));
        // Belge aralığı, varsayılan rota, başka arayüz ve bilinmeyen değer reddedilir.
        for (op, v) in [
            (route("203.0.113.0/24", SPIKE_ADAPTER), "present"),
            // Önek /16'dan geniş olamaz, ağ biçiminde olmalı, ayrılmış blok içeremez.
            (route("128.0.0.0/15", SPIKE_ADAPTER), "present"),
            (route("128.116.1.0/17", SPIKE_ADAPTER), "present"),
            (
                route(&["192.168", ".0.0/16"].concat(), SPIKE_ADAPTER),
                "present",
            ),
            (
                route(&["172.16", ".0.0/16"].concat(), SPIKE_ADAPTER),
                "present",
            ),
            (route("100.64.0.0/16", SPIKE_ADAPTER), "present"),
            (route("198.18.0.0/16", SPIKE_ADAPTER), "present"),
            (route("192.0.0.0/16", SPIKE_ADAPTER), "present"),
            (route("2606:4700::/32", SPIKE_ADAPTER), "present"),
            (route("0.0.0.0/0", SPIKE_ADAPTER), "present"),
            (route("203.0.113.7/32", "Wi-Fi"), "present"),
            (route("203.0.113.7/32", SPIKE_ADAPTER), "maybe"),
            (route("not-an-ip/32", SPIKE_ADAPTER), "present"),
            // Değiştirilmiş journal ağ geçidini, DNS'i ya da yerel ağı TUN'a çekemesin.
            (
                route(&["192.168", ".1.1/32"].concat(), SPIKE_ADAPTER),
                "present",
            ),
            (
                route(&["10.0", ".0.1/32"].concat(), SPIKE_ADAPTER),
                "present",
            ),
            (route("127.0.0.1/32", SPIKE_ADAPTER), "present"),
            (route("198.18.0.2/32", SPIKE_ADAPTER), "present"),
            (route("224.0.0.251/32", SPIKE_ADAPTER), "present"),
        ] {
            assert!(write_script(&op, v).is_err(), "{op:?} {v}");
        }
    }

    #[test]
    fn address_script_only_in_rfc2544_range_on_spike_adapter() {
        let add = write_script(&addr("198.18.0.1/30", SPIKE_ADAPTER), "present").unwrap();
        assert!(add.starts_with("New-NetIPAddress "), "{add}");
        assert!(add.contains("-IPAddress '198.18.0.1'") && add.contains("-PrefixLength 30"));
        assert!(add.contains("-PolicyStore ActiveStore"), "{add}");
        let del = write_script(&addr("198.18.0.1/30", SPIKE_ADAPTER), "absent").unwrap();
        assert!(del.starts_with("Remove-NetIPAddress ") && del.contains("-Confirm:$false"));
        for op in [
            addr("203.0.113.9/30", SPIKE_ADAPTER),
            addr("198.18.0.1/30", "Ethernet"),
            addr("198.18.0.1/8", SPIKE_ADAPTER),
            addr("198.18.0.1", SPIKE_ADAPTER),
        ] {
            assert!(write_script(&op, "present").is_err(), "{op:?}");
        }
    }

    #[test]
    fn adapter_and_hop_limit_have_no_write_script() {
        let tun = OpKind::TunAdapter {
            name: SPIKE_ADAPTER.into(),
        };
        assert!(
            write_script(&tun, "present").is_err(),
            "adaptörü DLL oluşturur"
        );
        let hop = OpKind::HopLimit {
            family: IpFamily::V4,
        };
        assert!(write_script(&hop, "128").is_err(), "ADR-010");
        assert!(RealOps.write(&hop, "128").is_err(), "ADR-010");
    }

    #[test]
    fn pnp_instance_id_must_be_a_wintun_software_device() {
        assert!(valid_wintun_instance_id(
            r"SWD\WINTUN\{1F0C5B3A-8E2D-4C71-9A6B-2D4E8F1A7C30}"
        ));
        // Windows kimliği karışık harfle bildirir (S4 ikinci koşu, 2026-11-12).
        assert!(valid_wintun_instance_id(
            r"SWD\Wintun\{7C2E4B61-9D3A-4F58-B0E7-5A1CD2F48E93}"
        ));
        for bad in [
            r"PCI\VEN_8086&DEV_1234",
            r"SWD\WINTUN\{1F0C5B3A} & calc",
            r"SWD\WINTUN\..\x",
            "",
        ] {
            assert!(!valid_wintun_instance_id(bad), "{bad:?}");
        }
    }

    /// Spike adaptörünün açıklaması "AlterDNS Tunnel" (Wintun, tünel türünün sonuna " Tunnel"
    /// ekler); yalnız bu açıklama ve Wintun kimliği kabul edilir.
    #[test]
    fn spike_device_check_matches_real_adapter() {
        let id = r"SWD\Wintun\{7C2E4B61-9D3A-4F58-B0E7-5A1CD2F48E93}";
        assert!(spike_device_ok("AlterDNS Tunnel", id));
        assert!(spike_device_ok("AlterDNS Tunnel #2", id));
        assert!(!spike_device_ok("WireGuard Tunnel", id));
        assert!(!spike_device_ok("Intel(R) Wi-Fi", id));
        assert!(!spike_device_ok(
            "AlterDNS Tunnel",
            r"PCI\VEN_8086&DEV_1234"
        ));
    }
}
