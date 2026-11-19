// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Log metninden kişisel veri ayıklama (`docs/ARCHITECTURE.md` §10): kullanıcı profil yolundaki
//! kullanıcı adı maskelenir.

/// Kişisel sayılmayan profil dizinleri (küçük harf).
const SHARED_PROFILES: [&str; 4] = ["public", "default", "default user", "all users"];
const MASK: &str = "<kullanıcı>";

/// `…\Users\<ad>\…` ve `…/Users/<ad>/…` biçimlerindeki `<ad>`'ı `<kullanıcı>` yapar. Ad Türkçe
/// karakter ve boşluk içerebilir; ayraç ya da metin sonuna kadar sürer.
pub fn redact_user_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let found = ["\\Users\\", "/Users/", "\\users\\", "/users/"]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, m.len())))
            .min_by_key(|(i, _)| *i);
        let Some((i, len)) = found else {
            out.push_str(rest);
            return out;
        };
        let start = i + len;
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(['\\', '/', '"', '\'']).unwrap_or(tail.len());
        let name = &tail[..end];
        if name.is_empty() || SHARED_PROFILES.contains(&name.to_lowercase().as_str()) {
            out.push_str(name);
        } else {
            out.push_str(MASK);
        }
        rest = &tail[end..];
    }
}
