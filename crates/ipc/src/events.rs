// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Olay aboneliği (K22). Abonelik çağıranın belirtecindeki oturum kimliğiyle etiketlenir;
//! kullanıcıya özgü bir olay yalnızca o oturuma gider. Yayıncı hiçbir zaman beklemez: abonenin
//! kuyruğu doluysa olay o abone için düşer, yavaş abonenin bağlantısı yazma zaman aşımında
//! kapanır.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;

use crate::envelope::{Event, PROTOCOL};

/// Abone başına kuyruk uzunluğu.
const QUEUE: usize = 64;

/// `None` herkese, `Some(oturum)` yalnızca o oturumun abonelerine.
pub fn routes_to(target: Option<u32>, subscriber_session: u32) -> bool {
    target.is_none_or(|t| t == subscriber_session)
}

struct Subscriber {
    id: u64,
    session: u32,
    tx: SyncSender<Event>,
}

#[derive(Default)]
struct Inner {
    next: u64,
    subs: Vec<Subscriber>,
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<Mutex<Inner>>,
}

impl EventBus {
    pub fn publish(&self, name: &str, data: Value, session: Option<u32>) {
        let ev = Event {
            v: PROTOCOL,
            event: name.to_string(),
            data,
        };
        let mut g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        g.subs.retain(|s| {
            !routes_to(session, s.session)
                || !matches!(
                    s.tx.try_send(ev.clone()),
                    Err(TrySendError::Disconnected(_))
                )
        });
    }

    pub(crate) fn subscribe(&self, session: u32) -> (u64, Receiver<Event>) {
        let (tx, rx) = sync_channel(QUEUE);
        let mut g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        g.next += 1;
        let id = g.next;
        g.subs.push(Subscriber { id, session, tx });
        (id, rx)
    }

    pub(crate) fn unsubscribe(&self, id: u64) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .subs
            .retain(|s| s.id != id);
    }

    pub(crate) fn clear(&self) {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .subs
            .clear();
    }
}
