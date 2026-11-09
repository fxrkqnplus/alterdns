// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Gerçek sistem katmanı (`SystemOps`). Faz 0 Görev 8'de yalnızca okuma sorguları ve
//! geçici dizindeki test dosyası (`FileValue`) yazılabilir. Adaptör, adres ve rota yazımı
//! Görev 7'de (Wintun spike'ı), hop limit yazımı yalnızca `hoplimit.rs`'de (Görev 9, D5) eklenir.

use std::process::Command;

use anyhow::{Context, bail};

use super::journal::{IpFamily, OpKind, SystemOps};

pub struct RealOps;

/// PowerShell'e gömülecek değer: tek tırnak içermemeli.
fn quoted(v: &str) -> anyhow::Result<String> {
    if v.contains('\'') {
        bail!("geçersiz değer: {v}");
    }
    Ok(format!("'{v}'"))
}

/// Salt okunur PowerShell sorgusu; çıktının ilk satırını döndürür.
fn ps_query(script: &str) -> anyhow::Result<String> {
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .context("powershell.exe çalıştırılamadı")?;
    if !out.status.success() {
        bail!(
            "sorgu başarısız: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string())
}

fn ensure_temp(path: &std::path::Path) -> anyhow::Result<()> {
    if !path.starts_with(std::env::temp_dir()) {
        bail!(
            "FileValue yalnızca geçici dizin altında kullanılabilir: {}",
            path.display()
        );
    }
    Ok(())
}

impl SystemOps for RealOps {
    fn read(&self, op: &OpKind) -> anyhow::Result<String> {
        match op {
            OpKind::FileValue { path } => {
                ensure_temp(path)?;
                Ok(std::fs::read_to_string(path).unwrap_or_else(|_| "absent".into()))
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
                ensure_temp(path)?;
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
                "{other:?} yazımı bu görevde desteklenmiyor (adaptör/rota: Görev 7; hop limit: Görev 9)"
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
            path: std::path::PathBuf::from(r"C:\Windows\win.ini"),
        };
        assert!(RealOps.read(&op).is_err());
        assert!(RealOps.write(&op, "x").is_err());
    }

    #[test]
    fn quoted_rejects_single_quote() {
        assert!(quoted("a'b").is_err());
        assert_eq!(quoted("AlterDNS-Spike").unwrap(), "'AlterDNS-Spike'");
    }
}
