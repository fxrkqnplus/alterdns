# Bağlantı Yönetimi — Araştırma Planı ve Kanıtlar

Durum: plan + ön kanıtlar (2026-11-09). §7 Faz 0 deneyleriyle doldurulacak.

**Çerçeve:** Bu modül kullanıcının kendi cihazında ve kullanmaya yetkili olduğu bağlantıda, VPN veya proxy kullanmadan, DPI kaynaklı erişim kısıtlamalarını aşmak içindir. Başka cihazların trafiğine müdahale etmez. Operatörün tarife ve tethering koşulları kullanıcının sorumluluğundadır. AlterDNS TTL değerini kendiliğinden değiştirmez.

## 1. Soru

Hop limit 65 kullanılan bir sistemde (telefon hotspot'u üzerinden bağlanırken tethering tespitini önlemek için yaygın bir ayar), `netsh int ipv4 set glob defaultcurhoplimit=65` komutundan sonra GoodbyeDPI'ın çalışmamaya başladığı gözlemlendi. **Neden varsayılmayacak.** Aşağıdaki hipotezler ölçümle desteklenecek veya çürütülecek. Ayrıca modül farklı bilgisayarlarda da çalışmalıdır; uyumluluk etkenlerinin tamamı test edilecek.

## 2. Kanıtlar

**E1–E7, E13–E15 (yerel kanıtlar):** Geliştirici makinesinden salt okunur toplanan ağ, hop limit, DNS, o tarihte kurulu olan DPI paketi, log ve güvenlik ayarı bulguları; S0 envanteri ve yanıtları (2026-11-10). Kişisel olduğu için public depoda değil, git dışı `CLAUDE.local.md` dosyasındadır. Eski DPI paketi 2026-11-09'da kaldırıldı; E4 ve E7 o tarihten önceki durumu anlatır.

**Genel özet:** Telefon hotspot'u üzerinden ve hop limit 65 iken:
- **Doğrudan erişim:** Discord ve Roblox HTTPS bağlantıları sıfırlanıyor (RST).
- **TTL'e bağlı olmayan `-6` (wrong-seq) kipi + şifreli DNS:** Web ve API erişimi sağlanıyor.
- **S0 (2026-11-10):**
  - Sorunun gözlendiği yapılandırma TTL'e bağlı, sabit TTL'li bir sahte paket kipiydi (E12). DNS, şifresiz olarak standart dışı bir porta yönlendiriliyordu.
  - Hop limitle aynı dönemde ağ da değişti (H2 karıştırıcı değişkeni gerçek).
  - Değişiklikten sonra web, uygulama ve ses erişiminin hepsi kayboldu (H5 tek başına açıklayamaz).
  - Eski DPI servisi Ekim'de iki kez beklenmedik şekilde kapanmış. Defender aynı tehdit adını lab indirmesinden önce de tespit etmiş (H6).

| # | Kanıt | Kaynak |
|---|---|---|
| E1–E7, E13–E15 | Yerel kanıtlar (yukarıda özetlendi) | `CLAUDE.local.md` |
| E8 | GoodbyeDPI kip tanımları: `-5` = `--auto-ttl` (TTL'e bağlı sahte paket); `-6` = `--wrong-seq`; `-7` = `--wrong-chksum`; `-8` = seq + chksum; `-9` = `-8` + `-q` (QUIC engeli, varsayılan). `--wrong-chksum` için "VM'de veya bazı yönlendiricilerde çalışmayabilir" notu var. `--set-ttl`/`--auto-ttl` için "TEHLİKELİ" uyarısı var | GoodbyeDPI README |
| E9 | GoodbyeDPI yazarının `--wrong-seq` commit notu: yöntem "TTL değerlerinin değiştirilmesini yasaklayan ağlarda (tethering engeli/ücretli özelliği olan mobil ağlar)" işe yarayabilir | Commit `bbb6af89` (2021-12-25) |
| E10 | Hop limit 65 yöntemi hotspot gizleme amacıyla yaygın: dizüstünün 65 TTL'i telefondan geçince 64 olur ve telefonun kendi trafiğine benzer. Operatörler TTL ile tethering tespiti yapıyor | Çeşitli kaynaklar (doğrulanacak ayrıntı: Türk operatörleri) |
| E12 | GoodbyeDPI-Turkey `windows/service_install_dnsredir_turkey.cmd` servisi `-5 --set-ttl 5 --dns-addr 77.88.8.8 --dns-port 1253 …` ile kurar. Kaynakta (`src/goodbyedpi.c`) sonra gelen `--set-ttl`, `-5`'in otomatik TTL'ini kapatır: sabit TTL 5'li sahte paket + 2 bayt ters parçalama + en çok 1200 bayt yük; wrong-seq ve wrong-chksum yok | `cagritaskn/GoodbyeDPI-Turkey` (Apache-2.0), dosyanın son commit'i `bd88019`; okuma 2026-11-10 |
| E11 | Discord sesi ayrı UDP bağlantısı kullanır; GoodbyeDPI UDP'yi işlemez. zapret UDP/QUIC desync ve `--dpi-desync-fake-discord` destekler | GoodbyeDPI issue #276; zapret README |

**Ön gözlem (hipotez değil, kayıt):** Yerel kanıtlar, hop limit 65 iken **TTL'e bağlı olmayan** bir tekniğin (`-6`) web/API düzeyinde çalıştığını gösteriyor. "GoodbyeDPI çalışmıyor" gözlemi muhtemelen belirli bir kipe, DNS yöntemine veya trafik türüne (ses, oyun) özgüdür. Hangisi olduğu ölçülmeden bilinemez.

**Public depo notu:** §7'ye yalnızca genelleştirilmiş sonuçlar yazılır. Ağ adresi, log satırı ve paket yolu `CLAUDE.local.md` ve `lab/results/` (git dışı) içinde kalır.

## 3. Hipotezler

| # | Hipotez | Destekleyen | Test | Destek ölçütü / çürütme ölçütü |
|---|---|---|---|---|
| H1 | Başarısız olan yapılandırma TTL'e bağlı bir sahte paket kipiydi (`-5`/`--auto-ttl`/`--set-ttl`). Mobil yolda bu kipteki sahte paketler düşürülüyor, normalleştiriliyor veya DPI'a ulaşmıyor | E8, E9 | ~~GoodbyeDPI kipleri~~ (iptal, D8). **S2-U:** hop limit 65 iken TTL'e bağlı `Disorder` (kendi soketimizde `IP_TTL`; `Fake` Faz 0'da uygulanmadı) ile TTL'den bağımsız `Split`/`TlsRecordSplit`/`Oob`; her biri 3 tekrar | TTL'e bağlı teknikler başarısız, bağımsızlar başarılı → destek. TTL'e bağlılar başarılı → çürür |
| H2 | Karıştırıcı değişken: TTL değişikliğiyle aynı dönemde ağ da değişti (ev ağı → iPhone hotspot); fark operatör DPI'ından kaynaklanıyor | Kullanım bağlamı | Başka bir ağ (ev Wi-Fi/Ethernet) varsa aynı matris. Yoksa kullanıcıdan TTL değişikliği öncesi hangi ağı kullandığı sorulur | Ev ağında 65 ve 128 arasında fark yoksa ama hotspot'ta varsa → ağ kaynaklı |
| H3 | DNS yolu: hazır betikler DNS'i şifresiz biçimde standart dışı porta (Yandex `77.88.8.8:1253`) yönlendiriyor. Hotspot/operatör yolunda bu UDP trafiği engelleniyor veya DNS telefonda önbellekleniyor ya da değiştiriliyor | E3, E5 (DoH ile çalışıyor) | Hedef alan adları için sistem DNS, DoH ve `77.88.8.8:1253` yanıt karşılaştırması; motor kapalı | DoH dışındakiler başarısız veya farklı yanıt → destek |
| H4 | Hatalı checksum'lı sahte paketler iPhone NAT'ında (pf) düşüyor ve DPI'a hiç ulaşmıyor | E8 (`--wrong-chksum` notu) | ~~`-7` tek başına~~ (iptal, D8). Hatalı checksum'lı paket kendi soketimizle gönderilemez; sürücüsüz yöntemle test edilemez | Faz 0'da "belirsiz" (gerekçe: test yöntemi yok) |
| H5 | "Çalışmıyor" gözlemi web değil, UDP trafiğiyle ilgili (Discord sesi, Roblox oyun). GoodbyeDPI bunları kapsamıyor | E11 | Discord masaüstü sesli kanal, Roblox oyuna katılım; web probları geçerken | Web geçiyor, ses/oyun başarısız → destek |
| H6 | Yerel durum: DPI servisi durmuş veya çökmüş, zamanlanmış görev tetiklenmemiş, başka bir WinDivert istemcisi var, Defender müdahalesi | E4 (yerel) | Paket kaldırıldığı için canlı durum gözlenemez. Tarihsel kanıt: olay günlüğü (Service Control Manager 7031/7034, WinDivert), Defender koruma geçmişi, paketten kalan bağlantı logları (`CLAUDE.local.md`) | Hata kaydı varsa destek; kanıt yetersizse belirsiz |
| H7 | Operatör tethering olarak tanınan trafiği (TTL 127) farklı bir politika veya yoldan geçiriyor. 65 ile trafik "telefonun kendi trafiği" yoluna giriyor ve bu yolda DPI davranışı farklı | E10 (bağlam) | **Lab (S5-soket):** Sistem hop limiti değişmez (D5, ADR-014). Yalnızca prob soketlerine `IP_TTL` 64 / 65 / 128 verilir; tekniksiz + S2-U'nun en iyi 3 tekniği | Bir teknik 65'te geçip 128'de (veya 64'te) başarısızsa ya da tersi → destek |
| H8 | GoodbyeDPI'ın otomatik TTL mantığı, gelen paket TTL'inden mesafe tahmin eder (128/64 − TTL). Giden TTL'in (65) bu hesaba doğrudan etkisi olmamalı | E8 | S2-U: sabit `IP_TTL` taraması (N = 1…8) ile `Disorder`; `Fake` Faz 0'da uygulanmadı ("test edilmedi"). GoodbyeDPI'ın gelen TTL'den otomatik hesabı ölçülmez (iptal, D8) | Giden TTL'den bağımsızlık beklenir; kısmi test, gerekçesiyle |
| H9 | MTU veya parçalama: hücresel yolda etkin MTU daha düşük; ters parçalama ve `--max-payload` etkileşimi | — | S2-U: bölme konumu taraması (sabit ofset, SNI ortası) ve `TlsRecordSplit`; Path MTU ölçümü | Varyantlar arasında anlamlı fark → destek |

Yeni hipotez çıkarsa tabloya eklenir. Her hipotezin sonucu §7'ye kanıtıyla yazılır.

## 4. Lab düzeneği tasarımı (`xtask lab`)

**Güvenlik**
- Her senaryo başlamadan önce kullanıcıya şunlar gösterilir ve onay alınır: ne değişecek, ne kadar sürecek, geri alma nasıl olacak.
- Başlangıç anlık görüntüsü alınır: hop limit v4/v6, DNS, DoH tablosu, servisler, görevler, WinDivert durumu.
- Değişiklikler journal'a yazılır. Ayrı bir bekçi işlemi kurulur: süre (varsayılan 60 sn) dolunca başlangıç durumuna döner.
- Upstream ikililer yalnızca resmi sürüm sayfalarından (GitHub sürümleri, wintun.net) indirilir. SHA-256 değerleri `lab/pins.toml` dosyasında sabittir. Ayrı bir dizinde çalıştırılır, iş bitince silinir.
- Aynı anda yalnızca bir motor çalışır. Lab dışından çalışan bir DPI aracı (`goodbyedpi.exe`, `winws.exe`) veya lab dizini dışına işaret eden bir `WinDivert` servisi varsa senaryo başlamaz; kullanıcıya bildirilir ve ona onaysız dokunulmaz.
- Upstream motorun kurduğu WinDivert sürücü servisi journal'a yazılır; motor ailesi bitince durdurulur ve servisin kalktığı doğrulanır.
- Senaryo bitince başlangıç anlık görüntüsü karşılaştırılır; fark varsa geri alınır ve rapor edilir.
- Ölçümler veri tüketir; mobil veride senaryo başına tahmini tüketim gösterilir.

**Prob seti** (`data/probes/*.toml`; her biri zaman aşımlı):
- Kontrol: `connectivitycheck.gstatic.com/generate_204`, `www.msftconnecttest.com/connecttest.txt`, engelsiz bir HTTPS site.
- Discord: `discord.com`, `gateway.discord.gg` (TLS el sıkışma), `cdn.discordapp.com`, `discord.media` (ses sunucuları: UDP; ayrı test).
- Roblox: `www.roblox.com`, `apis.roblox.com`, `clientsettings.roblox.com` (yerel referans paketteki uç noktalar dikkate alınır).
- Engelli olduğu bilinen başka web siteleri: kullanıcıdan alınır; depoya yazılmaz, yerel yapılandırmada tutulur.

**Her prob için kaydedilen veriler:** DNS (sistem / DoH / alternatif) yanıtları, TCP bağlantı sonucu, TLS sonucu (RST, zaman aşımı, sertifika uyuşmazlığı, başarı), HTTP durum kodu, süreler, tekrar sayısı.

**Paket yakalama:** Seçili hücrelerde `pktmon` (yerleşik) ile kısa yakalama alınır. Çıktı `lab/results/` altında kalır (git dışı; kişisel veri içerebilir).

## 5. Deney protokolü (Faz 0)

1. **S0 — Envanter (salt okunur):** E1–E7'yi yeniden topla. Olay günlüğünden GoodbyeDPI ve WinDivert hata kayıtlarını çıkar (H6). Kullanıcıya şunu sor: "TTL'i değiştirdiğiniz dönemde ağınız da değişti mi? Hangi GoodbyeDPI kipini veya betiğini kullanıyordunuz? Çalışmayan şey web siteleri mi, Discord sesi mi, Roblox oyunu mu?"
2. **S1 — Motor kapalı temel ölçüm:** DNS karşılaştırması (H3) ve TCP/TLS sınıflandırma (DNS / SNI / IP / yok).
3. ~~**S2 — Upstream GoodbyeDPI kip matrisi**~~ ve ~~**S3 — zapret (winws)**~~: **iptal** (2026-11-09, D8). Defender WinDivert arşivini engelledi; WinDivert'e bağlı motorlar çalıştırılmaz.
4. **S2-U — Sürücüsüz soket teknikleri** (hop limit 65; sistem değişikliği yok): `None`, `Split`, `TlsRecordSplit`, `Disorder { IP_TTL 1…8 }`, `Oob` (`Fake` Faz 0'da yok) × bölme konumu {sabit ofset, SNI ortası} × DNS {sistem, DoH} × 3 tekrar. `IP_TTL` yalnızca probun kendi soketindedir. Path MTU ölçümü (H1, H8, H9).
5. **Wintun spike'ı** (onaylı, yönetici terminali): tek bir test IP'si için TUN rotası, kullanıcı modunda TCP sonlandırma + teknikli yeniden kurma, UDP iletimi; CPU, bellek ve gecikme ölçümü (H5'in UDP kısmı).
6. **S4 — Gerçek istemciler (el ile, yönergeli; Wintun spike'ından sonra):** Discord masaüstü (giriş, metin, sesli kanal), Roblox (oyuna katılım). S2-U'nun en iyi 2 tekniğiyle, Wintun yolundan (H5).
7. **S5-soket (H7):** Sistem hop limiti değişmez (D5 kapandı, ADR-014). Tekniksiz bağlantı ve S2-U'nun en iyi 3 tekniği, prob soketinde `IP_TTL` 64 / 65 / 128 ile 3'er kez denenir. Operatör 64 ve 128 paketlerini paylaşım trafiği gibi görebilir; çalıştırmadan önce kullanıcı onayı alınır.
8. **S6 — Başka ağ varsa:** S1 ve S2-U tekrarlanır (H2).
9. **Rapor:** Hipotez tablosu (destek / çürüdü / belirsiz + kanıt), Faz 2 için başlangıç strateji kataloğu ve sıralama önerisi.

## 6. Uyumluluk etkenleri (farklı bilgisayarlar)

| Etken | Risk | Tespit | Davranış |
|---|---|---|---|
| Windows sürümü/build | Sürücü ve API farkları | `RtlGetVersion` | Desteklenmeyende `ADNS-INS-006` |
| ARM64 | WinDivert resmi ARM64 sürücüsü yok | İşlemci mimarisi | Bağlantı modülü kurulmaz (`ADNS-DPI-005`) |
| HVCI / Bellek bütünlüğü | Uyumsuz sürücü yüklenmez | `Win32_DeviceGuard` | Sürücü yükleme testi; kapattırma önerilmez |
| Sürücü engel listesi / WDAC / kurum ilkesi | Sürücü engellenebilir | Yükleme hatası (577 vb.) | `ADNS-DPI-002` |
| Smart App Control | İmzasız exe'ler engellenir | `VerifiedAndReputablePolicyState` | Kod imzası kararı (D3); kurulum öncesi uyarı |
| Defender / üçüncü taraf AV | WinDivert HackTool/RiskTool olarak işaretlenebilir; karantinaya alınabilir | WSC, Defender geçmişi | Şeffaf açıklama; AV kapattırılmaz; yanlış pozitif bildirim yönergesi |
| Başka WinDivert kullanıcıları | Sürüm çakışması, çift işleme | Servis/işlem tespiti, `WINDIVERT_LAYER_REFLECT` | `ADNS-DPI-003/004`; onayla duraklatma |
| VPN / proxy istemcileri | Trafik tünelde; DPI motoru anlamsız | Arayüz türü, rota, WinHTTP proxy | `ADNS-NET-003`; modül beklemede |
| Killer NIC "Advanced Stream Detect" | GoodbyeDPI ile uyumsuzluğu bildirilmiş | Sürücü/hizmet adı | Uyarı ve yönerge |
| IPv6-only / NAT64 / 464XLAT | IPv4 tekniklerinin yolu farklı | Adres ve önek; DNS64 | IPv6 teknikleri; DNS64 korunur |
| Captive portal | Yanlış "engel" sınıflandırması | NCSI / HTTP 204 kontrolü | `ADNS-NET-002`; ayar ertelenir |
| Ölçülü bağlantı | Veri tüketimi | NLM maliyet | Hız testi yok (`ADNS-NET-004`) |
| Hotspot türü (iPhone/Android, Wi-Fi/USB/Bluetooth) | NAT davranışı farklı (seq/checksum toleransı) | Ağ geçidi aralığı, MAC OUI, adaptör türü (ipucu) | Aday sıralamasına prior olarak girer; kesin kural yok |
| Hop limit (64/65/128/özel) | TTL tabanlı tekniklere etki | Salt okunur okuma | Eksen olarak kaydedilir; asla değiştirilmez |
| Çoklu adaptör, Hyper-V/WSL vSwitch | Yanlış arayüzde filtre; WSL trafiği FORWARD katmanında | Rota tablosu | Varsayılan rotanın arayüzü; WSL kapsamı sınırlama olarak belgelenir |
| Roblox Hyperion | WinDivert ile etkileşim bilinmiyor | Gerçek oyun testi | Sonuca göre "uyumluluk notu" veya istisna |
| Kurumsal TLS denetimi | Sertifika uyuşmazlığı | Prob sertifika zinciri | Modül devre dışı ve açıklama; sertifika denetimi kapatılmaz |

## 7. Sonuçlar (Faz 0'da doldurulacak)

Ölçümler 2026-11-10'da, geliştirici makinesinde, telefon hotspot'u üzerinden ve sistem hop limiti 65 iken yapıldı. Ham veri `lab/results/` altında, git dışı. Sonuçlar **bu ağ ve bu tarih** içindir; başka ağlarda S6 ile yeniden ölçülecek.

**S1 (motor kapalı, 72 prob):**
- Kontrol probları her DNS yolunda 3/3 geçti. Discord'un ve Roblox'un 3'er hedefi her DNS yolunda 0/3: TLS sıfırlaması (`TlsReset`).
- Sistem DNS'i, DoH ve `77.88.8.8:1253` aynı adresleri döndürdü.
- TCP bağlantısı kuruluyor; sıfırlama ClientHello'dan yaklaşık 30–40 ms sonra geliyor. Bu, SNI tabanlı DPI'ın RST enjeksiyonuyla uyumlu.

**S2-U (sürücüsüz teknikler, 828 prob):** Discord ve Roblox'tan 6 hedef × 3 tekrar × DNS {sistem, DoH} = hücre başına 36 prob. DNS yolu sonucu değiştirmedi.

| Teknik | Bölme: 1. bayt | Bölme: SNI ortası |
|---|---|---|
| Yok | 0/36 | — |
| `Split` (iki TCP bölütü) | **36/36** | 0/36 |
| `TlsRecordSplit` (iki TLS kaydı) | **36/36** | **36/36** |
| `Oob` (1 acil bayt) | **36/36** | **36/36** |
| `Disorder`, ilk bölüt `IP_TTL` 1–2 | 0/72 | 0/72 |
| `Disorder`, ilk bölüt `IP_TTL` 3–8 | 215/216 (1 TCP zaman aşımı) | 0/216 |

Diğer ölçümler:
- Başarılı tekniklerde TLS el sıkışması medyan yaklaşık 100–120 ms sürdü.
- HTTP yanıtları gerçek sunuculardan geldi (200/403/404); TLS doğrulaması Windows sertifika deposuyla yapıldı.
- Path MTU (DF bayraklı ping, 1.1.1.1): 1500.

| Hipotez | Durum | Kanıt (dosya/ölçüm) | Not |
|---|---|---|---|
| H1 | Belirsiz (kısmi kanıt) | S2-U, S0 (E12, E13) | TTL'den bağımsız `TlsRecordSplit`, `Oob` ve `Split` (1. bayt) hop limit 65'te çalışıyor. TTL'e bağlı `Disorder` yalnızca ilk bölüt TTL ≥ 3 ile geçiyor; bu durumda sonuç `Split` (1. bayt) ile aynı. TTL ≤ 2'li bölüt DPI'a ulaşmıyor, TTL ≥ 3'lü ulaşıyor. Eski yapılandırmanın sabit TTL 5'i bu yolda DPI'a ulaşacak büyüklükte; sunucuya varmadan düşüp düşmediği ölçülmedi. Sahte paket (`Fake`) Faz 0'da uygulanmadı. "Sahte paket DPI'a ulaşmıyor" açıklaması bu ağ için desteklenmedi |
| H2 | Belirsiz; karıştırıcı değişken doğrulandı | S0 yanıtı (E13) | Hop limit değişikliğiyle aynı dönemde ağ da değişti. Ayırmak için S6 (başka ağ) gerekli |
| H3 | Çürüdü (bu ağda) | S1 | Üç DNS yolu aynı adresleri verdi; UDP 1253 engelli değil. Engel DNS değil, SNI tabanlı TLS sıfırlaması. Wattpad'de de aynı: üç DNS yolunda da TLS sıfırlaması (2026-11-12) |
| H4 | Belirsiz | — | Hatalı checksum'lı paket sürücüsüz yöntemle gönderilemez; test yöntemi yok |
| H5 | Kısmen | S0 yanıtı (E13), Wintun spike'ı, S4 (ilk koşu) | Web dahil erişim kaybolduğu için tek açıklama UDP değil. Wintun yolu UDP'yi iletiyor. S4'te tarayıcıdan Discord sesli sohbeti çalıştı; ses trafiğinin TUN'dan geçip geçmediği ölçülmedi (ses sunucusu adresleri öğrenilemedi). İkinci koşuda Roblox Player açıldı ve oyun oynandı: oyun trafiği (UDP, Roblox'un kendi bloğu) motor üzerinden çalıştı, Hyperion uyarısı yok. Discord masaüstü açılmadığı için uygulama sesi ölçülmedi |
| H6 | Belirsiz (destekleyen kanıt var) | S0 (E14, E15) | Eski DPI servisi Ekim'de iki kez beklenmedik şekilde kapandı; Defender aynı tehdit adını lab indirmesinden önce tespit etti. TTL değişikliğinin tarihi bilinmeden ilişki kurulamaz |
| H7 | Çürüdü (akış düzeyinde, bu ağda) | S5-soket | Prob soketinde TTL 64, 65 ve 128 arasında fark yok: tekniksiz bağlantı üçünde de sıfırlandı (27/27 her biri), `TlsRecordSplit@SNI`, `Oob@SNI` ve `Split@1` üçünde de 27/27 geçti (Discord, Roblox, Wattpad). Teknikler Windows varsayılanı 128'de de çalışıyor; hop limit 65 gerekmiyor. Operatörün cihaz düzeyindeki tethering sınıflaması ölçülmedi (ADR-014 gereği sistem değeri değiştirilmez) |
| H8 | Kısmen | S2-U | Sabit `IP_TTL` taraması DPI'ın yaklaşık 2.–3. sıçrama arasında olduğunu gösterdi (eşik TTL 3). Sistem hop limiti değiştirilmediği için giden TTL'den bağımsızlık doğrudan ölçülmedi; `Fake` test edilmedi |
| H9 | Çürüdü (MTU kısmı) | S2-U | Path MTU 1500. Bölme konumu fark yaratıyor (`Split` 1. baytta çalışıyor, SNI ortasında çalışmıyor), ama bu MTU değil, DPI'ın ayrıştırma davranışı |

**Wintun spike'ı (2026-11-11; ayrıntı `spikes/wintun/RESULTS.md`):**
- **Kurulum:** Yalnızca test IP'si (S1'de engelli `discord-web` adreslerinden biri) /32 rotayla Wintun adaptörüne yönlendirildi. Kullanıcı modu motor (`ipstack`) TCP akışını sonlandırıp tekniği kendi soketinde uyguladı.
- **Sonuç:** Uygulamanın hiçbir şey yapmadığı bağlantılar sistem genelinde geçti.
  - Motor tekniksiz: 0/3.
  - `TlsRecordSplit@SNI`, `Oob@SNI`, `Split@1`: her biri 3/3.
  - `TlsRecordSplit@SNI` ×50: 50/50.
- **Diğer trafik:** etkilenmedi. UDP iletildi.
- **Gecikme:** toplam p50 +12 ms, p95 +231 ms.
- **Kurtarma:** işlem öldürülünce adaptör ve rota 1,6 sn içinde kalktı.

**Wattpad ve S5-soket (2026-11-12):**
- **Wattpad (S1):** `wattpad.com`, `www` ve `api` alan adları üç DNS yolunda da TLS sıfırlamasıyla engelli (9/9 her biri); kontrol probları geçti.
- **Wattpad (S2-U):** Discord ve Roblox ile aynı örüntü:
  - `TlsRecordSplit` (iki konum), `Oob` (iki konum) ve `Split@1`: 18/18.
  - `Split@SNI`: 0/18; `Disorder@SNI`: her TTL'de 0/18.
  - `Disorder@1`: TTL ≤ 2'de 0/18, TTL ≥ 3'te 18/18.
- **S5-soket (H7):** Sistem hop limiti değiştirilmedi; yalnızca prob soketinin TTL'i 64 / 65 / 128 oldu. Sonuç TTL'den bağımsız: tekniksiz bağlantı her TTL'de sıfırlandı, en iyi 3 teknik her TTL'de 27/27 geçti.
- **Korunan değerler (ADR-014):** üç koşunun başında ve sonunda değişmedi.

**S4, ilk koşu (2026-11-12; Wintun + kullanıcı modu motor, `TlsRecordSplit@SNI`, 310 sn):**
- **Yönlendirme:** Roblox'un kendi 4 bloğu ve alan adlarından çözülen 28 tek IP (Discord, Roblox, Wattpad).
- **Tarayıcı (normal ve gizli sekme):** Discord, Roblox ve Wattpad web tünel kurulur kurulmaz açıldı. Discord web'de bir kullanıcıyla sesli sohbet sorunsuz kuruldu.
- **Masaüstü istemciler:** Discord masaüstü ve Roblox Player hiç açılmadı (web'deki "oyna" düğmesi de Player'ı başlatamadı).
  - Muhtemel neden (ölçülmedi): istemcilerin açılışta kullandığı adlar rota kümesinin dışında kaldı; bu adreslere giden trafik TUN'a girmeden doğrudan çıktı. Koşudan sonra çözülen adreslerde `dl.discordapp.net`, `media.discordapp.net`, `latency.discord.media`, `clientsettingscdn.roblox.com`, `setup.rbxcdn.com` ve `setup-aws.rbxcdn.com` kümede değildi; `updates.discord.com`, `clientsettings.roblox.com`, `gamejoin.roblox.com` kümedeydi.
  - Çalışırken öğrenme hiç adres eklemedi: bu makinede Windows DNS önbelleği boş (`Get-DnsClientCache` ve `ipconfig /displaydns` 0 kayıt, yeni bir sorgudan sonra da). Kayıt defterinde TTL geçersiz kılması yok; neden bilinmiyor. Ayrıca Chromium tabanlı istemciler kendi çözücülerini kullanabilir. DNS önbelleğine dayanan öğrenme bu yüzden güvenilir değil.
- **Motor:** 27 TCP, 24 UDP oturumu (16 450 giden, 112 265 gelen datagram); döngü 0; kontrol probu 16/16.
- **Söküm ve korunan değerler:** 34 kayıt geri alındı, kalıntı yok; korunan değerler değişmedi. Öncesinde yeniden koşulan `kill-test` de temizdi (adaptör ve rota 1,7 sn içinde kalktı).
- **Faz 2 için çıkarım:** Hedef başına adres listesi kırılgan; istemciler çok sayıda CDN adı kullanıyor. Motorun trafiği daha geniş alması ve tekniği SNI'ye göre seçmesi (ya da DNS'i motorun kendisinin çözmesi) değerlendirilmeli. Bu, kullanıcının varsayılan rotasına dokunmadan, yalnızca kendi rotalarımızla yapılmalı (ADR-014).

**S4, ikinci koşu (2026-11-12; eksik CDN adları eklendi: 4 önek + 48 tek IP, 304 sn):**
- **Roblox Player:** açıldı; uygulamadan ve web'deki "oyna" düğmesinden oyuna girildi ve oynandı. Hyperion uyarısı ya da çökme yok.
- **Tarayıcı:** Discord (metin, ses), Roblox ve Wattpad yine çalıştı.
- **Discord masaüstü:** "Checking for updates" ekranında kaldı.
  - Güncelleyicinin günlüğü: tünel yokken `updates.discord.com` bildirim isteği yaklaşık 100 ms içinde hatayla bitiyor (DPI sıfırlamasıyla uyumlu). Tünel açıkken aynı istek yanıtsız kalıyor; ne hata ne başarı yazılıyor.
  - Bu adın adresleri rota kümesinde; istek tünele giriyor. Tarayıcının aynı adreslere UDP (QUIC) ile gitmiş olabileceği, güncelleyicinin ise TCP + TLS kullandığı düşünülüyor; ölçülmedi.
  - Sonraki adım: motorda bağlantı başına kayıt (hedef, SNI, iki yöndeki bayt, kapanış nedeni).
- **Üçüncü koşu (bağlantı kaydıyla, 180 sn):** Discord güncelleyicisinin `updates.discord.com` bağlantıları tünele girdi; ilk uçuş (ClientHello) yalnızca 175 bayttı. `TlsRecordSplit@SNI` uygulanmasına rağmen karşı taraf yaklaşık 150 ms içinde bağlantıyı sıfırladı, hiç yanıt gelmedi. Aynı koşuda tarayıcı boyutlu (1,8–2 KB) ClientHello'lar geçti: `gateway.discord.gg` (≈1,6 MB veri), `cdn.discordapp.com` ve bir `*.discord.media` ses sunucusu. Sonuç: sorun yönlendirme değil; bu teknik küçük ClientHello'da DPI'ı atlatmıyor. Sonraki adım: aynı istemciyle `Oob@SNI` ve `Split@1` denemesi (S2-U'da ikisi de 36/36). Faz 2 için: teknik, istemcinin ClientHello biçimine göre seçilebilmeli.
- **Motor:** 100 TCP, 44 UDP oturumu; döngü 0; kontrol probu 16/16.
- **Söküm:** Wintun oturumu 15 sn içinde kapanmadı; adaptör, işlem çıkınca kalktı. Journal'ın adaptör kaydı geri alınamadı: doğrulama, adaptörün gerçek açıklamasını ("AlterDNS Tunnel") ve karışık harfli PnP kimliğini (`SWD\Wintun\{…}`) reddediyordu. Düzeltildi (test); bekleme 30 sn'ye çıktı; kayıt `lab recover` ile kapatıldı, kalıntı yok.
- **Korunan değerler:** "dns" farkı bizim adaptörümüzden geldi (Windows'un yer tutucu DNS adresleri; adaptör son okumada hâlâ vardı). Kullanıcının değerleri o sabahki okumayla birebir aynı. Betik artık kendi adaptörümüzü DNS bölümünde de dışarıda tutuyor.

**Faz 2 için ilk sinyal (bu ağ):**
- Aday sıralama: `TlsRecordSplit` → `Oob` → `Split` (1. bayt). Üçü de sürücüsüz ve TTL'e bağlı değil.
- `Disorder` bir şey katmadı. Düşük TTL'li bölütler sonucu kötüleştirdi.
- Wintun yolu (Görev 7) aynı teknikleri sistem genelinde uygulayabilir mi, ve UDP (ses/oyun) nasıl davranıyor: bunlar sonraki adım.

## 8. GoodbyeDPI paketleri ve güvenlik uyarıları

- **2025 SilentCryptoMiner kampanyası (Kaspersky GReAT):**
  - Saldırganlar GitHub'daki popüler bir DPI atlatma aracını değiştirip madencili arşivler dağıttı. Kampanya ≥ 2.000 kurban (Rusya) ile raporlandı.
  - Dağıtımda YouTube içerik üreticilerine sahte telif şikâyetleriyle şantaj yapıldı. Kurulum yönergeleri antivirüsün kapatılmasını istiyordu.
  - Kaspersky, WinDivert ("Windows Packet Divert") araçlarının kötüye kullanımının arttığını raporladı.

  **Sonuçlar:**
  1. Yalnızca resmi kaynak ve sabitlenmiş hash kullanılır.
  2. AlterDNS kullanıcıya asla AV kapattırmaz.
  3. Geliştirici makinesindeki eski DPI paketi karşılaştırma yapılmadan kaldırıldı (2026-11-09); yerel hash karşılaştırması iptal edildi. Dağıtılan ve lab'da kullanılan ikililer resmi arşivle hash bazında doğrulanır (`lab/pins.toml`).
- **zapret yazarının uyarısı:** zapret README'si dolandırıcılara karşı bir uyarıyla açılıyor: zapret özgür ve açık kaynaktır; yalnız belirli bir kaynaktan indirilmesini dayatan veya bağlantı, video ve dosya kaldırtmaya çalışan kişilere itibar edilmemelidir (özet; 2026-11-09). Bu, SilentCryptoMiner kampanyasındaki sahte telif şikâyeti yöntemiyle örtüşüyor. Özeti `THIRD_PARTY.md`'de.
- **AV etiketleri:** WinDivert, bazı ürünlerde yüksek riskli araç veya HackTool olarak işaretlenebiliyor (zapret-win-bundle notu). **Gözlem (2026-11-09):** Microsoft Defender, resmi sürüm sayfasından indirilen `WinDivert-2.2.2-A.zip` arşivine erişimi `Misleading:Win32/Malgent!AMTB` tespitiyle engelledi. Bu, varsayılan Defender'lı kullanıcı sistemlerinde bağlantı modülünün kurulumunu doğrudan etkiler; Faz 2 ve Faz 6'da açık risk olarak izlenir. Sigma kurallarında WinDivert sürücü yüklemesi bir algılama kuralı. Kurumsal ortamlarda alarm üretebilir; uygulama bunu açıkça belirtir.
- **Sürücü imzası:** WinDivert 2.2.0 varyantları (A/B/C/D) yalnızca sürücü imzası bakımından farklı. Dolaşımdaki 2.2 sürücülerinden biri Cloudveil Technology Inc. EV imzası taşıyor (2019 zaman damgası). Dağıtılan veya makinede bulunan sürücüler resmi arşivle hash bazında karşılaştırılır.
- **GoodbyeDPI'ın son sürümü:** 0.2.3rc3, 2024-09-14 (GitHub API). 2026-11-09 itibarıyla daha yeni sürüm yok; depodaki son commit 2026-01-19.

## 9. Faz 2 için ön tasarım notları

- Başlangıç sıralaması kanıtlara dayanır. E5/E6'ya göre ilk aday DoH aktarıcı + wrong-seq sahte paket + ters segmentasyondu; wrong-seq paket sürücüsü ister ve WinDivert askıda olduğu için D8'e bağlıdır. Sürücüsüz motorda karşılığı S2-U sonuçlarından seçilir. Son karar Faz 0 sonuçlarından sonra verilir.
- Puanlama: başarı zorunludur; sonra kararlılık (3/3), p95 el sıkışma süresi, yönlendirilen paket/sn (CPU vekili). Eşitlikte daha az müdahaleci teknik seçilir.
- Profil anahtarı: ağ parmak izi hash'i + hop limit + IPv6 varlığı. Profil 30 gün kullanılmazsa önceliği düşer.
