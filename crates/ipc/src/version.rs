// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sürüm uyumu (K19): `MAJOR.MINOR.PATCH` sayısal karşılaştırılır; `-` ya da `+` sonrası yok
//! sayılır.

use alterdns_errors::{AdnsError, AdnsErrorCode};

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| {
        (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse::<u64>().ok())
            .flatten()
    });
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Some(a)), Some(Some(b)), Some(Some(c)), None) => Some((a, b, c)),
        _ => None,
    }
}

/// `have >= min` değilse ya da biri okunamazsa `IpcVersionMismatch`.
pub fn version_compatible(have: &str, min: &str) -> Result<(), AdnsError> {
    match (parse(have), parse(min)) {
        (Some(h), Some(m)) if h >= m => Ok(()),
        _ => Err(
            AdnsError::new(AdnsErrorCode::IpcVersionMismatch).with_source(std::io::Error::other(
                format!("sürüm {have:?}, en az {min:?}"),
            )),
        ),
    }
}
