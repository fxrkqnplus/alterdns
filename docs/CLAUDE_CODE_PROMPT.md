# AlterDNS — Claude Code Geliştirme Promptu

> **Kullanım:** Claude Code'u proje klasöründe başlat ve şunu yaz:
> *"docs/CLAUDE_CODE_PROMPT.md, CLAUDE.md ve CLAUDE.local.md dosyalarını oku. Bölüm 14'teki Faz 0 adımlarıyla başla. Her fazın sonunda Bölüm 13'teki biçimde rapor ver ve devam etmeden önce onayımı bekle."*
>
> Hazırlanma tarihi: 2026-11-09 (güncelleme: kullanıcı kararları D1–D3). Dış kaynaklar bu tarihte kontrol edildi. Sürüm ve lisans bilgilerini uygulamaya geçmeden önce yeniden doğrula. Geliştirici makinesine özgü ortam bilgileri yalnızca git dışı `CLAUDE.local.md` dosyasındadır.

---

## 1. Rolün ve hedefin

Sen bu projenin baş geliştiricisisin. Hedefin AlterDNS'i aşağıdaki gereksinimlerin tamamını karşılayan, düşük kaynak tüketen, kararlı ve bakımı kolay bir Windows uygulaması olarak sıfırdan geliştirmek.

**Uygulama adı:** AlterDNS
**Yapımcı ve telif sahibi:** Furkan Okan
**Lisans:** GPL-3.0-or-later
**Depo:** `https://github.com/fxrkqnplus/alterdns` (public)

"Furkan Okan" adı şu yerlerde kullanılır: kaynak kod telif başlıkları, exe sürüm bilgileri (CompanyName, LegalCopyright), kaldırma kaydı (Publisher), uzantı manifestindeki `author`, Hakkında ekranı. Üçüncü taraf bileşenlerin telif bildirimleri korunur. SignPath imzası alındığında Authenticode imzasında yayıncı "SignPath Foundation" görünür. Bu programın şartıdır; dosya bilgilerindeki telif sahibi değişmez.

**Çalışma ilkelerin:**
- **Önce doğrula, sonra uygula.** Sürüm, lisans ve API davranışlarını güncel belgeden kontrol et (Context7 MCP, resmi depo, crates.io). Doğrulanmamış varsayımları "doğrulanacak" diye yaz.
- **Ölçmeden "çalışıyor" deme.** Her kabul ölçütü bir test veya ölçümle kanıtlanır.
- **Geri alınamayacak adımda dur ve sor.** Kapsam: sistem ayarı, sürücü, servis, push, release, depo ayarı, imzalama ve kullanıcının mevcut yazılımına dokunan her işlem.

## 2. Hedef platform ve geliştirici ortamı

- **Hedef:** Windows 11 (öncelikli) ve Windows 10 22H2 (en iyi çaba), yalnızca x64. Bağlantı modülü ARM64'te desteklenmez.
- **Geliştirici ortamı:** Windows sürümü, güvenlik ayarları, ağ (telefon hotspot'u), hop limit, araç zinciri ve yerel kanıtlar `CLAUDE.local.md` dosyasındadır.
- **Yerel ortam notu (2026-11-09):** Windows SDK ve Build Tools Faz 0 başında onarıldı; `cargo build` çalışıyor. Makinedeki eski DPI paketi (servis, görev, WinDivert sürücüsü) kaldırıldı; D6 kapandı.
- **Logolar:** `alterdns.png` (1007×1026, opak siyah zemin) ve `alterdns.ico` (16–256 px, opak; 16 px'te monogram okunmuyor). Kare ana görsel, saydam varyantlar ve sadeleştirilmiş tepsi glifi üretilmeli.

## 3. Değişmez kurallar

`CLAUDE.md` → "Değişmez kurallar" bölümü bağlayıcıdır. Özet:

1. Ağa dokunan her deneme journal, zaman sınırı, ayrı işlemde geri alma bekçisi ve kullanıcının o oturumdaki onayıyla yapılır.
2. Hop limit/TTL okunur, asla yazılmaz.
3. Zararlı yazılım ana makinede çalıştırılmaz.
4. AV ve Smart App Control kapatılmaz; istisna eklenmez.
5. Makinedeki başka DPI araçlarına onaysız dokunulmaz.
6. Depo public: gizli ve kişisel bilgi commit edilmez.
7. Bulgu uydurulmaz.
8. Bütün bileşenler GPL-3.0 ile uyumlu, OSI onaylı lisanslı olur. Kapalı bileşen kullanılmaz.

## 4. Teknoloji kararları (ayrıntı: `docs/DECISIONS.md`)

- **Dil: Rust.** Gerekçeler:
  - Güvenilmeyen girdiyi ayrıştırma ve paket işleme için bellek güvenliği.
  - Çöp toplayıcı olmadığı için düşük RAM.
  - Resmi `windows` crate'i.
  - YARA-X yerel Rust kütüphanesi.
  - Tek ikili dağıtım ve `cargo-deny`.

  Alternatifler (C++, C#/.NET + WinUI 3, Go) ADR-001'de karşılaştırıldı. Uzantı zorunlu olarak TypeScript.
- **Lisans: GPL-3.0-or-later** (D2). SignPath Foundation'ın ücretsiz OSS imzası için OSI lisansı ve "kapalı bileşen yok" şartı gerekiyor.
- **Arayüz:** Varsayılan aday **Slint**, **GPL-3.0 lisans seçeneğiyle**. Royalty-free lisansı kapalı bir lisans olduğu için kullanılmaz. Slint "GPL-3.0-only" olduğundan dağıtılan birleşik ikili GPL-3.0 koşullarına tabidir; proje kodu "or-later" kalır. Seçim Faz 0 UI spike'ında kesinleşir. Yedekler: Win32 + Direct2D/DirectWrite, iced/egui (MIT/Apache). WebView2 tabanlı seçenekler RAM bütçesi nedeniyle varsayılan değil.
- **Paket motoru:** Resmi, imzalı **WinDivert 2.2.2-A**. LGPL-3.0 seçeneği kullanılır; DLL dinamik bağlanır. Teknikler Rust'ta yeniden uygulanır:
  - GoodbyeDPI Apache-2.0 lisanslı; GPL-3.0 ile uyumlu. Kod uyarlanırsa NOTICE dosyasına yazılır.
  - zapret'ten kod alınmadan önce lisansı doğrulanır ve GPL-3.0 uyumu kontrol edilir.

  Upstream ikililer yalnızca lab'da kullanılır; ürünle dağıtılmaz.
- **Tarama:**
  - YARA-X (BSD-3)
  - Defender entegrasyonu (MpCmdRun, AMSI)
  - Yerleşik statik analizörler
  - Hash itibarı
  - RAR desteği: UnRAR **kullanılmaz** (özgür olmayan lisans). Seçenek, RAR4/RAR5 okuma desteği olan libarchive (BSD-2); sürüm desteği doğrulanacak. Uygun değilse RAR içeriği "incelenemedi" olarak raporlanır.
  - ClamAV (GPL-2.0) isteğe bağlı ve ayrı işlemde çalışır; UnRAR modülü olmadan derlenir veya kullanılır.
  - YARA kural setlerinden yalnızca açık lisanslı olanlar kullanılır.
- **Tarayıcı:**
  - MV3 uzantı (Chrome ve Edge, sonra diğer Chromium tarayıcılar) ve native messaging host.
  - **Mağaza yok:** Uzantı uygulama klasöründen geliştirici moduyla yüklenir (ADR-011).
  - Engelleme, istek ağa çıkmadan declarativeNetRequest ile yapılır.
- **Kurulum ve güncelleme:** Rust önyükleyici ve bileşen yöneticisi tek motordur. A/B sürüm dizinleri kullanılır. Ed25519 imzalı manifest **public deponun GitHub Releases sayfasından** alınır (D1). Otomatik geri dönüş vardır.
- **Güvenilir derleme ve imzalama (yalnız ücretsiz):** GitHub Actions (public depo), sabitlenmiş araç zinciri, `--locked`, deterministik derleme bayrakları, SBOM, **GitHub Artifact Attestations** (Sigstore, `gh attestation verify`), Ed25519 manifest. Onaylanırsa **SignPath Foundation** ile Authenticode imzası eklenir (ADR-012).

## 5. Mimari özet (ayrıntı: `docs/ARCHITECTURE.md`)

```
alterdns-svc.exe   (Windows servisi, LocalSystem)   bağlantı motoru, itibar DB, güncelleyici, journal, IPC sunucusu
alterdns-tray.exe  (kullanıcı oturumu)               tepsi simgesi, flyout, bildirimler, indirme izleyici, tarama aracısı
alterdns-ui.exe    (isteğe bağlı açılır; kapanınca çıkar)  ana pencere
alterdns-scan.exe  (AppContainer + Low IL + Job Object, ağsız)  güvenilmeyen içerik ayrıştırma ve analiz
alterdns-nmh.exe   (native messaging host)           uzantı ↔ servis köprüsü
alterdns-setup.exe / alterdns-maint.exe              kurulum, bileşen ekleme/kaldırma, kaldırma, geri dönüş
%ProgramFiles%\AlterDNS\browser-extension\           sabit yolda paketlenmemiş MV3 uzantısı (geliştirici moduyla yüklenir)
```

## 6. Gereksinimler ve kabul ölçütleri

Her madde bir veya daha fazla teste bağlıdır. Test kimlikleri `docs/TESTING.md` belgesindedir.

### 6.1 Dosya güvenliği

**Kapsam:**
- Desteklenen türler:
  - Arşivler: ZIP, RAR (libarchive ile, doğrulanacak), 7Z, TAR/GZ, CAB, ISO/IMG, VHD/VHDX
  - Çalıştırılabilirler: EXE, DLL, SYS, MSI, MSIX
  - Betikler: BAT, CMD, PS1, VBS, JS, HTA, WSF
  - Belgeler ve web içeriği: LNK, Office (OLE/OOXML), PDF, HTML/SVG, CHM
  - Medya: görseller, ses, video
  - Diğer her tür
- Tür içerikten tespit edilir. Uzantı-içerik uyuşmazlığı, çift uzantı ve RTLO karakteri ayrıca bulgu sayılır.
- Gömülü içerik özyinelemeli incelenir. Sınırlar: derinlik ≤ 5, toplam ≤ 1 GB, sıkıştırma oranı ≤ 100×, öğe sayısı ≤ 10.000. Yol aşımı engellenir.

**Koruma:**
- Dosya hiçbir zaman çalıştırılmaz.
- Ayrıştırma `alterdns-scan.exe` içinde yapılır: AppContainer, ağ yok, Low IL, Job sınırları (bellek 256 MB varsayılan, üst sınır 512 MB; CPU süresi; alt işlem yasak). Dosya salt okunur handle olarak aktarılır.
- Windows Sandbox ile dinamik inceleme deneyseldir.

**Kaynaklar:**
- Defender (MpCmdRun; çıkış kodları doğrulanacak) ve AMSI.
- Açık lisanslı YARA-X kuralları.
- Hash itibarı: MalwareBazaar (Auth-Key), isteğe bağlı olarak kullanıcının VirusTotal anahtarı. Yalnız hash gönderilir; dosya yükleme her dosya için onay ister.
- Statik analizörler.
- MOTW (`Zone.Identifier`) bilgisi URL itibarıyla çapraz kontrol edilir.

**Rapor:**
- Karar: `Güvenli` ("bilinen tehdit bulunamadı") / `Şüpheli` / `Riskli`.
- Her bulgu için kaynak, önem, güven, gerekçe ve konum verilir.
- **"İncelenemeyenler"** bölümü vardır. Çalıştırılabilir bir parça incelenemezse karar en az `Şüpheli (eksik inceleme)` olur.
- Rapor JSON olarak saklanır, HTML olarak dışa aktarılabilir.

**Kaldırma:**
- Karantina varsayılandır; kalıcı silme iki adımlı onay ister.
- İşlemler handle tabanlıdır (TOCTOU koruması).
- Kilitli dosya yeniden başlatmada silinir.
- Sonrasında **doğrulama** yapılır: yol, hash ve çalışan işlem kontrol edilir.

### 6.2 Tarayıcı koruması

**Engelleme anı:**
- Riskli adres, istek ağa çıkmadan engelleme sayfasına yönlendirilir (DNR).
- DNR kapasitesini aşan URL düzeyindeki girdiler için yedek yol: `webNavigation` → native host → `tabs.update`. Bu yolun sınırlaması belgelenir.
- **Test ölçütü:** Yerel test sunucusu engellenen adres için 0 istek alır.

**Engelleme sayfası:**
- Gösterilenler: kategori, kaynak listeleri ve tarihleri, adres (tıklanamaz), açıklama.
- Seçenekler:
  - "Geri dön" (birincil)
  - İki adımlı "Yine de devam et" (yalnız o sekme ve oturum)
  - "Hatalı engel bildir" (gönderilecek veri önce gösterilir)

**İtibar kaynakları:**
- Kaynak bağımsız besleme hattı kurulur; her kaynağın lisansı, aralığı ve atfı kayıt altındadır.
- Adaylar: USOM/TR-CERT, abuse.ch URLhaus, Google Safe Browsing v5 (ticari olmayan kullanım; D4), kullanıcı listeleri.

**İzinler:**
- `declarativeNetRequest` (+ yönlendirme için host erişimi), `webNavigation`, `storage`, `nativeMessaging`, `alarms`. İsteğe bağlı: `downloads`.
- Her izin için düz Türkçe gerekçe `extension/INSTALL.md` ve kurulum sihirbazında gösterilir.

**Dağıtım ve kurulum (ADR-011; mağaza yok):**
- **Sabit yol ve kimlik:**
  - Uzantı A/B sürüm dizinlerinin **dışında**, sabit bir yolda durur: `%ProgramFiles%\AlterDNS\browser-extension\`. Bu dizine yalnızca yönetici yazabilir; bu da kurcalamaya karşı korur.
  - Manifestte `key` alanı bulunur, böylece uzantı kimliği sabit kalır. Native host `allowed_origins` listesi yalnız bu kimliği içerir. Özel anahtar depoya girmez.
- **"Tarayıcıda kur" düğmesi** (uygulama ve flyout):
  1. Kurulu Chromium tarayıcılarını tespit eder (Chrome, Edge, Brave, Opera, Vivaldi).
  2. Seçileni başlatır.
  3. `chrome://extensions` / `edge://extensions` adresini panoya kopyalar. Bu adreslerin komut satırından açılıp açılmadığı doğrulanacak; açılabiliyorsa doğrudan açılır.
  4. Uzantı klasörünü Explorer'da gösterir ve yolu kopyalar.
  5. Uygulama içinde ekran görüntülü adım adım yönerge sunar: Geliştirici modu → "Paketlenmemiş öğe yükle" → klasörü seç.
  6. Native host'tan `hello` gelince "Bağlandı" gösterilir.
- **Otomatik yükleme yok:** Google Chrome markalı sürümler `--load-extension` bayrağını kabul etmiyor (2026 raporu). Kurumsal ilke yolu da yönetilmeyen cihazlarda mağaza dışı uzantıyı zorla kuramaz. Bu yüzden kurulum kullanıcı eliyle yapılır; Edge davranışı doğrulanacak.
- **Geliştirici modu riskleri:** Tarayıcı uyarı gösterebilir veya güncellemeden sonra uzantıyı devre dışı bırakabilir.
  - Uygulama native host kalp atışıyla durumu izler.
  - Bağlantı kopunca bildirim ve "Yeniden etkinleştir" yönergesi gösterilir (`ADNS-WEB-005`).
  - Uyarıyı gizleyen yamalar veya ilkeler **önerilmez**.
- **Güncelleme:** Uygulama güncellemesi uzantı dosyalarını sabit yolda değiştirir. Native host uzantıya haber verir; uzantı `chrome.runtime.reload()` ile yenilenir (paketlenmemiş uzantıda doğrulanacak).
- **Firefox:** Sonraki aşamada. AMO "unlisted" ücretsiz imzalama yolu doğrulanacak.

### 6.3 Bağlantı yönetimi

**Amaç:** Kullanıcının kendi cihazında ve kullanmaya yetkili olduğu bağlantıda, VPN veya proxy kullanmadan DPI kaynaklı engelleri aşmak (Discord, Roblox, engelli siteler). IP tabanlı engel aşılamaz ve bu durum kullanıcıya açıkça bildirilir.

**Teknik kataloğu:** `data/strategies/*.toml`. Her strateji DNS × TCP × UDP tekniklerinin bir birleşimidir.
- **TCP:**
  - Segmentasyon (sabit konumda veya SNI'dan önce), ters sıra
  - Sahte paketler: TTL/otomatik TTL, hatalı checksum, geçmiş SEQ/ACK, sahte SNI
  - HTTP varyasyonları
  - Pasif DPI engelleme
- **UDP/QUIC:** QUIC engelleme, sahte QUIC Initial, Discord ses keşfi için sahte paket. Fizibilite ölçülerek belirlenir.
- **DNS:**
  - Sistem DNS'i
  - Motor içi, alan adı sınırlı DoH aktarıcı (tercih edilen; sistem ayarı değiştirmez)
  - Journal'lı sistem DoH (onay ister)

**Otomatik uyum:**
1. Hash'lenmiş ağ parmak izi çıkarılır.
2. Engel sınıflandırılır: DNS / SNI / IP / yok.
3. Adaylar sıralanır: bu ağda daha önce başarılı olan → genel başarı → düşük maliyet.
4. Zaman sınırları: prob başına ≤ 4 sn, deneme başına ≤ 12 sn, toplam ≤ 90 sn.
5. Puanlama: kararlılık (3/3), p95 el sıkışma süresi, CPU vekili.
6. Ölçülü bağlantıda hız testi yapılmaz.
7. Başarısız denemenin değişiklikleri journal'dan geri alınır. Servis çökerse trafik normale döner; açılışta kurtarma yapılır.
8. Genel internet probu başarısız olursa motor kapanır, değişiklikler geri alınır ve hata kodu gösterilir.

**Arka plan uyumu:**
- Dinlenen olaylar: ağ arayüzü, rota, NLM, WLAN değişiklikleri ve uykudan uyanma. 2–3 sn debounce uygulanır.
- Bilinen ağda profil hızlıca uygulanır; bilinmeyen ağda arama yapılır.
- Motor hatasında geri çekilmeli yeniden başlatma: 1/2/5/10/30/60 sn.
- Periyodik kontrol: 30 dk + rastgele sapma.

**TTL 65 araştırması:** `docs/research/CONNECTIVITY_RESEARCH.md` protokolüne göre Faz 0'da yapılır. Neden varsayılmaz. 65 bütün uyumluluk testlerinde bir eksendir. 128 karşılaştırması yalnızca lab'da, D5 onayı ve otomatik geri dönüşle yapılır.

**Diğer bilgisayarlarda uyumluluk:** Araştırma belgesindeki "Uyumluluk etkenleri" listesinin tamamı test matrisine girer.

**Kaynak ve güvenilirlik:**
- Bileşenlerin kaynağı, lisansı, imzası ve hash'i doğrulanır.
- GoodbyeDPI paketleriyle ilgili güvenlik uyarıları `THIRD_PARTY.md` ve `SECURITY.md` dosyalarında değerlendirilir: 2025 SilentCryptoMiner kampanyası, sahte depolar, WinDivert'in kötüye kullanımı, AV etiketleri.

### 6.4 Arayüz (ayrıntı: `docs/UI_SPEC.md`)

**Tepsi simgesi:**
- Görev çubuğu temasına uyar ve durum rozeti gösterir.
- Sağ tık (varsayılan olarak sol tık da) flyout'u açar.

**Flyout:**
- **Sürüklenemez ve taşınamaz:** başlık yok, `HTCLIENT`, `SC_MOVE` engelli.
- Odak kaybında, Esc ile veya simgeye tekrar tıklanınca kapanır.
- Win11 malzemesi ve yuvarlak köşeler kullanılır.
- Açılış 150 ms; azaltılmış hareket ayarına uyar.
- Yazı tipi: Segoe UI Variable / Segoe UI.

**Ana pencere:** Tek tıkla açılır. Ekranın ortasında, 960×640, tam ekran değil, büyütme kapalı. Ayrı işlemde çalışır.

**Diğer:**
- Kurulu olmayan modüller "Yüklü değil — Ekle" olarak gösterilir.
- Standart onay ekranı: ne, neden, eski → yeni, nasıl geri alınır, risk.

### 6.5 Kurulum, bileşenler, kaldırma (ayrıntı: `docs/ARCHITECTURE.md` §8)

- **Bileşen seçimi:** Çekirdek (zorunlu), Dosya güvenliği (alt: YARA kuralları, ClamAV, Sandbox), Tarayıcı koruması, Bağlantı yönetimi. Her birinin boyutu, izinleri ve sistem değişiklikleri gösterilir. **Yalnızca seçilenler indirilir.**
- **Kurulum ekranları:** Lisans ekranında GPL-3.0 metni gösterilir; kabul zorunlu değildir, bilgilendirme amaçlıdır. Gizlilik politikası gösterilir (SignPath şartı).
- **Sonradan değişiklik:** Bileşen uygulamadan veya "Uygulamalar > Değiştir" ile eklenip kaldırılır.
- **Kaldırma:**
  - (A) Tam temizlik.
  - (B) Raporları koruyarak kaldırma.
  - Her iki modda da journal geri alınır. AlterDNS'in oluşturmadığı hiçbir şeye dokunulmaz. Kalıntı taraması yapılır.
  - Uzantı tarayıcıdan programla kaldırılamaz; kullanıcıya yönerge gösterilir.

### 6.6 Güncelleme (D1: public depo)

- **Kanal:** `https://github.com/fxrkqnplus/alterdns/releases`.
- **Sürüm bilgisi:** Uygulama API kullanmadan `…/releases/latest/download/release-manifest.json` ve imza dosyasını indirir. Bu, kimlik doğrulamasız API hız sınırına takılmamak içindir (doğrulanacak). Ön sürüm kanalı için ayrı manifest varlığı kullanılır. İstemciye token gömülmez.
- **Manifest güvenliği:** Ed25519 imzası (açık anahtar uygulamaya derlenir, anahtar devri desteklenir), bileşen başına SHA-256, sürüm düşürme koruması, son geçerlilik tarihi. Authenticode, SignPath imzası alındıktan sonra doğrulanır.
- **Akış:** Günlük kontrol (rastgele sapmalı), ETag, ölçülü bağlantıda erteleme, staging, A/B kurulum, 60 sn sağlık kontrolü. Başarısız olursa otomatik geri dönülür ve hata kodu gösterilir.
- **Veri beslemeleri:** Ayrı takvimle güncellenir. Son iyi bilinen kopya korunur.

### 6.7 Hata yönetimi

- Kod şeması `ADNS-<ALAN>-<NNN>`; katalog `errors/catalog.toml` (başlangıç: `docs/ERROR_CATALOG.md`).
- Her kod şunları içerir: önem, başlık, mesaj, nedenler, çözümler (TR + EN), teknik ayrıntı.
- Kontrol noktaları: sürücü, IPC, journal, DNS, güncelleme imzası ve sağlık kontrolü, tarama sınırları, karantina doğrulaması, native host ve uzantı durumu, besleme, kurulum/kaldırma.
- Arayüz kodu, başlığı, mesajı, nedenleri, adımları ve "Ayrıntıları kopyala" seçeneğini gösterir.

### 6.8 Güvenilir derleme ve imzalama (D3: yalnız ücretsiz yollar; ADR-012)

**Derleme (GitHub Actions, public depo):**
- Sabitlenmiş araç zinciri, `cargo build --locked --release`.
- Deterministik bayraklar: `-C link-arg=/Brepro`, `--remap-path-prefix`, `codegen-units=1`.
- SBOM (CycloneDX) ve `THIRD_PARTY_NOTICES` üretilir.
- Hedef: CI'da ve yerelde üretilen imzasız ikililerin hash'lerinin aynı olması. Sağlanamazsa farklar belgelenir.

**Kanıt:**
- Bütün sürüm varlıkları için `actions/attest-build-provenance` (ve SBOM attestation) üretilir. Public depoda ücretsizdir ve Sigstore'un genel örneğini kullanır.
- Sürüm notlarında `gh attestation verify <dosya> -R fxrkqnplus/alterdns` yönergesi ve `SHA256SUMS` bulunur.

**Uygulama içi doğrulama:** Ed25519 imzalı manifest. Anahtar, korumalı `release` ortamında (gerekli onaylayıcı: kullanıcı) veya çevrim dışı yerelde saklanır. Seçim Faz 5'te.

**Authenticode (SignPath Foundation):**
- **Şartlar:** OSI lisansı; ikili lisans yok; kapalı bileşen yok; projenin imzalanacak biçimde daha önce yayımlanmış olması; MFA; roller (Yazar/İnceleyen/Onaylayan); her sürüm için elle onay; ürün adı = proje adı; sitede "Code signing policy" sayfası ve şu ifade: "Free code signing provided by SignPath.io, certificate by SignPath Foundation"; gizlilik politikası; sistem değişikliklerinde uyarı; kaldırma desteği.
- **Kapsam dışı:** Üçüncü taraf ikililer (WinDivert DLL/SYS) bizim tarafımızdan imzalanmaz. Paket içinde kalmalarına izin var.
- **Risk:** Programın "hacking tool yok" maddesi, çalışma ortamındaki güvenlik önlemlerini aşan yazılımı yasaklıyor. Bağlantı modülünün bu kapsamda değerlendirilip değerlendirilmeyeceği belirsiz. Başvuru şeffaf yapılır; ret durumunda seçenekler D7'de.
- **İmzadan önce:** İlk ön sürümler imzasız yayımlanır.
  - README ve sürüm notları SmartScreen uyarısını ve attestation ile doğrulamayı açıklar.
  - Smart App Control'ün kapatılması **önerilmez**; SAC açık sistemlerde imza gelene kadar kurulum yapılamayacağı belirtilir.

**Değerlendirilip elenen yol:** Microsoft Store imzası ücretsiz ama yalnızca MSIX içindir. Çekirdek sürücüsü ve LocalSystem servisi gerektiren bu mimariye uymuyor.

## 7. Çalışma yöntemi ve araçlar

Kullanıcının Claude Code ortamında 2026-11-09'da yüklü olduğu doğrulanan araçlar (`/plugin` ve `/mcp` ile tekrar kontrol et):

| Araç | Kullanım |
|---|---|
| `superpowers` | Her faz: `writing-plans` → `test-driven-development` → `verification-before-completion`. Ayrıca `systematic-debugging`, `requesting-code-review`, `using-git-worktrees`, `finishing-a-development-branch`, tasarım soruları için `brainstorming` |
| `backend-design` | `think-before-coding`, `error-handling-as-design`, `idempotency-and-side-effects`, `observability-by-default`, `security-discipline`, `testing-with-discernment`. Ajanlar: `security-reviewer`, `incident-thinker`, `component-architect` |
| `security-guidance` | Açık kalsın |
| `modern-web-guidance:chrome-extensions` | Uzantı kodu (MV3 kuralları, DNR, native messaging) |
| `frontend-design`, `vectorlab-ux-skills`, `design-skills` | Engelleme sayfası, kurulum sihirbazı, flyout/ana pencere, UX metinleri, erişilebilirlik |
| `playwright` | Uzantı E2E (Chromium + paketlenmemiş uzantı) |
| Context7 MCP | `windows`, `slint`, `yara-x`, `tokio`, `windivert`, `tracing` vb. belgeler |
| Exa / Firecrawl | Besleme koşulları, upstream değişiklikleri, SignPath ve tarayıcı politikası değişiklikleri |
| `gh` CLI | Issue, release, Actions, `gh attestation verify` (yazma işlemleri onay ister) |
| `claude-mem` | Oturumlar arası bellek |

**Kullanılmayacaklar:** animatedwebs, social-media-skills, video-shotcraft, unity, roblox-dev, equity-research, investment-banking, image-generation, humanizer, Supabase, Higgsfield.

**Faz akışı:**
1. `ROADMAP.md` maddelerini oku.
2. `writing-plans` ile planı yaz.
3. Kullanıcı onayı al.
4. TDD ile küçük commit'ler at (push onaylı).
5. Faz sonu: `cargo xtask check`, perf ölçümü, `security-reviewer`, `verification-before-completion`, `CLAUDE.md` / `ROADMAP.md` güncellemesi, rapor.

## 8. Fazlar (ayrıntı: `docs/ROADMAP.md`)

| Faz | İçerik | Bağımlılık |
|---|---|---|
| 0 | Ortam doğrulama (Windows SDK), depo ve güvenlik ayarları, lisans doğrulama, spike'lar (UI, WinDivert, sandbox), **bağlantı araştırması ve TTL 65 deneyleri** | — |
| 1 | Çekirdek: workspace, hata kataloğu, log, config, IPC, journal, servis, tepsi/flyout/ana pencere kabukları, dev-install, CI | 0 |
| 2 | Bağlantı yönetimi | 1 |
| 3 | Dosya güvenliği | 1 |
| 4 | Tarayıcı koruması: besleme, native host, uzantı, **kurulum sihirbazı**, engelleme sayfası, `PRIVACY.md` | 1 (3 ile paralel) |
| 5 | Kurulum, bileşen yöneticisi, güncelleyici, kaldırma, **yayın hattı (attestation + Ed25519)**, **ilk imzasız ön sürüm** | 1–4 |
| 6 | Sağlamlaştırma, uyumluluk matrisi, **SignPath başvurusu** (code signing policy, roller, MFA) | 2–5 |
| 7 | **İmzalı** v1.0 yayını (SignPath onayına bağlı; değilse D7) | 6 |

## 9. Test ve doğrulama (ayrıntı: `docs/TESTING.md`)

**Test katmanları:**
- Birim ve proptest
- Fuzz (platformdan bağımsız crate'ler WSL/Linux'ta)
- Entegrasyon
- Sistem (VM anlık görüntü karşılaştırması)
- Ağ lab testleri
- Uzantı E2E (Playwright)
- Kurulum sihirbazı ve paketlenmemiş uzantı akışı
- Performans
- Güvenlik
- **Yayın hattı:** deterministik derleme, attestation doğrulaması, manifest imzası, imza sonrası hash zinciri

**"Hatasızlık" ölçülebilir biçimde tanımlanır:**
- Testler yeşil
- clippy ve deny temiz
- Test dışı kodda `unwrap`/`expect`/`panic` yok
- Fuzz turları çökme olmadan tamamlandı
- Yüksek düzey güvenlik bulgusu yok
- Bilinen açık hata yok
- Her hata kodu en az bir testte üretiliyor

## 10. Performans bütçeleri (ölçüm: `docs/TESTING.md` §5)

En düşük hedef makine: 2 çekirdek x64, 4 GB RAM, SSD.

| Ölçüt | Bütçe |
|---|---|
| Servis boşta private bytes | ≤ 12 MB |
| Servis boşta CPU (10 dk) | ≤ %0,1; 60 sn'den sık zamanlayıcı yok |
| İş hacmi kaybı | ≤ %2 |
| El sıkışma gecikmesi artışı p95 / RTT değişimi | ≤ 5 ms / ≤ 1 ms |
| Tepsi ajanı (flyout kapalı) | ≤ 8 MB (spike ile gerekçeli revize edilebilir) |
| Flyout açık / açılış (sıcak p95 / soğuk) | ≤ 45 MB / ≤ 150 ms / ≤ 400 ms |
| Ana pencere RAM / ilk çizim | ≤ 90 MB / ≤ 600 ms |
| Tarama işçisi bellek sınırı | 256 MB |
| Hash + YARA hızı | ≥ 50 MB/sn |
| Çekirdek kurulum boyutu | ≤ 25 MB |
| Tepsi simgesinin görünmesi | oturum açıldıktan sonra ≤ 2 sn |
| Boşta disk yazımı | yalnız log döngüsü; loglar ≤ 10 MB |

## 11. Teknik sınırlar ve doğrulanması gereken varsayımlar

1. DPI atlatma ağa bağlıdır; garanti verilmez. IP engeli aşılamaz.
2. Discord sesi ve Roblox oyunu UDP kullanır. Fizibilite ölçülecek; port aralıkları doğrulanacak.
3. Roblox Hyperion ile WinDivert etkileşimi bilinmiyor; gerçek oyun testiyle görülecek.
4. Roblox engelinin durumu haberlerde çelişkili. Engel listesi sabit kodlanmaz, ölçülür.
5. WinDivert ARM64'ü resmi olarak desteklemiyor; hedef yalnız x64.
6. WinDivert ve DPI araçları AV'lerde HackTool/RiskTool olarak işaretlenebilir. Şeffaflık esastır; AV kapattırılmaz.
7. **İmza:** SignPath başvurusu onaylanana kadar ikililer imzasızdır. Bu sürede SmartScreen uyarısı çıkar ve Smart App Control açık sistemler uygulamayı engeller. SignPath imzasında yayıncı "SignPath Foundation" görünür. "Hacking tool" maddesi riski var (D7).
8. **Lisans:** UnRAR kullanılmaz. Slint yalnız GPL-3.0 seçeneğiyle kullanılır. ClamAV UnRAR'sız kullanılır. YARA kuralları yalnız açık lisanslı olur. Kapalı bileşen kabul edilmez.
9. **Kullanım koşulları:** Google Safe Browsing ve VirusTotal public API ticari olmayan kullanım içindir. abuse.ch ticari kullanımda abonelik isteyebilir (D4).
10. **MV3 DNR sınırları:** dinamik 30.000, oturum 5.000, statik 30.000 + 300.000 paylaşımlı. `requestDomains` ile paketleme sınırı spike'ta ölçülecek.
11. **Geliştirici modu:** Tarayıcı uyarı gösterebilir veya uzantıyı devre dışı bırakabilir. `--load-extension` Chrome markalı sürümde kabul edilmiyor. `chrome://` adreslerinin komut satırından açılıp açılmadığı, Edge davranışı ve `runtime.reload` doğrulanacak. Kurumsal ilkeyle kilitlenmiş tarayıcılarda geliştirici modu kapalı olabilir; bu durumda açık bir hata kodu gösterilir.
12. **Güncelleme indirmesi:** `releases/latest/download` yolunun API hız sınırına girmediği doğrulanacak.
13. Windows Sandbox yalnız Pro ve üzeri sürümlerde var.
14. Tarama yeni bir zararlıyı garanti yakalayamaz.
15. Tepsi GUID'i exe yoluna bağlıdır; sabit başlatıcı yolu gerekir.
16. Aynı adlı `WinDivert` servisi başka bir sürücü yoluna bağlıyken (ör. GoodbyeDPI kurulu sistem) ne olur? Faz 6 uyumluluk matrisinde, GoodbyeDPI kurulu bir VM'de ölçülecek.
17. TTL 128 lab deneyi operatörün tethering tespitini tetikleyebilir (D5).

## 12. Kullanıcı kararları

**Verilen kararlar (2026-11-09):**
- **D1 ✔** Depo public; güncellemeler `fxrkqnplus/alterdns` GitHub Releases'tan.
- **D2 ✔** Lisans GPL-3.0-or-later; ad ve logo `TRADEMARK.md` kapsamında.
- **D3 ✔** Yalnız ücretsiz imzalama ve doğrulama: SignPath Foundation başvurusu + Artifact Attestations + Ed25519.
- **Uzantı ✔** Mağaza yok; uygulama klasöründen geliştirici moduyla yükleme ve "Tarayıcıda kur" düğmesi.
- **D6 ✔** Eski DPI paketi kaldırıldı (2026-11-09); karar kapandı.

**Açık kararlar (zamanı gelince sor):**
- **D4 — Ticari olmayan / ticari** (Faz 4 başı)
- **D5 — Lab'da TTL 128 karşılaştırması** (Faz 0)
- **D7 — SignPath reddederse veya bağlantı modülünü kapsam dışı sayarsa** (Faz 6). Seçenekler:
  - İmzasız dağıtıma devam etmek
  - Bağlantı modülünü ayrı bir proje veya depo olarak imzasız dağıtmak
  - Başka ücretsiz bir program bulunursa onu değerlendirmek

## 13. Raporlama biçimi (her faz sonunda)

```
## Faz N raporu — <tarih>
Durum: tamamlandı / kısmen (neden)
Yapılanlar:
Doğrulama: (komut → sonuç; test sayıları; perf tablosu)
Bütçe dışı kalanlar ve gerekçe:
Güvenlik incelemesi bulguları:
ADR değişiklikleri:
Açık riskler:
Kullanıcıdan beklenen kararlar / onaylar:
Sonraki faz önerisi:
```

## 14. İlk görev — Faz 0

1. `superpowers:writing-plans` ile Faz 0 planını yaz ve onaya sun.
2. **Araç zinciri:**
   - Geçici bir projede `cargo build` çalıştır.
   - Windows SDK eksikse "Windows 11 SDK" kurulumunu kullanıcıdan iste.
   - `cargo-deny`, `cargo-about`, `cargo-cyclonedx` ve gerekirse `cargo-nextest` kur.
3. **Depo:**
   - `git init`, remote `https://github.com/fxrkqnplus/alterdns`.
   - `LICENSE` (GPL-3.0, kanonik metin) ve `TRADEMARK.md` dosyalarını doğrula.
   - `.gitignore`'un `CLAUDE.local.md`, `lab/results/` ve gizli dosyaları kapsadığını kontrol et.
   - İlk commit'ten önce `git status` / `git ls-files` ile kişisel bilgi taşıyan bir dosya olmadığını göster.
   - Logoları `assets/brand/` altına **kopyala**.
   - `docs/claude-settings.template.json` dosyasını onayla `.claude/settings.json` olarak kopyala ve `/status` ile doğrula.
   - Push için onay iste.
4. **GitHub depo güvenlik ayarları** (kullanıcı arayüzden yapar ya da onayla `gh` ile):
   - Secret scanning ve push protection
   - Dependabot alerts ve security updates
   - Varsayılan dalda koruma (durum kontrolleri zorunlu)
   - Actions: varsayılan `GITHUB_TOKEN` salt okunur; yalnızca SHA ile sabitlenmiş action'lar
   - `release` ortamı (gerekli onaylayıcı: kullanıcı)
   - Hesapta MFA
   - CodeQL'in Rust desteği doğrulanırsa etkinleştirilir
5. **Lisans ve üçüncü taraf doğrulaması:** `THIRD_PARTY.md` içindeki ⏳ satırlar kontrol edilir. Özellikle:
   - zapret lisansı
   - libarchive'in RAR5 desteği
   - Slint GPL kullanım koşulları
   - YARA kural setlerinin lisansları
   - WinDivert 2.2.2-A hash'i ve imzası
6. **Spike'lar:**
   - UI (Slint GPL vs Win32/D2D)
   - WinDivert 2.2.2-A sürücü yükleme, HVCI ve kaldırma (**onay**)
   - Sandbox
7. **Bağlantı araştırma düzeneği:** `CONNECTIVITY_RESEARCH.md` §5. Önce salt okunur ölçümler; sonra onayla upstream motorlar; D5 onaylanırsa TTL 128. Sonuçlar `lab/results/` (git dışı) altına yazılır. Özet `CONNECTIVITY_RESEARCH.md` §7'ye yalnızca genelleştirilmiş biçimde girer; kişisel ağ ayrıntıları `CLAUDE.local.md`'de kalır.
8. Faz 0 raporu hazırlanır. ADR-002 ve ADR-003 ölçümlerle kesinleşir.
