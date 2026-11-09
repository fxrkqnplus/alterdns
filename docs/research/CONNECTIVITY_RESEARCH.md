# Bağlantı Yönetimi — Araştırma Planı ve Kanıtlar

Durum: plan + ön kanıtlar (2026-11-09). §7 Faz 0 deneyleriyle doldurulacak.

**Çerçeve:** Bu modül kullanıcının kendi cihazında ve kullanmaya yetkili olduğu bağlantıda, VPN veya proxy kullanmadan, DPI kaynaklı erişim kısıtlamalarını aşmak içindir. Başka cihazların trafiğine müdahale etmez. Operatörün tarife ve tethering koşulları kullanıcının sorumluluğundadır. AlterDNS TTL değerini kendiliğinden değiştirmez.

## 1. Soru

Hop limit 65 kullanılan bir sistemde (telefon hotspot'u üzerinden bağlanırken tethering tespitini önlemek için yaygın bir ayar), `netsh int ipv4 set glob defaultcurhoplimit=65` komutundan sonra GoodbyeDPI'ın çalışmamaya başladığı gözlemlendi. **Neden varsayılmayacak.** Aşağıdaki hipotezler ölçümle desteklenecek veya çürütülecek. Ayrıca modül farklı bilgisayarlarda da çalışmalıdır; uyumluluk etkenlerinin tamamı test edilecek.

## 2. Kanıtlar

**E1–E7 (yerel kanıtlar):** Geliştirici makinesinden salt okunur toplanan ağ, hop limit, DNS, o tarihte kurulu olan DPI paketi, log ve güvenlik ayarı bulguları. Kişisel olduğu için public depoda değil, git dışı `CLAUDE.local.md` dosyasındadır. Eski DPI paketi 2026-11-09'da kaldırıldı; E4 ve E7 o tarihten önceki durumu anlatır.

**Genel özet:** Telefon hotspot'u üzerinden ve hop limit 65 iken:
- **Doğrudan erişim:** Discord ve Roblox HTTPS bağlantıları sıfırlanıyor (RST).
- **TTL'e bağlı olmayan `-6` (wrong-seq) kipi + şifreli DNS:** Web ve API erişimi sağlanıyor.

| # | Kanıt | Kaynak |
|---|---|---|
| E1–E7 | Yerel kanıtlar (yukarıda özetlendi) | `CLAUDE.local.md` |
| E8 | GoodbyeDPI kip tanımları: `-5` = `--auto-ttl` (TTL'e bağlı sahte paket); `-6` = `--wrong-seq`; `-7` = `--wrong-chksum`; `-8` = seq + chksum; `-9` = `-8` + `-q` (QUIC engeli, varsayılan). `--wrong-chksum` için "VM'de veya bazı yönlendiricilerde çalışmayabilir" notu var. `--set-ttl`/`--auto-ttl` için "TEHLİKELİ" uyarısı var | GoodbyeDPI README |
| E9 | GoodbyeDPI yazarının `--wrong-seq` commit notu: yöntem "TTL değerlerinin değiştirilmesini yasaklayan ağlarda (tethering engeli/ücretli özelliği olan mobil ağlar)" işe yarayabilir | Commit `bbb6af89` (2021-12-25) |
| E10 | Hop limit 65 yöntemi hotspot gizleme amacıyla yaygın: dizüstünün 65 TTL'i telefondan geçince 64 olur ve telefonun kendi trafiğine benzer. Operatörler TTL ile tethering tespiti yapıyor | Çeşitli kaynaklar (doğrulanacak ayrıntı: Türk operatörleri) |
| E11 | Discord sesi ayrı UDP bağlantısı kullanır; GoodbyeDPI UDP'yi işlemez. zapret UDP/QUIC desync ve `--dpi-desync-fake-discord` destekler | GoodbyeDPI issue #276; zapret README |

**Ön gözlem (hipotez değil, kayıt):** Yerel kanıtlar, hop limit 65 iken **TTL'e bağlı olmayan** bir tekniğin (`-6`) web/API düzeyinde çalıştığını gösteriyor. "GoodbyeDPI çalışmıyor" gözlemi muhtemelen belirli bir kipe, DNS yöntemine veya trafik türüne (ses, oyun) özgüdür. Hangisi olduğu ölçülmeden bilinemez.

**Public depo notu:** §7'ye yalnızca genelleştirilmiş sonuçlar yazılır. Ağ adresi, log satırı ve paket yolu `CLAUDE.local.md` ve `lab/results/` (git dışı) içinde kalır.

## 3. Hipotezler

| # | Hipotez | Destekleyen | Test | Destek ölçütü / çürütme ölçütü |
|---|---|---|---|---|
| H1 | Başarısız olan yapılandırma TTL'e bağlı bir sahte paket kipiydi (`-5`/`--auto-ttl`/`--set-ttl`). Mobil yolda bu kipteki sahte paketler düşürülüyor, normalleştiriliyor veya DPI'a ulaşmıyor | E8, E9 | Aynı ağda, hop limit 65 iken `-5`, `-6`, `-7`, `-8`, `-9`; her biri 3 tekrar | `-5` başarısız, `-6` başarılı → destek. `-5` başarılı → çürür |
| H2 | Karıştırıcı değişken: TTL değişikliğiyle aynı dönemde ağ da değişti (ev ağı → iPhone hotspot); fark operatör DPI'ından kaynaklanıyor | Kullanım bağlamı | Başka bir ağ (ev Wi-Fi/Ethernet) varsa aynı matris. Yoksa kullanıcıdan TTL değişikliği öncesi hangi ağı kullandığı sorulur | Ev ağında 65 ve 128 arasında fark yoksa ama hotspot'ta varsa → ağ kaynaklı |
| H3 | DNS yolu: hazır betikler DNS'i şifresiz biçimde standart dışı porta (Yandex `77.88.8.8:1253`) yönlendiriyor. Hotspot/operatör yolunda bu UDP trafiği engelleniyor veya DNS telefonda önbellekleniyor ya da değiştiriliyor | E3, E5 (DoH ile çalışıyor) | Hedef alan adları için sistem DNS, DoH ve `77.88.8.8:1253` yanıt karşılaştırması; motor kapalı | DoH dışındakiler başarısız veya farklı yanıt → destek |
| H4 | Hatalı checksum'lı sahte paketler iPhone NAT'ında (pf) düşüyor ve DPI'a hiç ulaşmıyor | E8 (`--wrong-chksum` notu) | `-7` tek başına; mümkünse telefon tarafı gözlem yok, yalnız sonuç | `-7` başarısız, `-6` başarılı → destek (H1 ile birlikte yorumlanır) |
| H5 | "Çalışmıyor" gözlemi web değil, UDP trafiğiyle ilgili (Discord sesi, Roblox oyun). GoodbyeDPI bunları kapsamıyor | E11 | Discord masaüstü sesli kanal, Roblox oyuna katılım; web probları geçerken | Web geçiyor, ses/oyun başarısız → destek |
| H6 | Yerel durum: DPI servisi durmuş veya çökmüş, zamanlanmış görev tetiklenmemiş, başka bir WinDivert istemcisi var, Defender müdahalesi | E4 (yerel) | Paket kaldırıldığı için canlı durum gözlenemez. Tarihsel kanıt: olay günlüğü (Service Control Manager 7031/7034, WinDivert), Defender koruma geçmişi, paketten kalan bağlantı logları (`CLAUDE.local.md`) | Hata kaydı varsa destek; kanıt yetersizse belirsiz |
| H7 | Operatör tethering olarak tanınan trafiği (TTL 127) farklı bir politika veya yoldan geçiriyor. 65 ile trafik "telefonun kendi trafiği" yoluna giriyor ve bu yolda DPI davranışı farklı | E10 (bağlam) | **Lab, D5 onayıyla**: kısa süreli 128 ↔ 65 karşılaştırması; aynı matris. Bekçi ≤ 60 sn içinde 65'e döner | 128'de çalışan kip 65'te başarısızsa (veya tersi) → destek |
| H8 | GoodbyeDPI'ın otomatik TTL mantığı, gelen paket TTL'inden mesafe tahmin eder (128/64 − TTL). Giden TTL'in (65) bu hesaba doğrudan etkisi olmamalı | E8 | H1 testindeki `-5` sonuçları, `--auto-ttl` ve `--set-ttl N` (N = 3…8) taraması | Giden TTL'den bağımsızlık beklenir; aksi görülürse ayrıca incelenir |
| H9 | MTU veya parçalama: hücresel yolda etkin MTU daha düşük; ters parçalama ve `--max-payload` etkileşimi | — | `-6` ile `--native-frag` / `--reverse-frag` / `--frag-by-sni` varyantları; Path MTU ölçümü | Varyantlar arasında anlamlı fark → destek |

Yeni hipotez çıkarsa tabloya eklenir. Her hipotezin sonucu §7'ye kanıtıyla yazılır.

## 4. Lab düzeneği tasarımı (`xtask lab`)

**Güvenlik**
- Her senaryo başlamadan önce kullanıcıya şunlar gösterilir ve onay alınır: ne değişecek, ne kadar sürecek, geri alma nasıl olacak.
- Başlangıç anlık görüntüsü alınır: hop limit v4/v6, DNS, DoH tablosu, servisler, görevler, WinDivert durumu.
- Değişiklikler journal'a yazılır. Ayrı bir bekçi işlemi kurulur: süre (varsayılan 60 sn) dolunca başlangıç durumuna döner.
- Upstream ikililer resmi GitHub sürümlerinden indirilir. SHA-256 değerleri `lab/pins.toml` dosyasında sabittir. Ayrı bir dizinde çalıştırılır, iş bitince silinir.
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
3. **S2 — Upstream GoodbyeDPI kip matrisi** (hop limit 65): `-5`, `-6`, `-7`, `-8`, `-9`, `-6 --frag-by-sni`, `-6 --native-frag`; her biri DNS modu {sistem, DoH} ile 3 tekrar (H1, H4, H8, H9).
4. **S3 — zapret (winws) karşılaştırması:** TCP stratejileri (fake, multisplit, fakedsplit varyantları) ve UDP stratejileri (QUIC fake, Discord). Lisans doğrulandıktan sonra yalnızca ölçüm için kullanılır (H5).
5. **S4 — Gerçek istemciler (el ile, yönergeli):** Discord masaüstü (giriş, metin, sesli kanal), Roblox (oyuna katılım). En iyi 2 yapılandırmayla (H5).
6. **S5 — D5 onaylanırsa:** S2'nin özeti, hop limit 128 ile ≤ 10 dk içinde tekrarlanır. Bekçi 65'e döner (H7).
7. **S6 — Başka ağ varsa:** S1–S2 tekrarlanır (H2).
8. **Rapor:** Hipotez tablosu (destek / çürüdü / belirsiz + kanıt), Faz 2 için başlangıç strateji kataloğu ve sıralama önerisi.

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

| Hipotez | Durum | Kanıt (dosya/ölçüm) | Not |
|---|---|---|---|
| H1 | | | |
| H2 | | | |
| H3 | | | |
| H4 | | | |
| H5 | | | |
| H6 | | | |
| H7 | | | |
| H8 | | | |
| H9 | | | |

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
- **AV etiketleri:** WinDivert, bazı ürünlerde yüksek riskli araç veya HackTool olarak işaretlenebiliyor (zapret-win-bundle notu). Sigma kurallarında WinDivert sürücü yüklemesi bir algılama kuralı. Kurumsal ortamlarda alarm üretebilir; uygulama bunu açıkça belirtir.
- **Sürücü imzası:** WinDivert 2.2.0 varyantları (A/B/C/D) yalnızca sürücü imzası bakımından farklı. Dolaşımdaki 2.2 sürücülerinden biri Cloudveil Technology Inc. EV imzası taşıyor (2019 zaman damgası). Dağıtılan veya makinede bulunan sürücüler resmi arşivle hash bazında karşılaştırılır.
- **GoodbyeDPI'ın son sürümü:** 0.2.3rc3, 2024-09-14 (GitHub API). 2026-11-09 itibarıyla daha yeni sürüm yok; depodaki son commit 2026-01-19.

## 9. Faz 2 için ön tasarım notları

- Başlangıç sıralaması kanıtlara dayanır. Bu ağda ilk aday: DoH aktarıcı + wrong-seq sahte paket + ters segmentasyon (E5/E6). Son karar Faz 0 sonuçlarından sonra verilir.
- Puanlama: başarı zorunludur; sonra kararlılık (3/3), p95 el sıkışma süresi, yönlendirilen paket/sn (CPU vekili). Eşitlikte daha az müdahaleci teknik seçilir.
- Profil anahtarı: ağ parmak izi hash'i + hop limit + IPv6 varlığı. Profil 30 gün kullanılmazsa önceliği düşer.
