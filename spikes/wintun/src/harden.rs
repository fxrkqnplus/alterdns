// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Yükseltilmiş çalıştırma öncesi sertleştirme denetimleri (güvenlik incelemesi, 2026-11-11):
//! bağlantı noktası (junction/symlink) içeren lab yolları, değiştirilmiş `SystemRoot`, kullanıcının
//! canlı bağlantısını kesebilecek bir test IP'si.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

/// `\\?\C:\…` → `C:\…`. UNC (`\\?\UNC\…`) ve tanınmayan biçimler reddedilir (`None`).
pub fn strip_verbatim(p: &str) -> Option<String> {
    let rest = p.strip_prefix(r"\\?\").unwrap_or(p);
    let mut chars = rest.chars();
    let drive = matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(d), Some(':'), Some('\\')) if d.is_ascii_alphabetic()
    );
    drive.then(|| rest.to_string())
}

/// Verilen yollardan bağlantı noktası (reparse point) olanlar. Var olmayan yol atlanır.
pub fn reparse_points(paths: &[PathBuf]) -> Vec<PathBuf> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    paths
        .iter()
        .filter(|p| {
            std::fs::symlink_metadata(p)
                .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        })
        .cloned()
        .collect()
}

/// `SystemRoot` ortam değişkeni gerçek Windows diziniyle aynı mı (büyük/küçük harf duyarsız)?
pub fn system_root_matches(env: Option<&str>, actual: &Path) -> bool {
    let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
    env.is_some_and(|e| norm(e) == norm(&actual.to_string_lossy()))
}

/// Test IP'si canlı bir şeyle çakışıyor mu? DNS sunucusu, varsayılan ağ geçidi ya da kurulu bir
/// TCP bağlantısının uzak adresi olamaz (rota o bağlantıyı TUN'a çeker ve keser).
pub fn ip_conflict(
    ip: Ipv4Addr,
    dns: &[String],
    gateways: &[String],
    established: &[String],
) -> Option<String> {
    let ip = ip.to_string();
    let has = |list: &[String]| list.iter().any(|a| a.trim() == ip);
    if has(dns) {
        Some(format!("test IP'si bir DNS sunucusu: {ip}"))
    } else if has(gateways) {
        Some(format!("test IP'si varsayılan ağ geçidi: {ip}"))
    } else if has(established) {
        Some(format!(
            "test IP'sine şu an açık bir TCP bağlantısı var (rota onu keserdi): {ip}"
        ))
    } else {
        None
    }
}

/// S4 rota kümesi bir DNS sunucusunu ya da varsayılan ağ geçidini içeriyorsa çakışma vardır:
/// rota o trafiği TUN'a çekerdi. (Önek kuralı özel aralıkları zaten dışarıda tutar; bu
/// denetim genel adresli DNS ve ağ geçitleri içindir.)
pub fn prefix_conflict(
    set: &crate::forward::RouteSet,
    dns: &[String],
    gateways: &[String],
) -> Option<String> {
    let inside = |list: &[String]| {
        list.iter()
            .filter_map(|a| a.trim().parse::<std::net::IpAddr>().ok())
            .find(|ip| set.contains(*ip))
    };
    if let Some(ip) = inside(dns) {
        Some(format!("S4 rotaları bir DNS sunucusunu içeriyor: {ip}"))
    } else {
        inside(gateways).map(|ip| format!("S4 rotaları varsayılan ağ geçidini içeriyor: {ip}"))
    }
}

/// Canlı ağ durumundan çakışma denetimi (salt okunur PowerShell sorguları).
pub fn live_ip_conflict(ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
    use xtask::lab::ops::ps_query;
    let list = |script: &str| -> anyhow::Result<Vec<String>> {
        Ok(ps_query(&format!("({script}) -join ','"))?
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect())
    };
    let dns = list(
        "Get-DnsClientServerAddress -AddressFamily IPv4 | ForEach-Object { $_.ServerAddresses }",
    )?;
    let gw = list(
        "Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | ForEach-Object { $_.NextHop }",
    )?;
    let est = list(
        "Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | ForEach-Object { $_.RemoteAddress }",
    )?;
    Ok(ip_conflict(ip, &dns, &gw, &est))
}

/// S4 rota kümesi için canlı çakışma denetimi (salt okunur).
pub fn live_prefix_conflict(set: &crate::forward::RouteSet) -> anyhow::Result<Option<String>> {
    use xtask::lab::ops::ps_query;
    let list = |script: &str| -> anyhow::Result<Vec<String>> {
        Ok(ps_query(&format!("({script}) -join ','"))?
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect())
    };
    let dns = list("Get-DnsClientServerAddress | ForEach-Object { $_.ServerAddresses }")?;
    let gw = list(
        "Get-NetRoute -DestinationPrefix '0.0.0.0/0', '::/0' -ErrorAction SilentlyContinue | ForEach-Object { $_.NextHop }",
    )?;
    Ok(prefix_conflict(set, &dns, &gw))
}

/// Yükseltilmiş komutların ön koşulları: `SystemRoot` gerçek dizin, lab yollarında bağlantı
/// noktası yok, test IP'si canlı trafik taşımıyor.
pub fn elevated_preconditions(root: &Path, test_ip: Ipv4Addr) -> anyhow::Result<()> {
    let actual = crate::ffi::windows_dir()?;
    if !system_root_matches(std::env::var("SystemRoot").ok().as_deref(), &actual) {
        anyhow::bail!(
            "SystemRoot ortam değişkeni gerçek Windows diziniyle ({}) aynı değil; çalıştırılmadı",
            actual.display()
        );
    }
    let rel = [
        "lab",
        "lab/bin",
        "lab/bin/wintun-0.14.1",
        "lab/bin/wintun-0.14.1/wintun",
        "lab/bin/wintun-0.14.1/wintun/bin",
        "lab/bin/wintun-0.14.1/wintun/bin/amd64",
        "lab/results",
        "lab/results/journal",
        "lab/local",
        "target",
        "target/xtask",
        "target/xtask/debug",
    ];
    let mut paths = vec![root.to_path_buf()];
    paths.extend(rel.iter().map(|r| root.join(r)));
    let found = reparse_points(&paths);
    if !found.is_empty() {
        anyhow::bail!("lab yollarında bağlantı noktası (junction/symlink) var: {found:?}");
    }
    if let Some(why) = live_ip_conflict(test_ip)? {
        anyhow::bail!("{why}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_conflict_catches_dns_and_gateway_inside_routes() {
        use crate::forward::RouteSet;
        let set = RouteSet::new(&[(Ipv4Addr::new(128, 116, 0, 0), 17)]).unwrap();
        set.add_host(Ipv4Addr::new(1, 1, 1, 1));
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert!(prefix_conflict(&set, &s(&["128.116.4.4"]), &[]).is_some());
        assert!(prefix_conflict(&set, &[], &s(&["1.1.1.1"])).is_some());
        assert!(
            prefix_conflict(&set, &s(&["8.8.8.8", "fe80::1", "x"]), &s(&["9.9.9.9"])).is_none()
        );
    }

    #[test]
    fn strip_verbatim_only_accepts_drive_paths() {
        assert_eq!(
            strip_verbatim(r"\\?\C:\Sistem\x\wintun.dll").as_deref(),
            Some(r"C:\Sistem\x\wintun.dll")
        );
        assert_eq!(
            strip_verbatim(r"C:\x\y.dll").as_deref(),
            Some(r"C:\x\y.dll")
        );
        assert_eq!(strip_verbatim(r"\\?\UNC\sunucu\pay\x.dll"), None);
        assert_eq!(strip_verbatim(r"\\sunucu\pay\x.dll"), None);
        assert_eq!(strip_verbatim(r"göreli\x.dll"), None);
    }

    #[test]
    fn reparse_points_detects_junction() {
        let base = std::env::temp_dir().join(format!("alterdns-reparse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("gercek");
        let link = base.join("baglanti");
        std::fs::create_dir_all(&real).unwrap();
        let st = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&real)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success(), "junction oluşturulamadı");
        let found = reparse_points(&[real.clone(), link.clone(), base.join("yok")]);
        assert_eq!(found, vec![link.clone()]);
        std::fs::remove_dir(&link).unwrap(); // yalnızca bağlantıyı siler
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn system_root_must_match_real_windows_dir() {
        let actual = Path::new(r"C:\Windows");
        assert!(system_root_matches(Some(r"C:\Windows"), actual));
        assert!(system_root_matches(Some(r"c:\windows"), actual));
        assert!(!system_root_matches(Some(r"D:\sahte\Windows"), actual));
        assert!(!system_root_matches(None, actual));
    }

    #[test]
    fn test_ip_must_not_carry_live_traffic() {
        let ip = Ipv4Addr::new(1, 1, 1, 1);
        let none: [String; 0] = [];
        assert_eq!(ip_conflict(ip, &none, &none, &none), None);
        let hit = ["1.1.1.1".to_string()];
        assert!(ip_conflict(ip, &hit, &none, &none).unwrap().contains("DNS"));
        assert!(
            ip_conflict(ip, &none, &hit, &none)
                .unwrap()
                .contains("geçidi")
        );
        assert!(
            ip_conflict(ip, &none, &none, &hit)
                .unwrap()
                .contains("bağlantı")
        );
        let other = ["9.9.9.9".to_string(), " 1.1.1.1 ".to_string()];
        assert!(
            ip_conflict(ip, &none, &none, &other).is_some(),
            "boşluk kırpılır"
        );
    }
}
