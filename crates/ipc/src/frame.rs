// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Çerçeve: 4 bayt küçük-endian uzunluk + UTF-8 JSON. Mesaj en çok 1 MiB'tır. Ayrıştırma
//! hiçbir girdide panik yapmaz; bozuk girdi `IpcBadRequest`, başka zarf sürümü
//! `IpcVersionMismatch` olur.

use alterdns_errors::{AdnsError, AdnsErrorCode};
use serde_json::Value;

use crate::envelope::{Message, PROTOCOL, valid_id};

pub const MAX_MESSAGE: usize = 1024 * 1024;

fn bad(detail: impl Into<String>) -> AdnsError {
    AdnsError::new(AdnsErrorCode::IpcBadRequest).with_source(std::io::Error::other(detail.into()))
}

pub fn encode(msg: &Message) -> Result<Vec<u8>, AdnsError> {
    let body = serde_json::to_vec(msg).map_err(|e| bad(e.to_string()))?;
    if body.len() > MAX_MESSAGE {
        return Err(bad(format!("mesaj çok büyük: {} bayt", body.len())));
    }
    let len = u32::try_from(body.len()).map_err(|e| bad(e.to_string()))?;
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Uzunluk öneki: 0 ya da `MAX_MESSAGE`'tan büyükse reddedilir (gövde okunmaz).
pub fn decode_len(prefix: [u8; 4]) -> Result<usize, AdnsError> {
    let n = u32::from_le_bytes(prefix) as usize;
    if n == 0 || n > MAX_MESSAGE {
        return Err(bad(format!("geçersiz mesaj uzunluğu: {n}")));
    }
    Ok(n)
}

pub fn decode(body: &[u8]) -> Result<Message, AdnsError> {
    if body.len() > MAX_MESSAGE {
        return Err(bad("mesaj çok büyük"));
    }
    let value: Value = serde_json::from_slice(body).map_err(|e| bad(e.to_string()))?;
    match value.get("v").and_then(Value::as_u64) {
        None => return Err(bad("zarf sürümü yok")),
        Some(v) if v != u64::from(PROTOCOL) => {
            return Err(AdnsError::new(AdnsErrorCode::IpcVersionMismatch)
                .with_source(std::io::Error::other(format!("zarf sürümü {v}"))));
        }
        Some(_) => {}
    }
    let msg: Message = serde_json::from_value(value).map_err(|e| bad(e.to_string()))?;
    let id_ok = match &msg {
        Message::Request(r) => valid_id(&r.id),
        Message::Response(r) => valid_id(&r.id),
        Message::Event(_) => true,
    };
    if !id_ok {
        return Err(bad("geçersiz istek kimliği"));
    }
    Ok(msg)
}
