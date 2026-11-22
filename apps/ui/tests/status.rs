// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Durum döngüsü: geri çekilmeli yeniden bağlanma, servis yeniden başlama bildirimi ve
//! bağlantı kopunca "bağlanılamıyor" durumu.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::Event;
use alterdns_ui_app::status::{Notice, StatusMachine, StatusSource, UiStatus, run};
use serde_json::{Value, json};

#[test]
fn status_machine_backoff_caps_at_30() {
    let mut m = StatusMachine::default();
    let waits: Vec<u64> = (0..7).map(|_| m.on_failure().as_secs()).collect();
    assert_eq!(waits, vec![2, 4, 8, 16, 30, 30, 30]);
}

#[test]
fn status_machine_resets_backoff_after_success() {
    let mut m = StatusMachine::default();
    m.on_failure();
    m.on_failure();
    let _ = m.on_connected(1);
    assert_eq!(m.on_failure(), Duration::from_secs(2));
}

#[test]
fn status_machine_reports_restart_only_on_instance_change() {
    let mut m = StatusMachine::default();
    assert_eq!(m.on_connected(7), None);
    m.on_failure();
    assert_eq!(m.on_connected(7), None);
    m.on_failure();
    assert_eq!(m.on_connected(8), Some(Notice::ServiceRestarted));
    assert_eq!(m.on_connected(8), None);
}

/// Sırayla verilen bağlantı sonuçları; her başarılı bağlantıda olay okuması bir kez boş döner,
/// ikincisinde bağlantı kopar. Liste bitince durdurma bayrağı kalkar.
struct Fake {
    connects: VecDeque<Result<u64, AdnsErrorCode>>,
    events: VecDeque<Result<Option<Event>, AdnsErrorCode>>,
    stop: Arc<AtomicBool>,
}

impl StatusSource for Fake {
    fn connect(&mut self) -> Result<u64, AdnsError> {
        match self.connects.pop_front() {
            Some(Ok(i)) => Ok(i),
            Some(Err(c)) => Err(AdnsError::new(c)),
            None => {
                self.stop.store(true, Ordering::SeqCst);
                Err(AdnsError::new(AdnsErrorCode::IpcConnect))
            }
        }
    }

    fn status(&mut self) -> Result<Value, AdnsError> {
        Ok(json!({ "version": "0.0.0" }))
    }

    fn subscribe(&mut self) -> Result<(), AdnsError> {
        Ok(())
    }

    fn next_event(&mut self) -> Result<Option<Event>, AdnsError> {
        match self.events.pop_front() {
            Some(Ok(e)) => Ok(e),
            Some(Err(c)) => Err(AdnsError::new(c)),
            None => {
                self.stop.store(true, Ordering::SeqCst);
                Ok(None)
            }
        }
    }
}

type Seen = Arc<Mutex<Vec<(UiStatus, Option<Notice>)>>>;

fn collect(source: Fake, stop: Arc<AtomicBool>) -> Vec<(UiStatus, Option<Notice>)> {
    let seen: Seen = Arc::default();
    let sink = seen.clone();
    run(source, move |s, n| sink.lock().unwrap().push((s, n)), stop);
    seen.lock().unwrap().clone()
}

#[test]
fn run_emits_unreachable_then_ready() {
    let stop = Arc::new(AtomicBool::new(false));
    let source = Fake {
        connects: VecDeque::from([Err(AdnsErrorCode::IpcConnect), Ok(1)]),
        events: VecDeque::from([Ok(None)]),
        stop: stop.clone(),
    };
    let seen = collect(source, stop);
    assert_eq!(
        seen,
        vec![
            (UiStatus::Connecting, None),
            (UiStatus::Unreachable(AdnsErrorCode::IpcConnect), None),
            (
                UiStatus::Ready {
                    version: "0.0.0".into()
                },
                None
            ),
        ]
    );
}

#[test]
fn run_reports_disconnect_and_restart() {
    let stop = Arc::new(AtomicBool::new(false));
    let source = Fake {
        connects: VecDeque::from([Ok(1), Ok(2)]),
        // İlk bağlantıda olay okuması kopar; ikincisinde liste biter ve döngü durur.
        events: VecDeque::from([Err(AdnsErrorCode::IpcConnect)]),
        stop: stop.clone(),
    };
    let seen = collect(source, stop);
    let ready = UiStatus::Ready {
        version: "0.0.0".into(),
    };
    assert_eq!(
        seen,
        vec![
            (UiStatus::Connecting, None),
            (ready.clone(), None),
            (UiStatus::Unreachable(AdnsErrorCode::IpcConnect), None),
            (ready, Some(Notice::ServiceRestarted)),
        ]
    );
}

#[test]
fn run_returns_promptly_when_stopped_during_backoff() {
    let stop = Arc::new(AtomicBool::new(false));
    let source = Fake {
        connects: VecDeque::from([Err(AdnsErrorCode::IpcConnect); 3]),
        events: VecDeque::new(),
        stop: stop.clone(),
    };
    let stopper = stop.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        stopper.store(true, Ordering::SeqCst);
    });
    let started = std::time::Instant::now();
    let _ = collect(source, stop);
    t.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}
