// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Log (`docs/ARCHITECTURE.md` §10): JSON satırları, `<ad>.log` ve `<ad>.log.1`, her biri en çok
//! `LOG_FILE_MAX`. Her satır diske yazılmadan önce `redact_user_paths`'ten geçer; kullanıcı adı
//! loga hiç yazılmaz. Varsayılan düzey `info`.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use alterdns_errors::{AdnsError, AdnsErrorCode};
use tracing_subscriber::fmt::MakeWriter;

use crate::redact::redact_user_paths;

/// Bir log dosyasının üst sınırı; toplam ≤ 10 MB.
pub const LOG_FILE_MAX: u64 = 5 * 1024 * 1024;
/// Tek satırın üst sınırı (aşan kısım kesilir).
const LINE_MAX: usize = 16 * 1024;

struct State {
    current: PathBuf,
    previous: PathBuf,
    max: u64,
    file: File,
    len: u64,
}

impl State {
    fn rotate(&mut self) -> io::Result<()> {
        let _ = std::fs::remove_file(&self.previous);
        std::fs::rename(&self.current, &self.previous)?;
        self.file = open(&self.current)?;
        self.len = 0;
        Ok(())
    }
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// Boyutu sınırlı, maskeleyen yazıcı. `tracing`'in her olayı tek bir `write` çağrısıyla gelir.
#[derive(Clone)]
pub struct CappedWriter {
    state: Arc<Mutex<State>>,
}

impl CappedWriter {
    pub fn new(dir: &Path, name: &str, max: u64) -> io::Result<CappedWriter> {
        let current = dir.join(format!("{name}.log"));
        let file = open(&current)?;
        let len = file.metadata()?.len();
        Ok(CappedWriter {
            state: Arc::new(Mutex::new(State {
                previous: dir.join(format!("{name}.log.1")),
                current,
                max,
                file,
                len,
            })),
        })
    }
}

impl Write for CappedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut text = redact_user_paths(&String::from_utf8_lossy(buf));
        if text.len() > LINE_MAX {
            let mut cut = LINE_MAX;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push('\n');
        }
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if s.len > 0 && s.len + text.len() as u64 > s.max {
            s.rotate()?;
        }
        s.file.write_all(text.as_bytes())?;
        s.len += text.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .file
            .flush()
    }
}

impl<'a> MakeWriter<'a> for CappedWriter {
    type Writer = CappedWriter;

    fn make_writer(&'a self) -> CappedWriter {
        self.clone()
    }
}

/// JSON satırları yazan, `info` düzeyli abone.
pub fn log_subscriber(writer: CappedWriter) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .finish()
}

/// İşlemin logunu `dir\<name>.log`'a bağlar. Dizin önceden doğrulanmış olmalıdır (servis: K20).
pub fn init_log(dir: &Path, name: &str) -> Result<(), AdnsError> {
    let writer = CappedWriter::new(dir, name, LOG_FILE_MAX)
        .map_err(|e| AdnsError::new(AdnsErrorCode::GenUnexpected).with_source(e))?;
    tracing::subscriber::set_global_default(log_subscriber(writer))
        .map_err(|e| AdnsError::new(AdnsErrorCode::GenUnexpected).with_source(e))
}
