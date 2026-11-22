// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Journal dosyası: txn başına bir JSON Lines dosyası. Her satır yazıldıktan sonra `sync_all`
//! yapılır; satır diske yazılamazsa çağıran değişikliği uygulamaz. Yarım kalan son satır
//! (elektrik kesintisi) açılışta kesilip atılır; ortadaki bozuk satır kurtarmayı durdurur.
//!
//! Yeni bir satır türü, durum ya da ömür eklemek şema sürümünü artırmayı gerektirir: eski ikili
//! bilinmeyen satırı ayrıştıramaz ve dosyayı (yalnızca onu) kurtaramaz.

use std::fs::{File, OpenOptions};
use std::io::{Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alterdns_errors::{AdnsError, AdnsErrorCode};

use crate::model::{ChangeRecord, Lifetime, Line, SCHEMA, Snapshot, State, now_ms, valid_txn_id};

/// Yeni kayıtlar ve txn başlangıcı için dosya boyutu sınırı.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Durum geçişleri ve commit için sınır. Her kayıt en çok dört satır yazar (`Pending`, `Applied`,
/// `Conflict`, `Reverted`/`Acknowledged`); böylece sınıra yakın bir txn de geri alınabilir.
pub const MAX_READ_BYTES: u64 = 4 * MAX_FILE_BYTES;
pub const MAX_VALUE_BYTES: usize = 4 * 1024;
pub const LOCK_WAIT: Duration = Duration::from_secs(10);
const LOCK_FILE: &str = ".journal.lock";

fn apply_err(e: impl std::error::Error + Send + Sync + 'static) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SysApplyFailed).with_source(e)
}

fn revert_err(msg: String) -> AdnsError {
    AdnsError::new(AdnsErrorCode::SysRevertFailed).with_source(std::io::Error::other(msg))
}

fn value_too_long(s: &Snapshot) -> bool {
    matches!(s, Snapshot::Present(v) if v.len() > MAX_VALUE_BYTES)
}

pub struct JournalFile {
    path: PathBuf,
    txn_id: String,
    lines: Vec<Line>,
    file: File,
}

/// Txn dosyasının yolu: `<dizin>\<txn>.jsonl`. Kimlik önceden doğrulanmış olmalıdır.
pub(crate) fn txn_path(dir: &Path, txn_id: &str) -> PathBuf {
    dir.join(format!("{txn_id}.jsonl"))
}

impl JournalFile {
    /// Yeni txn dosyası (`create_new`): şema ve `TxnBegin` satırları yazılır.
    pub fn create(
        dir: &Path,
        txn_id: &str,
        component: &str,
        lifetime: Lifetime,
    ) -> Result<JournalFile, AdnsError> {
        if !valid_txn_id(txn_id) {
            return Err(revert_err(format!("geçersiz txn kimliği: {txn_id:?}")));
        }
        let path = txn_path(dir, txn_id);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(apply_err)?;
        // Txn yaşadığı sürece dosya özel kilitlidir: açılıştaki kurtarma ya da başka bir işlem
        // canlı bir txn'i geri alamaz. Sahip işlem ölünce kilit işletim sistemince bırakılır.
        file.try_lock()
            .map_err(|e| apply_err(std::io::Error::other(e.to_string())))?;
        let mut j = JournalFile {
            path,
            txn_id: txn_id.to_string(),
            lines: Vec::new(),
            file,
        };
        j.append(&Line::Schema { schema: SCHEMA })?;
        j.append(&Line::TxnBegin {
            txn_id: txn_id.to_string(),
            component: component.to_string(),
            lifetime,
            ts: now_ms(),
        })?;
        Ok(j)
    }

    /// Satırı yazar ve diske zorlar. Hata `SysApplyFailed`'dir; çağıran değişikliği uygulamaz.
    pub fn append(&mut self, line: &Line) -> Result<(), AdnsError> {
        if let Line::Record(r) = line
            && (value_too_long(&r.before) || value_too_long(&r.after))
        {
            return Err(apply_err(std::io::Error::other("kayıt değeri çok uzun")));
        }
        let mut text = serde_json::to_string(line).map_err(apply_err)?;
        text.push('\n');
        let limit = match line {
            Line::Record(r) if r.state != State::Pending => MAX_READ_BYTES,
            Line::TxnCommit { .. } => MAX_READ_BYTES,
            _ => MAX_FILE_BYTES,
        };
        let len = self.file.metadata().map_err(apply_err)?.len();
        if len + text.len() as u64 > limit {
            return Err(apply_err(std::io::Error::other(
                "journal dosyası çok büyük",
            )));
        }
        self.file.write_all(text.as_bytes()).map_err(apply_err)?;
        self.file.sync_all().map_err(apply_err)?;
        self.lines.push(line.clone());
        Ok(())
    }

    /// Var olan dosyayı geri alma ve kurtarma için açar. Dosya canlı bir txn'e aitse (kilitli)
    /// `None` döner: o txn'e dokunulmaz. Kilit alınırsa içerik aynı tutamaçtan okunur.
    ///
    /// Satır sonu olmayan son parça (yazılırken kesilmiş satır) dosyadan kesilir; yoksa sonraki
    /// satırlar ona eklenir ve dosya bir daha okunamaz. Kesilen satır güvenle atılabilir: kayıt
    /// diske tam yazılmadan sistem değiştirilmez, durum satırı kaybolursa kayıt yeniden
    /// değerlendirilir.
    pub fn open(path: &Path) -> Result<Option<JournalFile>, AdnsError> {
        let txn_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| valid_txn_id(s))
            .ok_or_else(|| revert_err(format!("geçersiz journal adı: {}", path.display())))?
            .to_string();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| revert_err(e.to_string()))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(e)) => return Err(revert_err(e.to_string())),
        }
        let len = file
            .metadata()
            .map_err(|e| revert_err(e.to_string()))?
            .len();
        if len > MAX_READ_BYTES {
            return Err(revert_err("journal dosyası çok büyük".into()));
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).map_err(|e| revert_err(e.to_string()))?;
        if bytes.last().is_some_and(|b| *b != b'\n') {
            let keep = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            bytes.truncate(keep);
            file.set_len(keep as u64)
                .and_then(|()| file.sync_all())
                .map_err(|e| revert_err(format!("yarım satır kesilemedi: {e}")))?;
        }
        file.seek(SeekFrom::End(0))
            .map_err(|e| revert_err(e.to_string()))?;
        let text = String::from_utf8(bytes).map_err(|e| revert_err(e.to_string()))?;
        let lines = parse_lines(&text)?;
        Ok(Some(JournalFile {
            path: path.to_path_buf(),
            txn_id,
            lines,
            file,
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn txn_id(&self) -> &str {
        &self.txn_id
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn lifetime(&self) -> Option<Lifetime> {
        self.lines.iter().find_map(|l| match l {
            Line::TxnBegin { lifetime, .. } => Some(*lifetime),
            _ => None,
        })
    }

    pub fn component(&self) -> Option<&str> {
        self.lines.iter().find_map(|l| match l {
            Line::TxnBegin { component, .. } => Some(component.as_str()),
            _ => None,
        })
    }

    pub fn committed(&self) -> bool {
        self.lines
            .iter()
            .any(|l| matches!(l, Line::TxnCommit { .. }))
    }

    /// Her kayıt kimliğinin son hali, ilk görülme sırasıyla.
    pub fn records(&self) -> Vec<ChangeRecord> {
        let mut out: Vec<ChangeRecord> = Vec::new();
        for l in &self.lines {
            if let Line::Record(r) = l {
                match out.iter_mut().find(|x| x.id == r.id) {
                    Some(x) => *x = r.clone(),
                    None => out.push(r.clone()),
                }
            }
        }
        out
    }
}

/// Satırları ayrıştırır: ilk satır şema olmalı (daha yeni şema reddedilir); yarım son satır yok
/// sayılır; ortadaki bozuk satır hatadır.
fn parse_lines(text: &str) -> Result<Vec<Line>, AdnsError> {
    let raw: Vec<&str> = text.split('\n').collect();
    let complete = text.ends_with('\n');
    let mut lines = Vec::new();
    for (i, l) in raw.iter().enumerate() {
        let l = l.trim_end_matches('\r');
        if l.is_empty() {
            continue;
        }
        let last = i + 1 == raw.len() || (i + 2 == raw.len() && complete);
        match serde_json::from_str::<Line>(l) {
            Ok(line) => lines.push(line),
            Err(_) if last && !complete => break,
            Err(e) => return Err(revert_err(format!("bozuk journal satırı {}: {e}", i + 1))),
        }
    }
    match lines.first() {
        // Boş dosya: oluşturma, şema satırı diske yazılmadan kesildi; kayıt yok.
        None if text.trim().is_empty() => Ok(lines),
        Some(Line::Schema { schema }) if *schema <= SCHEMA => Ok(lines),
        Some(Line::Schema { schema }) => {
            Err(revert_err(format!("daha yeni journal şeması: {schema}")))
        }
        _ => Err(revert_err("şema satırı yok".into())),
    }
}

/// Journal dizininin özel kilidi; düşünce bırakılır.
pub struct DirLock {
    _file: File,
}

pub fn lock_dir(dir: &Path) -> Result<DirLock, AdnsError> {
    lock_dir_with(dir, LOCK_WAIT)
}

/// Kilidi en çok `wait` süre bekler; süre dolunca hata verir (bekçinin geri almasını
/// süresiz bekletmesin).
pub fn lock_dir_with(dir: &Path, wait: Duration) -> Result<DirLock, AdnsError> {
    let path = dir.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(apply_err)?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(DirLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(revert_err(format!(
                    "{} başka bir işlemde kilitli ({} sn beklendi)",
                    path.display(),
                    wait.as_secs()
                )));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(apply_err(e)),
        }
    }
}

/// Dizindeki journal dosyaları: adı geçerli txn kimliği olan `*.jsonl`'ler.
pub fn list(dir: &Path) -> Result<Vec<PathBuf>, AdnsError> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(revert_err(e.to_string())),
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "jsonl")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(valid_txn_id)
        })
        .collect();
    out.sort();
    Ok(out)
}
