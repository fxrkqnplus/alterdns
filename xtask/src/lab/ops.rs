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

use super::journal::{IpFamily, OpKind, SystemOps, lab_temp_path};

pub struct RealOps;

/// PowerShell sorgusunun üst süresi (bekçi kirasının altında kalır).
const PS_TIMEOUT: Duration = Duration::from_secs(15);

/// `%SystemRoot%\System32` altındaki bir sistem aracı. Çalışma dizinindeki aynı adlı bir
/// dosyanın araya girmemesi için mutlak yol kullanılır.
pub(crate) fn system32(exe: &str) -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join(exe)
}

pub(crate) fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
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

/// Salt okunur PowerShell sorgusu; çıktının ilk satırını döndürür. 15 sn'de bitmezse sonlandırılır.
pub(crate) fn ps_query(script: &str) -> anyhow::Result<String> {
    let mut child = Command::new(powershell())
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
            OpKind::TunAdapter { name } => ps_query(&format!(
                "if (Get-NetAdapter -Name {} -ErrorAction SilentlyContinue) {{ 'present' }} else {{ 'absent' }}",
                quoted(name)?
            )),
            OpKind::UnicastAddress { if_name, cidr } => {
                let ip = cidr.split('/').next().unwrap_or(cidr);
                ps_query(&format!(
                    "if (Get-NetIPAddress -InterfaceAlias {} -IPAddress {} -ErrorAction SilentlyContinue) {{ 'present' }} else {{ 'absent' }}",
                    quoted(if_name)?,
                    quoted(ip)?
                ))
            }
            OpKind::Route { prefix, if_name } => ps_query(&format!(
                "if (Get-NetRoute -DestinationPrefix {} -InterfaceAlias {} -ErrorAction SilentlyContinue) {{ 'present' }} else {{ 'absent' }}",
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
            other => bail!(
                "{other:?} yazımı bu görevde desteklenmiyor (adaptör/rota: Görev 7; hop limit yazılmaz, ADR-010)"
            ),
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
    fn system_tools_use_absolute_paths() {
        assert!(powershell().is_absolute());
        assert!(system32("taskkill.exe").ends_with(r"System32\taskkill.exe"));
    }
}
