// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wintun DLL'i ve Windows API'leri için ince FFI katmanı (spike; `unsafe` yalnızca burada).
//! Bildirimler `wintun.h` (GPL-2.0 OR MIT; MIT seçeneği) dosyasından alınmıştır.

use std::path::Path;

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

/// DLL'in SHA-256'sı beklenenle aynı değilse yüklemeyi reddeder.
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
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unicast_if_accepts_loopback_index() {
        let sock =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        // Döngü arayüzünün dizini Windows'ta 1'dir.
        set_unicast_if_v4(&sock, 1).unwrap();
    }
}
