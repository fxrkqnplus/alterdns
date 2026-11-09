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
| WinDivert sürücüsü | Başka yerel yazılım | Sürücüyü kötüye kullanma (yönetici ister) | Resmi imzalı sürücü; AlterDNS dizini korumalı; başka WinDivert istemcileri izlenir |
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
- **Kod imzalama:** Sertifika SignPath Foundation'dadır (D3; ADR-012); projede özel anahtar bulunmaz. Yine de `.pfx` / `.key` dosyaları `.gitignore` ve `.claude/settings.json` deny kuralıyla engellenir.
- **API anahtarları** (abuse.ch, Safe Browsing): Dağıtılan ikiliye gömülebilecek anahtarlar yalnızca istemci için tasarlanmış, kısıtlanabilen anahtarlardır (koşullara bakılır). Kullanıcıya ait anahtarlar (VirusTotal) DPAPI ile saklanır.

## 5. Güvenlik incelemesi süreci

- IPC, journal, güncelleyici, native host, sandbox aracısı ve karantina kodundaki her değişiklik birleştirilmeden önce `backend-design:security-reviewer` ajanıyla incelenir. `security-guidance` eklentisi düzenleme sırasında açık kalır.
- Faz sonlarında bütün kod tabanı incelenir. Bulgular aşağıdaki tabloya yazılır.

## 5a. Public depo ve kişisel veri (ADR-013)

- Geliştirici makinesine özgü ortam bilgileri `CLAUDE.local.md` dosyasındadır. Lab sonuçları `lab/results/` altında kalır. İkisi de git dışıdır.
- `xtask check` commit öncesi token, anahtar, EICAR dizesi, özel ağ adresi (`10.`, `172.16–31.`, `192.168.`) ve kullanıcı yolu (`C:\Users\<ad>`) kalıplarını tarar. İzin verilen örnek adresler beyaz listede tutulur.
- Issue ve PR şablonları, tanılama paketlerinin public ortama eklenmemesi konusunda uyarır.

## 6. Açık bulgular

| Kimlik | Bulgu | Önem | Durum | Faz |
|---|---|---|---|---|
| — | — | — | — | — |

## 7. Güvenlik açığı bildirimi

Yayın öncesinde depoya `SECURITY.md` (GitHub biçiminde) eklenir. İçeriği: bildirim yolu (e-posta veya GitHub Security Advisories) ve yanıt süresi hedefi.
