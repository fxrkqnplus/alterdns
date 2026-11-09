# CLAUDE.md — AlterDNS

Bu dosya Claude Code'un her oturumda okuduğu proje belleğidir; kısa tutulur. Ayrıntılar `docs/` altındadır.

- Başlangıç görevi ve tam gereksinimler: **`docs/CLAUDE_CODE_PROMPT.md`**
- Geliştirici makinesine özgü ortam bilgisi, yerel kanıtlar ve kişisel kurallar: **`CLAUDE.local.md`** (git dışı; otomatik yüklenir)

## Proje

AlterDNS, Windows için modüler bir masaüstü güvenlik ve bağlantı uygulamasıdır. Üç modülü vardır:

1. **Dosya güvenliği:** İndirilen dosyaları ve gömülü içeriklerini çalıştırmadan, sandbox içinde inceler. Sonuç `Güvenli` / `Şüpheli` / `Riskli` olarak verilir; incelenemeyen kısımlar ayrıca belirtilir. Karantina ve silme işlemleri doğrulanır.
2. **Tarayıcı koruması:** MV3 uzantısı ve native messaging host'tan oluşur. Riskli adreslere girilmeden önce AlterDNS engelleme sayfası gösterilir. Uzantı mağazada yayımlanmaz; uygulama klasöründen **geliştirici moduyla** yüklenir (ADR-011).
3. **Bağlantı yönetimi:** Yerel DPI atlatma motorudur; uzak bir VPN veya proxy sunucusu kullanmaz, trafik bu bilgisayarda işlenir. Motor teknolojisi **D8** ile yeniden değerlendiriliyor (ADR-003): aday Wintun TUN + kullanıcı modu motor, yedek yerel aktarıcı (PAC), WinDivert askıda (Defender tespiti). Ağa göre ayar yapar, zaman sınırlı denemeler yapar ve başarısız değişiklikleri geri alır.

Yönetim, sistem tepsisi simgesinden açılan flyout ile yapılır. Ana pencere ayrı bir işlemde çalışır. Kurulum modülerdir.

- **Yapımcı ve telif sahibi:** Furkan Okan.
- **Lisans:** **GPL-3.0-or-later** (`LICENSE`). Ad ve logo bu lisansın kapsamı dışındadır (`TRADEMARK.md`).
- **Depo:** `https://github.com/fxrkqnplus/alterdns`, **public**. Güncellemeler bu deponun GitHub Releases sayfasından alınır.
- **İmzalama:** Yalnızca ücretsiz yollar kullanılır. Hedef SignPath Foundation (OSS) Authenticode imzası; buna ek olarak GitHub Artifact Attestations ve Ed25519 imzalı manifest (ADR-012).
- **Push, release ve depo ayarları** kullanıcı onayı ister.

## Belge dizini

| Belge | İçerik |
|---|---|
| `docs/CLAUDE_CODE_PROMPT.md` | Ana görev tanımı, kabul ölçütleri, çalışma yöntemi |
| `docs/ARCHITECTURE.md` | İşlem modeli, crate'ler, IPC, veri yolları, modüller, yayın hattı |
| `docs/ROADMAP.md` | Fazlar, bağımlılıklar, testler, çıkış ölçütleri |
| `docs/TESTING.md` | Test stratejisi, performans bütçeleri, ölçüm yöntemleri |
| `docs/ERROR_CATALOG.md` | Hata kodu şeması ve başlangıç kataloğu |
| `docs/SECURITY.md` | Tehdit modeli, sandbox, tedarik zinciri, imzalama |
| `docs/UI_SPEC.md` | Tepsi, flyout, ana pencere, uzantı kurulum sihirbazı, engelleme sayfası |
| `docs/DECISIONS.md` | ADR'ler ve kullanıcı kararları |
| `docs/research/CONNECTIVITY_RESEARCH.md` | TTL 65 ve DPI uyumluluk araştırma planı, hipotezler |
| `docs/research/THIRD_PARTY.md` | Üçüncü taraf bileşenler, lisans uyumu, doğrulama durumu |
| `docs/claude-settings.template.json` | `.claude/settings.json` izin kuralları şablonu (Faz 0'da onayla kopyalanır) |

## Değişmez kurallar

1. **Geliştirici makinesinin bağlantısını koparma.** Ağ ayarı, DNS, rota, sanal adaptör, paket sürücüsü veya hop limit üzerindeki her deneme şu koşullara uyar:
   - `xtask lab` düzeneğinde, zaman sınırlı çalışır.
   - Ayrı işlemde çalışan bir geri alma bekçisiyle (≤ 60 sn) korunur.
   - Kullanıcının o oturumdaki açık onayını gerektirir.

   Journal'a yazılmamış sistem değişikliği yapılmaz. Ayrıntı: `CLAUDE.local.md`.
2. **Hop limit/TTL ayarı okunur, asla yazılmaz** (ADR-010). Kaldırma sırasında da kullanıcıya ait ayarlara dokunulmaz.
3. **Zararlı yazılımı ana makinede çalıştırma.** Gerçek örnekler yalnızca ağı ve paylaşılan klasörü kapalı izole bir VM'de kullanılır. EICAR testleri `--features eicar-tests` ile VM'de çalışır. EICAR dizesi depoya düz metin olarak yazılmaz.
4. **Antivirüsü ve Smart App Control'ü kapatma**, istisna ekleme ve kullanıcıya bunları önerme. (SAC kapatılırsa Windows sıfırlanmadan geri açılamaz.)
5. **Makinedeki başka DPI araçlarına onaysız dokunma.** Okuyabilirsin; durdurmak için onay iste ve iş bitince eski durumuna getir.
6. **Depo public; gizli ve kişisel bilgi commit edilmez.** Kapsam: token, imza anahtarı, API anahtarı, `CLAUDE.local.md` içeriği, lab sonuçları, ağ adresleri ve loglar. Güncelleme istemcisine token gömülmez.
7. **Bulgu uydurma.** Doğrulamadığın sürüm, lisans veya API davranışını "doğrulandı" diye yazma; `THIRD_PARTY.md` durum sütununu güncel tut.
8. **Lisans uyumu (GPL-3.0-or-later + SignPath "kapalı bileşen yok" şartı).** Yeni bağımlılık OSI onaylı ve GPL-3.0 ile uyumlu olmalı.
   - İzin verilenler: MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, LGPL-3.0, GPL-3.0, Unicode-3.0.
   - Yasaklılar: kapalı veya özgür olmayan lisanslar (UnRAR dahil), GPL-2.0-only bağlama, Slint royalty-free lisansı (Slint yalnızca GPL-3.0 seçeneğiyle).
   - Denetim `cargo deny check licenses` ile yapılır.

## Kod kuralları

- **Dil:** Rust (stable, MSVC, x86_64-pc-windows-msvc). Uzantı TypeScript (MV3).
- **Telif ve lisans başlığı** (CI denetler):
  ```
  // AlterDNS — Copyright (C) 2026 Furkan Okan
  // SPDX-License-Identifier: GPL-3.0-or-later
  ```
- **Exe sürüm bilgisi:** `CompanyName=Furkan Okan`, `LegalCopyright=Copyright © 2026 Furkan Okan`, `ProductName=AlterDNS` (SignPath ürün adı kuralı), sürüm bütün ikililerde aynı. `build.rs` + `winresource` ile eklenir.
- **Hata kodları:** Kullanıcıya ulaşan her hata `ADNS-<ALAN>-<NNN>` kodlu bir `AdnsError` olur. Tek kaynak `errors/catalog.toml`; Rust kodu build.rs ile üretilir.
- **Panik yasakları:** Test dışı kodda `unwrap()`, `expect()` ve `panic!` kullanılmaz (clippy deny). FFI crate'leri dışında `#![forbid(unsafe_code)]`; her `unsafe` bloğun üstünde `// SAFETY:` açıklaması bulunur.
- **Sistem değişiklikleri:** Hepsi `alterdns-journal` üzerinden yapılır: önce kaydedilir, sonra uygulanır, gerekirse geri alınır.
- **Log ve telemetri:** `tracing`; boyutu sınırlı, kişisel verisi azaltılmış loglar. Telemetri yok.
- **Dil desteği:** Varsayılan Türkçe, ikinci dil İngilizce; kullanıcıya görünen metin koda sabit yazılmaz.
- **Bağımlılıklar:** Her yeni crate için lisans, bakım ve güvenlik geçmişi kontrol edilir. Sürüm seçimini Context7 veya crates.io'dan yap; hafızadan sürüm yazma.
- **GitHub Actions:** Action'lar tam SHA ile sabitlenir. `permissions:` en az yetkiyle tanımlanır. `pull_request_target` kullanılmaz. Gizli değişkenler yalnızca korumalı `release` ortamında bulunur.

## Komutlar

Faz 1'de oluşturulacak:

```
cargo xtask check         # fmt + clippy -D warnings + test + deny + başlık/gizli bilgi denetimi
cargo xtask test          # birim + entegrasyon (sistem değiştirmeyenler)
cargo xtask perf          # performans bütçesi ölçümü
cargo xtask dev-install   # geliştirme kurulumu (yönetici, journal'lı)
cargo xtask dev-uninstall # kalıntı denetimiyle kaldırma
cargo xtask lab <senaryo> # bağlantı araştırma düzeneği (onay + bekçi + zaman sınırı)
cargo xtask package       # bileşen paketleri + manifest (yerel deneme)
cd extension && npm ci && npm run build && npm test
```

## Çalışma yöntemi

- **Faz planı:** Her fazın başında `superpowers:writing-plans` ile `docs/superpowers/plans/` altına yazılır. Uygulama TDD ile yapılır. İş bitmeden `verification-before-completion` çalıştırılır.
- **Güvenlik incelemesi:** IPC, güncelleyici, native host, sandbox, journal ve release workflow değişiklikleri `backend-design:security-reviewer` ile incelenir.
- **Arıza analizi:** Bağlantı motoru ve güncelleyici için `incident-thinker` kullanılır.
- **Uzantı:** `modern-web-guidance:chrome-extensions` kullanılır. Mağaza belgesi yerine `extension/INSTALL.md` ve `PRIVACY.md` güncel tutulur.
- **Arayüz ve metinler:** `vectorlab-ux-skills`, `design-skills:ux-writing` ve `accessibility-audit` kullanılır.
- **Faz sonu raporu:** Yapılanlar, testler (komut + sonuç), ölçümler, açık riskler ve beklenen kararlar.

## Mevcut durum

- [ ] Faz 0 — Ortam doğrulama, depo güvenlik ayarları, spike'lar, bağlantı araştırması
- [ ] Faz 1 — Çekirdek altyapı
- [ ] Faz 2 — Bağlantı yönetimi
- [ ] Faz 3 — Dosya güvenliği
- [ ] Faz 4 — Tarayıcı koruması
- [ ] Faz 5 — Kurulum, bileşenler, güncelleme, kaldırma, yayın hattı (ilk imzasız ön sürüm)
- [ ] Faz 6 — Sağlamlaştırma, uyumluluk matrisi, SignPath başvurusu
- [ ] Faz 7 — İmzalı v1.0 yayını

Bir faz bitince bu listeyi ve `docs/ROADMAP.md` dosyasını güncelle.
