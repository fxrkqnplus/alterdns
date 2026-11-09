# Üçüncü Taraf Bileşenler, Veri Kaynakları ve Lisanslar

**Durum sütunu:**
- ✅ kaynaktan doğrulandı (tarih ve bağlantı var)
- 🔶 kısmen doğrulandı veya ikincil kaynaktan öğrenildi
- ⏳ doğrulanacak

Kullanmadan önce her ⏳ satır resmi kaynaktan kontrol edilir ve güncellenir. Lisans metinleri dağıtımdaki `THIRD_PARTY_NOTICES` dosyasına girer (`cargo-about` + el ile eklenenler).

Son toplu kontrol: **2026-11-09**.

## 0. Lisans politikası (D2: GPL-3.0-or-later + SignPath şartları)

AlterDNS **GPL-3.0-or-later** lisanslıdır. SignPath Foundation şunları şart koşar:
- Bütün bileşenler için OSI onaylı lisans
- Ticari ikili lisanslama yok
- Kapalı veya özgür olmayan bileşen yok (sistem kütüphaneleri hariç)

Bu nedenle her bağımlılık şu kurallardan geçer:

| Durum | Lisanslar |
|---|---|
| ✅ Kabul | MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0, LGPL-2.1+/3.0, GPL-3.0(-or-later), CC0 (veri için) |
| ⚠️ Dikkat | GPL-2.0-only bağlanamaz; yalnızca ayrı işlem olarak toplu dağıtılabilir (ClamAV gibi). LGPL kütüphaneler dinamik bağlanır ve değiştirilebilirlik korunur |
| ❌ Ret | Kapalı veya özgür olmayan lisanslar (UnRAR lisansı, Slint Royalty-free 2.0, Elastic License 2.0, "ticari olmayan kullanım" lisanslı kod), lisanssız kod |

`deny.toml` bu politikayı uygular. Veri beslemeleri (itibar listeleri) "bileşen" değil, hizmet olarak çalışma zamanında indirilir; yine de lisansları kayıt altına alınır.

## 1. Yazılım bileşenleri

| Bileşen | Kullanım | Lisans | Kaynak | Sürüm (kontrol tarihi) | Durum | Yükümlülükler / notlar |
|---|---|---|---|---|---|---|
| WinDivert | Paket yakalama ve enjeksiyon sürücüsü + DLL | LGPL-3.0 **veya** GPL-2.0 (ikisi de "veya sonrası"); **LGPL-3.0 seçilir** | reqrypt.org/windivert.html, github.com/basil00/WinDivert | 2.2.2 "Latest" (2026-11-09). 2.2.0 varyantları yalnızca sürücü imzasıyla farklı | 🔶 lisans ✅, sürüm ✅, arşiv hash'i ⏳ | DLL dinamik bağlanır; kaynak ve lisans dağıtıma eklenir. Resmi sürücü yazarı tarafından imzalıdır; biz imzalamayız (SignPath: üçüncü taraf ikili pakette bulunabilir). ARM64 resmi belgede yok |
| GoodbyeDPI | **Yalnız lab karşılaştırması**; teknik referans | Apache-2.0 (GPL-3.0 ile uyumlu) | github.com/ValdikSS/GoodbyeDPI | 0.2.3rc3 (son sürüm görünüyor; 2026-01-19 commit'i var) | ✅ lisans; ⏳ yeni sürüm | Ürünle dağıtılmaz. Kod uyarlanırsa NOTICE eklenir |
| zapret / zapret2 (winws) | **Yalnız lab karşılaştırması**; UDP/QUIC referansı | `docs/LICENSE.txt` (README'de ad yok) | github.com/bol-van/zapret, …/zapret-win-bundle | zapret1 EOL; zapret2 güncel | ⏳ lisans | Lisans doğrulanıp GPL-3.0 uyumu görülmeden koddan uyarlama yapılmaz. Bundle cygwin içerir |
| YARA-X (`yara-x`) | Kural eşleme | BSD-3-Clause | github.com/VirusTotal/yara-x | 1.x; PyPI 1.22.1 (2026-10-26). Crate sürümü ⏳ | 🔶 | — |
| Slint | UI çatısı (aday) | GPL-3.0-only **veya** Royalty-free 2.0 **veya** ücretli | slint.dev | 1.x (sürüm ⏳) | ✅ seçenekler | **GPL-3.0 seçeneği kullanılır.** Royalty-free kapalı bir lisans olduğu için SignPath şartıyla çelişir. Birleşik ikili GPL-3.0 koşullarına tabidir |
| ClamAV (isteğe bağlı) | İmza tabanlı tarama | GPL-2.0; UnRAR modülü özgür değil | github.com/Cisco-Talos/clamav | 1.5.x (1.5.4, 2026-08-07, ikincil kaynak) | 🔶 | Ayrı işlemde çalışır. **UnRAR'sız** derlenir veya dağıtılır (`ENABLE_UNRAR=OFF`, doğrulanacak). Kaynak teklifi gerekir. freshclam ≤ saatte 1. DB ≈ 80 + 22 MB |
| libarchive (RAR4/RAR5 okuma için aday) | RAR ve diğer arşivler | BSD-2-Clause | libarchive.org | ⏳ RAR5 okuma desteği ve sürüm | ⏳ | C kütüphanesi; yalnızca sandbox işçisinde çalışır. Rust bağlantısı (`compress-tools` vb.) lisansı ⏳ |
| UnRAR | — | Özgür olmayan lisans | rarlab.com | — | ❌ | **Kullanılmaz** (SignPath ve GPL politikası) |
| `windows` / `windows-sys` | Win32/WinRT | MIT veya Apache-2.0 | microsoft/windows-rs | ⏳ | ⏳ | — |
| `windivert` / `windivert-sys` crate | WinDivert bağları (aday) | ⏳ | crates.io | ⏳ | ⏳ | Alternatif: kendi ince FFI katmanı (`winsys`) |
| Altyapı crate'leri (`rustls`, `tokio`, `serde`, `tracing`, `ed25519-dalek` …) | — | Çoğunlukla MIT/Apache-2.0 | crates.io | ⏳ | ⏳ | `cargo deny` |
| Arşiv ve ayrıştırıcı crate'leri (`zip`, `sevenz-rust2`, `tar`, `flate2`, `cab`, `cfb`, `goblin`/`pelite`/`object`, `lopdf` …) | Açıcılar ve analizörler | ⏳ | crates.io | ⏳ | ⏳ | Bakım ve güvenlik geçmişi kontrol edilir; sandbox'ta çalışır |
| Uzantı bağımlılıkları (npm) | Derleme ve test | ⏳ | npm | ⏳ | ⏳ | Çalışma zamanı bağımlılığı en aza indirilir; `npm ci` + lockfile; lisans denetimi |

## 2. İtibar ve tehdit veri kaynakları

| Kaynak | Veri | Lisans / kullanım koşulu | Erişim | Durum | Not |
|---|---|---|---|---|---|
| USOM (TR-CERT) | Oltalama, dolandırıcılık ve zararlı URL/alan adları | ⏳ yeniden kullanım koşulu | `https://www.usom.gov.tr/url-list.txt` | 🔶 erişim; ⏳ koşullar | Türkiye için en ilgili kaynak |
| abuse.ch URLhaus | Zararlı yazılım URL'leri | Ücretsiz ve adil kullanım; ticari kullanımda abonelik gerekebilir. Koşullar 2026-10-21'de güncellendi | API Auth-Key zorunlu; döküm koşulu ⏳ | ✅ koşullar | D4 |
| abuse.ch MalwareBazaar / ThreatFox | Hash ve IOC | Aynı | Auth-Key | ✅ / 🔶 | Yalnız hash sorgusu |
| Google Safe Browsing v5 | Tehdit listeleri | **Yalnız ticari olmayan kullanım**; ticari kullanım için Web Risk | API anahtarı; yerel liste veya hash-prefix modu | ✅ koşullar | D4 |
| VirusTotal (kullanıcı anahtarı) | Hash itibarı | Public API ticari olmayan kullanım | Kullanıcı anahtarı | ⏳ | Yükleme varsayılan kapalı |
| OpenPhish / PhishTank / Phishing.Database / HaGeZi TIF | Alan adları | ⏳ | ⏳ | ⏳ | Lisansı uygun olanlar |
| YARA kural setleri | Kurallar | Sete göre değişir | GitHub | ⏳ | **Yalnız açık lisanslılar** (MIT/BSD/Apache/GPL/CC0). DRL 1.1 ve Elastic License 2.0 kullanılmaz |
| CIRCL hashlookup | Bilinen-iyi hash | ⏳ | API | ⏳ | Yanlış pozitif azaltma |
| Microsoft Defender | Yerel AV | Windows bileşeni | `MpCmdRun.exe`, AMSI | ⏳ çıkış kodları | AlterDNS AV olarak kaydolmaz |

## 3. Platform, dağıtım ve imzalama

| Konu | Bulgu | Durum |
|---|---|---|
| Chrome uzantı dağıtımı (Windows) | Kararlı ve beta kanalda yalnız CWS'deki uzantılar kurulabilir; istisnalar kurumsal ilke ve **geliştirici modu**. AlterDNS geliştirici modunu kullanır (ADR-011) | ✅ developer.chrome.com |
| `--load-extension` | Google Chrome markalı sürüm bu bayrağı reddediyor ("not allowed in Google Chrome") | 🔶 (2026 raporu, macOS test). Windows ve Edge için ⏳ |
| `chrome://extensions` adresini dışarıdan açma | `chrome.exe chrome://extensions` ile sayfanın açılmadığı bildirilmiş | 🔶 (eski SO sorusu). Güncel davranış ⏳; açılmıyorsa panoya kopyalama + yönerge |
| Geliştirici modu davranışı | Eski başlangıç uyarısı kaldırılmış bildiriliyor; ancak paketlenmemiş uzantılar güncelleme veya profil yeniden yüklemesinden sonra devre dışı kalabiliyor. Edge'de uyarı penceresi bildirildi | 🔶 (ikincil kaynaklar). Faz 4'te ölçülecek |
| Sabit uzantı kimliği | Manifest `key` alanı paketlenmemiş uzantıda kimliği sabitler | 🔶 (Chrome belgesi/skill). Faz 4'te test edilecek |
| Firefox | Kalıcı kurulum için imza gerekir; AMO "unlisted" imzalama ücretsiz | ⏳ |
| MV3 DNR sınırları | Statik 30.000 + 300.000 paylaşımlı; dinamik 30.000; oturum 5.000 | 🔶 |
| Native messaging | Host exe'si tarayıcının başlatabileceği konumda olmalı. Paket yalıtımı (MSIX) engel olabilir | 🔶 |
| **SignPath Foundation** | Ücretsiz OSS Authenticode. Şartlar: OSI lisansı, ikili lisans yok, kapalı bileşen yok; aktif bakım ve yayımlanmış sürüm; MFA; roller (Yazar/İnceleyen/Onaylayan); doğrulanabilir otomatik derleme; her sürüm için elle onay; ürün adı = proje adı; "Code signing policy" sayfası ve zorunlu ifade; gizlilik politikası; sistem değişikliği uyarıları; kaldırma desteği; "hacking tool" ve PUP yasağı. Sertifika SignPath Foundation adına; **yayıncı SignPath Foundation** | ✅ signpath.org/terms (2026-11-09) |
| Microsoft'un imzalama seçenekleri sayfası | Açık kaynak için SignPath Foundation'ı öneriyor. Store'da MSIX imzası ücretsiz ama bu mimariye uymuyor (sürücü + servis) | ✅ learn.microsoft.com |
| GitHub Artifact Attestations | Public depolarda bütün planlarda ücretsiz; Sigstore Public Good Instance; `gh attestation verify` ile doğrulama | ✅ docs.github.com |
| Azure Artifact Signing | Bireysel public trust ABD/Kanada ile sınırlı; ücretli | 🔶 (kullanılmayacak) |
| İmzasız dağıtım | SmartScreen uyarısı; Smart App Control açık sistemlerde engel. SAC kapatıldıktan sonra Windows sıfırlanmadan geri açılamaz; bu yüzden kapatılması önerilmez | 🔶 |
| GitHub Releases indirme | `releases/latest/download/<varlık>` yolu API kullanmaz | ⏳ hız sınırı davranışı |
| Windows Sandbox | Pro/Enterprise/Education; `wsb` CLI 24H2+; ağ kapatma ve salt okunur eşleme | ✅ |

## 4. Geliştirici makinesindeki paketler

Geliştirici makinesindeki eski DPI paketi 2026-11-09'da kaldırıldı; resmi arşivle karşılaştırma yapılmadı. Tarihsel hash kaydı kişisel olduğu için `CLAUDE.local.md` dosyasındadır (git dışı).
