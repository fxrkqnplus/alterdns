// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Journal kayıt modeli ve satır biçimi (`docs/ARCHITECTURE.md` §6).

use serde::{Deserialize, Serialize};

/// Dosya biçiminin sürümü; ilk satır `{"t":"Schema","schema":1}`.
pub const SCHEMA: u32 = 1;

/// Txn ömrü: `Persistent` (kurulum, güncelleme) açılışta geri alınmaz; `Session` (ayarlayıcı
/// denemesi, motorun ağ nesneleri) commit edilmiş olsa da geri alınır.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lifetime {
    Persistent,
    Session,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Pending,
    Applied,
    Reverted,
    Conflict,
    Acknowledged,
    Failed,
}

impl State {
    /// Geri alınması gereken (kapanmamış) kayıt mı?
    pub fn is_open(self) -> bool {
        matches!(self, State::Pending | State::Applied | State::Conflict)
    }
}

/// Değişiklik türü. Kullanıcıya ait değerler için tür yoktur (ADR-010, ADR-014). Bilinmeyen tür
/// (daha yeni bir sürümün yazdığı) `Unknown` olarak okunur ve dokunulmadan açık bırakılır.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    ServiceCreate,
    ServiceConfig,
    RegistryValue,
    StartupEntry,
    FileInstall,
    UninstallEntry,
    /// Yalnızca testler: `%TEMP%\alterdns-*` altındaki dosya içeriği.
    #[cfg(any(test, feature = "test-ops"))]
    FileValue,
    #[serde(other)]
    Unknown,
}

/// Bir hedefin değeri: yok ya da metin olarak var (en çok `MAX_VALUE_BYTES`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Snapshot {
    Absent,
    Present(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub id: u32,
    pub txn_id: String,
    pub component: String,
    pub kind: Kind,
    pub target: String,
    pub before: Snapshot,
    pub after: Snapshot,
    pub state: State,
    pub created_by: String,
    pub ts: u64,
}

/// Journal dosyasının bir satırı.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Line {
    Schema {
        schema: u32,
    },
    TxnBegin {
        txn_id: String,
        component: String,
        lifetime: Lifetime,
        ts: u64,
    },
    TxnCommit {
        txn_id: String,
        ts: u64,
    },
    Record(ChangeRecord),
}

pub const CREATED_BY: &str = "alterdns";

/// `^[a-z0-9-]{1,64}$`: dosya adına güvenle dönüşür; yol ayıracı ve `..` içeremez.
pub fn valid_txn_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// `<bileşen>-<unix_ms>-<pid>`; bileşen adı kimlik alfabesine indirgenir, sonuç 64 karakterle
/// sınırlıdır.
pub fn new_txn_id(component: &str) -> String {
    let clean: String = component
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let suffix = format!("-{}-{}", now_ms(), std::process::id());
    let keep = 64usize.saturating_sub(suffix.len()).max(1);
    let head: String = clean.chars().take(keep).collect();
    let head = if head.is_empty() {
        "txn".to_string()
    } else {
        head
    };
    let mut id = format!("{head}{suffix}");
    id.truncate(64);
    id
}
