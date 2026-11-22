# AlterDNS — Güvenlik Tasarımı ve Tehdit Modeli

## 1. Varlıklar

- Kullanıcının bilgisayarı: SYSTEM yetkili servis ve çekirdek sürücüsü bu varlığa dokunur.
- Kullanıcı dosyaları ve tarayıcı gezinme bilgisi.
- Güncelleme kanalı ve imza anahtarları.
- Ağ bağlantısı: bozulursa kullanıcı internetsiz kalır.

## 2. Saldırganlar ve giriş noktaları

| Giriş noktası | Saldırgan | Risk | Kontrol |
|---|---|---|---|
| Taranan dosya | Zararlı dosya yazarı | Ayrıştırıcı açığıyla kod çalıştırma | Ayrıştırma yalnızca sandbox işçisinde (AppContainer, yetenek yok, Low IL, Job sınırları, alt işlem yasak). Rust bellek güvenliği. Fuzz testleri. Çökme = "incelenemedi" |
| Arşiv içeriği | Aynı | Yol aşımı, bomba, ADS/cihaz adları | Diske açma yok, bellek içi akış; bütçeler; ad doğrulaması |
| IPC pipe | Yerel düşük yetkili yazılım | Servis yetkisiyle ayar değiştirme | DACL, `FIRST_PIPE_INSTANCE`, çağıran imaj ve yol doğrulaması, onay belirteci, şema doğrulaması, boyut sınırı |
| Native messaging | Kötü amaçlı uzantı veya web sayfası | Host üzerinden servise erişim | `allowed_origins` yalnız AlterDNS uzantı kimlikleri; host'un origin argümanı doğrulaması; yalnız salt okunur sorgu yöntemleri; hız sınırı |
| Engelleme sayfası | Saldırganın hazırladığı URL | XSS, açık yönlendirme | Parametreler metin olarak işlenir; katı CSP; "devam et" yalnız özgün URL'ye ve iki adımlı onayla |
| Güncelleme kanalı | Ağda araya giren, ele geçirilmiş depo hesabı | Zararlı güncelleme | Ed25519 manifest imzası (anahtar depoda değil), SHA-256, sürüm düşürme ve dondurma koruması, Authenticode (varsa), HTTPS. A/B ve sağlık kontrolüyle geri dönüş |
| Veri beslemeleri | Ele geçirilmiş kaynak | Yanlış engel veya engelin kaldırılması | TLS, boyut ve değişim oranı sınırları, son iyi bilinen kopya, kaynak bazında devre dışı bırakma; kritik alan adları için izin listesi (sistem/AlterDNS uç noktaları) |
| DLL arama sırası | Yerel saldırgan | DLL yerleştirme | `SetDefaultDllDirectories`, kurulum dizini yalnız yöneticiye yazılabilir, tam yollar |
| Karantina ve silme | Yerel saldırgan | Junction/symlink ile yanlış dosyayı sildirme | Handle tabanlı işlem, `FILE_FLAG_OPEN_REPARSE_POINT`, kimlik (dosya ID + hash) doğrulaması |
| Paket/TUN sürücüsü (D8: Wintun adayı, WinDivert askıda) | Başka yerel yazılım | Sürücüyü veya adaptörü kötüye kullanma (yönetici ister); rotanın başka trafiği yakalaması | Resmi imzalı sürücü; AlterDNS dizini korumalı; yalnızca hedef IP'lere /32 rota; rota ve adaptör journal'lı; başka DPI araçları izlenir |
| Tedarik zinciri | Bağımlılık ele geçirme | Zararlı crate veya npm paketi | `cargo deny` (advisories, lisans, kaynak), `Cargo.lock` sabit, `npm ci` + lockfile, Dependabot, az bağımlılık, upstream ikililer için hash sabitleme |
| CI / yayın hattı (public depo) | Kötü niyetli PR, ele geçirilmiş action | Gizli değişken sızdırma, zararlı sürüm | Action'lar SHA ile sabit; varsayılan token salt okunur; `pull_request_target` yok; gizli değişkenler yalnız korumalı `release` ortamında (gerekli onaylayıcı); dal koruması; secret scanning ve push protection; köken kanıtı (`attest-build-provenance`); SignPath'te her sürüm için elle onay |
| Paketlenmemiş uzantı | Yerel düşük yetkili yazılım | Uzantı dosyalarını değiştirip engellemeyi kapatma veya veri toplama | Uzantı `%ProgramFiles%` altında, yalnız yönetici yazabilir. Bütünlük kontrolü: servis dosya hash'lerini manifestle karşılaştırır, sapmada uyarır |
| Uzantı kimliği taklidi | Aynı `key` ile yüklenen başka bir uzantı | Native host'a erişim | `key` açık olduğu için kimlik taklit edilebilir. Host yalnız salt okunur sorgu yöntemleri sunar (`lookup`, `rules.sync`, `hello`); ayar değiştiremez; hız sınırı uygular. Uzantının dosya yolu ve bütünlüğü servis tarafından kontrol edilir |
| Sahte AlterDNS kopyaları | Dağıtım dolandırıcıları (GPL çatallama serbest, ad/logo değil) | Kullanıcının zararlı kopya indirmesi | Tek resmi indirme noktası (`fxrkqnplus/alterdns` Releases), `gh attestation verify` ve SHA256SUMS yönergesi, SignPath imzası (onaylanınca), `TRADEMARK.md` (çatallar başka ad ve logo kullanır). "AV'yi veya SAC'yi kapatın" diyen her yönergenin sahte olduğu belgelerde açıkça yazılır |

## 3. İlkeler

- **Antivirüsle uyum:** AlterDNS bir AV'nin yerini almaz. AV kapatmayı veya istisna eklemeyi önermez, yapmaz. WinDivert'in algılanma olasılığı belgelerde şeffafça açıklanır.
- **Onay:**
  - Sistem değişikliği, çevrim içi sorgu, dosya yükleme, kalıcı silme ve lab deneyi açık onay ister.
  - Onay ekranı şunları gösterir: ne, neden, eski → yeni değer, nasıl geri alınır, risk.
  - Onaylar loglanır.
- **Gizlilik:**
  - Telemetri yoktur.
  - URL kontrolleri yereldir. Safe Browsing kullanılırsa yalnız hash prefix gönderilir.
  - Hash sorguları varsayılan olarak kurulumda sorulur. Dosya içeriği hiçbir zaman otomatik gönderilmez.
  - Ağ parmak izleri hash'lenir.
- **En az ayrıcalık:** Servis yalnızca gereken işleri yapar. Tarama, kullanıcı bağlamında ve sandbox'ta yapılır.
- **Kapanınca güvenli:** Motor hatasında trafik normal akar; yarım değişiklikler geri alınır.

## 4. Anahtar yönetimi

- **Güncelleme imza anahtarı (Ed25519):** Çevrim dışı üretilir ve depo dışında saklanır. Seçenekler: (a) yerel imzalama adımı (en güvenli; elle); (b) korumalı `release` ortamında gizli değişken (gerekli onaylayıcı). Karar Faz 5'te verilir. Anahtar devri için manifestte `next_key` alanı vardır. Kayıp durumu için kurtarma prosedürü yazılır.
- **SignPath:** Proje belirteci yalnız `release` ortamında tutulur. Her imzalama isteği SignPath arayüzünde elle onaylanır. GitHub ve SignPath hesaplarında MFA açıktır.
- **Kod imzalama:** Sertifika SignPath Foundation'dadır (D3; ADR-012); projede özel anahtar bulunmaz. Yine de `.pfx` / `.key` dosyaları `.gitignore` ile depo dışında tutulur.
- **API anahtarları** (abuse.ch, Safe Browsing): Dağıtılan ikiliye gömülebilecek anahtarlar yalnızca istemci için tasarlanmış, kısıtlanabilen anahtarlardır (koşullara bakılır). Kullanıcıya ait anahtarlar (VirusTotal) DPAPI ile saklanır.

## 5. Güvenlik incelemesi süreci

- IPC, journal, güncelleyici, native host, sandbox aracısı ve karantina kodundaki her değişiklik birleştirilmeden önce ayrı bir güvenlik incelemesinden geçer.
- Faz sonlarında bütün kod tabanı incelenir. Bulgular aşağıdaki tabloya yazılır.

## 5a. Public depo ve kişisel veri (ADR-013)

- Geliştirici makinesine özgü ortam bilgileri git dışı yerel not dosyasındadır. Lab sonuçları `lab/results/` altında kalır. İkisi de git dışıdır.
- `xtask check` commit öncesi token, anahtar, EICAR dizesi, özel ağ adresi (`10.`, `172.16–31.`, `192.168.`) ve kullanıcı yolu (`C:\Users\<ad>`) kalıplarını tarar. İzin verilen örnek adresler beyaz listede tutulur.
- Issue ve PR şablonları, tanılama paketlerinin public ortama eklenmemesi konusunda uyarır.

## 6. Açık bulgular

Faz 0 kapanış incelemesi (2026-11-15). Kapsam: `xtask lab` (journal, bekçi, işlemler, korunan değerler, problar), `korunan-degerler.ps1`, `xtask check` / `scan`, CI iş akışları ve lab'ı kullanan Wintun spike'ı. Bulgular kod okumasıyla bulundu; düzeltilenler testle doğrulandı.

| Kimlik | Bulgu | Önem | Durum | Faz |
|---|---|---|---|---|
| S-1 | Yükseltilmiş spike, bekçi olarak ayrı bir `xtask.exe` çalıştırıyordu. Bu ikili kullanıcının yazabildiği `target/` altında ve yalnızca tarihine bakılarak denetleniyordu | Yüksek | **Düzeltildi.** Bekçi spike'ın kendi ikilisidir (`lab watchdog`); yükseltilmiş işlem kendisi dışında bir ikili çalıştırmaz. Kalan risk: yönetici terminalinde çalıştırılan her geliştirme ikilisi kullanıcının yazabildiği bir dizindedir. Ürün ikilileri Program Files'ta ve imzalı olur | Faz 0; ürün Faz 5 |
| S-2 | PowerShell çağrıları devralınan `PSModulePath` ile modül yüklüyordu; ilk girdisi kullanıcının modül dizini. `system32()` yolu `SystemRoot` ortam değişkeninden geliyordu | Orta | **Düzeltildi.** Modüller yalnızca System32'den yüklenir. Sistem dizinleri Win32 API'sinden alınır (`winsys`); sahte `SystemRoot` ile alt işlem testi var | Faz 0–1 |
| S-3 | Korunan değerler: `xtask lab` senaryosu baştaki okuma başarısızken de sürüyor, sondaki okuma başarısızsa başarılı dönüyordu. Betik bölüm hatalarını yutuyordu; boş kalan bir bölüm sahte bir "değişmedi" verebilirdi | Orta | **Düzeltildi.** Baştaki okuma başarısızsa senaryo başlamaz, sondaki başarısızsa koşu başarısız sayılır (lab ve spike). Her bölüm ayrı okunur; okunamayan ya da eksik bölüm okumayı başarısız yapar (test) | Faz 0 |
| S-4 | `scan` UTF-8 olmayan dosyaları atlıyordu: ikili anahtar dosyaları ve UTF-16 metin taranmıyordu | Orta | **Düzeltildi.** UTF-8 ve UTF-16 (BOM'lu ve BOM'suz) çözülür; çözülemeyen metin bulgudur. Anahtar, sertifika, döküm ve iz dosyaları ile `lab/results/` ve `lab/local/` `.gitignore`'dan bağımsız bulgudur | Faz 1 |
| S-5 | `scan` kalıp boşlukları: CGNAT ve link-local IPv4, IPv6, e-posta, MAC, ek anahtar biçimleri. Çalışma ağacını tarıyordu; push öncesi denetim yoktu | Orta | **Düzeltildi.** Yeni kalıplar eklendi. Tarama kaynakları: çalışma ağacı, indeks (`--staged`) ve push edilen her commit (`pre-push` kancası, `cargo xtask hooks install`). Kalan sınırlar aşağıda | Faz 1 |
| S-6 | Spike'lar workspace dışında; `fmt`, `clippy`, `test` ve `deny` kapılarına girmiyor. `cargo xtask` takma adı `--locked` kullanmıyordu | Orta | **Kısmen düzeltildi.** Takma ad `--locked`. Spike'lar ürüne taşınana kadar workspace dışında ve derlenmeden durur. `spikes/ui` 1D sonunda silindi (2026-11-22); `wintun` Faz 2'de, `sandbox` Faz 3'te taşınıp silinir. Ürün kodu workspace'te ve kapıların içindedir | Faz 1–3 |
| S-7 | Kaynak taraması testi atlatılabiliyordu: parçalı argümanlar, eksik komut ve Win32 adları, sabit dizin ve uzantı listesi | Düşük | **Düzeltildi.** Bütün depo taranır (yerel ve üretilmiş dizinler hariç). Ek komut ve Win32 adları ile ek uzantılar var; parçalı argümanlar düzleştirilmiş metinde aranır (yerleştirilmiş örnekle denendi). Test bir uyarı düzeneğidir, güvenlik sınırı değildir | Faz 0 |
| S-8 | Nesne durumu okuması geçici bir CIM hatasını "yok" sayıyordu. Kaldırılmamış bir rota journal'da geri alınmış görünebilirdi | Düşük | **Düzeltildi.** Yalnızca `ObjectNotFound` "yok" sayılır, diğer hatalar okuma hatasıdır. Bu makinede beş "bulunamadı" durumunda ölçüldü; ayrıca testle doğrulandı | Faz 0 |
| S-9 | Yükseltilmiş yazma ve kilit yüzeyleri: journal dosyası önce denetlenip sonra oluşturuluyordu; kilit süresiz bekliyordu; test amaçlı `FileValue` işlemi üretim journal'ından erişilebilir; bekçi raporu yolunda bağlantı denetimi yok | Düşük | **Düzeltildi** (ürün journal'ı, 1B). Journal `create_new` ile oluşturulur; kilit en çok 10 sn beklenir. `FileValue` yalnızca `test-ops` özelliğiyle derlenen kodda vardır; ürün ikilisi onu bilinmeyen tür olarak okur ve dokunmaz. Journal ve bekçi raporu yolları txn kimliğinden türetilir, komut satırından yol alınmaz. `AlterDNS` → `journal` zincirindeki dizinlerden biri korumalı değilse bekçi hiçbir şeye dokunmaz. Lab journal'ı Faz 2'ye kadar eski kopyasıyla çalışır | Faz 1B |
| S-10 | Motor PID ile sonlandırılıyor; başlangıç zamanı doğrulamasıyla sonlandırma arasında küçük bir pencere var. "Kira ≤ 60 sn" geri almanın başlamasını sınırlıyor, bitmesini değil | Düşük | **Açık** | Faz 2: tutamaçla sonlandırma veya iş nesnesi |
| S-11 | `deny.toml` izin listesi `CONTRIBUTING.md` kural 8'den farklıydı: kullanılmayan ve OSI onaylı olmayan `CC0-1.0` vardı; Slint'in pano bağımlılığının lisansı `BSL-1.0` iki listede de yoktu; `wildcards = "deny"` yoktu. svc denetimi `apps/svc` yokken atlanıyor | Bilgi | **Düzeltildi.** `CC0-1.0` çıkarıldı. `BSL-1.0` (OSI onaylı, GPL-3.0 ile uyumlu) eklendi; arayüz spike'ının lisans denetimi geçiyor. `wildcards = "deny"` eklendi. Kural 8 ve iki `deny` dosyası aynı listeyi kullanıyor. svc denetimi Faz 1'de etkinleşir | Faz 0; svc Faz 1 |

### Faz 1A incelemesi (2026-11-20)

Kapsam: `winsys` (`unsafe` Win32 çağrıları), `core` (yapılandırma ve log maskeleme), `scan` / `check` ve push kancası. Bulgular kod okumasıyla bulundu; düzeltilenler testle doğrulandı.

**Düzeltilenler:**
- **Log maskeleme (yüksek):** Kaçışlı ters eğik çizgili yollarda kullanıcı adı maskelenmiyordu. Artık maskeleniyor: kaçışlı ayraçlar (Rust `{:?}`, JSON), büyük harf, WSL ve UNC yolları kapsanıyor. Ad boşluk içerebildiği için bir ayraç gelene kadar maskelenir; fazla maskeleme güvenli yöndür.
- **Push kancası:** Yalnızca son ağaca bakıyordu. Artık push edilen her commit'in eklediği ya da değiştirdiği dosyaları o commit'teki haliyle tarıyor; kanca girdisindeki commit kimlikleri yalnızca onaltılık olarak kabul ediliyor.
- **Tarama:** Şunlar artık yakalanıyor:
  - BOM'suz UTF-16 metin.
  - PGP ve şifreli minisign anahtarları.
  - Kelime içindeki token'lar; yeni token biçimleri.
  - SSH anahtarları, `.env.*`, `.npmrc` ve benzeri dosyalar.
  - Türkçe I/İ farkıyla yazılmış yerel terimler.

  Sertifika parmak izleri ve uzun IPv6 adresleri artık yanlış pozitif vermiyor. `--tree` revizyonu `-` ile başlayamaz.
- **Yapılandırma:**
  - Geçici dosya benzersiz adla ve `create_new` ile açılır.
  - Yedek alınamazsa uygulama varsayılanla devam eder.
  - Bozuk dosya ezilmeden önce yedeklenir.
  - Okunan dosya 64 KiB ile sınırlıdır.
- **`winsys`:** Yol arabelleği büyütmesi 32 768 birimle sınırlıdır.

**Kabul edilen sınırlar:**
- **İkili içerik:** Resim, arşiv ve PDF içeriği taranmaz. Bunlar inceleme sırasında elle kontrol edilir.
- **Yerel terimler:** Yerel terim listesi git dışındadır, bu yüzden CI'da çalışmaz. Kanca ve `cargo xtask check` yalnızca geliştirici makinesinde bu denetimi yapar.
- **Kancayı atlamak:** Kanca `--no-verify` ile atlanabilir ve push edilen dalın kendi `xtask` kodunu çalıştırır. CI son ağacı yine tarar.
- **`SHGetKnownFolderPath`:** Belgeler hata yolunda da belleğin serbest bırakılmasını ister, ancak `windows` crate'i bu yolda işaretçiyi döndürmez. Pratikte işaretçi NULL'dur; sızıntı olasılığı çok düşüktür.
- **Makine yapılandırma dizini:** ACL'i Faz 1B/1C'de veri dizini sertleştirmesiyle (K20) ayarlanır. O zamana kadar servis bu dosyayı yazmaz.

### Faz 1B incelemesi (2026-11-21)

Kapsam: `winsys` (korumalı dizin, sahip ve DACL denetimi, belirteç yükseltmesi), `journal` (dosya, txn, hedef izin listesi, kurtarma, gerçek kayıt defteri işlemleri, bekçi) ve `alterdns-maint`. Bulgular kod okumasıyla bulundu; düzeltilenler testle doğrulandı.

**Düzeltilenler:**
- **Journal dizini yalnızca yaprakta denetleniyordu (orta):**
  - Standart bir kullanıcı `%ProgramData%\AlterDNS`'i kurulumdan önce oluşturup sahibi olabilir. Bu durumda `journal`'ı denetimden sonra kendi dizinine giden bir bağlantıyla değiştirebilirdi.
  - SYSTEM bekçisi o journal'ı okuyup bekçi raporunu oraya yazardı.
  - Artık `AlterDNS` → `journal` zincirinin her halkası denetlenir (`verify_protected_chain`): reparse noktası olmamalı, sahibi güvenilir olmalı, başkasına yazma hakkı olmamalı.
  - `create_protected_dir` var olan bir dizini artık ancak bu denetimden geçerse kabul eder.
- **Kesilmiş son satır dosyada kalıyordu:** Sonraki satır ona eklenince dosya bir daha okunamıyordu. Artık açılışta satır sonu olmayan son parça kesilip atılır. Boş dosya (oluşturma yarıda kalmış) kayıtsız bir txn sayılır.
- **4 MiB sınırı geri almayı da durduruyordu:**
  - Sınıra yakın bir txn'de durum satırı yazılamıyordu ve bu, kalan bütün kayıtların geri alınmasını kesiyordu.
  - Artık sınır yalnızca yeni kayıtlara uygulanır; durum satırları ve commit 16 MiB'a kadar yazılabilir.
  - Durum satırı yine de yazılamazsa kayıt "başarısız" sayılır, açık kalır ve geri alma sürer.
- **Txn'ler ad sırasıyla geri alınıyordu:** Aynı değeri art arda değiştiren iki txn'de eskisi önce geri alınırsa çakışma görür, AlterDNS'in yazdığı değer yerinde kalırdı. Artık en yeni txn önce geri alınır.
- **ACE türleri:** DACL'de izin veren nesne ya da geri çağırmalı bir ACE varsa artık atlanmaz; dizin reddedilir.
- **SID işaretçisi:** SID işaretçisi artık ACE'nin başından türetilir; bütün SID'i kapsar.
- **Yükleme zamanı DLL'leri (K18):** `/DEPENDENTLOADFLAG:0x800` bayrağının exe'de bulunduğu PE yükleme yapılandırmasından okunarak test edilir.

**Açık ve sonraki fazlara kalanlar:**
- **Journal dizininin tutamaçla sabitlenmesi (1C):** Dizin, bekçi ve servis çalıştığı sürece paylaşımsız bir tutamaçla açık tutulur ve denetimler bu tutamaç üzerinden yapılır. Zincir denetimi tek başına yeterli: korumalı dizinleri yalnızca yöneticiler taşıyabilir. Tutamaç ek bir katmandır.
- **`FileInstall` hedef denetimi (1C):** `FileInstall` hedefi şimdilik yalnızca sözcüksel olarak denetleniyor. Gerçek işlem eklenince dosya tutamaçla açılır, reparse noktası reddedilir ve göreli yolda `:` (alternatif veri akışı) kabul edilmez.
- **Askıdaki ebeveyn (Faz 2):** Kira dolduğunda dosyayı tutan ebeveyn sonlandırılmaz. Bekçi 10 sn bekler ve geri almayı açılış kurtarmasına bırakır (raporda `InUse`). S-10 ile birlikte tutamaçla izlenecek.
- **Paketleme (Faz 5):** Ürün paketi, `test-ops` olmadan ayrı bir derlemeyle üretilir. İkilide `ALTERDNS_TEST_JOURNAL_DIR` dizesinin bulunmadığı doğrulanır, çünkü `xtask check` test derlemesini aynı `target` dizinine yazar.

**Kabul edilen sınırlar (1B):**
- **Bekçi stdin satır uzunluğu sınırsız:** Boruyu yalnızca bekçiyi başlatan işlem tutar ve o işlem güvenilirdir.
- **Yeni satır türü şema artışı ister:** Eski bir ikili bilinmeyen bir satır türünü, durumu ya da ömrü ayrıştıramaz ve o dosyayı kurtaramaz. Diğer dosyalar etkilenmez. Bilinmeyen değişiklik türü ise zaten "açık, dokunulmadı" sayılır.

Temiz bulunan alanlar:
- **Komut enjeksiyonu:** PowerShell'e giden değerler ASCII izin listesinden geçiyor.
- **Journal:** Doğrulama sağlam; geri alma kendi nesnelerini yalnızca kaldırıyor.
- **Adaptör:** Kaldırılacak adaptörün kimliği doğrulanıyor.
- **Bekçi protokolü:** Sağlam.
- **Korunan değerler betiği:** Salt okunur ve ikiliye gömülü.
- **Probalar:** TLS doğrulaması platform doğrulayıcısıyla yapılıyor; zaman aşımları ve boyut sınırları var.
- **CI:**
  - `pull_request_target` yok; `contents: read`; `persist-credentials: false`.
  - Action'lar SHA ile sabitli; önbellek ve gizli bilgi yok.
  - Dependabot bekleme süresi tanımlı.
- **Bağımlılık kaynakları:** Kaynak ve yank politikası katı.

## 7. Güvenlik açığı bildirimi

Yayın öncesinde depoya `SECURITY.md` (GitHub biçiminde) eklenir. İçeriği: bildirim yolu (e-posta veya GitHub Security Advisories) ve yanıt süresi hedefi.

### Faz 1C incelemesi (2026-11-21)

Kapsam: `winsys` (named pipe, çağıran belirteci, SCM, dizin sabitleme), `ipc` (çerçeve, yetki, sunucu, istemci, olaylar), `alterdns-svc`, `journal` kurulum işlemleri ve `xtask dev-install`. Plan aşamasında tasarım ayrıca incelendi; bulgular kod okumasıyla ve ölçümle bulundu, düzeltilenler testle doğrulandı.

**Tasarım aşamasında düzeltilenler:**
- **Pipe DACL'i (yüksek):**
  - Belgelerdeki "INTERACTIVE oku/yaz" (`GRGW`), `FILE_APPEND_DATA` = `FILE_CREATE_PIPE_INSTANCE` verir.
  - Bu makinede ölçüldü: normal kullanıcı, servis çalışırken aynı ada ikinci bir sunucu örneği açabiliyordu. Örnekler aynı güvenlik tanımlayıcısını paylaştığından istemcinin sahip denetimi bunu ayırt edemezdi.
  - Artık IU yalnızca `FILE_GENERIC_READ | FILE_WRITE_DATA` (`0x12008b`) alır; test, kullanıcının yeni örnek açamadığını doğrular.
- **Kimliğe bürünme:** Yalnızca pipe'tan veri okunduktan sonra çalışır. Tanımlama düzeyindeki belirteç ancak `OpenAsSelf = TRUE` ile açılır.
- **Örtüşmeli G/Ç:** İptalden sonra tamamlanma her zaman beklenir; arabellek ve `OVERLAPPED` o ana kadar yaşar.
- **Ad boşa çıkmaz:** Bir istemci bağlanınca sonraki dinleyen örnek hemen açılır.
- **CI yönetici belirteciyle koşar:** Testler `dev-install`'ı hiçbir zaman çağırmaz; yönetici terminalinde ya da CI'da gerçek kuruluma dönüşürdü.

**Uygulamada bulunanlar:**
- **Hata kodu kayboluyordu:** Hata yanıtından hemen sonra `DisconnectNamedPipe` okunmamış veriyi siliyordu. Artık istemcinin kapatması beklenir.
- **`vcruntime140.dll` bağımlılığı:** Yalnızca System32'den yükleme bayrağıyla birlikte, VC++ Redistributable olmayan makinede ikililer açılmazdı. C çalışma zamanı statik bağlanır; testler statik içe aktarmaların hepsinin KnownDLL, `ntdll` ya da API kümesi olduğunu PE'den doğrular. Uygulama dizinine bırakılan DLL'i yükleyecek bir içe aktarma kalmadığından DLL bırakma testinin yerini bu denetim alır.
- **Eski bir test kurulu servisi silebilirdi:** `ServiceCreate` yazımını `Absent` ile çağırıyordu. Kaldırıldı; servis ikilisi doğrulaması saf bir işlevle sınanır.

- **Servis oluşturma tutamacında `SERVICE_START` yoktu (gerçek koşuda bulundu):** `SC_ACTION_RESTART` başarısızlık eylemleri bu hakkı ister. İlk yönetici koşusu "Access is denied" ile geri alındı; kopyalanan iki dosya geri alındı ve kalıntı kalmadı. Hak eklendi; her ayar adımı hatasında adını taşır. Servis silinirken işlemin bitmesi de beklenir: "durdu" bildiriminden sonra işlem ikilisini bir süre kilitli tutabilir.

**Gerçek koşu (2026-11-22, kullanıcının yönetici terminali):** `dev-install` → T-SVC-001 ve T-SVC-003 (tırnaklı yol, otomatik başlangıç, LocalSystem, hizmet SID'i, 5/30/60 sn, 86 400 sn, hata kodunda kurtarma, ayrıcalıklar) → canlı IPC (sahibi SYSTEM, durum, yetkisiz yöntem reddi) → `dev-uninstall` (üç kayıt geri alındı, kalıntı taraması temiz). Hop limit, DNS ve DoH önce ve sonra aynı.

**İnceleme bulguları (düzeltildi):**
- **Pipe sahibi (orta, işlevsel):** İstemci sahibin SYSTEM olmasını ister. Ancak DACL'de sahip yoktu ve servis belirtecinin varsayılan sahibi Administrators olabilirdi; sağlık denetimi ve tepsi bağlanamazdı. Güvenlik tanımlayıcısı artık `O:SY` ile başlar. Kullanıcı kendi açtığı pipe'a bu sahibi veremez.
- **`dev-install --debug` test derlemesini kurabilirdi (düşük):** `cargo xtask check` `test-ops` derlemelerini `target\debug`'a yazar. Artık `dev-install` test dizelerini taşıyan ikiliyi reddeder; ürün derlemesinde bu dizelerin olmadığı test edilir.
- **Kimlik doğrulama öncesi bellek büyütme (düşük):** İlk mesaj 4 KiB, sonraki istekler 64 KiB ile sınırlı; aşan gövde okunmadan bağlantı kapanır.

**Sertleştirmeler (düzeltildi):**
- `SeImpersonatePrivilege` servisin ayrıcalıklarından çıkarıldı; tanımlama düzeyi kimlik okumaya yeter. Gerçek serviste doğrulandı: ayrıcalık listesi yalnızca `SeChangeNotifyPrivilege`, normal kullanıcı bağlanıp durum alabiliyor, ayrıcalıklı yöntem reddediliyor.
- `OVERLAPPED`, işlem sürerken yalnızca ham işaretçiyle kullanılır; çekirdeğin yazdığı yapıya Rust referansı tutulmaz.
- `logs` dizini de servis boyunca sabitlenir.
- Gereken ayrıcalık listesinin okunması arabellek sonuyla sınırlıdır.

**Açık ve kabul edilenler:**
- **Olay yönlendirme oturuma göredir:** Aynı oturumdaki iki kullanıcı (ör. `runas`) kullanıcıya özgü olayları paylaşır. Faz 1'de kullanıcıya özgü olay yok. Gelince yönlendirmeye SID de eklenir.
- **Boşta bağlantılar:** El sıkışmadan sonra boşta kalma süresi sınırı yok; bir kullanıcı 16 örneğin hepsini tutabilir. Yerel hizmet engelleme olarak kabul edildi; oturum başına sınır Faz 6'da değerlendirilir.
- **`verify_protected_dir` yol üzerinden denetler:** Reparse ve güvenlik okuması arasında küçük bir pencere var. Üst dizinler yalnızca yöneticilerin yazabildiği dizinlerdir ve servis dizinleri sabitler. Tutamaçla denetim Faz 6'da.
- **RDP ve `INTERACTIVE` (K10):** Microsoft belgesine göre doğrulandı. Microsoft Learn "Security identifiers" sayfası, S-1-5-4 satırı: Uzak Masaüstü Hizmetleri bağlantısıyla açılan oturum etkileşimli sayılır ve kullanıcının erişim belirtecinde Interactive SID bulunur; ayrıca Remote Interactive Logon SID (S-1-5-14) eklenir. `InteractiveUser` sınıfı bu yüzden RDP oturumlarını da kabul eder. RDP ile ölçülmedi. Bu makinedeki konsol oturumunda S-1-5-4 ve CONSOLE LOGON (S-1-2-1) etkin.
### Faz 1D incelemesi (2026-11-22)

Kapsam: `winsys` tepsi, pencere sertleştirme, tek örnek, kabuk, ekran ve tema; `alterdns-ui.exe` (tepsi ve ana pencere kipleri, durum döngüsü); journal `StartupEntry` (HKLM Run); `xtask dev-uninstall`'ın arayüzü kapatması ve kullanıcı verisini silmesi. Kod okumasıyla incelendi; yüksek ve orta bulgu yok.

**Uygulamada bulunanlar:**
- **comctl32 alt sınıfı ikiliyi açılmaz yapıyordu:** `SetWindowSubclass` comctl32 v5.82'de yalnızca sıra numarasıyla dışa aktarılır; bildirimsiz bir test ikilisi `STATUS_ENTRYPOINT_NOT_FOUND` ile açılmadı. Aynı içe aktarma servis ikilisine sızsaydı servis başlamazdı. Alt sınıf `GWLP_WNDPROC` zinciriyle kurulur.
- **Kaldırma açık tepsiyle yarım kaldı (gerçek koşuda):** Kurulu `alterdns-ui.exe` çalışırken silinemedi. Servis ve Run değeri geri alındı; dosya journal'da açık kaldı ve ikinci koşuda temizlendi. `dev-uninstall` artık arayüze kapanma iletisi gönderir ve arayüzün kullanıcı verisini siler.

**Gerçek koşu (2026-11-22, kullanıcının yönetici terminali ve masaüstü):** `dev-install` → T-UI-001/002/005/006/008 → dil değişimi → servis durdurma ve başlatma (`ADNS-IPC-001` rozeti, `ADNS-SVC-002` bildirimi) → `--tray --logon` (açılır; kullanıcı başlatmayı kapatınca açılmaz) → `dev-uninstall` (kalıntı taraması temiz). Hop limit, DNS ve DoH önce ve sonra aynı.

**İnceleme bulguları (düzeltildi, hepsi düşük):**
- **`WM_SETTINGCHANGE` lParam'ı işaretçi olarak okunuyordu:** Aynı oturumdan `PostMessage` ile gelen geçersiz bir değer işlemi düşürebilirdi. lParam artık hiç okunmaz; her ayar değişiminde tema yeniden okunur.
- **Kurulu ikili doğrulamasında korumalı zincir denetimi yoktu:** Servis yolu ve Run komutu artık sürüm dizininden kurulum köküne kadar yalnızca yöneticilerin yazabildiği dizinlerde olmalıdır (K20).
- **Tek örnek kilidi sessizce çıkıyordu:** Kilit başkasındayken pencere bulunamazsa (ilk örnek henüz pencere açmamış ya da adı başka bir işlem tutuyor) 3 sn beklenir, sonra `ADNS-UI-001` gösterilir.
- **Ön plan izni:** `ASFW_ANY` yerine yalnızca başlatılan ya da öne getirilecek pencere işlemine izin verilir.
- **Gereksiz ileti filtresi:** `TaskbarCreated` için `MSGFLT_ALLOW` kaldırıldı; düşük bütünlükten gelen iletiler UIPI'de kalır.
- **Geri çağrılarda panik riski:** Pencere ve kanca işleyicilerinde iş parçacığı yerel erişimi `try_with`, ödünç alma `try_borrow` ile yapılır; kayıt yazılamazsa önceki pencere işleyicisi geri yüklenir. Kayıtlı ileti 0 dönerse `WM_NULL` ile eşleşmez.
- **`dev-uninstall` pencere sahibini doğrulamıyordu:** Kapanma iletisi yalnızca sahibi kurulu `alterdns-ui.exe` olan pencerelere gider.
- **Önceden var olan Run değeri:** Geri almada doğrulamadan geçemeyeceği için `dev-install` bu durumda baştan reddeder; değere dokunmaz.

**Doğru bulunanlar (özet):** `Rc` işaretçisiyle ileti penceresi ömrü, `WNDPROC` dönüşümü ve `WM_NCDESTROY` sırası, kanca, yayın kodlarının sınır denetimi, Run komutunun birebir biçim doğrulaması ve yazmadan önce yapılması, bağlantı izin listesi (depo öneki ve zorunlu `/`), `remove_dir_all`'ın bağlantıları izlememesi.

**Açık ve kabul edilenler:**
- **Yayınlar oturumla sınırlı:** Başka bir kullanıcı oturumunda açık arayüz kapatılamaz; kaldırma bu durumda temiz olmayan sonuçla durur ve iletide belirtilir.
- **Kullanıcı verisinin silinmesi:** `dev-uninstall` yükseltilmiş belirtecin profilindeki `%LOCALAPPDATA%\AlterDNS`'i siler; başka bir yönetici hesabıyla yükseltilirse o hesabın verisidir. Ürün kaldırıcısında kullanıcıya sorulması Faz 5'te değerlendirilir.
- **Aynı bütünlükteki işlemler:** Aynı kullanıcının orta bütünlükteki bir işlemi arayüze ileti gönderebilir ya da onu sonlandırabilir; bu bir güvenlik sınırı sayılmaz.

### Faz 1E incelemesi (2026-11-23)

Kapsam: `winsys` ölçüm ilkelleri (işlem ve iş parçacığı sayaçları, ilk çizim sondası), arayüzün ölçüm iletileri (`MSG_PERF_FLYOUT_OPEN` ve `MSG_PERF_FLYOUT_CLOSE`, `--perf`), `xtask perf` ve `xtask test --live`. Kod okumasıyla incelendi; kritik bulgu yok.

**İnceleme bulguları (düzeltildi):**
- **`perf ui` tepsiyi kapatıp ölçmeden durabiliyordu:** Kapatma kararı pencere sınıfı başına sırayla veriliyordu. Kurulu tepsi kapatıldıktan sonra ana pencere başka bir kopyadan çalışıyorsa ölçüm başlamıyordu. Karar artık iki sınıf için kapatmadan önce verilir. Ölçüm için kapatılan kurulu tepsi sonda yeniden başlatılır.
- **Regresyon tabanı:** Taban, ölçülen bölümü taşıyan en yeni eski dosyadır; yoksa bu açıkça bildirilir. Bütçeyi aşan ya da kötüleşen sonuç dosyaya yazılmaz, böylece sonraki günün tabanı olamaz.
- **Ana pencere testleri kullanıcının penceresini kapatabiliyordu:** Kapanma iletisi bütün AlterDNS ana pencerelerine gidiyordu. Kullanıcının penceresi açıksa testler atlanır.

**Açık ve kabul edilenler:**
- **Ölçüm iletileri ürün ikilisinde:** Aynı oturumdaki bir işlem flyout'u açıp kapatabilir ya da `--perf` ile ana pencere başlatabilir. Bu, yukarıdaki "aynı bütünlükteki işlemler" kabulünün kapsamındadır. Tepsi çökmez, veri sızmaz. İlk çizim sondası yalnızca `--perf` ile kurulur ve penceresiyle birlikte kalkar.
