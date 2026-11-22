// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! IPC istemcisi. Pipe tanımlama düzeyinde açılır (sunucu istemcinin kimliğine bürünemez) ve
//! sahibi SYSTEM değilse bağlanılmaz (K21). Bağlantı koptuğunda çağrılar `IpcConnect` döner;
//! yeniden bağlanmak için yeni bir `Client` açılır.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_winsys::{PipeConn, PipeOwner, connect_pipe};
use serde_json::{Value, json};

use crate::envelope::{Event, Message, PROTOCOL, Request, WireError};
use crate::frame::{decode, decode_len, encode};
use crate::version::version_compatible;

/// İstemcinin sürümü (workspace sürümü).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ConnectOptions<'a> {
    pub pipe_name: &'a str,
    pub client_version: &'a str,
    pub min_server: &'a str,
    owner: PipeOwner,
}

impl<'a> ConnectOptions<'a> {
    pub fn product(pipe_name: &'a str) -> ConnectOptions<'a> {
        ConnectOptions {
            pipe_name,
            client_version: VERSION,
            min_server: VERSION,
            owner: PipeOwner::System,
        }
    }

    /// Yalnızca testler: sahibi bu işlemin varsayılan sahibi olan pipe'a güvenir.
    #[cfg(feature = "test-ops")]
    #[must_use]
    pub fn trust_own_pipes(mut self) -> ConnectOptions<'a> {
        self.owner = PipeOwner::ProcessDefaultOwner;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloInfo {
    pub server_version: String,
    pub min_client: String,
    pub instance: u64,
}

pub struct Client {
    pipe: PipeConn,
    next_id: u64,
    queue: VecDeque<Event>,
}

fn connect_err(e: impl std::error::Error + Send + Sync + 'static) -> AdnsError {
    AdnsError::new(AdnsErrorCode::IpcConnect).with_source(e)
}

enum ReadErr {
    Timeout,
    Failed(AdnsError),
}

impl Client {
    pub fn connect(opts: &ConnectOptions<'_>) -> Result<(Client, HelloInfo), AdnsError> {
        let pipe = connect_pipe(opts.pipe_name, opts.owner, CLIENT_TIMEOUT).map_err(connect_err)?;
        let mut c = Client {
            pipe,
            next_id: 0,
            queue: VecDeque::new(),
        };
        let v = c.call(
            "svc.hello",
            json!({"client_version": opts.client_version, "min_server": opts.min_server}),
        )?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let (Some(server_version), Some(min_client), Some(instance)) = (
            text("server_version"),
            text("min_client"),
            v.get("instance").and_then(Value::as_u64),
        ) else {
            return Err(AdnsError::new(AdnsErrorCode::IpcVersionMismatch)
                .with_source(std::io::Error::other("hello yanıtı eksik")));
        };
        version_compatible(opts.client_version, &min_client)?;
        Ok((
            c,
            HelloInfo {
                server_version,
                min_client,
                instance,
            },
        ))
    }

    fn read(&mut self, prefix_timeout: Option<Duration>) -> Result<Message, ReadErr> {
        let mut prefix = [0u8; 4];
        match self.pipe.read_exact_timeout(&mut prefix, prefix_timeout) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Err(ReadErr::Timeout),
            Err(e) => return Err(ReadErr::Failed(connect_err(e))),
        }
        let n = decode_len(prefix).map_err(ReadErr::Failed)?;
        let mut body = vec![0u8; n];
        self.pipe
            .read_exact_timeout(&mut body, Some(CLIENT_TIMEOUT))
            .map_err(|e| ReadErr::Failed(connect_err(e)))?;
        decode(&body).map_err(ReadErr::Failed)
    }

    /// Yöntemi çağırır; yanıt beklenirken gelen olaylar kuyruğa alınır.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, AdnsError> {
        self.next_id += 1;
        let id = format!("c{}", self.next_id);
        let frame = encode(&Message::Request(Request {
            v: PROTOCOL,
            id: id.clone(),
            method: method.to_string(),
            params,
        }))?;
        self.pipe
            .write_all_timeout(&frame, CLIENT_TIMEOUT)
            .map_err(connect_err)?;
        loop {
            match self.read(Some(CLIENT_TIMEOUT)) {
                Ok(Message::Response(r)) if r.id == id => {
                    return if r.ok {
                        Ok(r.result.unwrap_or(Value::Null))
                    } else {
                        Err(r.error.map_or_else(
                            || AdnsError::new(AdnsErrorCode::GenUnexpected),
                            WireError::into_error,
                        ))
                    };
                }
                Ok(Message::Event(e)) => self.queue.push_back(e),
                Ok(_) => {}
                Err(ReadErr::Timeout) => {
                    return Err(connect_err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "yanıt gelmedi",
                    )));
                }
                Err(ReadErr::Failed(e)) => return Err(e),
            }
        }
    }

    /// Sıradaki olay; `timeout` içinde gelmezse `Ok(None)`.
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, AdnsError> {
        if let Some(e) = self.queue.pop_front() {
            return Ok(Some(e));
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.read(Some(deadline.saturating_duration_since(Instant::now()))) {
                Ok(Message::Event(e)) => return Ok(Some(e)),
                Ok(_) => {}
                Err(ReadErr::Timeout) => return Ok(None),
                Err(ReadErr::Failed(e)) => return Err(e),
            }
        }
    }
}
