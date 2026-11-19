// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Yapılandırma, şema v1 (`docs/ARCHITECTURE.md` §5).
//!
//! - Bilinmeyen anahtarlar korunur (dosya `toml::Table` olarak saklanır, bilinen alanlar
//!   üzerine yazılır).
//! - Bozuk, boş ya da şemasız dosya `.bak` olarak saklanır ve varsayılanla devam edilir.
//! - Daha yeni şema okunur ama üzerine yazılmaz.
//! - Yazma: aynı dizinde geçici dosya → `sync_all` → yeniden adlandırma. Yazma başarısızsa
//!   özgün dosya değişmez.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use alterdns_errors::{AdnsError, AdnsErrorCode, Lang};
use toml::{Table, Value};

pub const SCHEMA: i64 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LeftClick {
    #[default]
    Flyout,
    MainWindow,
}

impl LeftClick {
    fn code(self) -> &'static str {
        match self {
            LeftClick::Flyout => "flyout",
            LeftClick::MainWindow => "main_window",
        }
    }

    fn from_code(s: &str) -> Option<LeftClick> {
        match s {
            "flyout" => Some(LeftClick::Flyout),
            "main_window" => Some(LeftClick::MainWindow),
            _ => None,
        }
    }
}

/// Kullanıcı yapılandırması (`%LOCALAPPDATA%\AlterDNS\config\config.toml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserConfig {
    pub language: Lang,
    pub left_click: LeftClick,
    pub notifications: bool,
    pub run_at_logon: bool,
    pub online_lookups: bool,
}

impl Default for UserConfig {
    fn default() -> UserConfig {
        UserConfig {
            language: Lang::Tr,
            left_click: LeftClick::Flyout,
            notifications: true,
            run_at_logon: true,
            online_lookups: false,
        }
    }
}

/// Makine yapılandırması (`%ProgramData%\AlterDNS\config\machine.toml`); Faz 1'de yalnızca şema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineConfig {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    Loaded,
    /// Dosya yoktu (`backup: None`) ya da bozuktu ve yedeği alındı.
    Defaulted {
        backup: Option<PathBuf>,
    },
    /// Daha yeni bir sürümün yazdığı dosya: okundu, üzerine yazılmaz.
    NewerSchema,
}

fn sys_error(e: std::io::Error) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SysApplyFailed).with_source(e)
}

fn section<'a>(t: &'a Table, name: &str) -> Option<&'a Table> {
    t.get(name).and_then(Value::as_table)
}

fn get_bool(t: &Table, sec: &str, key: &str, default: bool) -> bool {
    section(t, sec)
        .and_then(|s| s.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn get_str<'a>(t: &'a Table, sec: &str, key: &str) -> Option<&'a str> {
    section(t, sec)
        .and_then(|s| s.get(key))
        .and_then(Value::as_str)
}

/// Tanınan alanlar okunur; geçersiz değer o alanın varsayılanına düşer.
fn from_table(t: &Table) -> UserConfig {
    let d = UserConfig::default();
    UserConfig {
        language: get_str(t, "ui", "language")
            .and_then(Lang::from_code)
            .unwrap_or(d.language),
        left_click: get_str(t, "ui", "left_click")
            .and_then(LeftClick::from_code)
            .unwrap_or(d.left_click),
        notifications: get_bool(t, "ui", "notifications", d.notifications),
        run_at_logon: get_bool(t, "startup", "run_at_logon", d.run_at_logon),
        online_lookups: get_bool(t, "privacy", "online_lookups", d.online_lookups),
    }
}

/// Dosyayı okur: (tablo, şema). Ayrıştırılamaz ya da şemasızsa `None`.
fn parse(bytes: &[u8]) -> Option<(Table, i64)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let table: Table = text.parse().ok()?;
    let schema = table.get("schema")?.as_integer()?;
    (schema >= 1).then_some((table, schema))
}

pub fn load_user(path: &Path) -> Result<(UserConfig, LoadOutcome), AdnsError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((
                UserConfig::default(),
                LoadOutcome::Defaulted { backup: None },
            ));
        }
        Err(e) => return Err(sys_error(e)),
    };
    match parse(&bytes) {
        Some((t, schema)) if schema == SCHEMA => Ok((from_table(&t), LoadOutcome::Loaded)),
        Some((t, _)) => Ok((from_table(&t), LoadOutcome::NewerSchema)),
        None => {
            let backup = path.with_extension("toml.bak");
            std::fs::rename(path, &backup).map_err(sys_error)?;
            Ok((
                UserConfig::default(),
                LoadOutcome::Defaulted {
                    backup: Some(backup),
                },
            ))
        }
    }
}

fn set(t: &mut Table, sec: &str, key: &str, value: Value) {
    let entry = t
        .entry(sec.to_string())
        .or_insert_with(|| Value::Table(Table::new()));
    if !entry.is_table() {
        *entry = Value::Table(Table::new());
    }
    if let Value::Table(s) = entry {
        s.insert(key.to_string(), value);
    }
}

pub fn save_user(path: &Path, cfg: &UserConfig) -> Result<(), AdnsError> {
    let mut table = match std::fs::read(path) {
        Ok(bytes) => match parse(&bytes) {
            Some((_, schema)) if schema > SCHEMA => {
                return Err(AdnsError::new(AdnsErrorCode::SysApplyFailed));
            }
            Some((t, _)) => t,
            None => Table::new(),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Table::new(),
        Err(e) => return Err(sys_error(e)),
    };
    table.insert("schema".into(), Value::Integer(SCHEMA));
    set(
        &mut table,
        "ui",
        "language",
        Value::String(cfg.language.code().into()),
    );
    set(
        &mut table,
        "ui",
        "left_click",
        Value::String(cfg.left_click.code().into()),
    );
    set(
        &mut table,
        "ui",
        "notifications",
        Value::Boolean(cfg.notifications),
    );
    set(
        &mut table,
        "startup",
        "run_at_logon",
        Value::Boolean(cfg.run_at_logon),
    );
    set(
        &mut table,
        "privacy",
        "online_lookups",
        Value::Boolean(cfg.online_lookups),
    );
    let text = toml::to_string(&table)
        .map_err(|e| AdnsError::new(AdnsErrorCode::SysApplyFailed).with_source(e))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(sys_error)?;
    }
    let tmp = path.with_extension("toml.tmp");
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(sys_error)
}
