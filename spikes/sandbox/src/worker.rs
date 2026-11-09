// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! İşçi: AppContainer içinde tek bir erişim denemesi yapar ve sonucu JSON olarak yazar.

use std::ffi::c_void;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::time::Duration;

use anyhow::Context;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel, TokenIsAppContainer,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

type Outcome = (bool, Option<i32>, Value);

pub fn main(args: &[String]) -> anyhow::Result<()> {
    let probe = crate::arg(args, "--probe").context("--probe yok")?;
    let (ok, error, detail) = match probe {
        "noop" => (true, None, json!({})),
        "identity" => identity()?,
        "read-handle" => read_handle(args)?,
        "open-user-file" => {
            let path = crate::arg(args, "--user-file").context("--user-file yok")?;
            io_outcome(std::fs::File::open(path))
        }
        "write-temp" => {
            // Kullanıcının (broker'ın) temp dizini; işçi yazamamalı.
            let dir = crate::arg(args, "--user-temp").context("--user-temp yok")?;
            io_outcome(std::fs::write(
                std::path::Path::new(dir).join("alterdns-sandbox-write-probe.txt"),
                b"x",
            ))
        }
        "write-own-temp" => {
            // AppContainer içinde temp_dir() kapsayıcının kendi özel deposudur.
            let path = std::env::temp_dir().join("alterdns-sandbox-own-probe.txt");
            let (ok, error, _) = io_outcome(std::fs::write(&path, b"x"));
            let _ = std::fs::remove_file(&path);
            (ok, error, json!({ "path": path.display().to_string() }))
        }
        "connect-loopback" => {
            let port: u16 = crate::arg(args, "--port").context("--port yok")?.parse()?;
            connect(SocketAddr::from(([127, 0, 0, 1], port)))
        }
        "connect-external" => connect(SocketAddr::from(([1, 1, 1, 1], 443))),
        "spawn-child" => io_outcome(
            std::process::Command::new(r"C:\Windows\System32\cmd.exe")
                .args(["/c", "exit", "0"])
                .spawn()
                .and_then(|mut c| c.wait()),
        ),
        "alloc" => alloc(300 << 20),
        other => anyhow::bail!("bilinmeyen prob: {other}"),
    };
    println!(
        "{}",
        json!({ "probe": probe, "ok": ok, "error": error, "detail": detail })
    );
    Ok(())
}

fn io_outcome<T>(r: std::io::Result<T>) -> Outcome {
    match r {
        Ok(_) => (true, None, json!({})),
        Err(e) => (false, e.raw_os_error(), json!({ "message": e.to_string() })),
    }
}

fn connect(addr: SocketAddr) -> Outcome {
    io_outcome(TcpStream::connect_timeout(&addr, Duration::from_secs(3)))
}

fn alloc(bytes: usize) -> Outcome {
    let mut v: Vec<u8> = Vec::new();
    match v.try_reserve_exact(bytes) {
        Ok(()) => {
            v.resize(bytes, 1);
            (true, None, json!({ "bytes": bytes }))
        }
        Err(e) => (
            false,
            None,
            json!({ "bytes": bytes, "message": e.to_string() }),
        ),
    }
}

fn read_handle(args: &[String]) -> anyhow::Result<Outcome> {
    let raw: usize = crate::arg(args, "--handle")
        .context("--handle yok")?
        .parse()?;
    // SAFETY: Broker bu handle'ı yalnızca bu işleme (HANDLE_LIST) devretti ve değerini
    // komut satırıyla bildirdi. İşçi handle'ın tek sahibidir; File düşünce kapanır.
    let mut file = unsafe { std::fs::File::from_raw_handle(raw as RawHandle) };
    let mut data = Vec::new();
    Ok(match file.read_to_end(&mut data) {
        Ok(n) => (
            true,
            None,
            json!({ "bytes": n, "sha256": hex(&Sha256::digest(&data)) }),
        ),
        Err(e) => io_outcome::<()>(Err(e)),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn identity() -> anyhow::Result<Outcome> {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess sözde handle döndürür; OpenProcessToken yalnızca
    // `token` çıktısını yazar. Handle aşağıda kapatılır.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }?;

    let mut is_appcontainer: u32 = 0;
    let mut len = 0u32;
    // SAFETY: Çıktı arabelleği 4 baytlık bir u32'dir; boyut doğru bildiriliyor.
    let r1 = unsafe {
        GetTokenInformation(
            token,
            TokenIsAppContainer,
            Some(&mut is_appcontainer as *mut u32 as *mut c_void),
            std::mem::size_of::<u32>() as u32,
            &mut len,
        )
    };

    // TOKEN_MANDATORY_LABEL işaretçi içerdiği için 8 bayt hizalı arabellek kullanılır.
    let mut buf = [0u64; 32];
    // SAFETY: Arabellek 256 bayt ve 8 bayt hizalı; boyut doğru bildiriliyor.
    let r2 = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr() as *mut c_void),
            std::mem::size_of_val(&buf) as u32,
            &mut len,
        )
    };
    // SAFETY: `token` OpenProcessToken'dan geldi ve bir kez kapatılıyor.
    unsafe {
        let _ = CloseHandle(token);
    }
    r1?;
    r2?;

    // SAFETY: GetTokenInformation(TokenIntegrityLevel) başarılı; arabellek başı
    // TOKEN_MANDATORY_LABEL'dir ve SID işaretçisi aynı arabelleğin içini gösterir.
    let rid = unsafe {
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        *GetSidSubAuthority(sid, u32::from(count) - 1)
    };
    Ok((
        true,
        None,
        json!({ "is_appcontainer": is_appcontainer != 0, "integrity_rid": rid }),
    ))
}
