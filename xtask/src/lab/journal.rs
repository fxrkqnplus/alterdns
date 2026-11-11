// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lab journal'ı (ARCHITECTURE §6'nın küçük öncülü): önce kaydet (fsync), sonra uygula,
//! gerekirse ters sırada geri al. Biçim: JSON Lines, her durum değişimi bir satır; son satır kazanır.
//!
//! Sahiplik kuralı: yalnızca lab'ın oluşturduğu öğelere (ad `AlterDNS-Spike` olan adaptör,
//! o adaptördeki adres ve rota, geçici dizindeki `alterdns-*` test dosyası) dokunulur. Sistem hop
//! limiti sahiplenilmez (ADR-010). Geri alma anında değer bizim yazdığımızdan farklıysa
//! (kullanıcı değiştirmiş) dokunulmaz ve kayıt, kullanıcı onaylayana kadar yeni koşuyu engeller.
//!
//! Journal dosyası diskte durduğu için güvenilmeyen girdi sayılır: yüklenirken her değer
//! doğrulanır; geri alma sahiplik denetiminden geçmeyen öğeyi okumaz bile.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Lab'ın oluşturabileceği tek sanal adaptörün adı.
pub const SPIKE_ADAPTER: &str = "AlterDNS-Spike";
/// Journal dosyasının üst sınırı (bozuk veya şişirilmiş dosyaya karşı).
const MAX_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
/// Bir kayıttaki önceki/sonraki değerin üst sınırı.
const MAX_VALUE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpFamily {
    V4,
    V6,
}

/// Journal'a yazılabilen sistem öğeleri.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OpKind {
    TunAdapter {
        name: String,
    },
    UnicastAddress {
        if_name: String,
        cidr: String,
    },
    Route {
        prefix: String,
        if_name: String,
    },
    HopLimit {
        family: IpFamily,
    },
    /// Yalnızca geçici dizindeki `alterdns-*` alt dizininde; testler için.
    FileValue {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryState {
    Pending,
    Applied,
    Reverted,
    /// Değer ne öncekine ne bizimkine eşit ya da öğe lab'a ait değil; dokunulmadı.
    Conflict,
    /// Kullanıcı çakışmayı `lab recover` içinde onayladı; kayıt kapandı.
    Acknowledged,
    /// Uygulama yazması başarısız oldu ve değer değişmedi.
    Failed,
}

impl EntryState {
    /// Yeni koşuyu engelleyen (kapanmamış) durum mu?
    pub fn is_open(self) -> bool {
        matches!(
            self,
            EntryState::Pending | EntryState::Applied | EntryState::Conflict
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u32,
    pub op: OpKind,
    pub before: String,
    pub after: String,
    pub state: EntryState,
}

/// Sistemi okuyan/yazan katman; testlerde sahte uygulama kullanılır.
pub trait SystemOps {
    fn read(&self, op: &OpKind) -> anyhow::Result<String>;
    fn write(&self, op: &OpKind, value: &str) -> anyhow::Result<()>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RevertReport {
    pub reverted: Vec<u32>,
    pub conflicts: Vec<u32>,
    /// Okuma veya yazma hatası; kayıt açık kalır, `lab recover` yeniden dener.
    pub failed: Vec<(u32, String)>,
}

pub struct Journal {
    path: PathBuf,
    entries: Vec<Entry>,
    /// Son satır yarım kaldıysa geçerli içeriğin bayt uzunluğu; ilk eklemeden önce dosya kırpılır.
    torn_tail_at: Option<u64>,
}

/// Geçici dizinde `alterdns-*` ile başlayan bir alt dizindeki dosya mı? Yalnızca sözcüksel
/// denetimdir: `..`, kök ve önek bileşenleri reddedilir. Bağlantı/junction denetimi `ops.rs`'tedir.
pub fn lab_temp_path(path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(std::env::temp_dir()) else {
        return false;
    };
    let comps: Vec<Component<'_>> = rest.components().collect();
    comps.len() >= 2
        && comps.iter().all(|c| matches!(c, Component::Normal(_)))
        && comps
            .first()
            .and_then(|c| c.as_os_str().to_str())
            .is_some_and(|s| s.starts_with("alterdns-"))
}

/// Kayıt değerlerinin biçim denetimi (diskteki journal güvenilmeyen girdidir).
fn validate(e: &Entry) -> anyhow::Result<()> {
    let token = |v: &str| {
        if super::ops::valid_token(v) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("geçersiz değer: {v:?}"))
        }
    };
    match &e.op {
        OpKind::TunAdapter { name } => token(name)?,
        OpKind::UnicastAddress { if_name, cidr } => {
            token(if_name)?;
            token(cidr)?;
        }
        OpKind::Route { prefix, if_name } => {
            token(prefix)?;
            token(if_name)?;
        }
        OpKind::HopLimit { .. } => {}
        OpKind::FileValue { path } => {
            if !lab_temp_path(path) {
                bail!("FileValue lab geçici dizini dışında: {}", path.display());
            }
        }
    }
    let presence = |v: &str| v == "present" || v == "absent";
    if matches!(
        e.op,
        OpKind::TunAdapter { .. } | OpKind::UnicastAddress { .. } | OpKind::Route { .. }
    ) && !(presence(&e.before) && presence(&e.after))
    {
        bail!("kayıt {}: değerler yalnızca present/absent olabilir", e.id);
    }
    if e.before.len() > MAX_VALUE_BYTES || e.after.len() > MAX_VALUE_BYTES {
        bail!("kayıt değeri çok uzun (kimlik {})", e.id);
    }
    Ok(())
}

/// Özel kilit dosyası; düşünce bırakılır.
fn lock_file(path: &Path) -> anyhow::Result<std::fs::File> {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("{} açılamadı", path.display()))?;
    f.lock()
        .with_context(|| format!("{} kilitlenemedi", path.display()))?;
    Ok(f)
}

impl Journal {
    /// Yeni koşu journal'ı. Dizinde kapanmamış bir journal varsa hata verir. Aynı anda iki
    /// koşunun başlamasını dizin kilidi önler.
    pub fn create(dir: &Path, run_id: &str) -> anyhow::Result<Journal> {
        std::fs::create_dir_all(dir)?;
        let _lock = lock_file(&dir.join(".journal.lock"))?;
        let stale = Self::find_stale(dir)?;
        if !stale.is_empty() {
            bail!(
                "bitmemiş journal var ({}); önce `cargo xtask lab recover` çalıştırın",
                stale
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let path = dir.join(format!("{run_id}.jsonl"));
        if path.exists() {
            bail!("{} zaten var", path.display());
        }
        std::fs::File::create(&path)?.sync_all()?;
        Ok(Journal {
            path,
            entries: Vec::new(),
            torn_tail_at: None,
        })
    }

    /// Var olan journal'ı okur (bekçi ve `lab recover` için). Her kimlik için son satır geçerlidir.
    /// Satır sonu olmayan, ayrıştırılamayan son satır yazma sırasında kesilmiştir: yazma-önce
    /// kuralı gereği o satırın anlattığı sistem değişikliği başlamamıştır; yok sayılır.
    pub fn open(path: &Path) -> anyhow::Result<Journal> {
        let len = std::fs::metadata(path)
            .with_context(|| format!("{} okunamadı", path.display()))?
            .len();
        if len > MAX_JOURNAL_BYTES {
            bail!("{}: journal çok büyük ({len} bayt)", path.display());
        }
        let bytes = std::fs::read(path)?;
        let ends_clean = bytes.last().is_none_or(|b| *b == b'\n');
        let lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
        let mut entries: Vec<Entry> = Vec::new();
        let mut index: HashMap<u32, usize> = HashMap::new();
        let mut torn_tail_at = None;
        let mut offset = 0u64;
        for (i, raw) in lines.iter().enumerate() {
            let start = offset;
            offset += u64::try_from(raw.len()).unwrap_or(u64::MAX) + 1;
            if raw.trim_ascii().is_empty() {
                continue;
            }
            let is_last = i + 1 == lines.len();
            match serde_json::from_slice::<Entry>(raw.trim_ascii()) {
                Ok(e) => {
                    validate(&e).with_context(|| format!("{}:{}", path.display(), i + 1))?;
                    match index.get(&e.id) {
                        Some(&at) => {
                            if let Some(slot) = entries.get_mut(at) {
                                *slot = e;
                            }
                        }
                        None => {
                            index.insert(e.id, entries.len());
                            entries.push(e);
                        }
                    }
                }
                Err(_) if is_last && !ends_clean => {
                    eprintln!(
                        "uyarı: {}: yarım kalmış son satır yok sayıldı",
                        path.display()
                    );
                    torn_tail_at = Some(start);
                }
                Err(err) => bail!(
                    "{}:{}: bozuk journal satırı: {err} (lab/RUNBOOK.md)",
                    path.display(),
                    i + 1
                ),
            }
        }
        Ok(Journal {
            path: path.to_path_buf(),
            entries,
            torn_tail_at,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    fn persist(&mut self, e: &Entry) -> anyhow::Result<()> {
        use std::io::Write;
        if let Some(len) = self.torn_tail_at.take() {
            let f = std::fs::OpenOptions::new().write(true).open(&self.path)?;
            f.set_len(len)?;
            f.sync_all()?;
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(e)?)?;
        f.sync_all()?;
        Ok(())
    }

    fn set_state(&mut self, id: u32, state: EntryState) -> anyhow::Result<()> {
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow::anyhow!("journal kaydı yok: {id}"))?;
        e.state = state;
        let snapshot = e.clone();
        self.persist(&snapshot)
    }

    fn push_pending(
        &mut self,
        ops: &dyn SystemOps,
        op: OpKind,
        after: &str,
    ) -> anyhow::Result<u32> {
        let before = ops.read(&op)?;
        let id = self.entries.iter().map(|e| e.id).max().map_or(1, |m| m + 1);
        let e = Entry {
            id,
            op,
            before,
            after: after.to_string(),
            state: EntryState::Pending,
        };
        validate(&e)?;
        self.persist(&e)?;
        self.entries.push(e);
        Ok(id)
    }

    /// Önce okur, `Pending` yazar (fsync), sistemi değiştirir, `Applied` yazar (fsync).
    /// Yazma başarısız olursa değer yeniden okunur: değişmediyse `Failed`, değiştiyse (yarım
    /// yazma) kayıt `Pending` kalır ve bayat journal olarak geri alınır.
    pub fn apply(&mut self, ops: &dyn SystemOps, op: OpKind, after: &str) -> anyhow::Result<u32> {
        if !owned(&op) {
            bail!("lab bu öğeye dokunamaz (sahiplik kuralı): {op:?}");
        }
        let id = self.push_pending(ops, op.clone(), after)?;
        if let Err(e) = ops.write(&op, after) {
            let before = self
                .entries
                .iter()
                .find(|x| x.id == id)
                .map(|x| x.before.clone());
            if ops.read(&op).ok().is_some_and(|v| Some(v) == before) {
                self.set_state(id, EntryState::Failed)?;
            }
            return Err(e);
        }
        self.set_state(id, EntryState::Applied)?;
        Ok(id)
    }

    /// Değişikliği başka bir bileşen yapacaksa: önceki değeri `Pending` olarak yazar (fsync).
    pub fn record_external(
        &mut self,
        ops: &dyn SystemOps,
        op: OpKind,
        after: &str,
    ) -> anyhow::Result<u32> {
        self.push_pending(ops, op, after)
    }

    /// Dış bileşen değişikliği yaptıktan sonra `Applied` yazar.
    pub fn mark_applied(&mut self, id: u32) -> anyhow::Result<()> {
        self.set_state(id, EntryState::Applied)
    }

    /// Kapanmamış kayıtları ters sırada geri alır. Journal kilidi alınır ve diskteki son durum
    /// yeniden okunur (bekçi ile ebeveyn aynı anda geri almaya kalkabilir).
    /// - Öğe lab'a ait değilse okunmaz bile: `Conflict`.
    /// - Değer önceki hâldeyse kayıt kapanır; bizim yazdığımız değerdeyse önceki değer geri yazılır.
    /// - Başka bir değerdeyse dokunulmaz: `Conflict`.
    /// - Okuma/yazma hatasında kayıt açık kalır (`failed`); `lab recover` yeniden dener.
    pub fn revert_all(&mut self, ops: &dyn SystemOps) -> anyhow::Result<RevertReport> {
        let _lock = lock_file(&self.lock_path())?;
        let fresh = Journal::open(&self.path)?;
        self.entries = fresh.entries;
        self.torn_tail_at = fresh.torn_tail_at;

        let mut report = RevertReport::default();
        let open: Vec<Entry> = self
            .entries
            .iter()
            .rev()
            .filter(|e| e.state.is_open())
            .cloned()
            .collect();
        for e in open {
            if !owned(&e.op) {
                self.mark_conflict(&e)?;
                report.conflicts.push(e.id);
                continue;
            }
            let current = match ops.read(&e.op) {
                Ok(v) => v,
                Err(err) => {
                    report.failed.push((e.id, format!("{err:#}")));
                    continue;
                }
            };
            if current == e.before {
                self.set_state(e.id, EntryState::Reverted)?;
                report.reverted.push(e.id);
            } else if current == e.after {
                match ops.write(&e.op, &e.before) {
                    Ok(()) => {
                        self.set_state(e.id, EntryState::Reverted)?;
                        report.reverted.push(e.id);
                    }
                    Err(err) => report.failed.push((e.id, format!("{err:#}"))),
                }
            } else {
                self.mark_conflict(&e)?;
                report.conflicts.push(e.id);
            }
        }
        Ok(report)
    }

    fn mark_conflict(&mut self, e: &Entry) -> anyhow::Result<()> {
        if e.state == EntryState::Conflict {
            return Ok(());
        }
        self.set_state(e.id, EntryState::Conflict)
    }

    /// Kullanıcı onayıyla çakışmaları kapatır (`lab recover`); kapatılan kimlikleri döndürür.
    pub fn acknowledge_conflicts(&mut self) -> anyhow::Result<Vec<u32>> {
        let _lock = lock_file(&self.lock_path())?;
        let ids: Vec<u32> = self
            .entries
            .iter()
            .filter(|e| e.state == EntryState::Conflict)
            .map(|e| e.id)
            .collect();
        for id in &ids {
            self.set_state(*id, EntryState::Acknowledged)?;
        }
        Ok(ids)
    }

    /// Dizindeki kapanmamış journal dosyaları. Okunamayan dosya da (güvenli tarafta kalmak için)
    /// listelenir; `lab recover` ayrıntıyı gösterir.
    pub fn find_stale(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let mut stale = Vec::new();
        if !dir.exists() {
            return Ok(stale);
        }
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            match Journal::open(&path) {
                Ok(j) if j.entries.iter().any(|e| e.state.is_open()) => stale.push(path),
                Ok(_) => {}
                Err(e) => {
                    eprintln!("uyarı: {e:#}");
                    stale.push(path);
                }
            }
        }
        Ok(stale)
    }
}

/// Sahiplik kuralı: lab bu öğeye dokunabilir mi?
pub fn owned(op: &OpKind) -> bool {
    match op {
        OpKind::TunAdapter { name } => name == SPIKE_ADAPTER,
        OpKind::UnicastAddress { if_name, .. } | OpKind::Route { if_name, .. } => {
            if_name == SPIKE_ADAPTER
        }
        // ADR-010: sistem hop limiti yazılmaz. S5 (D5 onayı) gelirse yalnızca hoplimit.rs açar.
        OpKind::HopLimit { .. } => false,
        OpKind::FileValue { path } => lab_temp_path(path),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use super::*;

    /// Bellek içi sistem; her `write` anında journal dosyasındaki son satırı kaydeder.
    #[derive(Default)]
    pub struct FakeOps {
        pub values: RefCell<HashMap<String, String>>,
        pub journal_at_write: RefCell<Vec<String>>,
        pub journal_path: RefCell<Option<PathBuf>>,
        pub writes: Cell<usize>,
        pub fail_read: Cell<bool>,
        /// `Some(kısmi)`: yazma hata verir; `kısmi` ise değer yine de değişir.
        pub fail_write: Cell<Option<bool>>,
    }

    impl FakeOps {
        pub fn set(&self, op: &OpKind, v: &str) {
            self.values
                .borrow_mut()
                .insert(format!("{op:?}"), v.to_string());
        }
        pub fn get(&self, op: &OpKind) -> String {
            self.values
                .borrow()
                .get(&format!("{op:?}"))
                .cloned()
                .unwrap_or_else(|| "absent".into())
        }
    }

    impl SystemOps for FakeOps {
        fn read(&self, op: &OpKind) -> anyhow::Result<String> {
            if self.fail_read.get() {
                anyhow::bail!("okuma hatası (test)");
            }
            Ok(self.get(op))
        }
        fn write(&self, op: &OpKind, value: &str) -> anyhow::Result<()> {
            if let Some(p) = self.journal_path.borrow().as_ref() {
                let last = std::fs::read_to_string(p)
                    .unwrap_or_default()
                    .lines()
                    .last()
                    .unwrap_or("")
                    .to_string();
                self.journal_at_write.borrow_mut().push(last);
            }
            self.writes.set(self.writes.get() + 1);
            if let Some(partial) = self.fail_write.get() {
                if partial {
                    self.set(op, "yarım");
                }
                anyhow::bail!("yazma hatası (test)");
            }
            self.set(op, value);
            Ok(())
        }
    }

    pub fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alterdns-lab-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn adapter() -> OpKind {
        OpKind::TunAdapter {
            name: SPIKE_ADAPTER.into(),
        }
    }

    fn route(if_name: &str) -> OpKind {
        OpKind::Route {
            prefix: "203.0.113.7/32".into(),
            if_name: if_name.into(),
        }
    }

    #[test]
    fn apply_writes_pending_before_system_change() {
        let dir = temp_dir("pending");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        *ops.journal_path.borrow_mut() = Some(j.path().to_path_buf());
        j.apply(&ops, adapter(), "present").unwrap();
        let at_write = ops.journal_at_write.borrow();
        assert_eq!(at_write.len(), 1);
        assert!(
            at_write[0].contains("Pending") && at_write[0].contains("\"before\":\"absent\""),
            "{at_write:?}"
        );
        assert_eq!(j.entries()[0].state, EntryState::Applied);
    }

    #[test]
    fn revert_all_restores_before_in_reverse_order() {
        let dir = temp_dir("revert");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        let a = j.apply(&ops, adapter(), "present").unwrap();
        let b = j.apply(&ops, route(SPIKE_ADAPTER), "present").unwrap();
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.reverted, vec![b, a]);
        assert_eq!(ops.get(&adapter()), "absent");
        assert_eq!(ops.get(&route(SPIKE_ADAPTER)), "absent");
        assert!(j.entries().iter().all(|e| e.state == EntryState::Reverted));
    }

    #[test]
    fn revert_skips_value_changed_by_user() {
        let dir = temp_dir("conflict");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        let id = j.apply(&ops, adapter(), "present").unwrap();
        ops.set(&adapter(), "baska"); // kullanıcı arada değiştirdi
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.conflicts, vec![id]);
        assert_eq!(ops.get(&adapter()), "baska");
    }

    #[test]
    fn create_refuses_when_stale_journal_exists() {
        let dir = temp_dir("stale");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        j.apply(&ops, adapter(), "present").unwrap();
        assert_eq!(Journal::find_stale(&dir).unwrap().len(), 1);
        assert!(Journal::create(&dir, "run2").is_err());
        j.revert_all(&ops).unwrap();
        assert!(Journal::find_stale(&dir).unwrap().is_empty());
        assert!(Journal::create(&dir, "run2").is_ok());
    }

    #[test]
    fn journal_reopens_with_last_state() {
        let dir = temp_dir("reopen");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        j.apply(&ops, adapter(), "present").unwrap();
        let again = Journal::open(j.path()).unwrap();
        assert_eq!(again.entries(), j.entries());
    }

    #[test]
    fn route_revert_only_if_on_spike_adapter() {
        let dir = temp_dir("own-route");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        assert!(
            j.apply(&ops, route("Wi-Fi"), "present").is_err(),
            "lab dışı arayüze rota yazılmamalı"
        );
        let id = j.record_external(&ops, route("Wi-Fi"), "present").unwrap();
        j.mark_applied(id).unwrap();
        ops.set(&route("Wi-Fi"), "present");
        let reads_before = ops.writes.get();
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.conflicts, vec![id]);
        assert_eq!(ops.get(&route("Wi-Fi")), "present");
        assert_eq!(ops.writes.get(), reads_before);
    }

    #[test]
    fn adapter_revert_only_if_named_spike() {
        assert!(owned(&adapter()));
        assert!(!owned(&OpKind::TunAdapter {
            name: "Wintun".into()
        }));
        assert!(owned(&OpKind::UnicastAddress {
            if_name: SPIKE_ADAPTER.into(),
            cidr: "198.18.0.1/30".into()
        }));
        assert!(!owned(&OpKind::UnicastAddress {
            if_name: "Ethernet".into(),
            cidr: "198.18.0.1/30".into()
        }));
        assert!(
            !owned(&OpKind::HopLimit {
                family: IpFamily::V4
            }),
            "ADR-010: sistem hop limiti sahiplenilmez"
        );
    }

    #[test]
    fn journal_lives_under_given_dir_as_jsonl() {
        let dir = temp_dir("path");
        let j = Journal::create(&dir, "run1").unwrap();
        assert_eq!(j.path(), dir.join("run1.jsonl"));
    }

    #[test]
    fn revert_read_error_keeps_entry_open_for_recover() {
        let dir = temp_dir("readerr");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        let id = j.apply(&ops, adapter(), "present").unwrap();
        ops.fail_read.set(true);
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.failed.len(), 1);
        assert_eq!(j.entries()[0].state, EntryState::Applied);
        assert_eq!(Journal::find_stale(&dir).unwrap().len(), 1);
        ops.fail_read.set(false);
        assert_eq!(j.revert_all(&ops).unwrap().reverted, vec![id]);
        assert!(Journal::find_stale(&dir).unwrap().is_empty());
    }

    #[test]
    fn apply_write_error_state_depends_on_actual_value() {
        let dir = temp_dir("writeerr");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        ops.fail_write.set(Some(false));
        assert!(j.apply(&ops, adapter(), "present").is_err());
        assert_eq!(j.entries()[0].state, EntryState::Failed);
        assert!(
            Journal::find_stale(&dir).unwrap().is_empty(),
            "değer değişmediyse bayat değil"
        );

        let dir2 = temp_dir("writeerr2");
        let mut j2 = Journal::create(&dir2, "run1").unwrap();
        ops.fail_write.set(Some(true));
        assert!(j2.apply(&ops, adapter(), "present").is_err());
        assert_eq!(j2.entries()[0].state, EntryState::Pending);
        assert_eq!(
            Journal::find_stale(&dir2).unwrap().len(),
            1,
            "yarım yazma geri alınmak üzere açık kalır"
        );
    }

    #[test]
    fn torn_last_line_is_ignored_and_truncated_on_append() {
        let dir = temp_dir("torn");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        j.apply(&ops, adapter(), "present").unwrap();
        let path = j.path().to_path_buf();
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            write!(f, "{{\"id\":2,\"op\":{{\"kind\":\"Tun").unwrap();
        }
        let mut again = Journal::open(&path).unwrap();
        assert_eq!(again.entries().len(), 1);
        again.revert_all(&ops).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'));
        assert!(!text.contains("\"Tun\n") && !text.contains("{\"kind\":\"Tun{"));
        let reopened = Journal::open(&path).unwrap();
        assert_eq!(reopened.entries()[0].state, EntryState::Reverted);
    }

    #[test]
    fn corrupt_middle_line_is_an_error_and_counts_as_stale() {
        let dir = temp_dir("corrupt");
        let path = dir.join("run1.jsonl");
        std::fs::write(&path, "bozuk\n{\"id\":1}\n").unwrap();
        let err = Journal::open(&path).err().unwrap();
        assert!(format!("{err:#}").contains("run1.jsonl:1"), "{err:#}");
        assert_eq!(Journal::find_stale(&dir).unwrap(), vec![path]);
    }

    #[test]
    fn open_rejects_unexpected_state_values() {
        let dir = temp_dir("values");
        let path = dir.join("run1.jsonl");
        let line = serde_json::json!({
            "id": 1,
            "op": { "kind": "Route", "prefix": "203.0.113.7/32", "if_name": SPIKE_ADAPTER },
            "before": "present", "after": "maybe", "state": "Applied"
        });
        std::fs::write(&path, format!("{line}\n")).unwrap();
        assert!(Journal::open(&path).is_err());
        let ok = serde_json::json!({
            "id": 1,
            "op": { "kind": "Route", "prefix": "203.0.113.7/32", "if_name": SPIKE_ADAPTER },
            "before": "absent", "after": "present", "state": "Applied"
        });
        std::fs::write(&path, format!("{ok}\n")).unwrap();
        assert!(Journal::open(&path).is_ok());
    }

    #[test]
    fn open_rejects_values_that_could_inject() {
        let dir = temp_dir("inject");
        let path = dir.join("run1.jsonl");
        let line = serde_json::json!({
            "id": 1,
            "op": { "kind": "TunAdapter", "name": "x\u{2019}; Start-Process calc; \u{2018}" },
            "before": "absent", "after": "present", "state": "Applied"
        });
        std::fs::write(&path, format!("{line}\n")).unwrap();
        assert!(Journal::open(&path).is_err());

        let outside = std::env::temp_dir()
            .join("alterdns-x")
            .join("..")
            .join("..")
            .join("hedef.txt");
        let line = serde_json::json!({
            "id": 1,
            "op": { "kind": "FileValue", "path": outside },
            "before": "a", "after": "b", "state": "Applied"
        });
        std::fs::write(&path, format!("{line}\n")).unwrap();
        assert!(Journal::open(&path).is_err());
    }

    #[test]
    fn lab_temp_path_rules() {
        let t = std::env::temp_dir();
        assert!(lab_temp_path(&t.join("alterdns-lab-1").join("f.txt")));
        assert!(
            !lab_temp_path(&t.join("alterdns-lab-1")),
            "dizinin kendisi değil"
        );
        assert!(!lab_temp_path(&t.join("baska").join("f.txt")));
        assert!(!lab_temp_path(
            &t.join("alterdns-x").join("..").join("..").join("f.txt")
        ));
        assert!(!lab_temp_path(Path::new(r"C:\Windows\win.ini")));
    }

    #[test]
    fn conflict_blocks_until_acknowledged() {
        let dir = temp_dir("ack");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        let id = j.apply(&ops, adapter(), "present").unwrap();
        ops.set(&adapter(), "baska");
        assert_eq!(j.revert_all(&ops).unwrap().conflicts, vec![id]);
        assert_eq!(Journal::find_stale(&dir).unwrap().len(), 1);
        assert_eq!(j.acknowledge_conflicts().unwrap(), vec![id]);
        assert!(Journal::find_stale(&dir).unwrap().is_empty());
        assert_eq!(ops.get(&adapter()), "baska", "onay değere dokunmaz");
    }

    #[test]
    fn second_reverter_sees_first_revert() {
        let dir = temp_dir("twice");
        let ops = FakeOps::default();
        let mut parent = Journal::create(&dir, "run1").unwrap();
        parent.apply(&ops, adapter(), "present").unwrap();
        let mut watchdog = Journal::open(parent.path()).unwrap();
        assert_eq!(parent.revert_all(&ops).unwrap().reverted.len(), 1);
        let writes = ops.writes.get();
        let second = watchdog.revert_all(&ops).unwrap();
        assert!(second.reverted.is_empty() && second.conflicts.is_empty());
        assert_eq!(ops.writes.get(), writes, "ikinci geri alma yazmaz");
    }
}
