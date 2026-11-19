// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Log metninden kişisel veri ayıklama (`docs/ARCHITECTURE.md` §10): kullanıcı profil yolundaki
//! kullanıcı adı maskelenir.

/// Kişisel sayılmayan profil dizinleri (küçük harf).
const SHARED_PROFILES: [&str; 4] = ["public", "default", "default user", "all users"];
const MASK: &str = "<kullanıcı>";

fn is_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

/// `…\Users\<ad>\…` biçimindeki `<ad>`'ı `<kullanıcı>` yapar. Tek geçişlidir ve şunları da
/// kapsar:
/// - Büyük/küçük harf farkı (`\USERS\`).
/// - Kaçışlı ayraçlar (çift ters eğik çizgi; Rust `{:?}` ve JSON çıktısı).
/// - `/` ayraçlı yollar.
///
/// Ad Türkçe karakter ve boşluk içerebilir; ayraç, tırnak ya da metin sonuna kadar sürer.
pub fn redact_user_paths(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i + 6 < bytes.len() {
        let is_marker = is_sep(bytes[i]) && bytes[i + 1..i + 6].eq_ignore_ascii_case(b"users");
        if !is_marker || !is_sep(bytes[i + 6]) {
            i += 1;
            continue;
        }
        // Ayraçları atla (kaçışlı `\\` dahil); ad, sonraki ayraç ya da tırnağa kadar sürer.
        let mut start = i + 6;
        while start < bytes.len() && is_sep(bytes[start]) {
            start += 1;
        }
        let end = bytes[start..]
            .iter()
            .position(|&b| is_sep(b) || b == b'"' || b == b'\'')
            .map_or(bytes.len(), |p| start + p);
        let name = &text[start..end];
        out.push_str(&text[copied..start]);
        if name.is_empty() || SHARED_PROFILES.contains(&name.to_lowercase().as_str()) {
            out.push_str(name);
        } else {
            out.push_str(MASK);
        }
        copied = end;
        i = end;
    }
    out.push_str(&text[copied..]);
    out
}
