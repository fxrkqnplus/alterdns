// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Çerçeve ve zarf: gidiş-dönüş, sınırlar, sürüm, bozuk girdi (T-IPC-003 fuzz).

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_ipc::{
    Event, MAX_MESSAGE, Message, PROTOCOL, Request, Response, WireError, decode, decode_len, encode,
};
use proptest::prelude::*;
use serde_json::json;

fn body(frame: &[u8]) -> &[u8] {
    let len = decode_len(frame[..4].try_into().unwrap()).unwrap();
    assert_eq!(len, frame.len() - 4);
    &frame[4..]
}

#[test]
fn roundtrip_each_kind() {
    let msgs = [
        Message::Request(Request {
            v: PROTOCOL,
            id: "1".into(),
            method: "svc.ping".into(),
            params: json!({}),
        }),
        Message::Response(Response::ok("1", json!({"pong": true}))),
        Message::Response(Response::err(
            "2",
            &AdnsError::new(AdnsErrorCode::IpcBadRequest),
        )),
        Message::Event(Event {
            v: PROTOCOL,
            event: "config.reloaded".into(),
            data: json!({}),
        }),
    ];
    for m in msgs {
        let frame = encode(&m).unwrap();
        assert_eq!(decode(body(&frame)).unwrap(), m);
    }
}

#[test]
fn oversized_length_is_rejected() {
    let too_big = u32::try_from(MAX_MESSAGE + 1).unwrap().to_le_bytes();
    assert_eq!(
        decode_len(too_big).unwrap_err().code(),
        AdnsErrorCode::IpcBadRequest
    );
    assert_eq!(
        decode_len([0; 4]).unwrap_err().code(),
        AdnsErrorCode::IpcBadRequest
    );
    let m = Message::Event(Event {
        v: PROTOCOL,
        event: "x".into(),
        data: json!("a".repeat(MAX_MESSAGE)),
    });
    assert_eq!(encode(&m).unwrap_err().code(), AdnsErrorCode::IpcBadRequest);
}

#[test]
fn wrong_protocol_version() {
    let e = decode(br#"{"v":2,"id":"1","method":"svc.ping"}"#).unwrap_err();
    assert_eq!(e.code(), AdnsErrorCode::IpcVersionMismatch);
}

#[test]
fn malformed_messages_are_bad_requests() {
    for raw in [
        &b"{"[..],
        br#"[1,2]"#,
        br#"{"v":1}"#,
        br#"{"v":1,"id":"../x","method":"svc.ping"}"#,
        br#"{"v":1,"id":"","method":"svc.ping"}"#,
        &[0xff, 0xfe, 0x00],
    ] {
        let e = decode(raw).unwrap_err();
        assert_eq!(e.code(), AdnsErrorCode::IpcBadRequest, "{raw:?}");
    }
}

#[test]
fn unknown_fields_are_ignored() {
    let m = decode(br#"{"v":1,"id":"7","method":"svc.ping","params":{},"yeni":42}"#).unwrap();
    assert!(matches!(m, Message::Request(r) if r.method == "svc.ping"));
}

#[test]
fn wire_error_roundtrip_keeps_code_and_params() {
    let e = AdnsError::new(AdnsErrorCode::IpcUnauthorized)
        .with("method", "svc.reload_config")
        .with_source(std::io::Error::other(r"D:\veri\gizli\yol"));
    let w = WireError::from_error(&e);
    assert_eq!(w.code, "ADNS-IPC-003");
    // Teknik kaynak (yollar içerebilir) kabloya çıkmaz.
    assert!(!serde_json::to_string(&w).unwrap().contains("gizli"));
    let back = w.into_error();
    assert_eq!(back.code(), AdnsErrorCode::IpcUnauthorized);
    assert_eq!(back.params()["method"], "svc.reload_config");
    let unknown = WireError {
        code: "ADNS-YOK-999".into(),
        params: Default::default(),
    };
    assert_eq!(unknown.into_error().code(), AdnsErrorCode::GenUnexpected);
}

proptest! {
    #[test]
    fn prop_decode_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = decode(&bytes);
        if bytes.len() >= 4 {
            let _ = decode_len(bytes[..4].try_into().unwrap());
        }
    }

    #[test]
    fn prop_request_roundtrip(id in "[A-Za-z0-9-]{1,64}", method in "[a-z.]{1,32}") {
        let m = Message::Request(Request { v: PROTOCOL, id, method, params: json!({"a": 1}) });
        let frame = encode(&m).unwrap();
        prop_assert_eq!(decode(&frame[4..]).unwrap(), m);
    }
}
