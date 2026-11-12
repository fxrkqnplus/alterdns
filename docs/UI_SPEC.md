# AlterDNS — Arayüz Tanımı

Referans deneyim: Cloudflare WARP'ın tepsi menüsü. Hedef: modern, sade, hızlı, düşük kaynak tüketimi. Tasarım sırasında `frontend-design`, `vectorlab-ux-skills` (motion, reduced-motion, typography, surfaces, loaders, errors, empty-states, labels, verbs) ve `design-skills:ux-writing` / `accessibility-audit` kullanılır.

## 1. Marka varlıkları

- **Kaynak dosyalar:**
  - `assets/brand/alterdns.png`: 1007×1026, opak siyah zemin, beyaz "AD" monogramı, eş merkezli halkalar.
  - `assets/brand/alterdns.ico`: 16–256 px, opak; 16 px'te monogram okunmuyor.
- **Üretilecekler** (`assets/brand/`; özgün dosyalar değiştirilmez):
  - Kare ana görsel (1024×1024; dolgu ile, kırpmadan).
  - Saydam zeminli monogram (açık ve koyu tema varyantı).
  - **Tepsi glifi:** Halkalar çıkarılmış, kalın "AD" veya tek halka + "A" sadeleştirmesi. 16/20/24/32 px (100/125/150/200 % DPI). Her biri açık ve koyu görev çubuğu için. Durum rozetleri: etkin (yeşil nokta), duraklatıldı (gri), uyarı (amber), hata (kırmızı). Glif önerileri kullanıcıya önizlemeyle sunulup onaylatılır.
  - Uygulama ICO'su: mevcut ICO korunur; 16/24 px için sadeleştirilmiş katman eklenmesi önerilir.
  - Uzantı simgeleri: 16, 32, 48, 128 px PNG (her boyut ayrı dosya).
  - Engelleme sayfası ve Hakkında için yüksek çözünürlüklü saydam logo.
- Vektör (SVG) kaynağı varsa kullanıcıdan istenir. Yoksa PNG'den türetilir ve keskinlik kontrol edilir.

## 2. Tipografi, renk, hareket

- **Yazı tipi:** Segoe UI Variable (Windows 11 sistem yazı tipi), Windows 10'da Segoe UI. Segoe dağıtılmaz. Ek bir marka yazı tipi istenirse yalnızca OFL lisanslı ve alt kümelenmiş olarak eklenir (boyut bütçesi).
- **Ölçek:** başlık 20/28, alt başlık 14/20 semibold, gövde 14/20, ikincil 12/16.
- **Renk:**
  - Marka tek renklidir (siyah/beyaz). Vurgu rengi için Windows vurgu rengi veya nötr bir marka mavisi kullanılır; spike'ta seçilir.
  - Durum renkleri: başarı, uyarı, hata. Her ikisi de WCAG AA kontrastını sağlar.
  - Açık ve koyu tema sistemle senkron.
- **Hareket:**
  - Flyout açılış: 150 ms kaydırma (8–12 px) + saydamlık, ease-out. Kapanış 100 ms.
  - Ana pencere geçişleri ≤ 200 ms.
  - `SPI_GETCLIENTAREAANIMATION` kapalıysa veya azaltılmış hareket seçiliyse animasyon yapılmaz.
  - Sürekli dönen yükleyiciler yalnızca gerçekten bekleme varken gösterilir; boşta animasyon yoktur (CPU bütçesi).
- **Malzeme:** Win11'de `DWMWA_SYSTEMBACKDROP_TYPE` (flyout için geçici pencere arka planı) ve `DWMWA_WINDOW_CORNER_PREFERENCE = ROUND`. Desteklenmiyorsa düz yüzey ve 1 px kenarlık kullanılır.

## 3. Tepsi simgesi

- `Shell_NotifyIcon` (NOTIFYICON_VERSION_4). Tooltip örneği: "AlterDNS — Bağlantı koruması etkin".
- **Sağ tık** flyout'u açar (gereksinim). **Sol tık** varsayılan olarak aynı flyout'u açar; Ayarlar'dan "ana pencereyi aç" olarak değiştirilebilir.
- **Klavye:** Win+B → ok tuşları → Enter / Shift+F10 flyout'u açar (`NIN_KEYSELECT`, `WM_CONTEXTMENU`).
- Explorer yeniden başlarsa (`TaskbarCreated` mesajı) simge yeniden eklenir.
- Tema değişiminde (`WM_SETTINGCHANGE` "ImmersiveColorSet") glif varyantı değişir.

## 4. Flyout (küçük menü)

**Pencere davranışı (zorunlu):**
- `WS_POPUP`, `WS_EX_TOOLWINDOW` (görev çubuğunda yok), topmost. Başlık çubuğu yok.
- `WM_NCHITTEST` her zaman `HTCLIENT` döner. Sistem menüsü yok. `SC_MOVE` ve `SC_SIZE` yok sayılır. Sürüklenemez ve yeniden boyutlandırılamaz.
- Konum `Shell_NotifyIconGetRect` + `SHAppBarMessage(ABM_GETTASKBARPOS)` ile hesaplanır. Görev çubuğunun kenarına göre hizalanır (alt, üst, sol, sağ) ve monitör çalışma alanı içinde tutulur. Simge taşma alanındaysa (gizli simgeler) görev çubuğu köşesine hizalanır.
- **Kapanma koşulları:**
  - `WM_ACTIVATE(WA_INACTIVE)` (masaüstüne veya başka pencereye tıklama)
  - Esc
  - Simgeye yeniden tıklama (aç/kapa)
  - Flyout'tan ana pencereye geçiş
  - Oturum kilitlenmesi

  Açılışta `SetForegroundWindow` ile odak alınır; odak alınamazsa geçici fare kancası (mouse hook) yedeği spike'ta değerlendirilir.
- **Boyut:** 360×(içeriğe göre, en fazla 520) mantıksal px. PerMonitorV2 DPI.

**İçerik (yukarıdan aşağı):**
1. **Başlık:** monogram + "AlterDNS" + genel durum etiketi ("Korunuyorsunuz" / "Dikkat gerekiyor" / "Duraklatıldı").
2. **Bağlantı kartı:**
   - Büyük anahtar "Bağlantı koruması".
   - Durum satırı: Bağlı / Ayarlanıyor… / Beklemede (VPN) / Hata `ADNS-…`.
   - Ağ satırı: "Wi-Fi • Kişisel erişim noktası • Profil: Uyumlu TCP".
   - Modül yüklü değilse: "Yüklü değil — Ekle".
3. **Dosya güvenliği kartı:** son tarama sonucu ve zamanı; "Dosya tara…" düğmesi; karantinadaki dosya sayısı.
4. **Tarayıcı koruması kartı:** bugün engellenen sayısı; uzantı bağlantı durumu (bağlı / bağlı değil / kurulu değil veya devre dışı → "Tarayıcıda kur" sihirbazı).
5. **Alt çubuk:** "Ana pencereyi aç" (birincil, tek tık), "Duraklat ▾" (15 dk / 1 saat / yeniden başlatmaya kadar), ayarlar simgesi, "Çıkış".

**Durum geçişleri:** Anahtar değişiminde iyimser (optimistic) gösterim yapılmaz. "Ayarlanıyor…" gösterilir ve servis olayı gelince güncellenir. Hata olursa kartın içinde kod ve tek satır özet çıkar; "Ayrıntılar" ana pencereyi ilgili sayfada açar.

## 5. Ana pencere

- **Konum ve boyut:**
  - İmlecin (veya tepsinin) bulunduğu monitörün çalışma alanında **ortalanmış**.
  - 960×640 mantıksal px; en küçük 800×560.
  - Büyütme düğmesi devre dışı; tam ekran yok.
  - Pencere taşınabilir (normal pencere).
- **İşlem:** Ayrı işlemdir (`alterdns-ui.exe`). Kapanınca sonlanır. Zaten açıksa öne getirilir; tek örnek kuralı adlandırılmış mutex ile sağlanır.
- **Sol gezinme:**
  1. **Genel Bakış:** üç modülün durumu, son olaylar, uyarılar.
  2. **Dosya Güvenliği:** Tara (sürükle-bırak alanı + dosya seç), Raporlar (liste, filtre, HTML dışa aktarma), Karantina (geri yükle / kalıcı sil).
  3. **Tarayıcı Koruması:** durum, kaynaklar ve son güncelleme tarihleri, engellenenler, izin listesi, **"Tarayıcıda kur"** sihirbazı ve tarayıcı başına bağlantı durumu.
  4. **Bağlantı:** durum, ağ ve profil, "Yeniden ayarla", profiller (sil / öncelik), tanılama (prob sonuçları), Gelişmiş (strateji seçimi, DNS modu), Lab (yalnızca açık onayla).
  5. **Bileşenler:** kurulu ve kurulu olmayan modüller, boyutlar, ekle / kaldır / onar.
  6. **Güncellemeler:** sürüm, kanal, son kontrol, "Şimdi denetle", geçmiş.
  7. **Ayarlar:** dil, başlangıçta çalış, bildirimler, sol tık davranışı, gizlilik ve çevrim içi sorgular, tanılama paketi.
  8. **Hakkında:**
     - AlterDNS sürümü
     - **Yapımcı: Furkan Okan**; **Copyright © 2026 Furkan Okan**
     - **Lisans: GPL-3.0-or-later** (lisans metni ve kaynak kod bağlantısı: `github.com/fxrkqnplus/alterdns`)
     - Ad ve logo notu (`TRADEMARK.md`)
     - Üçüncü taraf lisansları
     - Kod imzalama politikası. İmza alındıktan sonra şu ifade gösterilir: "Free code signing provided by SignPath.io, certificate by SignPath Foundation"
     - Gizlilik politikası
     - Slint'e teşekkür
- **Kurulu olmayan modül sayfası:** Ne yaptığını anlatan kısa metin, boyut, gerektirdiği izinler ve "Ekle" düğmesi.

## 6. Onay ekranı (standart bileşen)

```
[Başlık: fiil + nesne]  ör. "DNS ayarını değiştir"
Neden: tek cümle
Değişecekler:
  • Wi-Fi DNS sunucusu: 192.168.1.1 → 1.1.1.1 (şifreli)
Geri alma: "AlterDNS kapatıldığında veya kaldırıldığında eski değer geri yüklenir."
Risk / not: (varsa)
[Vazgeç]  [Değiştir]   ← birincil düğme işlemi adlandırır; "Tamam" kullanılmaz
```

## 7. Engelleme sayfası (uzantı)

- **Düzen:** Logo; başlık "Bu site engellendi"; kategori rozeti (Oltalama / Zararlı yazılım / Dolandırıcılık / Şüpheli); açıklama.
- **Ayrıntılar:**
  - Adres (tıklanamaz, alan adı vurgulu, IDN için punycode gösterimi)
  - Kaynak listeleri ve tarihleri ("USOM — 08.11.2026")
  - Neden bu karar verildi
- **Eylemler:**
  - "Geri dön" (birincil)
  - "Yine de devam et": ikincil, iki adımlı; ikinci adımda risk metni yer alır. Yalnızca bu sekme ve oturum için izin verir.
  - "Hatalı engel bildir": gönderilecek veri önce gösterilir.
- **Erişilebilirlik:** Uzantı paketinden yüklenir; uzak kaynak, betik içi satır (inline) ve izleyici kullanılmaz. Erişilebilirlik ve kontrast denetimi yapılır.

## 7a. "Tarayıcıda kur" sihirbazı (ADR-011)

Uygulama içinde 4 adımlı, ekran görüntülü bir panel; Chrome ve Edge için ayrı görsel setleri vardır.

1. **Tarayıcı seç:**
   - Tespit edilen tarayıcılar simge ve adlarıyla listelenir (Chrome, Edge, Brave, Opera, Vivaldi). Bulunamazsa `ADNS-WEB-007` gösterilir ve elle seçim sunulur.
   - Uzantının ne yaptığı ve hangi izinleri istediği tek ekranda özetlenir. Her izin düz Türkçe gerekçeyle verilir (`extension/INSTALL.md` ile aynı metin).
2. **Tarayıcıyı aç:**
   - Düğme: **"Tarayıcıyı aç"**.
   - Tarayıcı başlatılır; uzantılar sayfası adresi (`chrome://extensions` / `edge://extensions`) panoya kopyalanır. Komut satırından açılabildiği doğrulanırsa doğrudan açılır.
   - Metin: "Adres çubuğuna yapıştırıp Enter'a basın."
3. **Geliştirici modu ve yükleme:**
   - "Geliştirici modu" anahtarının yerini gösteren görsel.
   - "Paketlenmemiş öğe yükle" düğmesinin yerini gösteren görsel.
   - **"Klasörü göster"** düğmesi: Explorer'da `browser-extension` klasörü seçili açılır ve yol panoya kopyalanır.
   - Geliştirici modu uyarısıyla ilgili kısa, dürüst açıklama: "Tarayıcınız mağaza dışı uzantılar için uyarı gösterebilir; bu beklenen bir durumdur."
4. **Doğrulama:** Native host `hello` mesajı beklenir; yükleyici animasyonu en fazla 10 dk sürer.
   - Başarıda "Bağlandı ✓" ve bir test bağlantısıyla engelleme sayfası önizlemesi gösterilir.
   - Zaman aşımında olası nedenler listelenir: yanlış klasör seçildi, geliştirici modu kapalı, kurum ilkesi (`ADNS-WEB-006`).

**Kurallar:**
- Geliştirici modu uyarısını gizleyen yama veya ilke önerilmez.
- Kullanıcı istediği zaman sihirbazdan çıkabilir; modül "Kurulum tamamlanmadı" durumunda kalır.

## 8. Kurulum önyükleyicisi

1. Hoş geldiniz ekranı: yapımcı (Furkan Okan) ve lisans bilgisi. GPL-3.0 metni görüntülenebilir; kabul zorunlu değildir. İmzasız ön sürümde Smart App Control açıksa `ADNS-INS-007` uyarısı gösterilir.
2. **Bileşen seçimi:** her modül için açıklama, indirme boyutu, gerektirdiği izinler ve sistem değişiklikleri. Alt seçenekler (ClamAV, Sandbox) ayrı işaretlenir.
3. Gizlilik politikası ve tercihler: politika metni gösterilir (SignPath şartı). Çevrim içi hash sorguları ve Safe Browsing (D4) seçilir; veri gönderen her işlev kapatılabilir.
4. Özet ve onay ekranı (değişecek her şey listelenir).
5. İlerleme: indirme, doğrulama, kurulum. İptal edilebilir; iptal edilirse geri alma yapılır.
6. Bitiş: "Tarayıcıda kur" (sihirbazı açar) ve "Bağlantı korumasını şimdi ayarla" seçenekleri.

## 9. Metin kuralları

- Türkçe, yalın ve suçlamayan bir dil kullanılır. Düğmeler fiille başlar ("Karantinaya al", "Yeniden ayarla").
- Yanlış güven verilmez. "Güvenli" yerine "Bilinen tehdit bulunamadı" denir; incelenemeyen kısımlar belirtilir.
- Teknik terimler gerekiyorsa kısa açıklama eklenir ("DPI — ağ sağlayıcısının trafik içeriğine bakarak engelleme yapması").
- Hata metinleri katalogdan gelir; kodun içine elle yazılmaz.
