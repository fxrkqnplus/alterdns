# AlterDNS — Test ve Doğrulama Stratejisi

## 1. İlkeler

- Her kabul ölçütü en az bir otomatik teste veya tekrarlanabilir bir ölçüm betiğine bağlıdır. El ile yapılan testlerin adımları ve beklenen sonuçları yazılıdır.
- Sistemi değiştiren testler yalnızca VM'de veya `xtask lab` / `dev-install` düzeneğinde çalışır. Bu testler journal ve bekçi ile korunur.
- **Makineye özgü değerler (ADR-014):** Testler kullanıcıya ya da başka yazılıma ait değerleri (hop limit, DNS/DoH, proxy, MTU, rotalar, güvenlik duvarı…) değiştirmez; yalnızca AlterDNS'in kendi nesnelerini ekleyip kaldırır. Her lab senaryosu başta ve sonda bu değerlerin anlık görüntüsünü alır; fark testi başarısız yapar. Farklı değerlerin etkisi kendi soketimizde (ör. `IP_TTL`) veya uyumluluk matrisinde ölçülür.
- Testler deterministiktir. Ağa bağlı testler (`#[ignore]` + `--features net-tests`) ayrı koşulur ve sonuçları zaman damgalı kaydedilir.
- "Bitti" demeden önce şunlar çalıştırılır ve çıktıları rapora eklenir: `cargo xtask check`, `cargo xtask perf`.

## 2. Katmanlar ve araçlar

| Katman | Araç | Nerede |
|---|---|---|
| Birim | `cargo test` / `cargo nextest` | Her commit |
| Özellik tabanlı | `proptest` (paket kurucular, URL normalize, arşiv sınırları, karar motoru) | Her commit |
| Fuzz | `cargo-fuzz` (libFuzzer). Platformdan bağımsız crate'ler WSL/Linux'ta çalışır | CI'da kısa tur (her hedef 2 dk), gecelik/faz sonu uzun tur (≥ 1 saat) |
| Entegrasyon | Servis, IPC, journal, sandbox aracısı | Windows CI + yerel |
| Sistem | Kurulum/kaldırma anlık görüntü karşılaştırması | Hyper-V VM (checkpoint'ten döner) |
| Ağ lab | `xtask lab` senaryoları | Kullanıcı makinesi (onaylı) + VM |
| Uzantı | `vitest` (birim), Playwright (E2E; Chromium/Chrome for Testing ile paketlenmemiş uzantı) + kurulum sihirbazı için el ile test listesi | Her commit (E2E: PR) |
| Performans | `xtask perf` | Faz sonu + PR'da regresyon karşılaştırması |
| Güvenlik | Özel testler (§7) + güvenlik incelemesi | Faz sonu |
| Erişilebilirlik | Klavye gezinmesi, ekran okuyucu adları, kontrast, azaltılmış hareket | Faz 1 ve 6 |

## 3. Test kimlikleri (başlangıç listesi; planlarda genişletilir)

### Çekirdek
- **T-ERR-001** Katalogdaki her kod benzersiz ve şemaya uygun.
- **T-ERR-002** Her kodun TR ve EN alanları dolu (başlık, mesaj, en az 1 neden, en az 1 çözüm).
- **T-ERR-003** Kodda kullanılan her kod katalogda var. Katalogdaki her kod en az bir testte üretiliyor.
- **T-IPC-001** Yetkisiz işlem (kurulum dizini dışından exe) ayrıcalıklı yöntemi çağırınca `ADNS-IPC-003` döner.
- **T-IPC-002** Pipe squatting: servisten önce aynı adla pipe açan işlem varsa servis başlamaz ve `ADNS-IPC-005` verir.
- **T-IPC-003** 1 MB üstü mesaj reddedilir; bozuk JSON servisi çökertmez (fuzz).
- **T-IPC-004** Sürüm uyuşmazlığında `ADNS-IPC-004` döner.
- **T-JRN-001** Uygula → geri al sonrası kayıt defteri, servis ve DNS değerleri `before` ile aynı.
- **T-JRN-002** `Pending` kayıtla işlem öldürülür; açılışta kurtarma geri alır.
- **T-JRN-003** Sahiplik: kullanıcı değeri arada değiştirdiyse geri alma bu değeri ezmez ve kullanıcıya sorar.
- **T-JRN-004** Bekçi: ana işlem öldürülünce süre dolduğunda txn geri alınır.
- **T-SVC-001…004** Kur, başlat, durdur, kurtarma ayarları, kaldır.
- **T-UI-001** Flyout odak kaybında kapanır (başka pencereye tıklama, masaüstüne tıklama, Win tuşu).
- **T-UI-002** Flyout sürüklenemez: `WM_NCHITTEST`, Alt+Space menüsü ve `SC_MOVE` denemeleri.
- **T-UI-003** Görev çubuğu dört kenardayken ve otomatik gizliyken flyout simgeye hizalı ve çalışma alanı içinde.
- **T-UI-004** DPI 100/125/150/200 ve monitörler arası taşıma (PerMonitorV2).
- **T-UI-005** Ana pencere imlecin bulunduğu monitörün ortasında açılır, büyütme devre dışı, en küçük boyut korunur.
- **T-UI-006** Klavye: flyout'ta Tab sırası, Enter/Space, Esc. Tepsiye klavyeyle erişim (Win+B).
- **T-UI-007** Azaltılmış hareket ayarı açıkken animasyon yok.
- **T-UI-008** Açık/koyu tema geçişi anlık uygulanır; tepsi simgesi doğru varyanta geçer.

### Bağlantı
- **T-DPI-001…** Her teknik için pcap fikstürü (girdi paketi → beklenen çıktı paketleri; checksum'lar dahil). IPv4 ve IPv6.
- **T-DPI-020** SNI'ın bölündüğü ClientHello'da SNI uzunluk sınırları ve ECH/GREASE uzantıları doğru işleniyor.
- **T-DPI-030** Akış tablosu bellek sınırını aşmıyor (100k akış simülasyonu).
- **T-TUN-001** Zaman sınırları: prob 4 sn, deneme 12 sn, toplam 90 sn aşılmıyor (sahte prob ile).
- **T-TUN-002** Başarısız deneme sonrası journal boş, sistem durumu başlangıçla aynı.
- **T-TUN-003** Ölçülü bağlantıda veri tüketen ölçüm yapılmıyor.
- **T-TUN-004** Genel internet probu başarısızsa motor kapanıyor ve `ADNS-NET-010` veriliyor.
- **T-NET-001** Ağ olayları debounce ediliyor; aynı ağa dönüşte kayıtlı profil ≤ 5 sn içinde uygulanıyor.
- **T-NET-002** Parmak izi kişisel veri sızdırmıyor (yalnız hash'ler).
- **T-NET-003** Hop limit okunuyor ve **hiçbir kod yolu yazmıyor** (lab modu hariç; statik denetim + test).
- **T-DNS-001** DoH aktarıcı yalnızca listedeki alan adlarına yanıt enjekte ediyor; diğer DNS trafiğine dokunmuyor.
- **T-DNS-002** Sistem DoH modu journal'lı; geri alma eski sunuculara ve şablonlara dönüyor.
- **T-REC-001** Servis `TerminateProcess` ile öldürülünce trafik ≤ 1 sn içinde normale dönüyor.
- **T-LAB-001…006** Faz 0 araştırma senaryoları (`CONNECTIVITY_RESEARCH.md` §5).
- **T-LAB-010…015** Gerçek istemciler: Discord masaüstü (giriş, metin, ses), Roblox (oyuna katılım), engelli web siteleri, kontrol siteleri. El ile adımları yazılı.

### Dosya güvenliği
- **T-SCN-001** Tür tespiti uzantıya değil içeriğe bakıyor (yanlış uzantılı 50 örnek).
- **T-SCN-002** Arşiv sınırları: derinlik 6 → "incelenemedi: derinlik". Oran > 100× → durdurma. 10.001 öğe → durdurma. `../` ve mutlak yollar → bulgu, dışarı yazma yok.
- **T-SCN-003** Şifreli arşiv → "incelenemedi: şifreli"; kullanıcı parola girerse yeniden tarama.
- **T-SCN-004** Polyglot JPEG+ZIP, PNG sonrası PE, MP3 sonrası arşiv → bulgu.
- **T-SCN-005** RTLO, çift uzantı, uzantı-içerik uyuşmazlığı → bulgu.
- **T-SCN-006** Makrolu belge (zararsız AutoOpen), DDE alanı, harici şablon → `Şüpheli`.
- **T-SCN-007** LNK → `powershell -enc …` → `Şüpheli`/`Riskli` (kurala göre).
- **T-SCN-008** İmzalı Windows ikilileri → `Güvenli`; imza bilgisi raporda.
- **T-SCN-009** İşçi zaman aşımı veya çökme → ilgili parça "incelenemedi" olur, rapor yine üretilir, kod `ADNS-SCN-0xx`.
- **T-SCN-010** Karar tablosu testleri (her kural için en az bir örnek).
- **T-SCN-011** EICAR (VM, `eicar-tests` özelliği): düz, zip, 3 seviye iç içe → `Riskli`.
- **T-SBX-001** İşçi ağa bağlanamıyor, kullanıcı dosyalarını açamıyor, alt işlem başlatamıyor, bellek sınırını aşınca sonlanıyor.
- **T-QRN-001** Karantina → dosya yok, karantina hash'i doğru, geri yükleme özgün içeriği veriyor.
- **T-QRN-002** Junction/symlink ile özgün yolun hedefi değiştirilse bile yanlış dosya silinmiyor (TOCTOU).
- **T-QRN-003** Kilitli dosya → yeniden başlatmada silme planlanıyor; kullanıcı bilgilendiriliyor.
- **T-FUZZ-001…010** Ayrıştırıcı hedefleri: PE, LNK, OLE, OOXML, PDF, ZIP, 7z, TAR, görsel başlıkları, HTML/SVG tarayıcı, URL normalize.

### Tarayıcı
- **T-REP-001** Besleme bozuk veya boş gelirse son iyi bilinen kopya korunuyor ve `ADNS-REP-0xx` veriliyor.
- **T-REP-002** IDN/punycode, büyük-küçük harf, sondaki nokta, `www.` ve alt alan adı eşleşme kuralları.
- **T-REP-003** Sorgu süresi p99 < 1 ms (500k girdi).
- **T-NMH-001** Bilinmeyen origin'den gelen bağlantı reddediliyor; şema dışı mesaj reddediliyor.
- **T-EXT-001** DNR kapsamındaki engelli adres → engelleme sayfası; yerel test sunucusu **0 istek** kaydediyor.
- **T-EXT-002** "Yine de devam et" yalnızca o sekme ve oturum için geçerli.
- **T-EXT-003** Uygulama yokken uyarı gösteriliyor, son kurallar çalışıyor.
- **T-EXT-004** Servis çalışanı yeniden başlatıldıktan sonra durum korunuyor.
- **T-EXT-005** Engelleme sayfası erişilebilirlik denetimi (otomatik + el ile).
- **T-EXT-006** Paketlenmemiş kurulum: "Tarayıcıda kur" sihirbazıyla Chrome ve Edge'de kurulum adımları (el ile test listesi). `hello` alınınca "Bağlandı" görünür.
- **T-EXT-007** Sabit kimlik: farklı makinede ve farklı yoldan yüklemede bile kimlik aynı. Native host başka kimlikle gelen bağlantıyı reddeder.
- **T-EXT-008** Güncelleme: uzantı dosyaları atomik değişir; uzantı yeni sürüme geçer (`runtime.reload` veya yönerge). Eski ve yeni dosyalar karışmaz.
- **T-EXT-009** Devre dışı kalma: uzantı tarayıcıdan kapatılınca (tarayıcı açıkken) belirlenen süre sonunda `ADNS-WEB-005` bildirimi ve yönerge gelir.
- **T-EXT-010** Uzantı klasörü kullanıcı haklarıyla değiştirilemez (ACL testi).
- **T-EXT-011** Geliştirici modu kurumsal ilkeyle kapalıysa `ADNS-WEB-006` gösterilir.

### Kurulum ve güncelleme
- **T-INS-001** 8 bileşen kombinasyonu × (kur, bileşen ekle, bileşen kaldır, kaldır-A, kaldır-B). Her senaryodan sonra VM anlık görüntü farkı alınır.
- **T-INS-002** Kaldırma: kullanıcının hop limiti, DNS ayarları ve önceden var olan WinDivert servisi değişmemiş.
- **T-INS-003** Yalnızca seçilen bileşenler indirildi (indirme logu ve ağ sayacı).
- **T-UPD-001** Geçerli imza ve hash → güncelleme başarılı.
- **T-UPD-002** Bozuk imza, yanlış hash, süresi dolmuş manifest veya eski sürüm → reddedilir ve hata kodu verilir.
- **T-UPD-003** İndirme yarıda kesilir → devam eder veya yeniden dener; bozuk dosya kurulmaz.
- **T-UPD-004** `current` değişiminde güç kesintisi simülasyonu → açılışta tutarlı sürüm.
- **T-UPD-005** Sağlık kontrolü başarısız → otomatik geri dönüş ve `ADNS-UPD-0xx`.
- **T-UPD-006** Manifest, `releases/latest/download` yolundan API kullanmadan alınıyor. 403/429 yanıtında geri çekilme uygulanıyor ve `ADNS-UPD-007` veriliyor.

### Yayın hattı
- **T-REL-001** Yeniden üretilebilirlik: CI ve yerel imzasız ikililerin SHA-256 değerleri aynı. Değilse fark raporlanır.
- **T-REL-002** Her sürüm varlığı için `gh attestation verify <dosya> -R fxrkqnplus/alterdns` başarılı.
- **T-REL-003** `release-manifest.json` Ed25519 imzası doğrulanıyor. Manifestteki hash'ler, imzalı (son hâl) dosyalarla eşleşiyor.
- **T-REL-004** SBOM ve THIRD_PARTY_NOTICES eksiksiz. `cargo deny check licenses` lisans politikasına uygun.
- **T-REL-005** Release iş akışı yalnız korumalı `release` ortamında ve onayla çalışıyor. PR'lardan gizli değişkenlere erişim yok.
- **T-REL-006** SignPath etkinleştirildikten sonra: AlterDNS ikililerinde Authenticode geçerli, ürün adı "AlterDNS", sürüm bütün ikililerde aynı. Üçüncü taraf ikililer kendi imzalarıyla kalıyor.

### Spike ve performans
- **T-SPK-001** UI spike ölçüm tablosu (ADR-002 ölçütleri).
- **T-SPK-002** Wintun (D8): resmi arşivin hash'i, imzası ve Defender tepkisi; adaptör HVCI açıkken oluşuyor; tek IP rotası yalnızca o IP'yi etkiliyor; kullanıcı modu TCP/UDP iletimi; CPU/RAM/gecikme; spike işlemi öldürülünce rota ve adaptör ≤ 60 sn içinde kalkıyor. (İlk hâli WinDivert 2.2.2-A içindi; Defender tespiti nedeniyle değişti.)
- **T-SPK-003** Sandbox: işçinin dosya sistemi, ağ ve alt işlem erişimi yok.
- **T-SPK-004** DNR kapasitesi (Faz 4): `requestDomains` paketleme sınırı, bellek, güncelleme süresi.
- **T-PERF-001** Servis boşta private bytes ≤ 12 MB.
- **T-PERF-002** Servis boşta CPU ≤ %0,1 ve 60 sn'den sık uyanma yok.
- **T-PERF-003** Tepsi ajanı boşta ≤ 8 MB; flyout açık ≤ 45 MB; açılış p95 ≤ 150 ms (sıcak), ≤ 400 ms (soğuk).
- **T-PERF-004** Bağlantı motoru iş hacmi kaybı ≤ %2.
- **T-PERF-005** El sıkışma gecikmesi artışı p95 ≤ 5 ms; kararlı durumda RTT değişimi ≤ 1 ms.
- **T-PERF-006** Motor etkin, boşta CPU ≤ %0,1.
- **T-PERF-007** Tarama hızı ≥ 50 MB/sn (düz dosya, hash + YARA).
- **T-PERF-008** Tarama işçisi bellek sınırı zorunlu (256 MB aşımı → sonlanma + `ADNS-SCN-003`).
- **T-PERF-009** İtibar sorgusu p99 < 1 ms; uzantı kural senkronizasyonu tarayıcıyı bloklamıyor.
- **T-PERF-010** Ana pencere ≤ 90 MB, ilk çizim ≤ 600 ms; kapanınca işlem sonlanıyor.

## 4. Test verisi ve zararlı örnek politikası

- **Zararsız derlem:** `C:\Windows\System32` örnekleri (salt okunur kopya), yaygın imzalı uygulama kurulum dosyaları, kamu malı belgeler ve medya dosyaları. Derlem depoya konmaz; `xtask corpus fetch` ile hash listesinden yeniden kurulur.
- **Sentetik şüpheli derlem:** Test sırasında üretilir (makrolu belge, LNK, polyglot, bomba). Üretim betikleri depodadır; çıktılar depoya girmez.
- **EICAR:** Depoya düz metin yazılmaz; test sırasında parçalardan birleştirilir. Yalnızca VM'de ve `eicar-tests` özelliğiyle çalışır. Defender'ın EICAR'ı yakalaması beklenen bir davranıştır; test, Defender tespitini de doğrulayacak şekilde tasarlanır.
- **Gerçek zararlılar:** Yalnızca ağı kapalı, paylaşılan klasörü olmayan ve checkpoint'ten dönen izole bir VM'de kullanılır. Örnekler MalwareBazaar'dan şifreli arşiv olarak alınır. Ana makineye hiçbir zaman açılmış hâlde gelmez. Sonuçlar oran olarak raporlanır; tespit garantisi verilmez.

## 5. Performans ölçümü

**Ortam:**
- Referans makine: kullanıcının bilgisayarı.
- Düşük kaynaklı hedef: 2 vCPU, 4 GB RAM Hyper-V VM.
- Her ölçüm 5 tekrarın medyanıdır. Isınma turu sayılmaz.

| Ölçüt | Yöntem |
|---|---|
| Private bytes / working set | `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX.PrivateUsage`), 10 dk boyunca 10 sn aralıkla örnekleme; medyan ve en yüksek değer |
| Boşta CPU | `QueryProcessCycleTime` farkı / 10 dk; ayrıca WPR `CPU usage (precise)` ile uyandırma sayısı |
| Zamanlayıcı ve uyanma | WPR + WPA (`Generic Events` / `CPU Precise`); boşta 60 sn'den sık uyanma yok |
| İş hacmi kaybı | Aynı sunucuya 3 × 100 MB HTTPS indirme (ölçülü olmayan ağda veya VM'de yerel sunucu), modül açık/kapalı. Mobil veride ancak kullanıcı onayıyla ve küçük boyutla |
| El sıkışma gecikmesi | 200 TLS bağlantısı, `connect → ClientHello → ServerHello` süresi; p50/p95 |
| Flyout açılışı | Tıklama olayından ilk çerçevenin ekrana gelmesine kadar (ETW özel olayları + DWM present); 50 tekrar, p95 |
| Tarama hızı | 1 GB karışık derlem; MB/sn ve dosya/sn |
| Disk | Kurulum dizini boyutu; boşta saatlik yazılan bayt (`IO Write Bytes`) |

`xtask perf` sonuçları `perf/results/<tarih>.json` olarak kaydedilir. Bir önceki tabana göre > %10 kötüleşme CI'da başarısızlık sayılır.

## 6. Uyumluluk matrisi (Faz 6; Faz 2'den itibaren kısmi)

**Eksenler:**
- Windows sürümü: Win10 22H2, Win11 23H2, 24H2, 25H2, 26H2
- HVCI: açık / kapalı
- Smart App Control: açık / kapalı
- AV: yalnız Defender / üçüncü taraf (en az bir)
- Diğer WinDivert kullanıcısı: yok / GoodbyeDPI 0.2.3rc3 (2.2.0) / zapret / VPN istemcisi
- Ağ: ev Wi-Fi, Ethernet, iPhone hotspot, Android hotspot, USB tethering, IPv6-only/NAT64, captive portal
- Makinenin kendi hop limiti: 64, 65, 128 (değiştirilmez; farklı değerler farklı makinelerde/VM'lerde bulunur, ADR-014)
- Proxy: yok / WinINET elle / PAC / WinHTTP
- Başka VPN veya TUN: yok / WireGuard (Wintun) / OpenVPN (TAP) / `198.18.0.0/15` kullanan bir VPN
- IPv6: yok / var; MTU: 1500 / 1400 / 1280
- Adaptör sayısı: 1 / 2+ (Wi-Fi + Ethernet, Hyper-V/WSL vSwitch)
- Mimari: x64 (ARM64'te kurulum açık bir hata koduyla durmalı)

Bütün kombinasyonlar denenmez; çift yönlü (pairwise) kapsama ile seçilen alt küme kullanılır. Geliştirici makinesinin yapılandırması (HVCI açık, telefon hotspot'u, hop limit 65) her zaman matristedir.

- **T-CMP-001** (Yalnızca WinDivert modu geri gelirse; D8.) Aynı adlı `WinDivert` servisi başka bir sürücü yoluna bağlıyken (GoodbyeDPI kurulu VM; sürücü yüklü ve yüklü değil) AlterDNS bağlantı modülünün davranışı: hangi sürücüyü kullandığı, sürüm uyumsuzluğunda verilen hata kodu, kaldırmada diğer aracın servisine dokunulmaması.

## 7. Güvenlik testleri

- IPC yetkilendirme, pipe squatting, mesaj fuzz.
- DLL arama sırası: bütün exe'ler `SetDefaultDllDirectories` kullanır; kurulum dizini yazılabilir değildir. Uygulama dizinine kötü amaçlı DLL bırakma testi yapılır.
- Servis yolu tırnaklı; servis ve kurulum dizini ACL denetimi.
- Güncelleme: imza atlatma, sürüm düşürme, dondurma (eski manifest), kısmi indirme, MITM (geçersiz sertifika).
- Arşiv: yol aşımı, mutlak yol, NTFS ADS adları, cihaz adları (`CON`, `NUL`), çok uzun yollar.
- Karantina ve silme: junction, symlink, hardlink; TOCTOU.
- Native host: origin doğrulaması, şema dışı mesaj, büyük mesaj, hızlı tekrar (rate limit).
- Uzantı: engelleme sayfasında XSS (URL parametresi), CSP.
- Gizli bilgi taraması: depoda token, anahtar ve EICAR yok (`xtask check` içinde).

## 8. "Hatasızlık" ölçütleri (yayın kapısı)

Hiçbir yazılımın hatasız olduğu kanıtlanamaz. AlterDNS için yayın kapısı ölçülebilir hedeflerle tanımlanır:

1. Bütün otomatik testler yeşil. El ile test listesi işaretli ve kanıtlı.
2. `clippy -D warnings`, `cargo deny check`, `cargo fmt --check` temiz. Test dışı kodda `unwrap`/`expect`/`panic` yok.
3. Fuzz hedefleri faz sonu uzun turda çökme olmadan tamamlandı.
4. Güvenlik incelemesinde yüksek veya kritik açık bulgu yok.
5. Performans bütçeleri karşılandı; karşılanmayanlar gerekçeli ve kullanıcı onaylı.
6. Uyumluluk matrisinde her hücre "geçti" veya "bilinen sınırlama + hata kodu + kullanıcı yönergesi" olarak kayıtlı.
7. Bilinen açık hata sayısı 0. Ertelenen iyileştirmeler ayrı listede.
