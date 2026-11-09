# AlterDNS — Hata Kataloğu

Bu belge kod şemasını ve başlangıç kataloğunu tanımlar. Faz 1'de içerik `errors/catalog.toml` dosyasına taşınır ve İngilizce metinler eklenir. Bundan sonra **tek kaynak TOML dosyasıdır**; bu belge oradan üretilir (`xtask docs-errors`).

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

**Numaralandırma:** `001–099` kullanıcıya gösterilen hatalar, `100–199` uyarı ve bilgi durumları, `900–999` iç hatalardır (kullanıcıya genel mesajla gösterilir). Bir kod yayımlandıktan sonra anlamı değiştirilmez. Kullanılmayan kod silinmez, `deprecated = true` olarak işaretlenir.

**Önem düzeyleri:** `Bilgi`, `Uyarı`, `Hata`, `Kritik`. Kritik hatalarda bildirim her zaman gösterilir ve ilgili modül güvenli duruma geçer.

## 2. TOML biçimi

```toml
[[error]]
code = "ADNS-DPI-001"
severity = "error"            # info | warning | error | critical
area = "DPI"
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

## 3. Başlangıç kataloğu

### GEN / IPC / SVC

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-GEN-900 | Kritik | Beklenmeyen hata | Bir iç hata oluştu. İşlem güvenli biçimde durduruldu. | Yazılım hatası | Uygulamayı yeniden başlatın; tekrarlarsa "Tanılama paketi oluştur" ile kaydı paylaşın |
| ADNS-IPC-001 | Hata | AlterDNS hizmetine bağlanılamıyor | Arayüz arka plan hizmetine ulaşamadı. | Hizmet durdu; hizmet başlatılıyor; kurulum eksik | Birkaç saniye bekleyin; "Hizmeti yeniden başlat"; Bileşenler > Onar |
| ADNS-IPC-002 | Hata | Geçersiz istek | Hizmet isteği anlayamadı. | Sürüm farkı; bozuk veri | Uygulamayı güncelleyin; yeniden başlatın |
| ADNS-IPC-003 | Uyarı | Yetkisiz istek reddedildi | AlterDNS dışından gelen bir ayar değiştirme isteği engellendi. | Başka bir yazılım hizmete erişmeye çalıştı | İşlem gerekmiyor; tekrarlıyorsa güvenlik taraması yapın |
| ADNS-IPC-004 | Uyarı | Sürüm uyuşmazlığı | Arayüz ile hizmet farklı sürümlerde. | Yarım kalan güncelleme | "Güncellemeyi tamamla"; Bileşenler > Onar |
| ADNS-IPC-005 | Kritik | Hizmet kanalı kullanımda | Hizmetin iletişim kanalı başka bir uygulama tarafından açılmış; güvenlik için hizmet başlatılmadı. | Kötü amaçlı veya hatalı yazılım | Bilgisayarı yeniden başlatın; tekrarlarsa tam tarama yapın |
| ADNS-SVC-001 | Hata | Hizmet başlatılamadı | AlterDNS hizmeti başlamadı; korumalar devre dışı. | Dosyalar eksik; izinler bozuk; başka hata | Bileşenler > Onar; bilgisayarı yeniden başlatın |
| ADNS-SVC-002 | Uyarı | Hizmet yeniden başlatıldı | Hizmet beklenmedik şekilde durdu ve otomatik olarak yeniden başladı. | Yazılım hatası; kaynak yetersizliği | İşlem gerekmiyor; sık tekrarlarsa tanılama paketi oluşturun |

### SYS — Sistem değişiklikleri

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-SYS-001 | Hata | Ayar uygulanamadı | İstenen sistem ayarı değiştirilemedi; hiçbir değişiklik yapılmadı. | Yönetici izni yok; kurum ilkesi; ayar kilitli | Yönetici olarak tekrar deneyin; kurum cihazıysa BT yöneticinize danışın |
| ADNS-SYS-002 | Kritik | Değişiklik geri alınamadı | AlterDNS'in yaptığı bir ayar eski haline döndürülemedi. Ayrıntılar aşağıda. | Ayar kilitli; dosya veya kayıt silinmiş | "Yeniden dene"; olmazsa ekrandaki eski değeri el ile geri yükleme yönergesini izleyin |
| ADNS-SYS-003 | Uyarı | Ayar başka biri tarafından değiştirilmiş | Geri alınacak ayar AlterDNS'ten sonra değiştirilmiş; üzerine yazılmadı. | Kullanıcı veya başka uygulama değiştirdi | Mevcut değeri koru (önerilen) veya AlterDNS öncesi değeri geri yükle |
| ADNS-SYS-004 | Bilgi | Yönetici izni verilmedi | İşlem için Windows yönetici onayı gerekiyor; işlem iptal edildi. | UAC penceresi reddedildi | İşlemi yeniden başlatıp onay verin |

### NET / DPI / DNS — Bağlantı

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-NET-001 | Uyarı | İnternet bağlantısı yok | Ağa bağlısınız ama internete erişilemiyor; bağlantı koruması beklemede. | Modem/telefon bağlantısı; kota; operatör sorunu | Telefonun veya modemin internetini kontrol edin; bağlantı gelince otomatik devam edilir |
| ADNS-NET-002 | Bilgi | Giriş gerektiren ağ | Bu ağ tarayıcıda giriş istiyor; giriş yapılana kadar ayar yapılmaz. | Otel/kafe ağı (captive portal) | Tarayıcıda ağın giriş sayfasını tamamlayın |
| ADNS-NET-003 | Bilgi | VPN veya proxy etkin | VPN/proxy açıkken bağlantı koruması devre dışı kalır. | VPN istemcisi; sistem proxy'si | Gerekirse VPN'i kapatın; AlterDNS otomatik devam eder |
| ADNS-NET-004 | Bilgi | Ölçülü bağlantı | Veri tasarrufu için hız ölçümleri yapılmadı; yalnızca kısa kontroller kullanıldı. | Mobil veri / ölçülü ağ | İşlem gerekmiyor; Ayarlar'dan değiştirilebilir |
| ADNS-NET-010 | Hata | Bağlantı koruması kapatıldı | Etkinleştirilen yapılandırma genel internet erişimini bozdu; değişiklikler geri alındı. | Ağ yapılandırmayla uyumsuz | "Yeniden ayarla"; sorun sürerse tanılama paketi oluşturun |
| ADNS-NET-011 | Uyarı | Uygun yapılandırma bulunamadı | Bu ağda denenen yöntemlerin hiçbiri hedef sitelere erişimi sağlamadı. | Engel türü desteklenmiyor; ağ yeni yöntem kullanıyor | Daha sonra yeniden deneyin; strateji güncellemelerini bekleyin |
| ADNS-NET-012 | Bilgi | Engel IP tabanlı görünüyor | Hedefe giden bağlantı IP düzeyinde engelleniyor; bu yöntemle aşılamaz. | IP engeli | VPN/proxy olmadan çözüm yok; AlterDNS bu engeli aşmaz |
| ADNS-NET-013 | Uyarı | Ayar araması süre sınırına ulaştı | Otomatik ayar zaman sınırında tamamlanamadı; önceki güvenli durum korundu. | Yavaş/kararsız bağlantı | Bağlantı iyileşince "Yeniden ayarla" |
| ADNS-DPI-001 | Hata | Paket sürücüsü açılamadı | Bağlantı koruması ağ sürücüsünü başlatamadı. İnternet bağlantınız etkilenmedi. | Yönetici izni; sürücü dosyası eksik veya değişmiş; güvenlik yazılımı engeli | Bileşenler > Onar; güvenlik yazılımı bildirimlerini kontrol edin (antivirüsü kapatmayın) |
| ADNS-DPI-002 | Hata | Sürücü Windows tarafından engellendi | Sürücü imzası veya bellek bütünlüğü ilkesi nedeniyle yüklenemedi. | İmza doğrulanamadı (577); kurum WDAC ilkesi; sürücü engel listesi | Bileşenler > Onar; kurum cihazıysa BT yöneticinize danışın; bellek bütünlüğünü kapatmayın |
| ADNS-DPI-003 | Uyarı | Başka bir paket aracı çalışıyor | GoodbyeDPI, zapret veya benzeri bir araç aynı anda çalışıyor; çakışma olabilir. | Eski DPI aracı; VPN istemcisi | Diğer aracı geçici olarak durdurmayı onaylayın veya AlterDNS bağlantı modülünü kapalı tutun |
| ADNS-DPI-004 | Hata | Sürücü sürüm çakışması | Sistemde farklı sürümde bir WinDivert sürücüsü yüklü. | Başka uygulamanın sürücüsü | Diğer uygulamayı kapatıp yeniden deneyin; bilgisayarı yeniden başlatın |
| ADNS-DPI-005 | Hata | Desteklenmeyen işlemci | Bağlantı modülü ARM64 Windows'ta desteklenmiyor. | ARM64 cihaz | Diğer modüller kullanılabilir |
| ADNS-DPI-006 | Uyarı | Paket motoru yeniden başlatılıyor | Motor hata verdi; trafik normal akıyor, motor yeniden başlatılıyor. | Geçici sürücü/ağ hatası | İşlem gerekmiyor; tekrarlarsa tanılama paketi |
| ADNS-DNS-001 | Uyarı | Şifreli DNS'e ulaşılamıyor | Seçilen şifreli DNS sağlayıcısına bağlanılamadı; başka sağlayıcı deneniyor. | Sağlayıcı engelli veya kapalı | İşlem gerekmiyor; Ayarlar'dan sağlayıcı seçilebilir |
| ADNS-DNS-002 | Bilgi | DNS yanıtları tutarsız | Bu ağın DNS yanıtları şifreli DNS ile uyuşmuyor; DNS tabanlı engel olabilir. | DNS engeli/yönlendirmesi | AlterDNS gerekirse alan adına özel şifreli çözümleme kullanır |
| ADNS-DNS-003 | Hata | DNS ayarı değiştirilemedi | Sistem DNS ayarı uygulanamadı; ayarlar değişmedi. | Kurum ilkesi; yönetici izni | Alan adına özel modu kullanın (sistem ayarı değiştirmez) |

### SCN / QRN — Dosya güvenliği

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-SCN-001 | Hata | Güvenli inceleme ortamı başlatılamadı | Dosya, korumalı ortam başlamadığı için incelenmedi. Dosya açılmadı. | AppContainer profili bozuk; sistem kaynakları | Bileşenler > Onar; yeniden başlatın |
| ADNS-SCN-002 | Uyarı | İnceleme süre sınırını aştı | Bazı kısımlar süre sınırında incelenemedi; rapor "eksik inceleme" içeriyor. | Çok büyük veya karmaşık dosya | Ayarlar'dan süre sınırını artırıp yeniden tarayın |
| ADNS-SCN-003 | Uyarı | İnceleme bellek sınırını aştı | Bir parça bellek sınırı nedeniyle incelenemedi. | Bomba şüphesi; çok büyük içerik | Dosyayı açmayın; karantinayı değerlendirin |
| ADNS-SCN-004 | Hata | Dosya açılamadı | Dosya başka bir uygulama tarafından kullanılıyor veya erişim izni yok. | Kilitli dosya; izin | İndirme bittikten sonra tekrar deneyin |
| ADNS-SCN-005 | Uyarı | Arşiv sınırı aşıldı | Arşiv olağan dışı oranda açılıyor (sıkıştırma bombası şüphesi); inceleme durduruldu. | Kasıtlı bomba; aşırı büyük arşiv | Dosyayı açmayın; karantina önerilir |
| ADNS-SCN-006 | Bilgi | Şifreli içerik incelenemedi | Parola korumalı içerik incelenemedi. | Şifreli arşiv/belge | Parolayı biliyorsanız girip yeniden tarayın |
| ADNS-SCN-007 | Bilgi | Windows Defender taraması kullanılamadı | Defender etkin değil veya yanıt vermedi; diğer incelemeler yapıldı. | Başka AV etkin; Defender kapalı | İşlem gerekmiyor; rapor bunu belirtir |
| ADNS-SCN-008 | Uyarı | İnceleme kuralları güncel değil | Kural veritabanı X gündür güncellenmedi. | Bağlantı yok; besleme hatası | "Şimdi güncelle" |
| ADNS-SCN-009 | Bilgi | Çevrim içi hash sorgusu yapılamadı | Hash itibarı sorgulanamadı; yerel inceleme tamamlandı. | Bağlantı yok; anahtar geçersiz; kota | Ayarlar > Çevrim içi sorgular |
| ADNS-QRN-001 | Hata | Karantinaya alınamadı | Dosya karantinaya taşınamadı; dosya yerinde. | Kilitli dosya; izin; disk dolu | Dosyayı kullanan uygulamayı kapatın; "Yeniden başlatmada karantinaya al" |
| ADNS-QRN-002 | Uyarı | Dosya yeniden başlatmada silinecek | Dosya kullanımda olduğu için hemen silinemedi; yeniden başlatmada silinecek. | Çalışan işlem dosyayı tutuyor | Bilgisayarı yeniden başlatın |
| ADNS-QRN-003 | Hata | İşlem doğrulanamadı | Silme veya karantina sonrası denetim başarısız; dosya hâlâ mevcut olabilir. | Başka işlem dosyayı yeniden oluşturdu; izin | Tam tarama yapın; dosyanın konumunu kontrol edin |
| ADNS-QRN-004 | Uyarı | Geri yükleme konumu güvenli değil | Özgün konum değişmiş (bağlantı noktası/kısayol); geri yükleme durduruldu. | Junction/symlink | Farklı bir konuma geri yükleyin |

### WEB / REP — Tarayıcı koruması

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-WEB-001 | Uyarı | Uzantı uygulamaya bağlanamıyor | Tarayıcı uzantısı AlterDNS'e ulaşamıyor; son kurallarla çalışıyor. | Native host kaydı eksik; uygulama kapalı | Bileşenler > Tarayıcı koruması > Onar; tarayıcıyı yeniden başlatın |
| ADNS-WEB-002 | Bilgi | Uzantı kurulu değil | Tarayıcı koruması için uzantıyı tarayıcınıza eklemeniz gerekiyor. | Uzantı henüz yüklenmedi | "Tarayıcıda kur" (sihirbaz; geliştirici modu adımlarını gösterir) |
| ADNS-WEB-003 | Uyarı | Uzantı sürümü uyumsuz | Uzantı ile uygulama sürümleri uyumsuz. | Biri güncellenmedi | Tarayıcı uzantılarını ve AlterDNS'i güncelleyin |
| ADNS-WEB-004 | Uyarı | Engelleme kuralları sığmadı | Tarayıcı sınırları nedeniyle listenin bir kısmı ön engellemeye eklenemedi; kalan adresler gezinme sırasında kontrol ediliyor. | Tarayıcı kural sınırı | İşlem gerekmiyor |
| ADNS-WEB-005 | Uyarı | Tarayıcı uzantısı devre dışı kalmış olabilir | Tarayıcı açık ama AlterDNS uzantısından bir süredir haber alınamıyor; tarayıcıda engelleme çalışmıyor olabilir. | Tarayıcı güncellemesi sonrası geliştirici modu uzantısı kapatıldı; uzantı elle kapatıldı veya kaldırıldı | "Yeniden etkinleştir" yönergesi: Uzantılar sayfası → AlterDNS → aç; gerekirse "Tarayıcıda kur" ile yeniden yükleyin |
| ADNS-WEB-006 | Uyarı | Tarayıcı geliştirici modunu engelliyor | Bu tarayıcıda geliştirici modu kurum ilkesiyle kapatılmış; uzantı yüklenemiyor. | Kurum/okul yönetimli tarayıcı | Başka bir tarayıcı kullanın veya BT yöneticinize danışın; ilkeyi aşmaya çalışmayın |
| ADNS-WEB-007 | Bilgi | Desteklenen tarayıcı bulunamadı | Bilgisayarda Chrome, Edge veya başka bir Chromium tabanlı tarayıcı bulunamadı. | Tarayıcı kurulu değil; taşınabilir sürüm | Tarayıcı klasörünü elle seçin veya desteklenen bir tarayıcı kurun |
| ADNS-REP-001 | Uyarı | Tehdit listesi indirilemedi | <kaynak> listesi güncellenemedi; <tarih> tarihli kopya kullanılıyor. | Bağlantı; kaynak kapalı | Otomatik yeniden denenir |
| ADNS-REP-002 | Uyarı | Tehdit listesi doğrulanamadı | İndirilen liste beklenen biçimde değil; kullanılmadı. | Kaynak değişikliği; bozuk indirme | Otomatik yeniden denenir; sürerse güncelleme bekleyin |
| ADNS-REP-003 | Uyarı | Tehdit verisi eski | Tehdit listeleri X gündür güncellenmedi; koruma eksik olabilir. | Uzun süre çevrim dışı | İnternete bağlanınca "Şimdi güncelle" |

### UPD / INS / UI

| Kod | Önem | Başlık | Mesaj | Olası nedenler | Çözümler |
|---|---|---|---|---|---|
| ADNS-UPD-001 | Bilgi | Güncelleme sunucusuna ulaşılamadı | Güncellemeler denetlenemedi; daha sonra yeniden denenecek. | Bağlantı; sunucu | İşlem gerekmiyor |
| ADNS-UPD-002 | Kritik | Güncelleme imzası geçersiz | İndirilen güncelleme AlterDNS imzası taşımıyor; kurulmadı. | Bozulma; araya girme saldırısı | İşlem gerekmiyor; tekrarlarsa ağınızı kontrol edin |
| ADNS-UPD-003 | Hata | Paket bütünlüğü doğrulanamadı | Güncelleme dosyası bozuk; kurulmadı. | Yarım/bozuk indirme | Otomatik yeniden denenir |
| ADNS-UPD-004 | Uyarı | Eski sürüme geçiş reddedildi | Sunulan sürüm kurulu sürümden eski; güvenlik nedeniyle kurulmadı. | Eski manifest; saldırı | İşlem gerekmiyor |
| ADNS-UPD-005 | Hata | Güncelleme geri alındı | Yeni sürüm başlangıç denetimini geçemedi; önceki sürüme dönüldü. | Uyumsuzluk; dosya hatası | İşlem gerekmiyor; sonraki sürüm beklenir; tanılama paketi |
| ADNS-UPD-006 | Uyarı | Güncelleme bilgisi süresi dolmuş | Sunucudaki güncelleme bilgisi eski; güvenlik için kullanılmadı. | Sunucu güncellenmedi; dondurma saldırısı | Daha sonra yeniden denenir |
| ADNS-UPD-007 | Bilgi | Güncelleme sunucusu isteği sınırladı | GitHub geçici olarak yanıt vermedi veya isteği sınırladı; daha sonra yeniden denenecek. | Hız sınırı (403/429); ağ | İşlem gerekmiyor |
| ADNS-INS-001 | Bilgi | Yönetici izni gerekli | Kurulum için Windows yönetici onayı gerekiyor. | UAC reddedildi | Kurulumu yeniden başlatıp onaylayın |
| ADNS-INS-002 | Hata | Bileşen indirilemedi | <bileşen> indirilemedi; hiçbir değişiklik yapılmadı. | Bağlantı; sunucu | Yeniden deneyin; çevrim dışı kurulum paketini kullanın |
| ADNS-INS-003 | Hata | Yetersiz disk alanı | <gereken> boş alan gerekiyor. | Disk dolu | Alan açıp yeniden deneyin |
| ADNS-INS-004 | Hata | Bileşen kurulamadı | Kurulum tamamlanamadı; yapılan değişiklikler geri alındı. | Dosya kilidi; izin; AV engeli | Bilgisayarı yeniden başlatıp tekrar deneyin |
| ADNS-INS-005 | Uyarı | Kaldırma kalıntı buldu | Bazı öğeler kaldırılamadı (liste aşağıda). | Kilitli dosya; yeniden başlatma gerekli | Yeniden başlatın; liste ekrandan kopyalanabilir |
| ADNS-INS-006 | Hata | Desteklenmeyen Windows sürümü | Bu Windows sürümü desteklenmiyor. | Eski sürüm / ARM64 (bağlantı modülü) | Desteklenen sürümler listesine bakın |
| ADNS-INS-007 | Uyarı | Akıllı Uygulama Denetimi açık | Bu sürüm henüz kod imzası taşımadığı için Windows Akıllı Uygulama Denetimi bazı bileşenleri engelleyebilir. | İmzasız ön sürüm; SAC açık | İmzalı sürümü bekleyin. SAC'yi kapatmanız önerilmez (Windows sıfırlanmadan geri açılamaz) |
| ADNS-UI-001 | Hata | Arayüz başlatılamadı | Pencere oluşturulamadı; korumalar çalışmaya devam ediyor. | Grafik sürücüsü; uzak masaüstü | Yeniden deneyin; grafik sürücüsünü güncelleyin |
| ADNS-UI-002 | Bilgi | Bildirimler gösterilemiyor | Windows bildirimleri kapalı; uyarılar yalnızca uygulamada görünecek. | Odak yardımı; bildirim ayarı | Windows Ayarları > Bildirimler > AlterDNS |
