// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kayıt modeli ve journal dosyası.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use alterdns_errors::AdnsErrorCode;
use alterdns_journal::file::MAX_FILE_BYTES;
use alterdns_journal::{
    ChangeRecord, JournalFile, Kind, Lifetime, Line, Snapshot, State, list, lock_dir_with,
    new_txn_id, valid_txn_id,
};

mod common;

fn temp(name: &str) -> common::TempDir {
    common::temp("jf", name)
}

fn record(id: u32, kind: Kind, state: State) -> ChangeRecord {
    ChangeRecord {
        id,
        txn_id: "t-1".into(),
        component: "test".into(),
        kind,
        target: "hedef".into(),
        before: Snapshot::Absent,
        after: Snapshot::Present("1".into()),
        state,
        created_by: "alterdns".into(),
        ts: 0,
    }
}

#[test]
fn line_roundtrip() {
    let schema = serde_json::to_string(&Line::Schema { schema: 1 }).unwrap();
    assert_eq!(schema, r#"{"t":"Schema","schema":1}"#);
    for line in [
        Line::Schema { schema: 1 },
        Line::TxnBegin {
            txn_id: "t-1".into(),
            component: "c".into(),
            lifetime: Lifetime::Session,
            ts: 5,
        },
        Line::TxnCommit {
            txn_id: "t-1".into(),
            ts: 6,
        },
        Line::Record(record(1, Kind::RegistryValue, State::Applied)),
    ] {
        let text = serde_json::to_string(&line).unwrap();
        assert_eq!(serde_json::from_str::<Line>(&text).unwrap(), line, "{text}");
    }
}

#[test]
fn unknown_kind_parses_as_unknown() {
    let text = serde_json::to_string(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Applied,
    )))
    .unwrap()
    .replace("RegistryValue", "TunAdapter");
    let Line::Record(r) = serde_json::from_str::<Line>(&text).unwrap() else {
        panic!("kayıt değil");
    };
    assert_eq!(r.kind, Kind::Unknown);
}

#[test]
fn txn_ids_are_validated() {
    let id = new_txn_id("Dev Install!");
    assert!(valid_txn_id(&id), "{id}");
    assert!(id.starts_with("dev-install-"), "{id}");
    assert!(valid_txn_id(&new_txn_id(&"x".repeat(200))));
    for bad in ["", "../x", "A", "a/b", &"a".repeat(65)] {
        assert!(!valid_txn_id(bad), "{bad}");
    }
}

#[test]
fn create_writes_schema_and_begin() {
    let d = temp("create");
    let j = JournalFile::create(&d, "t-1", "test", Lifetime::Persistent).unwrap();
    assert_eq!(j.lifetime(), Some(Lifetime::Persistent));
    assert!(!j.committed());
    assert!(JournalFile::create(&d, "t-1", "test", Lifetime::Persistent).is_err());
    assert!(JournalFile::create(&d, "../kaç", "test", Lifetime::Persistent).is_err());
    // Canlı txn dosyası kilitlidir; içerik txn kapandıktan sonra okunur.
    let path = j.path().to_path_buf();
    drop(j);
    let text = std::fs::read_to_string(&path).unwrap();
    let first = text.lines().next().unwrap();
    assert_eq!(first, r#"{"t":"Schema","schema":1}"#);
}

#[test]
fn torn_tail_ignored_middle_corruption_stops() {
    let d = temp("torn");
    let mut j = JournalFile::create(&d, "t-2", "test", Lifetime::Session).unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Pending,
    )))
    .unwrap();
    let path = j.path().to_path_buf();
    drop(j);
    // Yarım son satır: yok sayılır.
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(r#"{"t":"Record","id":2,"#);
    std::fs::write(&path, &text).unwrap();
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(j.records().len(), 1);
    drop(j);
    // Ortadaki bozuk satır: hata.
    let broken = text.replacen(r#"{"t":"TxnBegin""#, r#"{"t":"Bozuk""#, 1);
    std::fs::write(&path, broken + "\n").unwrap();
    let err = JournalFile::open(&path).err().unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysRevertFailed);
}

/// Kesilmiş son satır dosyadan atılır; yoksa sonraki satır ona eklenir ve dosya bir daha
/// okunamazdı (inceleme bulgusu, 2026-11-21).
#[test]
fn torn_tail_is_cut_so_later_appends_stay_readable() {
    let d = temp("torncut");
    let mut j = JournalFile::create(&d, "t-9", "test", Lifetime::Session).unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Applied,
    )))
    .unwrap();
    let path = j.path().to_path_buf();
    drop(j);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(r#"{"t":"Record","id":1,"st"#);
    std::fs::write(&path, &text).unwrap();
    drop(JournalFile::open(&path).unwrap().unwrap());
    // Canlı tutamaç dosyayı kilitler; içerik tutamaç düştükten sonra okunur.
    assert!(std::fs::read_to_string(&path).unwrap().ends_with('\n'));
    let mut j = JournalFile::open(&path).unwrap().unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Reverted,
    )))
    .unwrap();
    drop(j);
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(j.records()[0].state, State::Reverted);
}

/// Oluşturma şema satırı diske yazılmadan kesildiyse dosya boştur: kayıt yok, hata değil.
#[test]
fn empty_file_is_an_empty_txn() {
    let d = temp("empty");
    let path = d.join("t-10.jsonl");
    std::fs::write(&path, "").unwrap();
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert!(j.records().is_empty());
    assert!(!j.committed());
    drop(j);
    std::fs::write(&path, r#"{"t":"Sche"#).unwrap();
    let j = JournalFile::open(&path).unwrap().unwrap();
    assert!(j.records().is_empty());
}

/// Yeni kayıt sınırı (4 MiB) durum satırlarını engellemez: sınıra yakın bir txn de geri alınır.
#[test]
fn state_lines_may_exceed_new_record_limit() {
    let d = temp("cap");
    let filler = "k".repeat(usize::try_from(MAX_FILE_BYTES).unwrap() - 6000);
    let mut j = JournalFile::create(&d, "t-11", &filler, Lifetime::Session).unwrap();
    let mut r = record(1, Kind::RegistryValue, State::Pending);
    r.after = Snapshot::Present("x".repeat(4000));
    r.before = Snapshot::Present("y".repeat(4000));
    let err = j.append(&Line::Record(r.clone())).err().unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysApplyFailed);
    r.state = State::Reverted;
    j.append(&Line::Record(r)).unwrap();
    let path = j.path().to_path_buf();
    drop(j);
    assert!(std::fs::metadata(&path).unwrap().len() > MAX_FILE_BYTES);
    assert_eq!(
        JournalFile::open(&path).unwrap().unwrap().records()[0].state,
        State::Reverted
    );
}

#[test]
fn newer_schema_is_refused_not_guessed() {
    let d = temp("newer");
    let path = d.join("t-3.jsonl");
    std::fs::write(&path, "{\"t\":\"Schema\",\"schema\":2}\n").unwrap();
    assert!(JournalFile::open(&path).is_err());
}

#[test]
fn unknown_kind_is_left_open() {
    let d = temp("unknown");
    let mut j = JournalFile::create(&d, "t-4", "test", Lifetime::Session).unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Applied,
    )))
    .unwrap();
    let path = j.path().to_path_buf();
    drop(j);
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("RegistryValue", "TunAdapter");
    std::fs::write(&path, text).unwrap();
    let j = JournalFile::open(&path).unwrap().unwrap();
    let r = &j.records()[0];
    assert_eq!(r.kind, Kind::Unknown);
    assert!(r.state.is_open());
}

#[test]
fn records_keep_latest_state_per_id() {
    let d = temp("latest");
    let mut j = JournalFile::create(&d, "t-5", "test", Lifetime::Session).unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Pending,
    )))
    .unwrap();
    j.append(&Line::Record(record(
        1,
        Kind::RegistryValue,
        State::Applied,
    )))
    .unwrap();
    j.append(&Line::TxnCommit {
        txn_id: "t-5".into(),
        ts: 1,
    })
    .unwrap();
    let path = j.path().to_path_buf();
    drop(j);
    let reopened = JournalFile::open(&path).unwrap().unwrap();
    assert_eq!(reopened.records().len(), 1);
    assert_eq!(reopened.records()[0].state, State::Applied);
    assert!(reopened.committed());
}

#[test]
fn live_txn_file_is_in_use() {
    let d = temp("inuse");
    let j = JournalFile::create(&d, "t-8", "test", Lifetime::Session).unwrap();
    assert!(JournalFile::open(j.path()).unwrap().is_none());
    let path = j.path().to_path_buf();
    drop(j);
    assert!(JournalFile::open(&path).unwrap().is_some());
}

#[test]
fn oversized_value_is_refused() {
    let d = temp("big");
    let mut j = JournalFile::create(&d, "t-6", "test", Lifetime::Session).unwrap();
    let mut r = record(1, Kind::RegistryValue, State::Pending);
    r.after = Snapshot::Present("x".repeat(5000));
    let err = j.append(&Line::Record(r)).err().unwrap();
    assert_eq!(err.code(), AdnsErrorCode::SysApplyFailed);
}

#[test]
fn lock_times_out_when_held() {
    let d = temp("lock");
    let _held = lock_dir_with(&d, Duration::from_millis(100)).unwrap();
    let start = std::time::Instant::now();
    assert!(lock_dir_with(&d, Duration::from_millis(300)).is_err());
    assert!(start.elapsed() >= Duration::from_millis(300));
}

#[test]
fn list_skips_invalid_names() {
    let d = temp("list");
    JournalFile::create(&d, "t-7", "test", Lifetime::Session).unwrap();
    std::fs::write(d.join("x.txt"), "").unwrap();
    std::fs::write(d.join("Buyuk.jsonl"), "").unwrap();
    let found = list(&d).unwrap();
    assert_eq!(found, [d.join("t-7.jsonl")]);
    assert!(list(&d.join("yok")).unwrap().is_empty());
}
