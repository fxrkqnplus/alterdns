// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! AlterDNS hata kataloğu: `ADNS-<ALAN>-<NNN>` kodları ve TR/EN metinler.
//!
//! Tek kaynak `errors/catalog.toml`; `build.rs` kataloğu doğrular ve `AdnsErrorCode`'u üretir.
//! Kullanıcıya ulaşan her hata bir `AdnsError`'dur. Crate platformdan bağımsızdır ve çalışma
//! zamanı bağımlılığı yoktur.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

/// Arayüz ve hata metinlerinin dili. Varsayılan Türkçe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    Tr,
    En,
}

impl Lang {
    /// Yapılandırmadaki kod: `"tr"` / `"en"`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Tr => "tr",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "tr" => Some(Lang::Tr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Bir dildeki metinler.
#[derive(Debug)]
pub struct Text {
    pub title: &'static str,
    pub message: &'static str,
    pub causes: &'static [&'static str],
    pub solutions: &'static [&'static str],
}

struct Meta {
    code: &'static str,
    severity: Severity,
    faz: u8,
    retryable: bool,
    user_action_required: bool,
    params: &'static [&'static str],
    tr: Text,
    en: Text,
}

include!(concat!(env!("OUT_DIR"), "/codes.rs"));

impl AdnsErrorCode {
    fn meta(self) -> &'static Meta {
        // Üretilen tablo enum sırasıyla aynıdır (build.rs); indeks her zaman geçerlidir.
        &META[self as usize]
    }

    /// `"ADNS-IPC-003"` biçiminde kod.
    pub fn as_str(self) -> &'static str {
        self.meta().code
    }

    pub fn from_code(code: &str) -> Option<AdnsErrorCode> {
        Self::ALL.iter().copied().find(|c| c.as_str() == code)
    }

    pub fn severity(self) -> Severity {
        self.meta().severity
    }

    /// Kodun ilk üretildiği faz.
    pub fn faz(self) -> u8 {
        self.meta().faz
    }

    pub fn retryable(self) -> bool {
        self.meta().retryable
    }

    pub fn user_action_required(self) -> bool {
        self.meta().user_action_required
    }

    /// Metinlerdeki `{ad}` yer tutucularının adları.
    pub fn params(self) -> &'static [&'static str] {
        self.meta().params
    }

    pub fn text(self, lang: Lang) -> &'static Text {
        match lang {
            Lang::Tr => &self.meta().tr,
            Lang::En => &self.meta().en,
        }
    }
}

impl fmt::Display for AdnsErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Kullanıcıya gösterilecek, parametreleri yerleştirilmiş metin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub title: String,
    pub message: String,
}

/// Kullanıcıya ulaşan hata: kod, metin parametreleri ve isteğe bağlı teknik kaynak.
#[derive(Debug)]
pub struct AdnsError {
    code: AdnsErrorCode,
    params: BTreeMap<String, String>,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl AdnsError {
    pub fn new(code: AdnsErrorCode) -> AdnsError {
        AdnsError {
            code,
            params: BTreeMap::new(),
            source: None,
        }
    }

    /// Metindeki `{key}` yer tutucusunun değeri.
    #[must_use]
    pub fn with(mut self, key: &str, value: impl Into<String>) -> AdnsError {
        self.params.insert(key.to_string(), value.into());
        self
    }

    /// Teknik neden (yalnızca log ve "Ayrıntıları kopyala" için).
    #[must_use]
    pub fn with_source(
        mut self,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> AdnsError {
        self.source = Some(Box::new(source));
        self
    }

    pub fn code(&self) -> AdnsErrorCode {
        self.code
    }

    pub fn params(&self) -> &BTreeMap<String, String> {
        &self.params
    }

    /// Verilmemiş parametre `?` olarak gösterilir; fazla parametre yok sayılır.
    pub fn render(&self, lang: Lang) -> Rendered {
        let t = self.code.text(lang);
        Rendered {
            title: fill(t.title, &self.params),
            message: fill(t.message, &self.params),
        }
    }
}

impl fmt::Display for AdnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.render(Lang::Tr).title)
    }
}

impl std::error::Error for AdnsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

impl From<AdnsErrorCode> for AdnsError {
    fn from(code: AdnsErrorCode) -> AdnsError {
        AdnsError::new(code)
    }
}

/// `{ad}` yer tutucularını değerleriyle değiştirir; değeri olmayan `?` olur.
fn fill(template: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_end = after.find('}');
        match name_end {
            Some(end)
                if !after[..end].is_empty()
                    && after[..end]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') =>
            {
                out.push_str(params.get(&after[..end]).map_or("?", String::as_str));
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
