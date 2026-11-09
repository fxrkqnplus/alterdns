// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lab journal'ı (ARCHITECTURE §6'nın küçük öncülü): önce kaydet (fsync), sonra uygula,
//! gerekirse ters sırada geri al. Biçim: JSON Lines, her durum değişimi bir satır; son satır kazanır.
//!
//! Sahiplik kuralı: yalnızca lab'ın oluşturduğu öğelere (ad `AlterDNS-Spike` olan adaptör,
//! o adaptördeki adres ve rota, hop limit, geçici dizindeki test dosyası) dokunulur. Geri
//! alma anında değer bizim yazdığımızdan farklıysa (kullanıcı değiştirmiş) dokunulmaz.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Lab'ın oluşturabileceği tek sanal adaptörün adı.
pub const SPIKE_ADAPTER: &str = "AlterDNS-Spike";

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
    /// Yalnızca geçici dizin altında; testler için.
    FileValue {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryState {
    Pending,
    Applied,
    Reverted,
    Conflict,
    Failed,
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
    pub failed: Vec<(u32, String)>,
}

pub struct Journal {
    path: PathBuf,
    entries: Vec<Entry>,
}

impl Journal {
    /// Yeni koşu journal'ı. Dizinde bitmemiş (Pending/Applied) bir journal varsa hata verir.
    pub fn create(dir: &Path, run_id: &str) -> anyhow::Result<Journal> {
        std::fs::create_dir_all(dir)?;
        let stale = Self::find_stale(dir)?;
        if !stale.is_empty() {
            anyhow::bail!(
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
            anyhow::bail!("{} zaten var", path.display());
        }
        std::fs::File::create(&path)?.sync_all()?;
        Ok(Journal {
            path,
            entries: Vec::new(),
        })
    }

    /// Var olan journal'ı okur (bekçi ve `lab recover` için). Her kimlik için son satır geçerlidir.
    pub fn open(path: &Path) -> anyhow::Result<Journal> {
        let text = std::fs::read_to_string(path)?;
        let mut entries: Vec<Entry> = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let e: Entry = serde_json::from_str(line)?;
            match entries.iter_mut().find(|x| x.id == e.id) {
                Some(x) => *x = e,
                None => entries.push(e),
            }
        }
        Ok(Journal {
            path: path.to_path_buf(),
            entries,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn persist(&self, e: &Entry) -> anyhow::Result<()> {
        use std::io::Write;
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
        self.persist(&e)?;
        self.entries.push(e);
        Ok(id)
    }

    /// Önce okur, `Pending` yazar (fsync), sistemi değiştirir, `Applied` yazar (fsync).
    pub fn apply(&mut self, ops: &dyn SystemOps, op: OpKind, after: &str) -> anyhow::Result<u32> {
        if !owned(&op) {
            anyhow::bail!("lab bu öğeye dokunamaz (sahiplik kuralı): {op:?}");
        }
        let id = self.push_pending(ops, op.clone(), after)?;
        if let Err(e) = ops.write(&op, after) {
            self.set_state(id, EntryState::Failed)?;
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

    /// Bitmemiş kayıtları ters sırada geri alır. Değer önceki hâldeyse yalnızca kapatılır;
    /// bizim yazdığımız değerdeyse önceki değer geri yazılır; başka bir değerdeyse ya da
    /// öğe lab'a ait değilse dokunulmaz (`Conflict`).
    pub fn revert_all(&mut self, ops: &dyn SystemOps) -> anyhow::Result<RevertReport> {
        let mut report = RevertReport::default();
        let open: Vec<Entry> = self
            .entries
            .iter()
            .rev()
            .filter(|e| matches!(e.state, EntryState::Pending | EntryState::Applied))
            .cloned()
            .collect();
        for e in open {
            let current = match ops.read(&e.op) {
                Ok(v) => v,
                Err(err) => {
                    self.set_state(e.id, EntryState::Failed)?;
                    report.failed.push((e.id, err.to_string()));
                    continue;
                }
            };
            if current == e.before {
                self.set_state(e.id, EntryState::Reverted)?;
                report.reverted.push(e.id);
            } else if current == e.after && owned(&e.op) {
                match ops.write(&e.op, &e.before) {
                    Ok(()) => {
                        self.set_state(e.id, EntryState::Reverted)?;
                        report.reverted.push(e.id);
                    }
                    Err(err) => {
                        self.set_state(e.id, EntryState::Failed)?;
                        report.failed.push((e.id, err.to_string()));
                    }
                }
            } else {
                self.set_state(e.id, EntryState::Conflict)?;
                report.conflicts.push(e.id);
            }
        }
        Ok(report)
    }

    /// Dizindeki bitmemiş (Pending/Applied kaydı olan) journal dosyaları.
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
            let j = Journal::open(&path)?;
            if j.entries
                .iter()
                .any(|e| matches!(e.state, EntryState::Pending | EntryState::Applied))
            {
                stale.push(path);
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
        OpKind::HopLimit { .. } => true,
        OpKind::FileValue { path } => path.starts_with(std::env::temp_dir()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// Bellek içi sistem; her `write` anında journal dosyasındaki son satırı kaydeder.
    #[derive(Default)]
    pub struct FakeOps {
        pub values: RefCell<HashMap<String, String>>,
        pub journal_at_write: RefCell<Vec<String>>,
        pub journal_path: RefCell<Option<PathBuf>>,
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

    fn hop() -> OpKind {
        OpKind::HopLimit {
            family: IpFamily::V4,
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
        ops.set(&hop(), "65");
        let mut j = Journal::create(&dir, "run1").unwrap();
        *ops.journal_path.borrow_mut() = Some(j.path().to_path_buf());
        j.apply(&ops, hop(), "128").unwrap();
        let at_write = ops.journal_at_write.borrow();
        assert_eq!(at_write.len(), 1);
        assert!(
            at_write[0].contains("Pending") && at_write[0].contains("\"before\":\"65\""),
            "{at_write:?}"
        );
        assert_eq!(j.entries()[0].state, EntryState::Applied);
    }

    #[test]
    fn revert_all_restores_before_in_reverse_order() {
        let dir = temp_dir("revert");
        let ops = FakeOps::default();
        ops.set(&hop(), "65");
        let mut j = Journal::create(&dir, "run1").unwrap();
        let a = j.apply(&ops, hop(), "128").unwrap();
        let b = j.apply(&ops, route(SPIKE_ADAPTER), "present").unwrap();
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.reverted, vec![b, a]);
        assert_eq!(ops.get(&hop()), "65");
        assert_eq!(ops.get(&route(SPIKE_ADAPTER)), "absent");
        assert!(j.entries().iter().all(|e| e.state == EntryState::Reverted));
    }

    #[test]
    fn revert_skips_value_changed_by_user() {
        let dir = temp_dir("conflict");
        let ops = FakeOps::default();
        ops.set(&hop(), "65");
        let mut j = Journal::create(&dir, "run1").unwrap();
        let id = j.apply(&ops, hop(), "128").unwrap();
        ops.set(&hop(), "64"); // kullanıcı arada değiştirdi
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.conflicts, vec![id]);
        assert_eq!(ops.get(&hop()), "64");
    }

    #[test]
    fn create_refuses_when_stale_journal_exists() {
        let dir = temp_dir("stale");
        let ops = FakeOps::default();
        let mut j = Journal::create(&dir, "run1").unwrap();
        j.apply(&ops, hop(), "128").unwrap();
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
        j.apply(&ops, hop(), "128").unwrap();
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
        let report = j.revert_all(&ops).unwrap();
        assert_eq!(report.conflicts, vec![id]);
        assert_eq!(ops.get(&route("Wi-Fi")), "present");
    }

    #[test]
    fn adapter_revert_only_if_named_spike() {
        assert!(owned(&OpKind::TunAdapter {
            name: SPIKE_ADAPTER.into()
        }));
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
        assert!(owned(&hop()));
    }

    #[test]
    fn journal_lives_under_given_dir_as_jsonl() {
        let dir = temp_dir("path");
        let j = Journal::create(&dir, "run1").unwrap();
        assert_eq!(j.path(), dir.join("run1.jsonl"));
    }
}
