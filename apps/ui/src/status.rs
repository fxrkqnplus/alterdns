// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Servis durumu döngüsü. Arayüz iş parçacığından ayrı çalışır: servise bağlanır, durumu okur,
//! olaylara abone olur ve bağlantı koparsa 2, 4, 8, 16, 30 sn (üst sınır) geri çekilmeyle yeniden
//! dener. Bağlıyken boşta uyanmaz; olay okuması uzun süre bekler. Yeniden bağlanınca servis
//! örneği (`instance`) değiştiyse `ServiceRestarted` bildirilir (ADNS-SVC-002).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{Client, ConnectOptions, Event, Method, PIPE_NAME};
use serde_json::{Value, json};

const BACKOFF_SECS: [u64; 5] = [2, 4, 8, 16, 30];
/// Bağlıyken olay bekleme süresi; dolunca yalnızca durdurma bayrağına bakılır.
const EVENT_WAIT: Duration = Duration::from_secs(3600);
/// Geri çekilme sırasında durdurma bayrağının denetlenme aralığı.
const STOP_POLL: Duration = Duration::from_millis(100);

/// Durum kaynağı: gerçek uygulamada IPC istemcisi, testlerde sahte.
pub trait StatusSource: Send {
    /// Bağlanır; servis örneğinin kimliğini döndürür.
    fn connect(&mut self) -> Result<u64, AdnsError>;
    fn status(&mut self) -> Result<Value, AdnsError>;
    fn subscribe(&mut self) -> Result<(), AdnsError>;
    /// Sıradaki olay; bekleme süresi dolarsa `Ok(None)`.
    fn next_event(&mut self) -> Result<Option<Event>, AdnsError>;
}

/// Arayüzün gösterdiği servis durumu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiStatus {
    Connecting,
    Ready { version: String },
    Unreachable(AdnsErrorCode),
}

/// Kullanıcıya bildirim olarak gösterilecek olay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Servis yeniden başladı (ADNS-SVC-002).
    ServiceRestarted,
}

impl Notice {
    /// Bildirimin metni katalogdan gelir.
    pub fn code(self) -> AdnsErrorCode {
        match self {
            Notice::ServiceRestarted => AdnsErrorCode::SvcRestarted,
        }
    }
}

/// Geri çekilme ve servis örneği takibi (saf).
#[derive(Debug, Default)]
pub struct StatusMachine {
    last_instance: Option<u64>,
    failures: usize,
}

impl StatusMachine {
    /// Başarılı bağlantı: geri çekilme sıfırlanır; örnek değiştiyse bildirim döner.
    pub fn on_connected(&mut self, instance: u64) -> Option<Notice> {
        self.failures = 0;
        let notice = match self.last_instance {
            Some(prev) if prev != instance => Some(Notice::ServiceRestarted),
            _ => None,
        };
        self.last_instance = Some(instance);
        notice
    }

    /// Başarısız deneme ya da kopan bağlantı: bir sonraki denemeye kadar beklenecek süre.
    pub fn on_failure(&mut self) -> Duration {
        let secs = BACKOFF_SECS[self.failures.min(BACKOFF_SECS.len() - 1)];
        self.failures = self.failures.saturating_add(1);
        Duration::from_secs(secs)
    }
}

/// Gerçek kaynak: ürün pipe'ı üzerinden IPC istemcisi.
#[derive(Default)]
pub struct IpcSource {
    client: Option<Client>,
}

impl IpcSource {
    fn client(&mut self) -> Result<&mut Client, AdnsError> {
        self.client
            .as_mut()
            .ok_or_else(|| AdnsError::new(AdnsErrorCode::IpcConnect))
    }
}

impl StatusSource for IpcSource {
    fn connect(&mut self) -> Result<u64, AdnsError> {
        self.client = None;
        let (client, hello) = Client::connect(&ConnectOptions::product(PIPE_NAME))?;
        self.client = Some(client);
        Ok(hello.instance)
    }

    fn status(&mut self) -> Result<Value, AdnsError> {
        self.client()?.call(Method::Status.name(), json!({}))
    }

    fn subscribe(&mut self) -> Result<(), AdnsError> {
        self.client()?
            .call(Method::Subscribe.name(), json!({}))
            .map(|_| ())
    }

    fn next_event(&mut self) -> Result<Option<Event>, AdnsError> {
        let result = self.client()?.next_event(EVENT_WAIT);
        if result.is_err() {
            self.client = None;
        }
        result
    }
}

fn stopped(stop: &AtomicBool) -> bool {
    stop.load(Ordering::SeqCst)
}

/// `wait` kadar bekler; durdurulursa erken döner (yanlış: durduruldu).
fn backoff(wait: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + wait;
    while !stopped(stop) {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(STOP_POLL));
    }
    false
}

fn connect_all(source: &mut impl StatusSource) -> Result<(u64, String), AdnsError> {
    let instance = source.connect()?;
    let status = source.status()?;
    source.subscribe()?;
    let version = status
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok((instance, version))
}

/// Döngüyü `stop` kalkana kadar çalıştırır. `sink` her durum değişiminde çağrılır (arayüze
/// `slint::invoke_from_event_loop` ile aktarılması çağıranın işidir).
pub fn run(
    mut source: impl StatusSource,
    mut sink: impl FnMut(UiStatus, Option<Notice>) + Send + 'static,
    stop: Arc<AtomicBool>,
) {
    let mut machine = StatusMachine::default();
    let mut last: Option<UiStatus> = None;
    let mut emit = |status: UiStatus, notice: Option<Notice>| {
        if last.as_ref() != Some(&status) || notice.is_some() {
            last = Some(status.clone());
            sink(status, notice);
        }
    };
    emit(UiStatus::Connecting, None);
    while !stopped(&stop) {
        match connect_all(&mut source) {
            Ok((instance, version)) => {
                let notice = machine.on_connected(instance);
                emit(UiStatus::Ready { version }, notice);
                // Bağlı: olaylar beklenir. Faz 1'de olaylar durumu değiştirmez; kopma aranır.
                let error = loop {
                    if stopped(&stop) {
                        return;
                    }
                    match source.next_event() {
                        Ok(Some(event)) => tracing::debug!(event = %event.event, "servis olayı"),
                        Ok(None) => {}
                        Err(e) => break e,
                    }
                };
                tracing::warn!(code = error.code().as_str(), "servis bağlantısı koptu");
                emit(UiStatus::Unreachable(AdnsErrorCode::IpcConnect), None);
            }
            Err(e) => {
                tracing::debug!(code = e.code().as_str(), "servise bağlanılamadı");
                emit(UiStatus::Unreachable(e.code()), None);
            }
        }
        if stopped(&stop) || !backoff(machine.on_failure(), &stop) {
            return;
        }
    }
}
