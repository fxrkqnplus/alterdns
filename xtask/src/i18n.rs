// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Arayüz çevirisi denetimi. `crates/ui/ui/**/*.slint` içindeki `@tr("…")` anahtarları ile
//! `crates/ui/lang/<dil>/LC_MESSAGES/alterdns-ui.po` dosyaları birebir eşleşmelidir: her anahtar
//! iki dilde de çevrilmiş (boş ya da bulanık değil), fazladan anahtar yok, yer tutucu sayısı aynı.
//!
//! Derleyici varsayılan bağlamı kapatır (`DefaultTranslationContext::None`); bağlam yalnızca
//! `@tr("bağlam" => "metin")` ile verilir. Anahtar biçimi gettext'teki gibidir:
//! `bağlam\u{4}metin` ya da `metin`. Çoğul biçimlerde anahtar tekil metindir.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, bail};

pub const LANGS: [&str; 2] = ["tr", "en"];
const UI_DIR: &str = r"crates\ui\ui";
const CONTEXT_SEP: char = '\u{4}';

fn po_path(lang: &str) -> String {
    format!(r"crates\ui\lang\{lang}\LC_MESSAGES\alterdns-ui.po")
}

/// Slint dize değişmezinin sonu (`\{…}` ara değerleri içindeki iç içe dizeler dahil).
fn skip_string(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' if b.get(i + 1) == Some(&b'{') => i = skip_interpolation(b, i + 2),
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

fn skip_interpolation(b: &[u8], mut i: usize) -> usize {
    let mut depth = 1;
    while i < b.len() {
        match b[i] {
            b'"' => i = skip_string(b, i),
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    b.len()
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// `i`'deki dize değişmezini çözer: (içerik, sonraki konum).
fn string_literal(text: &str, i: usize) -> Option<(String, usize)> {
    let b = text.as_bytes();
    if b.get(i) != Some(&b'"') {
        return None;
    }
    let end = skip_string(b, i);
    let raw = text.get(i + 1..end.checked_sub(1)?)?;
    Some((unescape(raw), end))
}

fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `@tr(` sonrasını çözer: anahtar ve sonraki konum.
fn tr_call(text: &str, i: usize) -> Option<(String, usize)> {
    let b = text.as_bytes();
    let (first, next) = string_literal(text, skip_ws(b, i))?;
    let after = skip_ws(b, next);
    if b.get(after..after + 2) == Some(b"=>") {
        let (msgid, end) = string_literal(text, skip_ws(b, after + 2))?;
        return Some((format!("{first}{CONTEXT_SEP}{msgid}"), end));
    }
    Some((first, next))
}

/// `.slint` kaynaklarındaki `@tr` anahtarları (yorumlar ve dize içleri atlanır).
pub fn tr_keys(slint_sources: &[(String, String)]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for (_, text) in slint_sources {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let rest = &b[i..];
            if rest.starts_with(b"//") {
                i += rest.iter().position(|&c| c == b'\n').unwrap_or(rest.len());
            } else if rest.starts_with(b"/*") {
                i += rest
                    .windows(2)
                    .position(|w| w == b"*/")
                    .map_or(rest.len(), |p| p + 2);
            } else if rest[0] == b'"' {
                i = skip_string(b, i);
            } else if rest.starts_with(b"@tr(") {
                match tr_call(text, i + 4) {
                    Some((key, next)) => {
                        keys.insert(key);
                        i = next;
                    }
                    None => i += 4,
                }
            } else {
                i += 1;
            }
        }
    }
    keys
}

#[derive(Default)]
struct Entry {
    ctx: Option<String>,
    id: Option<String>,
    strs: BTreeMap<usize, String>,
    fuzzy: bool,
}

#[derive(Clone, Copy)]
enum Field {
    Ctx,
    Id,
    IdPlural,
    Str(usize),
}

fn po_quoted(s: &str) -> anyhow::Result<String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .with_context(|| format!("tırnaksız değer: {s}"))?;
    Ok(unescape(inner))
}

fn finish(entry: Entry, out: &mut BTreeMap<String, String>) -> anyhow::Result<()> {
    let Some(id) = entry.id else {
        return Ok(());
    };
    if id.is_empty() && entry.ctx.is_none() {
        return Ok(()); // başlık
    }
    let key = match entry.ctx {
        Some(ctx) => format!("{ctx}{CONTEXT_SEP}{id}"),
        None => id,
    };
    let complete =
        !entry.fuzzy && !entry.strs.is_empty() && entry.strs.values().all(|s| !s.is_empty());
    let value = if complete {
        entry.strs.values().next().cloned().unwrap_or_default()
    } else {
        String::new()
    };
    if out.insert(key.clone(), value).is_some() {
        bail!("yinelenen anahtar: {key:?}");
    }
    Ok(())
}

/// `.po` girdileri: anahtar → çeviri. Bulanık ya da (bir çoğul biçimi dahil) boş çeviri `""`
/// olarak döner; başlık girdisi dahil edilmez.
pub fn po_entries(po: &str) -> anyhow::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut entry = Entry::default();
    let mut field: Option<Field> = None;
    let mut fuzzy_next = false;
    for (n, line) in po.lines().enumerate() {
        let line = line.trim();
        let at = || format!("satır {}", n + 1);
        if line.is_empty() {
            finish(std::mem::take(&mut entry), &mut out).with_context(at)?;
            field = None;
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            if comment.starts_with(',') && comment.contains("fuzzy") {
                fuzzy_next = true;
            }
            continue;
        }
        if line.starts_with('"') {
            let text = po_quoted(line).with_context(at)?;
            match field {
                Some(Field::Ctx) => entry.ctx.get_or_insert_default().push_str(&text),
                Some(Field::Id) => entry.id.get_or_insert_default().push_str(&text),
                Some(Field::IdPlural) => {}
                Some(Field::Str(i)) => entry.strs.entry(i).or_default().push_str(&text),
                None => bail!("{}: bağlamsız devam satırı", at()),
            }
            continue;
        }
        let (keyword, value) = line
            .split_once(char::is_whitespace)
            .with_context(|| format!("{}: değer yok", at()))?;
        let value = po_quoted(value).with_context(at)?;
        let starts_entry = matches!(keyword, "msgctxt" | "msgid");
        // Boş satırsız ardışık girdiler: yeni msgctxt/msgid öncekini kapatır.
        if starts_entry && entry.id.is_some() {
            finish(std::mem::take(&mut entry), &mut out).with_context(at)?;
        }
        if starts_entry && fuzzy_next {
            entry.fuzzy = true;
            fuzzy_next = false;
        }
        field = Some(match keyword {
            "msgctxt" => {
                entry.ctx = Some(value);
                Field::Ctx
            }
            "msgid" => {
                entry.id = Some(value);
                Field::Id
            }
            "msgid_plural" => Field::IdPlural,
            "msgstr" => {
                entry.strs.insert(0, value);
                Field::Str(0)
            }
            k => {
                let index = k
                    .strip_prefix("msgstr[")
                    .and_then(|r| r.strip_suffix(']'))
                    .and_then(|r| r.parse::<usize>().ok())
                    .with_context(|| format!("{}: bilinmeyen anahtar sözcük {k}", at()))?;
                entry.strs.insert(index, value);
                Field::Str(index)
            }
        });
    }
    finish(entry, &mut out)?;
    Ok(out)
}

/// `{}` / `{0}` biçimindeki yer tutucuların sayısı (`{{` kaçışı sayılmaz).
fn placeholders(s: &str) -> usize {
    s.replace("{{", "").matches('{').count()
}

fn show(key: &str) -> String {
    format!("{:?}", key.replace(CONTEXT_SEP, " => "))
}

fn slint_sources(dir: &Path, out: &mut Vec<(String, String)>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| dir.display().to_string())? {
        let path = entry?.path();
        if path.is_dir() {
            slint_sources(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "slint") {
            let text =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            out.push((path.display().to_string(), text));
        }
    }
    Ok(())
}

/// Her `@tr` anahtarı iki dilde çevrilmiş, fazladan anahtar yok, yer tutucu sayıları aynı.
/// `crates/ui/ui` yoksa denetlenecek bir şey yoktur.
pub fn check(root: &Path) -> anyhow::Result<()> {
    let ui = root.join(UI_DIR);
    if !ui.is_dir() {
        return Ok(());
    }
    let mut sources = Vec::new();
    slint_sources(&ui, &mut sources)?;
    let keys = tr_keys(&sources);
    let mut problems = Vec::new();
    for lang in LANGS {
        let path = root.join(po_path(lang));
        let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        let entries = po_entries(&text).with_context(|| path.display().to_string())?;
        for key in &keys {
            match entries.get(key) {
                None => problems.push(format!("{lang}: çeviri yok: {}", show(key))),
                Some(v) if v.is_empty() => {
                    problems.push(format!("{lang}: çeviri boş ya da bulanık: {}", show(key)));
                }
                Some(v) => {
                    let id = key.rsplit(CONTEXT_SEP).next().unwrap_or(key);
                    if placeholders(v) != placeholders(id) {
                        problems.push(format!("{lang}: yer tutucu sayısı farklı: {}", show(key)));
                    }
                }
            }
        }
        for key in entries.keys().filter(|k| !keys.contains(*k)) {
            problems.push(format!("{lang}: kullanılmayan anahtar: {}", show(key)));
        }
    }
    if !problems.is_empty() {
        bail!("çeviriler eksik ya da fazla:\n  {}", problems.join("\n  "));
    }
    Ok(())
}
