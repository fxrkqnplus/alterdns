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
| ✅ Kabul | MIT, Apache-2.0, BSD-2/3-Clause, 0BSD (2026-11-09, smoltcp için), ISC, Zlib, Unicode-3.0, MPL-2.0, LGPL-2.1+/3.0, GPL-3.0(-or-later), CC0 (veri için) |
| 🚫 svc ağacında yasak | GPL-3.0-only (ör. Slint): `wintun.dll`'i yükleyen `alterdns-svc` işleminde bulunamaz. `deny-svc.toml` + `cargo xtask check` ile zorunlu (ADR-003) |
| ⚠️ Dikkat | GPL-2.0-only bağlanamaz; yalnızca ayrı işlem olarak toplu dağıtılabilir (ClamAV gibi). LGPL kütüphaneler dinamik bağlanır ve değiştirilebilirlik korunur |
| ❌ Ret | Kapalı veya özgür olmayan lisanslar (UnRAR lisansı, Slint Royalty-free 2.0, Elastic License 2.0, "ticari olmayan kullanım" lisanslı kod), lisanssız kod |

`deny.toml` bu politikayı uygular. Veri beslemeleri (itibar listeleri) "bileşen" değil, hizmet olarak çalışma zamanında indirilir; yine de lisansları kayıt altına alınır.

## 1. Yazılım bileşenleri

| Bileşen | Kullanım | Lisans | Kaynak | Sürüm (kontrol tarihi) | Durum | Yükümlülükler / notlar |
|---|---|---|---|---|---|---|
| WinDivert | Paket yakalama ve enjeksiyon sürücüsü + DLL | LGPL-3.0 **veya** GPL-2.0 (ikisi de "veya sonrası"); **LGPL-3.0 seçilir** | reqrypt.org/windivert.html, github.com/basil00/WinDivert | 2.2.2 (sürüm 2022-09-21; hâlâ en son, 2026-11-09). Varlık `WinDivert-2.2.2-A.zip`, 405.137 bayt. 2.2.0 varyantları yalnızca sürücü imzasıyla farklı | ✅ lisans, sürüm. 🔶 arşiv hash'i: GitHub API bu eski varlık için `digest` vermiyor; beş bağımsız paket bildirimi (Scoop kovaları, CI betikleri) aynı SHA-256'yı kaydediyor: `63cb41763bb4b20f600b6de04e991a9c2be73279e317d4d82f237b150c5f3f15`. ⚠️ **2026-11-09: Microsoft Defender indirilen arşive erişimi `Misleading:Win32/Malgent!AMTB` tespitiyle engelledi** (boyut 405.137 bayt doğru; okuma engellendiği için yerel hash ve sürücü imzası doğrulanamadı). İstisna eklenmedi, AV kapatılmadı. **Defender tespiti nedeniyle silindi** (Defender kaldırdı). **Askıda (D8):** ürün motoru olarak kullanılmıyor; ileride isteğe bağlı gelişmiş mod adayı | DLL dinamik bağlanır; kaynak ve lisans dağıtıma eklenir. Resmi sürücü yazarı tarafından imzalıdır; biz imzalamayız (SignPath: üçüncü taraf ikili pakette bulunabilir). ARM64 resmi belgede yok |
| GoodbyeDPI | **Yalnız lab karşılaştırması**; teknik referans | Apache-2.0 (GPL-3.0 ile uyumlu) | github.com/ValdikSS/GoodbyeDPI | 0.2.3rc3 (2024-09-14); varlık `goodbyedpi-0.2.3rc3-2.zip`, 1.084.060 bayt. Daha yeni sürüm yok; son commit 2026-01-19 (kontrol 2026-11-09) | ✅ lisans, sürüm. 🔶 hash: API `digest` yok; tek bağımsız kaynak yalnız MD5 veriyor (`55d4ce5319b22fd3e034d0eaad96c770`); indirilen arşivin MD5'i eşleşti, SHA-256 `37f96b32d050dadcc930a639eba68e1ccd57ed5c04a5f77dfca908f01905a4c5` ilk kullanımda sabitlendi. **Defender tespiti nedeniyle silindi** (içinde WinDivert bulunduğu için açılmadan kaldırıldı; 2026-11-09) | Ürünle dağıtılmaz. Kod uyarlanırsa NOTICE eklenir. WinDivert'e bağlı lab karşılaştırması (S2) iptal (D8) |
| zapret / zapret2 (winws) | **Yalnız lab karşılaştırması**; UDP/QUIC referansı | **MIT** (`docs/LICENSE.txt`: zapret "2016-2024 bol-van", zapret2 "2016-2026 bol-van"; GitHub lisansı algılamıyor çünkü dosya `docs/` altında) | github.com/bol-van/zapret, github.com/bol-van/zapret2, github.com/bol-van/zapret-win-bundle | zapret1 EOL (README: yalnız hata düzeltmesi); zapret2 etkin (son push 2026-11-09). zapret-win-bundle'da sürüm yok; `master` `bb6a20a` (2026-11-05) | ✅ lisans (2026-11-09) | MIT, GPL-3.0 ile uyumlu; uyarlanırsa telif bildirimi korunur. Bundle cygwin ve başka bileşenler içerir; yalnız lab'da, dağıtılmaz. README ilk bölümü dolandırıcılara karşı uyarı: zapret özgür ve açık kaynaktır; yalnız belirli bir kaynaktan indirmeye zorlayan veya bağlantı/video kaldırtmaya çalışanlara itibar edilmemeli (özet) |
| Wintun | TUN adaptörü (D8 adayı) | Kaynak: GPL-2.0 (`COPYING`; "veya sonrası" ifadesi yok, yani GPL-2.0-only). API başlığı `wintun.h`: `GPL-2.0 OR MIT` (MIT seçilir). İmzalı `wintun.dll`: ayrı **Prebuilt Binaries License** (OSI onaylı değil) | wintun.net, git.zx2c4.com/wintun (ayna: github.com/WireGuard/wintun) | 0.14.1: `wintun-0.14.1.zip`, 750.540 bayt (HEAD, Last-Modified 2021-10-17). wintun.net SHA2-256 `07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51` (2026-11-09) | 🔶 lisans metinleri okundu; uyum değerlendirmesi §1.1; Defender tepkisi ve imza Görev 7'de | Ürün tanıtımında WireGuard/Wintun adı kullanılmaz; telif bildirimleri korunur. Kendi derlememiz imza ister (D3'e aykırı); README "Wintun" adlı sürücü dağıtmayın, resmi `wintun.dll`'i dağıtın diyor |
| byedpi | **Yalnız teknik referans**: sürücüsüz soket teknikleri (split, disorder, fake, OOB, TLS kaydı bölme) | MIT | github.com/hufrea/byedpi | v0.17.3 (2025-09-22); son push 2026-11-04 | ✅ lisans (2026-11-09) | Kod uyarlanırsa MIT telif bildirimi korunur; ürünle dağıtılmaz |
| YARA-X (`yara-x`) | Kural eşleme | BSD-3-Clause | github.com/VirusTotal/yara-x | Crate 1.23.0, rust-version 1.95 (crates.io, 2026-11-09) | ✅ | — |
| Slint | UI çatısı (aday) | Crate ifadesi: `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | slint.dev, github.com/slint-ui/slint `LICENSE.md` | 1.18.1 (2026-09-21), rust-version 1.92 | ✅ seçenekler ve koşullar (2026-11-09) | **GPL-3.0-only seçeneği kullanılır.** Slint kullanımını açıklama (AboutSlint / rozet) şartı yalnızca Royalty-free lisansta; GPL seçeneğinde ek atıf şartı yok. Bazı üçüncü taraf varlık ve kodlar kendi lisanslarını korur (`cargo deny` denetler). Birleşik ikili GPL-3.0 koşullarına tabidir |
| ClamAV (isteğe bağlı) | İmza tabanlı tarama | GPL-2.0; UnRAR modülü özgür değil | github.com/Cisco-Talos/clamav | 1.5.x (1.5.4, 2026-08-07, ikincil kaynak) | 🔶 | Ayrı işlemde çalışır. **UnRAR'sız** derlenir veya dağıtılır (`ENABLE_UNRAR=OFF`, doğrulanacak). Kaynak teklifi gerekir. freshclam ≤ saatte 1. DB ≈ 80 + 22 MB |
| libarchive (RAR4/RAR5 okuma için aday) | RAR ve diğer arşivler | Büyük ölçüde BSD-2-Clause. `COPYING`: birkaç dosya 3 maddeli UC Regents (BSD-3), bir dosya kamu malı, blake2 dosyaları CC0-1.0 / OpenSSL / Apache-2.0 arasında seçimli (CC0 veya Apache seçilir); derleme betiklerinin lisansları değişken | github.com/libarchive/libarchive | RAR 5.0 okuyucu NEWS'te 2018-10-06; ilk sürümü 3.4.0 (2019-06-11). Güncel 3.8.11 (2026-10-30) | ✅ RAR5 okuma ve lisans (2026-11-09). 🔶 Windows derleme yolu Faz 3'te | C kütüphanesi; yalnızca sandbox işçisinde çalışır. Rust bağlantısı `compress-tools` 0.16.1 (MIT OR Apache-2.0); libarchive'i sistemden/vcpkg'den ister |
| UnRAR | — | Özgür olmayan lisans | rarlab.com | — | ❌ | **Kullanılmaz** (SignPath ve GPL politikası) |
| Npcap | — (D8'de elendi) | Npcap lisansı: açık kaynak değil; yeniden dağıtılamaz; ücretsiz sürüm sınırlı sayıda sisteme kurulur; dağıtım ücretli OEM lisansı ister | github.com/nmap/npcap `LICENSE` | — | ❌ (2026-11-09) | Yakalama/enjeksiyon modelidir; özgün paketi durduramaz |
| Windows Packet Filter (WinpkFilter) | — (D8'de elendi) | `ndisapi` kullanıcı modu kütüphanesi MIT; sürücü ayrı lisanslı: ücretsiz katmanda ticari yeniden dağıtım hakkı ve sürücü kaynağı yok; ikili dağıtım ve kaynak ayrı ücretli lisanslar | github.com/wiresock/ndisapi, ntkernel.com lisans sayfası | — | ❌ (2026-11-09) | Kapalı kaynak sürücü; SignPath "kapalı bileşen yok" ve D3 ile çelişir |
| `windows` / `windows-sys` | Win32/WinRT | MIT OR Apache-2.0 | microsoft/windows-rs | `windows` 0.62.2, `windows-sys` 0.61.2 (crates.io, 2026-11-09) | ✅ | — |
| `windivert` / `windivert-sys` crate | WinDivert bağları (aday) | LGPL-3.0-or-later | crates.io | Kararlı: `windivert` 0.6.0, `windivert-sys` 0.10.0; en yeni sürümler beta (0.7.0-beta.4, 0.11.0-beta.2). Son güncelleme 2025-09/10 | ✅ lisans; 🔶 bakım (beta ağırlıklı) | Alternatif: kendi ince FFI katmanı (`winsys`); ADR-003 spike'ı DLL'i doğrudan yükler |
| TLS yığını (`rustls`, `rustls-platform-verifier`, `rustls-native-certs`) | Problar, güncelleyici, beslemeler | `rustls` ve `rustls-native-certs`: Apache-2.0 OR ISC OR MIT; `rustls-platform-verifier`: MIT OR Apache-2.0. `webpki-roots` (CDLA-Permissive-2.0) kullanılmaz | crates.io | `rustls` 0.23.46 kararlı (0.24 geliştirme sürümünde), `rustls-platform-verifier` 0.7.1, `rustls-native-certs` 0.8.5 (2026-11-09) | ✅ | Kripto sağlayıcısı: `ring` 0.17.14 (Apache-2.0 AND ISC) veya `aws-lc-rs` 1.19.0. `aws-lc-sys` 0.47.0 ifadesi OpenSSL içermiyor (ISC, Apache-2.0, MIT, BSD-3-Clause ve seçimli MIT-0); lisans engeli yok. `aws-lc-sys` Windows'ta ek derleme araçları isteyebileceği için önce `ring` denenir (Görev 8) |
| `hickory-proto` | DNS ileti kurma/ayrıştırma (lab problar, DoH) | MIT OR Apache-2.0 | crates.io | 0.26.3 (2026-11-09) | ✅ | — |
| Altyapı crate'leri (`tokio`, `serde`, `tracing`, `ed25519-dalek` …) | — | Çoğunlukla MIT/Apache-2.0 | crates.io | ⏳ | ⏳ | `cargo deny` |
| Geliştirme araçları (dağıtılmaz) | Lisans denetimi, SBOM, test | `cargo-deny`, `cargo-about`: MIT OR Apache-2.0; `cargo-cyclonedx`: Apache-2.0; `cargo-nextest`: MIT OR Apache-2.0 (crates.io) | crates.io | cargo-deny 0.20.2, cargo-about 0.9.3 (`--features cli` ile), cargo-cyclonedx 0.5.10, cargo-nextest 0.9.151 (2026-11-09) | ✅ kurulu | CI cargo-deny'ı 0.20.2'ye sabitler |
| Arşiv ve ayrıştırıcı crate'leri (`zip`, `sevenz-rust2`, `tar`, `flate2`, `cab`, `cfb`, `goblin`/`pelite`/`object`, `lopdf` …) | Açıcılar ve analizörler | ⏳ | crates.io | ⏳ | ⏳ | Bakım ve güvenlik geçmişi kontrol edilir; sandbox'ta çalışır |
| Uzantı bağımlılıkları (npm) | Derleme ve test | ⏳ | npm | ⏳ | ⏳ | Çalışma zamanı bağımlılığı en aza indirilir; `npm ci` + lockfile; lisans denetimi |

### 1.1 Wintun lisans değerlendirmesi (D8)

Hukuki görüş değildir; SignPath başvurusunda ve gerekirse hukuki danışmanlıkla teyit edilir.

**Kaynaktan okunan gerçekler (2026-11-09):**
1. Depodaki kaynak kod GPLv2 ile lisanslı. README, wintun.net'teki ön derlenmiş ikililerin "daha serbest" ayrı bir lisansla verildiğini söylüyor.
2. `prebuilt-binaries-license.txt` şartları:
   - Kullanım hakkı münhasır değil ve devredilemez.
   - Tersine mühendislik, değiştirme ve türev eser yasak; yalnızca `wintun.h` arayüzü ("Permitted API") üzerinden kullanım serbest.
   - Yeniden dağıtım yalnızca DLL'i yalnızca bu API üzerinden kullanan yazılımla birlikte serbest.
   - Telif bildirimleri kaldırılamaz; WireGuard/Wintun adları tanıtımda kullanılamaz.
   - Lisans şartları WireGuard LLC tarafından değiştirilebilir (madde 8).
3. `wintun.h` başlığı `GPL-2.0 OR MIT`.

**Değerlendirme:**
- **OSI ve SignPath:** Ön derlenmiş ikili lisansı OSI onaylı değil. DLL'in kaynağı açık (GPL-2.0) olsa da dağıtılan ikili bu lisansla gelir. SignPath'in "OSI lisansı / kapalı bileşen yok" şartı açısından **risk**; başvuruda açıkça sorulmalı (D7 ile ilişkili).
- **GPL-3.0-or-later ile birleşim:** DLL, AlterDNS işlemine yüklendiğinde birleşik eser sayılabilir. Ön derlenmiş lisansın değiştirme yasağı GPL-3.0 ile uyumlu değil; GPL-2.0-only kaynak da GPL-3.0 ile birleştirilemez.
- **Kararlaştırılan önlemler (2026-11-09, kullanıcı kararı; ADR-003):**
  - (i) **Uygulandı (2026-11-09):** Telif sahibi, kendi kodu için GPL-3.0 §7 kapsamında, değiştirilmemiş ve Wintun projesinin dağıttığı imzalı `wintun.dll` ile birleştirmeye ek izin verdi. Metin `LICENSE`'ın sonunda (GPL metni değişmedi); README'de duyuruldu. Katkılar bu ek izinle kabul edilir.
  - (ii) **Uygulanır:** `wintun.dll` yalnızca `alterdns-svc` işleminde yüklenir. Bu işlemin bağımlılık ağacında üçüncü taraf GPL-3.0-only kod (ör. Slint) bulunamaz. `deny-svc.toml` ve `cargo xtask check` bunu crate düzeyinde zorunlu kılar (svc crate'i oluştuğunda; Slint açıkça yasaklı). Negatif deneme: Slint'e bağlı deneme crate'i bu yapılandırmayla `bans FAILED, licenses FAILED` verdi (2026-11-09).
  - (iii) **Yedek:** DLL'i kurulum paketine koymayıp bileşen yöneticisiyle wintun.net'ten sabit hash'le indirmek. Yalnızca SignPath, DLL'in pakette bulunmasına itiraz ederse uygulanır.
- **SignPath ön sorusu:** Faz 6'yı beklemeden SignPath Foundation'a Wintun DLL'i (ön derlenmiş ikili lisansı) ve bağlantı modülü ("hacking tool" maddesi) hakkında ön soru gönderilecek. Yanıt ADR-003, D7 ve D8'e işlenir.
- **Ad kullanımı:** Ürün tanıtımında WireGuard/Wintun adı geçmez; Hakkında ekranında ve `THIRD_PARTY_NOTICES`'te atıf yapılır.
- **Sonuç:** 🔶 Teknik aday olarak uygun; lisans ve SignPath riski açık. Alternatif bir TUN sürücüsü (ör. OpenVPN `tap-windows6`, GPL-2.0, yalnız IOCTL ile konuşulur) ayrıca değerlendirilebilir (doğrulanacak).

### 1.2 Kullanıcı modu TCP/IP yığını ve TUN crate adayları (D8)

crates.io API, 2026-11-09. "İndirme" son 90 gündür.

| Crate | Lisans | Kararlı sürüm | Son güncelleme | İndirme (90 gün) | Durum / not |
|---|---|---|---|---|---|
| `smoltcp` | 0BSD | 0.14.0 | 2026-08-17 | 2,76 milyon | ✅ Olgun. 0BSD OSI onaylı ve GPL ile uyumlu; `deny.toml` ve `deny-svc.toml` izin listelerine eklendi (2026-11-09) |
| `netstack-smoltcp` | MIT OR Apache-2.0 | 0.2.5 | 2026-11-05 | 91 bin | ✅ smoltcp üstünde TUN yığını |
| `ipstack` | Apache-2.0 | 1.0.1 | 2026-07-12 | 43 bin | ✅ tun2proxy'nin kullandığı yığın |
| `lwip` | MIT/Apache-2.0 (eski sözdizimi; lwIP C kodu BSD-3) | 0.4.0 | 2026-07-30 | 283 | ⚠️ Kullanım çok düşük; depo bağlantısı yok |
| `netstack-lwip` | — | — | — | — | crates.io'da bulunamadı |
| `tun2proxy` | MIT | 0.8.7 | 2026-10-31 | 7 bin | Referans uygulama (TUN → proxy; `wintun-bindings` + `ipstack`) |
| `wintun-bindings` | MIT | 0.7.40 | 2026-08-17 | 256 bin | ✅ Etkin Wintun bağları |
| `wintun` | MIT | 0.5.1 | 2025-01-29 | 142 bin | 🔶 Bakım yavaş |
| `tun-rs` | Apache-2.0 | 2.8.18 | 2026-11-04 | 299 bin | ✅ Çok platformlu TUN |
| `tun` | WTFPL | 0.8.14 | 2026-07-21 | 559 bin | ❌ WTFPL OSI onaylı değil |
| `etherparse` | MIT OR Apache-2.0 | 0.21.0 | 2026-07-21 | 2,64 milyon | ✅ Paket ayrıştırma/kurma |

Seçim Wintun spike'ında (Görev 7) yapılır; güvenlik geçmişi (RustSec) `cargo deny` ile denetlenir.

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
| SignPath ön sorusu (Wintun ve bağlantı modülü) | Faz 6'yı beklemeden gönderilecek: (1) Wintun ön derlenmiş DLL'inin (OSI dışı ikili lisansı, kaynağı GPL-2.0) imzalanacak pakette bulunması "kapalı bileşen yok" şartına uyar mı; (2) bağlantı modülü "hacking tool" maddesine girer mi. Olumsuzsa Wintun için yedek yol: kurulumda wintun.net'ten sabit hash'le indirme (ADR-003) | ⏳ gönderilecek |
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
