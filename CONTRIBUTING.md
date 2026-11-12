# Katkı ve geliştirme kuralları — AlterDNS

Bu kurallar bütün kod, test ve deneyler için bağlayıcıdır. Mimari ve kararlar `docs/` altındadır (`docs/ARCHITECTURE.md`, `docs/DECISIONS.md`).

## Değişmez kurallar

1. **Geliştirici makinesinin bağlantısını koparma.** AlterDNS'in kendi nesneleriyle (kendi sanal adaptörü, kendi rotaları, paket sürücüsü) yapılan her deneme şu koşullara uyar:
   - `xtask lab` düzeneğinde, zaman sınırlı çalışır.
   - Ayrı işlemde çalışan bir geri alma bekçisiyle (≤ 60 sn) korunur.
   - Kullanıcının o oturumdaki açık onayını gerektirir.

   Journal'a yazılmamış sistem değişikliği yapılmaz.
2. **Kullanıcıya ait sistem değerleri okunur, asla yazılmaz; lab dahil, geçici olarak da** (ADR-010, ADR-014).
   - Kapsam: hop limit/TTL, DNS ve DoH, proxy, MTU, rotalar ve metrikler, güvenlik duvarı ve benzerleri.
   - Program ve testler makinedeki mevcut değeri okuyup ona uyum sağlar. Farklı değerlerin etkisi kendi soketimizde (ör. `IP_TTL`) ya da başka makine, VM veya ağda ölçülür.
   - Kaldırma sırasında da kullanıcıya ait ayarlara dokunulmaz.
3. **Zararlı yazılım ana makinede çalıştırılmaz.**
   - Gerçek örnekler yalnızca ağı ve paylaşılan klasörü kapalı, izole bir VM'de kullanılır.
   - EICAR testleri `--features eicar-tests` ile VM'de çalışır. EICAR dizesi depoya düz metin olarak yazılmaz.
4. **Antivirüs ve Smart App Control kapatılmaz, istisna eklenmez, kullanıcıya bunlar önerilmez.** (SAC kapatılırsa Windows sıfırlanmadan geri açılamaz.)
5. **Makinedeki başka DPI araçlarına onaysız dokunulmaz.** Okunabilirler. Durdurmak için onay alınır ve iş bitince eski durumlarına getirilirler.
6. **Depo public'tir; gizli ve kişisel bilgi commit edilmez.**
   - Kapsam: token, imza anahtarı, API anahtarı, yerel notlar, lab sonuçları, ağ adresleri ve loglar.
   - Güncelleme istemcisine token gömülmez.
   - `cargo xtask check` bunları tarar.
7. **Bulgu uydurulmaz.** Doğrulanmamış sürüm, lisans veya API davranışı "doğrulandı" diye yazılmaz. `docs/research/THIRD_PARTY.md` durum sütunu güncel tutulur.
8. **Lisans uyumu (GPL-3.0-or-later + SignPath "kapalı bileşen yok" şartı).** Yeni bağımlılık OSI onaylı ve GPL-3.0 ile uyumlu olmalıdır.
   - İzin verilenler: MIT, Apache-2.0, BSD, 0BSD, ISC, Zlib, MPL-2.0, LGPL-3.0, GPL-3.0, Unicode-3.0.
   - `alterdns-svc` ağacında GPL-3.0-only yasaktır (`wintun.dll` ek izni; `deny-svc.toml`, ADR-003).
   - Yasaklılar: kapalı veya özgür olmayan lisanslar (UnRAR dahil), GPL-2.0-only bağlama, Slint royalty-free lisansı (Slint yalnızca GPL-3.0 seçeneğiyle).
   - Denetim `cargo deny check licenses` ile yapılır.

## Kod kuralları

- **Dil:** Rust (stable, MSVC, `x86_64-pc-windows-msvc`). Uzantı TypeScript (MV3).
- **Telif ve lisans başlığı** (CI denetler):
  ```
  // AlterDNS — Copyright (C) 2026 Furkan Okan
  // SPDX-License-Identifier: GPL-3.0-or-later
  ```
- **Exe sürüm bilgisi:** `CompanyName=Furkan Okan`, `LegalCopyright=Copyright © 2026 Furkan Okan`, `ProductName=AlterDNS`. Sürüm bütün ikililerde aynıdır. `build.rs` + `winresource` ile eklenir.
- **Hata kodları:** Kullanıcıya ulaşan her hata `ADNS-<ALAN>-<NNN>` kodlu bir `AdnsError` olur. Tek kaynak `errors/catalog.toml`'dur.
- **Panik yasakları:** Test dışı kodda `unwrap()`, `expect()` ve `panic!` kullanılmaz (clippy deny). FFI crate'leri dışında `#![forbid(unsafe_code)]` geçerlidir. Her `unsafe` bloğun üstünde `// SAFETY:` açıklaması bulunur.
- **Sistem değişiklikleri:** Hepsi journal üzerinden yapılır: önce kaydedilir, sonra uygulanır, gerekirse geri alınır.
- **Log ve telemetri:** `tracing` kullanılır. Loglar boyutu sınırlı ve kişisel verisi azaltılmıştır. Telemetri yoktur.
- **Dil desteği:** Varsayılan Türkçe, ikinci dil İngilizcedir. Kullanıcıya görünen metin koda sabit yazılmaz.
- **Bağımlılıklar:** Her yeni crate için lisans, bakım ve güvenlik geçmişi kontrol edilir. Sürüm crates.io'dan doğrulanır.
- **GitHub Actions:**
  - Action'lar tam SHA ile sabitlenir.
  - `permissions:` en az yetkiyle tanımlanır.
  - `pull_request_target` kullanılmaz.
  - Gizli değişkenler yalnızca korumalı `release` ortamında bulunur.

## Çalışma yöntemi

- **Plan ve test:** Her fazın başında ayrıntılı bir uygulama planı yazılır. Uygulama test öncelikli (TDD) yapılır.
- **Güvenlik incelemesi:** IPC, güncelleyici, native host, sandbox, journal ve yayın iş akışı değişiklikleri birleştirilmeden önce incelenir.
- **Arıza analizi:** Bağlantı motoru ve güncelleyici için yapılır.
- **Kontrol:** Push'tan önce `cargo xtask check` temiz olmalıdır.
- **Onay:** Push, sürüm ve depo ayarları proje sahibinin onayını gerektirir.
