# AlterDNS — Hata Kataloğu

> Bu dosya `errors/catalog.toml`'dan üretilir; elle düzenlemeyin. Yeniden üretmek için: `cargo xtask docs-errors`.

Bu belge kod şemasını ve kataloğu tanımlar. **Tek kaynak `errors/catalog.toml` dosyasıdır**; İngilizce metinler de oradadır. §3 o dosyadan üretilir.

## 1. Şema

```
ADNS-<ALAN>-<NNN>
```

| Alan | Kapsam |
|---|---|
| `GEN` | Genel / beklenmeyen |
| `IPC` | Servis ↔ istemci iletişimi |
| `SVC` | Windows servisi |
| `SYS` | Sistem değişiklikleri ve journal |
| `NET` | Ağ durumu ve otomatik ayar |
| `DPI` | Paket motoru ve sürücü |
| `DNS` | DNS çözümleme ve DoH |
| `SCN` | Dosya taraması |
| `QRN` | Karantina ve silme |
| `WEB` | Tarayıcı uzantısı ve native host |
| `REP` | İtibar verisi beslemeleri |
| `UPD` | Güncelleme |
| `INS` | Kurulum, bileşenler, kaldırma |
| `UI` | Arayüz |

**Numaralandırma:**
- `001–899` kullanıcıya gösterilen durumlardır. Önem (bilgi, uyarı, hata, kritik) ayrı bir alandır.
- `900–999` iç hatalardır ve kullanıcıya genel bir mesajla gösterilir.
- Bir kod yayımlandıktan sonra anlamı değiştirilmez.
- Kullanılmayan kod silinmez, `deprecated = true` olarak işaretlenir.

**Ek alanlar:**
- `name`: Rust'taki varyant adı, PascalCase ve benzersiz.
- `faz`: kodun ilk üretildiği faz. "Her kod en az bir testte üretilir" kuralı (T-ERR-003) yalnızca geçerli faza kadarki kodlara uygulanır.
- `params`: metinlerde `{ad}` biçiminde geçen yer tutucuların listesi. Doğrulayıcı ikisinin eşleştiğini denetler.

**Önem düzeyleri:** `Bilgi`, `Uyarı`, `Hata`, `Kritik`. Kritik hatalarda bildirim her zaman gösterilir ve ilgili modül güvenli duruma geçer.

## 2. TOML biçimi

```toml
[[error]]
code = "ADNS-DPI-001"
name = "DpiDriverOpenFailed"
severity = "error"            # info | warning | error | critical
area = "DPI"
faz = 2
params = []
retryable = true
user_action_required = true
docs = "errors#adns-dpi-001"

[error.tr]
title = "Paket sürücüsü açılamadı"
message = "Bağlantı koruması ağ sürücüsünü başlatamadı. İnternet bağlantınız etkilenmedi."
causes = ["Yönetici izni verilmedi", "Sürücü dosyası eksik veya değiştirilmiş", "Güvenlik yazılımı sürücüyü engelledi"]
solutions = ["AlterDNS'i yeniden başlatın", "Bileşenler sayfasından 'Bağlantı yönetimi'ni onarın", "Güvenlik yazılımınızın bildirimlerini kontrol edin; antivirüsü kapatmayın"]

[error.en]
title = "Packet driver could not be opened"
# …

[error.tech]                  # yalnızca log ve "Ayrıntıları kopyala"
fields = ["win32_error", "driver_path", "driver_version"]
```

**Kurallar:**
- Mesajlar suçlayıcı değildir.
- Ne olduğu, kullanıcıyı nasıl etkilediği ve ne yapılabileceği söylenir.
- Teknik ayrıntı (Win32 kodu, yol) yalnızca "Ayrıntıları kopyala" ile görünür.
- Çözümler uygulanabilir adımlardır. "Antivirüsü kapatın" gibi güvenliği azaltan öneriler **yasaktır**.

## 3. Katalog

### GEN

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-GEN-900 | Kritik | 1 | Beklenmeyen hata | Bir iç hata oluştu. İşlem güvenli biçimde durduruldu. | Yazılım hatası | Uygulamayı yeniden başlatın; Tekrarlarsa "Tanılama paketi oluştur" ile kaydı paylaşın |

### IPC

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-IPC-001 | Hata | 1 | AlterDNS hizmetine bağlanılamıyor | Arayüz arka plan hizmetine ulaşamadı. | Hizmet durdu; Hizmet başlatılıyor; Kurulum eksik | Birkaç saniye bekleyin; "Hizmeti yeniden başlat"; Bileşenler > Onar |
| ADNS-IPC-002 | Hata | 1 | Geçersiz istek | Hizmet isteği anlayamadı. | Sürüm farkı; Bozuk veri | Uygulamayı güncelleyin; Yeniden başlatın |
| ADNS-IPC-003 | Uyarı | 1 | Yetkisiz istek reddedildi | AlterDNS dışından gelen bir ayar değiştirme isteği engellendi. | Başka bir yazılım hizmete erişmeye çalıştı | İşlem gerekmiyor; Tekrarlıyorsa güvenlik taraması yapın |
| ADNS-IPC-004 | Uyarı | 1 | Sürüm uyuşmazlığı | Arayüz ile hizmet farklı sürümlerde. | Yarım kalan güncelleme | "Güncellemeyi tamamla"; Bileşenler > Onar |
| ADNS-IPC-005 | Kritik | 1 | Hizmet kanalı kullanımda | Hizmetin iletişim kanalı başka bir uygulama tarafından açılmış; güvenlik için hizmet başlatılmadı. | Kötü amaçlı veya hatalı yazılım | Bilgisayarı yeniden başlatın; Tekrarlarsa tam tarama yapın |

### SVC

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-SVC-001 | Hata | 1 | Hizmet başlatılamadı | AlterDNS hizmeti başlamadı; korumalar devre dışı. | Dosyalar eksik; İzinler bozuk; Başka hata | Bileşenler > Onar; Bilgisayarı yeniden başlatın |
| ADNS-SVC-002 | Uyarı | 1 | Hizmet yeniden başlatıldı | Hizmet beklenmedik şekilde durdu ve otomatik olarak yeniden başladı. | Yazılım hatası; Kaynak yetersizliği | İşlem gerekmiyor; Sık tekrarlarsa tanılama paketi oluşturun |

### SYS

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-SYS-001 | Hata | 1 | Ayar uygulanamadı | İstenen sistem ayarı değiştirilemedi; hiçbir değişiklik yapılmadı. | Yönetici izni yok; Kurum ilkesi; Ayar kilitli | Yönetici olarak tekrar deneyin; Kurum cihazıysa BT yöneticinize danışın |
| ADNS-SYS-002 | Kritik | 1 | Değişiklik geri alınamadı | AlterDNS'in yaptığı bir ayar eski haline döndürülemedi. Ayrıntılar aşağıda. | Ayar kilitli; Dosya veya kayıt silinmiş | "Yeniden dene"; Olmazsa ekrandaki eski değeri el ile geri yükleme yönergesini izleyin |
| ADNS-SYS-003 | Uyarı | 1 | Ayar başka biri tarafından değiştirilmiş | Geri alınacak ayar AlterDNS'ten sonra değiştirilmiş; üzerine yazılmadı. | Kullanıcı veya başka uygulama değiştirdi | Mevcut değeri koru (önerilen) veya AlterDNS öncesi değeri geri yükle |
| ADNS-SYS-004 | Bilgi | 5 | Yönetici izni verilmedi | İşlem için Windows yönetici onayı gerekiyor; işlem iptal edildi. | UAC penceresi reddedildi | İşlemi yeniden başlatıp onay verin |

### NET

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-NET-001 | Uyarı | 2 | İnternet bağlantısı yok | Ağa bağlısınız ama internete erişilemiyor; bağlantı koruması beklemede. | Modem/telefon bağlantısı; Kota; Operatör sorunu | Telefonun veya modemin internetini kontrol edin; Bağlantı gelince otomatik devam edilir |
| ADNS-NET-002 | Bilgi | 2 | Giriş gerektiren ağ | Bu ağ tarayıcıda giriş istiyor; giriş yapılana kadar ayar yapılmaz. | Otel/kafe ağı (captive portal) | Tarayıcıda ağın giriş sayfasını tamamlayın |
| ADNS-NET-003 | Bilgi | 2 | VPN veya proxy etkin | VPN/proxy açıkken bağlantı koruması devre dışı kalır. | VPN istemcisi; Sistem proxy'si | Gerekirse VPN'i kapatın; AlterDNS otomatik devam eder |
| ADNS-NET-004 | Bilgi | 2 | Ölçülü bağlantı | Veri tasarrufu için hız ölçümleri yapılmadı; yalnızca kısa kontroller kullanıldı. | Mobil veri / ölçülü ağ | İşlem gerekmiyor; Ayarlar'dan değiştirilebilir |
| ADNS-NET-010 | Hata | 2 | Bağlantı koruması kapatıldı | Etkinleştirilen yapılandırma genel internet erişimini bozdu; değişiklikler geri alındı. | Ağ yapılandırmayla uyumsuz | "Yeniden ayarla"; Sorun sürerse tanılama paketi oluşturun |
| ADNS-NET-011 | Uyarı | 2 | Uygun yapılandırma bulunamadı | Bu ağda denenen yöntemlerin hiçbiri hedef sitelere erişimi sağlamadı. | Engel türü desteklenmiyor; Ağ yeni yöntem kullanıyor | Daha sonra yeniden deneyin; Strateji güncellemelerini bekleyin |
| ADNS-NET-012 | Bilgi | 2 | Engel IP tabanlı görünüyor | Hedefe giden bağlantı IP düzeyinde engelleniyor; bu yöntemle aşılamaz. | IP engeli | VPN/proxy olmadan çözüm yok; AlterDNS bu engeli aşmaz |
| ADNS-NET-013 | Uyarı | 2 | Ayar araması süre sınırına ulaştı | Otomatik ayar zaman sınırında tamamlanamadı; önceki güvenli durum korundu. | Yavaş/kararsız bağlantı | Bağlantı iyileşince "Yeniden ayarla" |

### DPI

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-DPI-001 | Hata | 2 | Paket sürücüsü açılamadı | Bağlantı koruması ağ sürücüsünü başlatamadı. İnternet bağlantınız etkilenmedi. | Yönetici izni; Sürücü dosyası eksik veya değişmiş; Güvenlik yazılımı engeli | Bileşenler > Onar; Güvenlik yazılımı bildirimlerini kontrol edin (antivirüsü kapatmayın) |
| ADNS-DPI-002 | Hata | 2 | Sürücü Windows tarafından engellendi | Sürücü imzası veya bellek bütünlüğü ilkesi nedeniyle yüklenemedi. | İmza doğrulanamadı (577); Kurum WDAC ilkesi; Sürücü engel listesi | Bileşenler > Onar; Kurum cihazıysa BT yöneticinize danışın; Bellek bütünlüğünü kapatmayın |
| ADNS-DPI-003 | Uyarı | 2 | Başka bir paket aracı çalışıyor | GoodbyeDPI, zapret veya benzeri bir araç aynı anda çalışıyor; çakışma olabilir. | Eski DPI aracı; VPN istemcisi | Diğer aracı geçici olarak durdurmayı onaylayın veya AlterDNS bağlantı modülünü kapalı tutun |
| ADNS-DPI-004 | Hata | 2 | Sürücü sürüm çakışması | Sistemde farklı sürümde bir WinDivert sürücüsü yüklü. | Başka uygulamanın sürücüsü | Diğer uygulamayı kapatıp yeniden deneyin; Bilgisayarı yeniden başlatın |
| ADNS-DPI-005 | Hata | 2 | Desteklenmeyen işlemci | Bağlantı modülü ARM64 Windows'ta desteklenmiyor. | ARM64 cihaz | Diğer modüller kullanılabilir |
| ADNS-DPI-006 | Uyarı | 2 | Paket motoru yeniden başlatılıyor | Motor hata verdi; trafik normal akıyor, motor yeniden başlatılıyor. | Geçici sürücü/ağ hatası | İşlem gerekmiyor; Tekrarlarsa tanılama paketi |

### DNS

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-DNS-001 | Uyarı | 2 | Şifreli DNS'e ulaşılamıyor | Seçilen şifreli DNS sağlayıcısına bağlanılamadı; başka sağlayıcı deneniyor. | Sağlayıcı engelli veya kapalı | İşlem gerekmiyor; Ayarlar'dan sağlayıcı seçilebilir |
| ADNS-DNS-002 | Bilgi | 2 | DNS yanıtları tutarsız | Bu ağın DNS yanıtları şifreli DNS ile uyuşmuyor; DNS tabanlı engel olabilir. | DNS engeli/yönlendirmesi | AlterDNS gerekirse alan adına özel şifreli çözümleme kullanır |
| ADNS-DNS-003 | Hata | 2 | DNS ayarı değiştirilemedi | Sistem DNS ayarı uygulanamadı; ayarlar değişmedi. | Kurum ilkesi; Yönetici izni | Alan adına özel modu kullanın (sistem ayarı değiştirmez) |

### SCN

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-SCN-001 | Hata | 3 | Güvenli inceleme ortamı başlatılamadı | Dosya, korumalı ortam başlamadığı için incelenmedi. Dosya açılmadı. | AppContainer profili bozuk; Sistem kaynakları | Bileşenler > Onar; Yeniden başlatın |
| ADNS-SCN-002 | Uyarı | 3 | İnceleme süre sınırını aştı | Bazı kısımlar süre sınırında incelenemedi; rapor "eksik inceleme" içeriyor. | Çok büyük veya karmaşık dosya | Ayarlar'dan süre sınırını artırıp yeniden tarayın |
| ADNS-SCN-003 | Uyarı | 3 | İnceleme bellek sınırını aştı | Bir parça bellek sınırı nedeniyle incelenemedi. | Bomba şüphesi; Çok büyük içerik | Dosyayı açmayın; Karantinayı değerlendirin |
| ADNS-SCN-004 | Hata | 3 | Dosya açılamadı | Dosya başka bir uygulama tarafından kullanılıyor veya erişim izni yok. | Kilitli dosya; İzin | İndirme bittikten sonra tekrar deneyin |
| ADNS-SCN-005 | Uyarı | 3 | Arşiv sınırı aşıldı | Arşiv olağan dışı oranda açılıyor (sıkıştırma bombası şüphesi); inceleme durduruldu. | Kasıtlı bomba; Aşırı büyük arşiv | Dosyayı açmayın; Karantina önerilir |
| ADNS-SCN-006 | Bilgi | 3 | Şifreli içerik incelenemedi | Parola korumalı içerik incelenemedi. | Şifreli arşiv/belge | Parolayı biliyorsanız girip yeniden tarayın |
| ADNS-SCN-007 | Bilgi | 3 | Windows Defender taraması kullanılamadı | Defender etkin değil veya yanıt vermedi; diğer incelemeler yapıldı. | Başka AV etkin; Defender kapalı | İşlem gerekmiyor; Rapor bunu belirtir |
| ADNS-SCN-008 | Uyarı | 3 | İnceleme kuralları güncel değil | Kural veritabanı {gun} gündür güncellenmedi. | Bağlantı yok; Besleme hatası | "Şimdi güncelle" |
| ADNS-SCN-009 | Bilgi | 3 | Çevrim içi hash sorgusu yapılamadı | Hash itibarı sorgulanamadı; yerel inceleme tamamlandı. | Bağlantı yok; Anahtar geçersiz; Kota | Ayarlar > Çevrim içi sorgular |

### QRN

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-QRN-001 | Hata | 3 | Karantinaya alınamadı | Dosya karantinaya taşınamadı; dosya yerinde. | Kilitli dosya; İzin; Disk dolu | Dosyayı kullanan uygulamayı kapatın; "Yeniden başlatmada karantinaya al" |
| ADNS-QRN-002 | Uyarı | 3 | Dosya yeniden başlatmada silinecek | Dosya kullanımda olduğu için hemen silinemedi; yeniden başlatmada silinecek. | Çalışan işlem dosyayı tutuyor | Bilgisayarı yeniden başlatın |
| ADNS-QRN-003 | Hata | 3 | İşlem doğrulanamadı | Silme veya karantina sonrası denetim başarısız; dosya hâlâ mevcut olabilir. | Başka işlem dosyayı yeniden oluşturdu; İzin | Tam tarama yapın; Dosyanın konumunu kontrol edin |
| ADNS-QRN-004 | Uyarı | 3 | Geri yükleme konumu güvenli değil | Özgün konum değişmiş (bağlantı noktası/kısayol); geri yükleme durduruldu. | Junction/symlink | Farklı bir konuma geri yükleyin |

### WEB

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-WEB-001 | Uyarı | 4 | Uzantı uygulamaya bağlanamıyor | Tarayıcı uzantısı AlterDNS'e ulaşamıyor; son kurallarla çalışıyor. | Native host kaydı eksik; Uygulama kapalı | Bileşenler > Tarayıcı koruması > Onar; Tarayıcıyı yeniden başlatın |
| ADNS-WEB-002 | Bilgi | 4 | Uzantı kurulu değil | Tarayıcı koruması için uzantıyı tarayıcınıza eklemeniz gerekiyor. | Uzantı henüz yüklenmedi | "Tarayıcıda kur" (sihirbaz; geliştirici modu adımlarını gösterir) |
| ADNS-WEB-003 | Uyarı | 4 | Uzantı sürümü uyumsuz | Uzantı ile uygulama sürümleri uyumsuz. | Biri güncellenmedi | Tarayıcı uzantılarını ve AlterDNS'i güncelleyin |
| ADNS-WEB-004 | Uyarı | 4 | Engelleme kuralları sığmadı | Tarayıcı sınırları nedeniyle listenin bir kısmı ön engellemeye eklenemedi; kalan adresler gezinme sırasında kontrol ediliyor. | Tarayıcı kural sınırı | İşlem gerekmiyor |
| ADNS-WEB-005 | Uyarı | 4 | Tarayıcı uzantısı devre dışı kalmış olabilir | Tarayıcı açık ama AlterDNS uzantısından bir süredir haber alınamıyor; tarayıcıda engelleme çalışmıyor olabilir. | Tarayıcı güncellemesi sonrası geliştirici modu uzantısı kapatıldı; Uzantı elle kapatıldı veya kaldırıldı | "Yeniden etkinleştir" yönergesi: Uzantılar sayfası → AlterDNS → aç; Gerekirse "Tarayıcıda kur" ile yeniden yükleyin |
| ADNS-WEB-006 | Uyarı | 4 | Tarayıcı geliştirici modunu engelliyor | Bu tarayıcıda geliştirici modu kurum ilkesiyle kapatılmış; uzantı yüklenemiyor. | Kurum/okul yönetimli tarayıcı | Başka bir tarayıcı kullanın veya BT yöneticinize danışın; İlkeyi aşmaya çalışmayın |
| ADNS-WEB-007 | Bilgi | 4 | Desteklenen tarayıcı bulunamadı | Bilgisayarda Chrome, Edge veya başka bir Chromium tabanlı tarayıcı bulunamadı. | Tarayıcı kurulu değil; Taşınabilir sürüm | Tarayıcı klasörünü elle seçin veya desteklenen bir tarayıcı kurun |

### REP

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-REP-001 | Uyarı | 4 | Tehdit listesi indirilemedi | {kaynak} listesi güncellenemedi; {tarih} tarihli kopya kullanılıyor. | Bağlantı; Kaynak kapalı | Otomatik yeniden denenir |
| ADNS-REP-002 | Uyarı | 4 | Tehdit listesi doğrulanamadı | İndirilen liste beklenen biçimde değil; kullanılmadı. | Kaynak değişikliği; Bozuk indirme | Otomatik yeniden denenir; Sürerse güncelleme bekleyin |
| ADNS-REP-003 | Uyarı | 4 | Tehdit verisi eski | Tehdit listeleri {gun} gündür güncellenmedi; koruma eksik olabilir. | Uzun süre çevrim dışı | İnternete bağlanınca "Şimdi güncelle" |

### UPD

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-UPD-001 | Bilgi | 5 | Güncelleme sunucusuna ulaşılamadı | Güncellemeler denetlenemedi; daha sonra yeniden denenecek. | Bağlantı; Sunucu | İşlem gerekmiyor |
| ADNS-UPD-002 | Kritik | 5 | Güncelleme imzası geçersiz | İndirilen güncelleme AlterDNS imzası taşımıyor; kurulmadı. | Bozulma; Araya girme saldırısı | İşlem gerekmiyor; Tekrarlarsa ağınızı kontrol edin |
| ADNS-UPD-003 | Hata | 5 | Paket bütünlüğü doğrulanamadı | Güncelleme dosyası bozuk; kurulmadı. | Yarım/bozuk indirme | Otomatik yeniden denenir |
| ADNS-UPD-004 | Uyarı | 5 | Eski sürüme geçiş reddedildi | Sunulan sürüm kurulu sürümden eski; güvenlik nedeniyle kurulmadı. | Eski manifest; Saldırı | İşlem gerekmiyor |
| ADNS-UPD-005 | Hata | 5 | Güncelleme geri alındı | Yeni sürüm başlangıç denetimini geçemedi; önceki sürüme dönüldü. | Uyumsuzluk; Dosya hatası | İşlem gerekmiyor; Sonraki sürüm beklenir; Tanılama paketi |
| ADNS-UPD-006 | Uyarı | 5 | Güncelleme bilgisi süresi dolmuş | Sunucudaki güncelleme bilgisi eski; güvenlik için kullanılmadı. | Sunucu güncellenmedi; Dondurma saldırısı | Daha sonra yeniden denenir |
| ADNS-UPD-007 | Bilgi | 5 | Güncelleme sunucusu isteği sınırladı | GitHub geçici olarak yanıt vermedi veya isteği sınırladı; daha sonra yeniden denenecek. | Hız sınırı (403/429); Ağ | İşlem gerekmiyor |

### INS

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-INS-001 | Bilgi | 5 | Yönetici izni gerekli | Kurulum için Windows yönetici onayı gerekiyor. | UAC reddedildi | Kurulumu yeniden başlatıp onaylayın |
| ADNS-INS-002 | Hata | 5 | Bileşen indirilemedi | {bilesen} indirilemedi; hiçbir değişiklik yapılmadı. | Bağlantı; Sunucu | Yeniden deneyin; Çevrim dışı kurulum paketini kullanın |
| ADNS-INS-003 | Hata | 5 | Yetersiz disk alanı | {gereken} boş alan gerekiyor. | Disk dolu | Alan açıp yeniden deneyin |
| ADNS-INS-004 | Hata | 5 | Bileşen kurulamadı | Kurulum tamamlanamadı; yapılan değişiklikler geri alındı. | Dosya kilidi; İzin; AV engeli | Bilgisayarı yeniden başlatıp tekrar deneyin |
| ADNS-INS-005 | Uyarı | 5 | Kaldırma kalıntı buldu | Bazı öğeler kaldırılamadı (liste aşağıda). | Kilitli dosya; Yeniden başlatma gerekli | Yeniden başlatın; Liste ekrandan kopyalanabilir |
| ADNS-INS-006 | Hata | 5 | Desteklenmeyen Windows sürümü | Bu Windows sürümü desteklenmiyor. | Eski sürüm / ARM64 (bağlantı modülü) | Desteklenen sürümler listesine bakın |
| ADNS-INS-007 | Uyarı | 5 | Akıllı Uygulama Denetimi açık | Bu sürüm henüz kod imzası taşımadığı için Windows Akıllı Uygulama Denetimi bazı bileşenleri engelleyebilir. | İmzasız ön sürüm; SAC açık | İmzalı sürümü bekleyin. SAC'yi kapatmanız önerilmez (Windows sıfırlanmadan geri açılamaz) |

### UI

| Kod | Önem | Faz | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|---|
| ADNS-UI-001 | Hata | 1 | Arayüz başlatılamadı | Pencere oluşturulamadı; korumalar çalışmaya devam ediyor. | Grafik sürücüsü; Uzak masaüstü | Yeniden deneyin; Grafik sürücüsünü güncelleyin |
| ADNS-UI-002 | Bilgi | 1 | Bildirimler gösterilemiyor | Windows bildirimleri kapalı; uyarılar yalnızca uygulamada görünecek. | Odak yardımı; Bildirim ayarı | Windows Ayarları > Bildirimler > AlterDNS |
