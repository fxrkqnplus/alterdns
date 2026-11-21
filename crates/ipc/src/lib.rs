// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Servis IPC'si (`docs/ARCHITECTURE.md` §4): named pipe üzerinde uzunluk önekli JSON.

#![forbid(unsafe_code)]

pub mod client;
pub mod envelope;
pub mod events;
pub mod frame;
pub mod method;
pub mod policy;
pub mod server;
pub mod version;

pub use client::{CLIENT_TIMEOUT, Client, ConnectOptions, HelloInfo, VERSION};
pub use envelope::{Event, Message, PROTOCOL, Request, Response, WireError, valid_id};
pub use events::{EventBus, routes_to};
pub use frame::{MAX_MESSAGE, decode, decode_len, encode};
pub use method::Method;
pub use policy::{Caller, CallerPolicy, authorize};
#[cfg(feature = "test-ops")]
pub use server::TEST_SDDL;
pub use server::{
    Handler, IO_TIMEOUT, MAX_HELLO, MAX_INSTANCES, MAX_REQUEST, PRODUCT_SDDL, Server, ServerConfig,
};
pub use version::version_compatible;

/// Servisin pipe'ı.
pub const PIPE_NAME: &str = r"\\.\pipe\AlterDNS.Svc.v1";
