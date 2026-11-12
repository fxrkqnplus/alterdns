# Wintun spike'ı — sonuçlar (Faz 0 Görev 7, T-SPK-002, D8)

- **Tarih ve ortam:** 2026-11-11, geliştirici makinesi. Windows 11 Pro 26H2, HVCI açık, telefon hotspot'u, sistem hop limiti 65.
- **Test hedefi:** S1'de engelli sınıflanan `discord-web` hedefinin bir IPv4 adresi. Adres git dışı `lab/local/lab.local.toml` dosyasında tutulur.
- **Ham veri:** git dışı `lab/results/wintun-*/` altında.
- **Komutlar ve işletim:** `lab/RUNBOOK.md` §6.

## Yapı

- **Wintun DLL'i:** `wintun.dll` 0.14.1, Authenticode imzası WireGuard LLC.
  - SHA-256 sabit; doğrulama dosya kilitliyken yapılır.
  - Yükleme gerçek yoldan yapılır ve yükleme sonrasında modül yolu yeniden denetlenir.
  - DLL'in bağımlılıkları yalnızca System32'den çözülür.
- **Adaptör:** `AlterDNS-Spike`, sabit GUID.
  - Adres `198.18.0.1/30` (RFC 2544), Wintun örneğindeki gibi `CreateUnicastIpAddressEntry` ve `DadState = Preferred` ile verilir. Kalıcı değildir.
  - Yalnızca test IP'si için tek bir /32 rota vardır, `ActiveStore`'da.
- **Kullanıcı modu yığını:** `ipstack` 1.0.1 (Apache-2.0).
- **Motor:**
  - TUN'a gelen TCP akışını sonlandırır ve gerçek hedefe kendi soketiyle bağlanır. Bu soket `IP_UNICAST_IF` ile fiziksel arayüze bağlıdır; böylece TUN'a geri dönmez.
  - İlk TLS kaydına S2-U tekniğini uygular, sonra iki yönlü kopyalar.
  - UDP datagramlarını kendi UDP soketiyle iletir.
  - Yalnızca test IP'sine giden akışları iletir; diğerlerini sayıp düşürür.
- **Koruma:**
  - Her değişiklik önce journal'a yazılır; ayrı işlemdeki bekçinin kirası en çok 60 sn'dir.
  - Döngü koruması: motorun kendi portundan TUN'a dönen akış motoru durdurur.
  - Eşzamanlı oturum sayısı en çok 64.
  - Ön koşullar: `SystemRoot` doğrulaması, lab yollarında junction/symlink olmaması, test IP'sinin canlı trafikle (DNS, ağ geçidi, açık bağlantı) çakışmaması.

## Ölçümler

| Ölçüm | Sonuç |
|---|---|
| Doğrudan, tekniksiz | 0/3 (TLS sıfırlaması) |
| Doğrudan, `TlsRecordSplit@SNI` (tekniği probun kendisi uygular) | 50/50 |
| TUN, motor tekniksiz | 0/3 (`TlsTimeout`; DPI sıfırlaması uygulamaya iletilmiyor, aşağıya bakın) |
| TUN, motor `TlsRecordSplit@SNI` / `Oob@SNI` / `Split@1` | 3/3 · 3/3 · 3/3 |
| TUN, motor `TlsRecordSplit@SNI` ×50 | 50/50 |
| Diğer trafik: kontrol probu, her TUN aşamasından önce, doğrudan | 4/4 |
| UDP 443: QUIC sürüm anlaşmasını zorlayan datagram ×3 | Doğrudan 3/3 yanıt, TUN 3/3 yanıt (Version Negotiation) |
| Döngü (`IP_UNICAST_IF` etkisi) | 0; motorun giden soketleri TUN'a dönmedi |
| Kurulum süresi | ≈ 7 sn (`kill-test` hazır olma süresi 6,96 sn) |
| Koşu süresi (`run`, doğrudan ölçüm dahil) | 54 sn |

**Gecikme** (50 el sıkışma, toplam süre, ms):

| Yol | p50 | p95 |
|---|---|---|
| Doğrudan (teknik probta) | 198 | 235 |
| TUN (teknik motorda) | 210 | 466 |
| Fark | +12 | +231 |

TLS aşaması iki yol arasında karşılaştırılamaz. TUN yolunda uygulamanın TCP el sıkışması yereldir (≈ 0 ms); motorun gerçek hedefe bağlanması TLS aşamasının içine düşer. p95 kuyruğunun nedeni araştırılmadı.

**Kaynak kullanımı:**
- **CPU:** 50 probluk TUN aşamasında (≈ 10 sn) 140 ms işlemci süresi; tek çekirdeğin ≈ %1,3'ü.
- **Özel bellek:** tünel öncesi 2,97 MB, tepe 13,3 MB. Artışın büyük kısmı muhtemelen 2 × 4 MiB Wintun halkalarıdır; bu doğrulanmadı.

**Kurtarma (`kill-test`):**
- Tüneli kuran işlem `TerminateProcess` ile öldürüldü.
- Adaptör, öldürmeden 1 sn sonra zaten yoktu: Wintun adaptörü işlemle birlikte kalkıyor.
- Rota 1,6 sn içinde kalktı.
- Bekçi `ParentGone` raporladı ve üç kaydı kapattı; bayat journal kalmadı.

**Sistem yan etkileri:**
- **Sürücü deposu:** önce ve sonra aynı. Wintun paketi (0.14.0.0) bu makinede başka bir yazılımdan zaten vardı. Çalışan sürücü 0.14; başka Wintun adaptörü yoktu.
- **Defender:** indirmede, arşivi açmada, DLL'i yüklemede ve sürücüyü yüklemede tespit yok (toplam tespit sayısı 3 → 3).
- **HVCI** açıkken sürücü yüklendi.
- **Adaptör açıkken:**
  - Arayüz metriği 5 (düşük).
  - Ağ profili "Public".
  - DNS sunucusu olarak Windows'un yer tutucu `fec0:0:0:ffff::1–3` adresleri.
  - IPv6 link-local adres ve çoklu yayın rotaları.
  - Motor hedef dışı 113 paketi düşürdü.
- **NLA profili:** sabit GUID yüzünden tek bir ağ profili kaydı kalır; silinmedi.

## Kurulumda öğrenilenler

İlk iki `kill-test` denemesi kurulumda durdu; ikisinde de değişiklikler güvenle geri alındı (2 kayıt, çakışma yok, kalıntı yok).

1. **İlk deneme:** Adres `New-NetIPAddress` ile verildi ve 15 sn içinde `Preferred` olmadı. İlk tahmin DAD'dı. Wintun örneğindeki gibi `DadState = Preferred` ile oluşturmaya geçildi; bu yol korunuyor.
2. **İkinci deneme:** Hata iletisine eklenen tanı asıl nedeni gösterdi: adres `Deprecated`, adaptör `Disconnected`.
   - Wintun adaptörü, üzerinde oturum açılana kadar bağlantısız görünür. Bağlantısız arayüzde adres kullanılamaz.
   - Yeni sıra: adaptör → adres → oturum ve motor → `Preferred` beklemesi → /32 rota.

## Faz 2 için notlar

- **Sıfırlamayı iletme:** DPI'ın giden bağlantıyı sıfırlaması uygulamaya hemen yansıtılmalı. Şu an uygulama 4 sn bekleyip zaman aşımı görüyor.
- **Sayaçlar:** `tcp_errors` sayacı her temiz olmayan kapanışı sayıyor; 62 oturumda 61 oldu. Bunların çoğu, probun yanıtın ilk satırından sonra bağlantıyı kendisi kesmesi. Kapanış nedenine göre ayrılmalı.
- **Gecikme:** p95 kuyruğu araştırılmalı. Adaylar: ilk uçuş tamponu, bölütler arası bekleme, `ipstack`.
- **Adaptör ayarları:**
  - Yüksek arayüz metriği.
  - IPv6'yı kapatmak ya da DNS'i açıkça boş bırakmak.
  - Hedef dışı paketlerin kaynağını azaltmak.
- **Halka kapasitesi:** bellek bütçesi için küçültülebilir.
- **Kalan ölçümler:** UDP teknikleri, Discord sesi ve Roblox oyunu (S4); IPv6 hedefler.
- **Kabul edilen risk:** Yükseltilmiş çalışmada kullanıcının yazabildiği ikililer (spike `exe`, `xtask.exe`) kullanıldı. Aynı kullanıcı yetkisiyle yazabilen biri spike'ın kendisini de değiştirebilir, bu yüzden bu risk geliştirme spike'ının tehdit modeli dışında tutuldu. Ürün, yalnızca yöneticinin yazabildiği Program Files altına kurulacak (Faz 5).

## S4 ilk koşu (Görev 9, 2026-11-12)

Ayrıntı ve çıkarımlar `docs/research/CONNECTIVITY_RESEARCH.md` §7'de. Özet:
- `hold` kipi: 4 önek + 28 tek IP, `TlsRecordSplit@SNI`, 310 sn.
- Tarayıcıda Discord (sesli sohbet dahil), Roblox ve Wattpad açıldı; masaüstü istemciler açılmadı.
- DNS önbelleğinden öğrenme bu makinede çalışmadı (önbellek boş).
- Söküm temiz; korunan değerler değişmedi.

**İkinci koşu:** eksik CDN adları eklendi. Roblox Player ve oyun çalıştı (Hyperion uyarısı yok); Discord masaüstü güncelleme denetiminde kaldı. Söküm sırasında oturum 15 sn'de kapanmadı ve adaptör doğrulaması gerçek açıklamayı reddetti; ikisi düzeltildi, kalıntı yok.

## Lisans ve SignPath

Değişmedi; ayrıntı `docs/research/THIRD_PARTY.md` §1.1. Ön derlenmiş DLL lisansı OSI onaylı değil. GPL-3.0 §7 ek izni ve "yalnızca `alterdns-svc` yükler" önlemi yürürlükte. SignPath ön sorusu açık.

## Sonuç (D8)

Aday mod (Wintun + kullanıcı modu motor) bu makinede ve bu ağda uçtan uca çalıştı:
- Bir uygulamanın engelli IP'ye bağlantısı, uygulama hiçbir şey yapmadan, motorun tekniğiyle sistem genelinde geçti.
- Diğer trafik etkilenmedi.
- UDP iletiliyor.
- Kurtarma 2 sn'nin altında.

Açık kalanlar:
- Lisans ve SignPath yanıtı.
- UDP teknikleri ve gerçek istemciler (S4).
- Gecikme kuyruğu.
- Adaptör yan etkilerinin azaltılması.
