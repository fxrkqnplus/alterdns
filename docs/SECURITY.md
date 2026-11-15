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
| S-2 | PowerShell çağrıları devralınan `PSModulePath` ile modül yüklüyordu; ilk girdisi kullanıcının modül dizini. `system32()` yolu `SystemRoot` ortam değişkeninden geliyor | Orta | **Kısmen düzeltildi.** Modüller yalnızca `System32\WindowsPowerShell\v1.0\Modules`'dan yüklenir (test). `SystemRoot` xtask'ta doğrulanmıyor; spike'ın yükseltilmiş komutları doğruluyor | Faz 1: Windows dizini Win32 API'sinden alınır |
| S-3 | Korunan değerler: `xtask lab` senaryosu baştaki okuma başarısızken de sürüyor, sondaki okuma başarısızsa başarılı dönüyordu. Betik bölüm hatalarını yutuyordu; boş kalan bir bölüm sahte bir "değişmedi" verebilirdi | Orta | **Düzeltildi.** Baştaki okuma başarısızsa senaryo başlamaz, sondaki başarısızsa koşu başarısız sayılır (lab ve spike). Her bölüm ayrı okunur; okunamayan ya da eksik bölüm okumayı başarısız yapar (test) | Faz 0 |
| S-4 | `scan` UTF-8 olmayan dosyaları atlıyor: ikili anahtar dosyaları ve UTF-16 metin taranmıyor | Orta | **Açık.** `.gitignore`'a ek hassas uzantılar eklendi. Yasaklı uzantı ve yol listesi, UTF-16 çözümü `xtask check`'in tam sürümünde | Faz 1 |
| S-5 | `scan` kalıp boşlukları: CGNAT ve link-local IPv4, IPv6, e-posta, MAC, ek anahtar biçimleri. Çalışma ağacını tarıyor, hazırlanan içeriği değil. Yerel terim listesi CI'da yok; push öncesi denetim yok | Orta | **Açık** | Faz 1 |
| S-6 | Spike'lar workspace dışında; `fmt`, `clippy`, `test` ve `deny` kapılarına girmiyor. `cargo xtask` takma adı `--locked` kullanmıyordu | Orta | **Kısmen düzeltildi.** Takma ad `--locked`. Spike'lar Faz 0'la birlikte emekliye ayrılır; ürün kodu workspace'te ve kapıların içindedir | Faz 1 |
| S-7 | Kaynak taraması testi atlatılabiliyordu: parçalı argümanlar, eksik komut ve Win32 adları, sabit dizin ve uzantı listesi | Düşük | **Düzeltildi.** Bütün depo taranır (yerel ve üretilmiş dizinler hariç). Ek komut ve Win32 adları ile ek uzantılar var; parçalı argümanlar düzleştirilmiş metinde aranır (yerleştirilmiş örnekle denendi). Test bir uyarı düzeneğidir, güvenlik sınırı değildir | Faz 0 |
| S-8 | Nesne durumu okuması geçici bir CIM hatasını "yok" sayıyordu. Kaldırılmamış bir rota journal'da geri alınmış görünebilirdi | Düşük | **Düzeltildi.** Yalnızca `ObjectNotFound` "yok" sayılır, diğer hatalar okuma hatasıdır. Bu makinede beş "bulunamadı" durumunda ölçüldü; ayrıca testle doğrulandı | Faz 0 |
| S-9 | Yükseltilmiş yazma ve kilit yüzeyleri: journal dosyası önce denetlenip sonra oluşturuluyordu; kilit süresiz bekliyordu; test amaçlı `FileValue` işlemi üretim journal'ından erişilebilir; bekçi raporu yolunda bağlantı denetimi yok | Düşük | **Kısmen düzeltildi.** Journal `create_new` ile oluşturulur; kilit en çok 10 sn beklenir. Açık: `FileValue` `%TEMP%\alterdns-*` ve 4 KB ile sınırlı; bekçi raporu yolu | Faz 1 (journal crate'i) |
| S-10 | Motor PID ile sonlandırılıyor; başlangıç zamanı doğrulamasıyla sonlandırma arasında küçük bir pencere var. "Kira ≤ 60 sn" geri almanın başlamasını sınırlıyor, bitmesini değil | Düşük | **Açık** | Faz 2: tutamaçla sonlandırma veya iş nesnesi |
| S-11 | `deny.toml` izin listesi `CONTRIBUTING.md` kural 8'den farklı (`CC0-1.0`, `GPL-3.0-only`); `wildcards = "deny"` yok; svc denetimi `apps/svc` yokken atlanıyor | Bilgi | **İncelenecek.** Lisans politikası farkı kullanıcı kararı gerektirir; svc denetimi Faz 1'de etkinleşir | Faz 1 |

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
