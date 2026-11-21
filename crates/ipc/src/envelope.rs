// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Zarf (`docs/ARCHITECTURE.md` §4): istek, yanıt ve olay. Bilinmeyen alanlar yok sayılır.
//! Kabloya yalnızca hata kodu ve metin parametreleri çıkar; teknik kaynak (yol, işletim sistemi
//! iletisi) servis logunda kalır.

use std::collections::BTreeMap;

use alterdns_errors::{AdnsError, AdnsErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Zarf sürümü (`"v"`).
pub const PROTOCOL: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub code: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

impl WireError {
    pub fn from_error(e: &AdnsError) -> WireError {
        WireError {
            code: e.code().as_str().to_string(),
            params: e.params().clone(),
        }
    }

    /// Bilinmeyen kod (daha yeni bir servis) `GenUnexpected` olur.
    pub fn into_error(self) -> AdnsError {
        let code = AdnsErrorCode::from_code(&self.code).unwrap_or(AdnsErrorCode::GenUnexpected);
        self.params
            .into_iter()
            .fold(AdnsError::new(code), |e, (k, v)| e.with(&k, v))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WireError>,
}

impl Response {
    pub fn ok(id: &str, result: Value) -> Response {
        Response {
            v: PROTOCOL,
            id: id.to_string(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: &str, e: &AdnsError) -> Response {
        Response {
            v: PROTOCOL,
            id: id.to_string(),
            ok: false,
            result: None,
            error: Some(WireError::from_error(e)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

/// Çerçevedeki mesaj. Tür alanlarla ayırt edilir: `method` istek, `ok` yanıt, `event` olaydır.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Event(Event),
}

/// İstek kimliği: 1–64 karakter, `[A-Za-z0-9-]`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
