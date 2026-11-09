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
| WinDivert | Paket yakalama ve enjeksiyon sürücüsü + DLL | LGPL-3.0 **veya** GPL-2.0 (ikisi de "veya sonrası"); **LGPL-3.0 seçilir** | reqrypt.org/windivert.html, github.com/basil00/WinDivert | 2.2.2 (sürüm 2022-09-21; hâlâ en son, 2026-11-09). Varlık `WinDivert-2.2.2-A.zip`, 405.137 bayt. 2.2.0 varyantları yalnızca sürücü imzasıyla farklı | ✅ lisans, sürüm. 🔶 arşiv hash'i: GitHub API bu eski varlık için `digest` vermiyor; beş bağımsız paket bildirimi (Scoop kovaları, CI betikleri) aynı SHA-256'yı kaydediyor: `63cb41763bb4b20f600b6de04e991a9c2be73279e317d4d82f237b150c5f3f15`. İndirmede doğrulanacak; sürücü imzası ⏳ | DLL dinamik bağlanır; kaynak ve lisans dağıtıma eklenir. Resmi sürücü yazarı tarafından imzalıdır; biz imzalamayız (SignPath: üçüncü taraf ikili pakette bulunabilir). ARM64 resmi belgede yok |
| GoodbyeDPI | **Yalnız lab karşılaştırması**; teknik referans | Apache-2.0 (GPL-3.0 ile uyumlu) | github.com/ValdikSS/GoodbyeDPI | 0.2.3rc3 (2024-09-14); varlık `goodbyedpi-0.2.3rc3-2.zip`, 1.084.060 bayt. Daha yeni sürüm yok; son commit 2026-01-19 (kontrol 2026-11-09) | ✅ lisans, sürüm. 🔶 hash: API `digest` yok; tek bağımsız kaynak yalnız MD5 veriyor (`55d4ce5319b22fd3e034d0eaad96c770`). İndirmede SHA-256 ilk kullanımda sabitlenir | Ürünle dağıtılmaz. Kod uyarlanırsa NOTICE eklenir |
| zapret / zapret2 (winws) | **Yalnız lab karşılaştırması**; UDP/QUIC referansı | **MIT** (`docs/LICENSE.txt`: zapret "2016-2024 bol-van", zapret2 "2016-2026 bol-van"; GitHub lisansı algılamıyor çünkü dosya `docs/` altında) | github.com/bol-van/zapret, github.com/bol-van/zapret2, github.com/bol-van/zapret-win-bundle | zapret1 EOL (README: yalnız hata düzeltmesi); zapret2 etkin (son push 2026-11-09). zapret-win-bundle'da sürüm yok; `master` `bb6a20a` (2026-11-05) | ✅ lisans (2026-11-09) | MIT, GPL-3.0 ile uyumlu; uyarlanırsa telif bildirimi korunur. Bundle cygwin ve başka bileşenler içerir; yalnız lab'da, dağıtılmaz. README ilk bölümü dolandırıcılara karşı uyarı: zapret özgür ve açık kaynaktır; yalnız belirli bir kaynaktan indirmeye zorlayan veya bağlantı/video kaldırtmaya çalışanlara itibar edilmemeli (özet) |
| YARA-X (`yara-x`) | Kural eşleme | BSD-3-Clause | github.com/VirusTotal/yara-x | Crate 1.23.0, rust-version 1.95 (crates.io, 2026-11-09) | ✅ | — |
| Slint | UI çatısı (aday) | Crate ifadesi: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | slint.dev, github.com/slint-ui/slint `LICENSE.md` | 1.18.1 (2026-09-21), rust-version 1.92 | ✅ seçenekler ve koşullar (2026-11-09) | **GPL-3.0-only seçeneği kullanılır.** Slint kullanımını açıklama (AboutSlint / rozet) şartı yalnızca Royalty-free lisansta; GPL seçeneğinde ek atıf şartı yok. Bazı üçüncü taraf varlık ve kodlar kendi lisanslarını korur (`cargo deny` denetler). Birleşik ikili GPL-3.0 koşullarına tabidir |
| ClamAV (isteğe bağlı) | İmza tabanlı tarama | GPL-2.0; UnRAR modülü özgür değil | github.com/Cisco-Talos/clamav | 1.5.x (1.5.4, 2026-08-07, ikincil kaynak) | 🔶 | Ayrı işlemde çalışır. **UnRAR'sız** derlenir veya dağıtılır (`ENABLE_UNRAR=OFF`, doğrulanacak). Kaynak teklifi gerekir. freshclam ≤ saatte 1. DB ≈ 80 + 22 MB |
| libarchive (RAR4/RAR5 okuma için aday) | RAR ve diğer arşivler | Büyük ölçüde BSD-2-Clause. `COPYING`: birkaç dosya 3 maddeli UC Regents (BSD-3), bir dosya kamu malı, blake2 dosyaları CC0-1.0 / OpenSSL / Apache-2.0 arasında seçimli (CC0 veya Apache seçilir); derleme betiklerinin lisansları değişken | github.com/libarchive/libarchive | RAR 5.0 okuyucu NEWS'te 2018-10-06; ilk sürümü 3.4.0 (2019-06-11). Güncel 3.8.11 (2026-10-30) | ✅ RAR5 okuma ve lisans (2026-11-09). 🔶 Windows derleme yolu Faz 3'te | C kütüphanesi; yalnızca sandbox işçisinde çalışır. Rust bağlantısı `compress-tools` 0.16.1 (MIT OR Apache-2.0); libarchive'i sistemden/vcpkg'den ister |
| UnRAR | — | Özgür olmayan lisans | rarlab.com | — | ❌ | **Kullanılmaz** (SignPath ve GPL politikası) |
| `windows` / `windows-sys` | Win32/WinRT | MIT OR Apache-2.0 | microsoft/windows-rs | `windows` 0.62.2, `windows-sys` 0.61.2 (crates.io, 2026-11-09) | ✅ | — |
| `windivert` / `windivert-sys` crate | WinDivert bağları (aday) | LGPL-3.0-or-later | crates.io | Kararlı: `windivert` 0.6.0, `windivert-sys` 0.10.0; en yeni sürümler beta (0.7.0-beta.4, 0.11.0-beta.2). Son güncelleme 2025-09/10 | ✅ lisans; 🔶 bakım (beta ağırlıklı) | Alternatif: kendi ince FFI katmanı (`winsys`); ADR-003 spike'ı DLL'i doğrudan yükler |
| TLS yığını (`rustls`, `rustls-platform-verifier`, `rustls-native-certs`) | Problar, güncelleyici, beslemeler | `rustls` ve `rustls-native-certs`: Apache-2.0 OR ISC OR MIT; `rustls-platform-verifier`: MIT OR Apache-2.0. `webpki-roots` (CDLA-Permissive-2.0) kullanılmaz | crates.io | `rustls` 0.23.46 kararlı (0.24 geliştirme sürümünde), `rustls-platform-verifier` 0.7.1, `rustls-native-certs` 0.8.5 (2026-11-09) | ✅ | Kripto sağlayıcısı: `ring` 0.17.14 (Apache-2.0 AND ISC) veya `aws-lc-rs` 1.19.0. `aws-lc-sys` 0.47.0 ifadesi OpenSSL içermiyor (ISC, Apache-2.0, MIT, BSD-3-Clause ve seçimli MIT-0); lisans engeli yok. `aws-lc-sys` Windows'ta ek derleme araçları isteyebileceği için önce `ring` denenir (Görev 8) |
| `hickory-proto` | DNS ileti kurma/ayrıştırma (lab problar, DoH) | MIT OR Apache-2.0 | crates.io | 0.26.3 (2026-11-09) | ✅ | — |
| Altyapı crate'leri (`tokio`, `serde`, `tracing`, `ed25519-dalek` …) | — | Çoğunlukla MIT/Apache-2.0 | crates.io | ⏳ | ⏳ | `cargo deny` |
| Geliştirme araçları (dağıtılmaz) | Lisans denetimi, SBOM, test | `cargo-deny`, `cargo-about`: MIT OR Apache-2.0; `cargo-cyclonedx`: Apache-2.0; `cargo-nextest`: MIT OR Apache-2.0 (crates.io) | crates.io | cargo-deny 0.20.2, cargo-about 0.9.3 (`--features cli` ile), cargo-cyclonedx 0.5.10, cargo-nextest 0.9.151 (2026-11-09) | ✅ kurulu | CI cargo-deny'ı 0.20.2'ye sabitler |
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
| YARA kural setleri | Kurallar | Sete göre (2026-11-09, GitHub lisans dosyaları): `reversinglabs/reversinglabs-yara-rules` **MIT ✅**; `eset/malware-ioc` **BSD-2-Clause ✅**; `YARAHQ/yara-forge` araç kodu GPL-3.0, paketlenen kurallar özgün lisanslarını korur **🔶** (yalnız izinli lisanslı kurallar süzülerek); `Yara-Rules/rules` GPL-2.0, son push 2024-04 **⚠️** (bakım zayıf; ayrı veri olarak bile önerilmez); `Neo23x0/signature-base` **DRL 1.1 ❌**; `elastic/protections-artifacts` **Elastic License 2.0 ❌** | GitHub | ✅ lisanslar | **Yalnız açık lisanslılar** (MIT/BSD/Apache/GPL/CC0). DRL 1.1 ve Elastic License 2.0 kullanılmaz. Kural başına lisans meta verisi Faz 3 beslemesinde denetlenir |
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
| GitHub sürüm varlığı `digest` alanı | API, 2022 ve 2024 tarihli varlıklar (WinDivert 2.2.2-A, GoodbyeDPI 0.2.3rc3) için `digest` döndürmüyor. Upstream hash'i bu yüzden bağımsız paket bildirimleriyle çapraz denetlenir | ✅ gözlem (2026-11-09) |
| CodeQL (Rust) | codeql.github.com Rust'ı (2021 ve 2024 sürümleri) destekliyor. REST belgesindeki default setup dil listesinde `rust` yok, ama dil verilmeden açılan default setup bu depoda `actions` ve `rust` dillerini seçti; ilk analiz başarılı. Depo ayarındaki `sha_pinning_required` GitHub'ın CodeQL iş akışını engellemedi | ✅ (2026-11-09) |
| Windows Sandbox | Pro/Enterprise/Education; `wsb` CLI 24H2+; ağ kapatma ve salt okunur eşleme | ✅ |

## 4. Geliştirici makinesindeki paketler

Geliştirici makinesindeki eski DPI paketi 2026-11-09'da kaldırıldı; resmi arşivle karşılaştırma yapılmadı. Tarihsel hash kaydı kişisel olduğu için `CLAUDE.local.md` dosyasındadır (git dışı).
