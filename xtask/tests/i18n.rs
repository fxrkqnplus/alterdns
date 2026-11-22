// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arayüz çevirileri: `.slint` içindeki `@tr` anahtarları ile `tr` ve `en` `.po` dosyaları
//! birebir eşleşmeli, çeviri boş olmamalı.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use xtask::i18n::{check, po_entries, tr_keys};
use xtask::testutil::temp;

fn src(name: &str, text: &str) -> (String, String) {
    (name.to_string(), text.to_string())
}

#[test]
fn tr_keys_finds_plain_context_and_plural() {
    let keys = tr_keys(&[src(
        "a.slint",
        r#"
        export component A {
            // @tr("Commented out")
            /* @tr("Block comment") */
            property <string> s: "@tr(\"Inside a string\")";
            text: @tr("Open main window");
            label: @tr( "Menu" => "Open" );
            count: @tr("{} file" | "{} files" % n);
            quote: @tr("Say \"hi\"\n");
        }
        "#,
    )]);
    let want: Vec<&str> = vec![
        "Menu\u{4}Open",
        "Open main window",
        "Say \"hi\"\n",
        "{} file",
    ];
    assert_eq!(keys.iter().map(String::as_str).collect::<Vec<_>>(), want);
}

#[test]
fn po_entries_parse_context_plural_multiline_and_fuzzy() {
    let po = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"

msgid "Open main window"
msgstr "Ana pencereyi "
"aç"

msgctxt "Menu"
msgid "Open"
msgstr "Aç"

msgid "{} file"
msgid_plural "{} files"
msgstr[0] "{} dosya"
msgstr[1] ""

#, fuzzy
msgid "Quit"
msgstr "Çık"

msgid "Say \"hi\"\n"
msgstr "\"Merhaba\" de\n"
"#;
    let e = po_entries(po).unwrap();
    assert_eq!(e["Open main window"], "Ana pencereyi aç");
    assert_eq!(e["Menu\u{4}Open"], "Aç");
    // Çoğul biçimlerinden biri boşsa çeviri eksik sayılır.
    assert_eq!(e["{} file"], "");
    // Bulanık (fuzzy) çeviri kullanılmaz.
    assert_eq!(e["Quit"], "");
    assert_eq!(e["Say \"hi\"\n"], "\"Merhaba\" de\n");
    assert!(!e.contains_key(""));
}

#[test]
fn po_entries_rejects_duplicates() {
    let po = "msgid \"A\"\nmsgstr \"1\"\n\nmsgid \"A\"\nmsgstr \"2\"\n";
    assert!(po_entries(po).is_err());
}

fn fixture(root: &Path, slint: &str, tr: &str, en: &str) {
    std::fs::create_dir_all(root.join(r"crates\ui\ui")).unwrap();
    std::fs::write(root.join(r"crates\ui\ui\a.slint"), slint).unwrap();
    for (lang, po) in [("tr", tr), ("en", en)] {
        let dir = root.join(format!(r"crates\ui\lang\{lang}\LC_MESSAGES"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alterdns-ui.po"), po).unwrap();
    }
}

const SLINT: &str = r#"export component A { text: @tr("Hello"); }"#;

#[test]
fn missing_translation_fails_check() {
    let root = temp("i18n", "missing").unwrap();
    fixture(
        &root,
        SLINT,
        "msgid \"Hello\"\nmsgstr \"\"\n",
        "msgid \"Hello\"\nmsgstr \"Hello\"\n",
    );
    let err = format!("{:#}", check(&root).unwrap_err());
    assert!(err.contains("Hello") && err.contains("tr"), "{err}");

    let root = temp("i18n", "absent").unwrap();
    fixture(&root, SLINT, "", "msgid \"Hello\"\nmsgstr \"Hello\"\n");
    assert!(check(&root).is_err());
}

#[test]
fn extra_key_fails_check() {
    let root = temp("i18n", "extra").unwrap();
    fixture(
        &root,
        SLINT,
        "msgid \"Hello\"\nmsgstr \"Merhaba\"\n\nmsgid \"Bye\"\nmsgstr \"Hoşça kal\"\n",
        "msgid \"Hello\"\nmsgstr \"Hello\"\n",
    );
    let err = format!("{:#}", check(&root).unwrap_err());
    assert!(err.contains("Bye"), "{err}");
}

#[test]
fn complete_fixture_passes_check() {
    let root = temp("i18n", "ok").unwrap();
    fixture(
        &root,
        SLINT,
        "msgid \"Hello\"\nmsgstr \"Merhaba\"\n",
        "msgid \"Hello\"\nmsgstr \"Hello\"\n",
    );
    check(&root).unwrap();
}

#[test]
fn repo_translations_are_complete() {
    check(&xtask::check::repo_root().unwrap()).unwrap();
}
