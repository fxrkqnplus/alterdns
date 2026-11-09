# AlterDNS — Mimari

Durum: taslak (2026-11-09). Faz 0 spike'larından sonra ADR'lerle kesinleşir.

## 1. İlkeler

- **En az ayrıcalık:** Yalnızca paket motoru, journal ve güncelleme uygulaması yönetici yetkisiyle çalışır. Güvenilmeyen içerik hiçbir zaman ayrıcalıklı bir işlemde ayrıştırılmaz.
- **Kapanınca güvenli:** Servis çökerse WinDivert handle'ı kapanır ve trafik normal akar. Yarım kalan sistem değişiklikleri açılışta journal'dan geri alınır.
- **Tembel yükleme:** Arayüz, tarama işçisi ve ana pencere ihtiyaç anında başlar ve işi bitince çıkar. Boştayken yalnızca servis ve küçük tepsi ajanı çalışır.
- **Olay güdümlü çalışma:** Yoklama (polling) yerine Windows bildirimleri kullanılır. Boştayken 60 sn'den sık zamanlayıcı yoktur.
- **Modülerlik:** Her modül ayrı bir bileşen paketidir. Kurulu olmayan modülün kodu diskte bulunmaz; arayüz bu modülü "Yüklü değil" olarak gösterir.

## 2. İşlem modeli

| İşlem | Bağlam | Yaşam süresi | Görev |
|---|---|---|---|
| `alterdns-svc.exe` | Windows servisi, LocalSystem. Hizmet SID'i tanımlı. `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 \| APPLICATION_DIR)` | Sürekli | Bağlantı motoru (WinDivert), ağ izleyici, ayarlayıcı, itibar DB ve beslemeleri, güncelleyici (indirme + doğrulama), journal, IPC sunucusu, yeniden başlatmada silme |
| `alterdns-tray.exe` | Etkileşimli kullanıcı, orta bütünlük | Oturum boyunca | Tepsi simgesi, flyout, toast bildirimleri, indirme klasörü izleyici, tarama aracısı (işçileri başlatır), karantina (kullanıcı dosyaları) |
| `alterdns-ui.exe` | Kullanıcı | İsteğe bağlı açılır; pencere kapanınca çıkar | Ana pencere |
| `alterdns-scan.exe` | AppContainer (yetenek yok), Low IL, Job Object | Tarama başına | Tür tespiti, açma, analizörler, YARA-X. Girdi: devralınmış salt okunur handle. Çıktı: pipe üzerinden JSON |
| `alterdns-nmh.exe` | Tarayıcının başlattığı kullanıcı işlemi | Tarayıcı bağlantısı boyunca | stdio native messaging ↔ servis pipe'ı. Mesajları şema doğrulamasından geçirir |
| `alterdns-maint.exe` | Yönetici (UAC) | Kurulum, değiştirme, kaldırma, güncelleme uygulama | Bileşen yöneticisi motoru |
| `alterdns-setup.exe` | Önyükleyici; yönetici yetkisini kendisi ister | Kurulum | Bileşen seçimi, manifest indirme, `maint` motorunu çalıştırma |

**Tepsi GUID'i notu:** `NOTIFYICONDATA.guidItem` exe yoluna bağlıdır. A/B sürüm dizinlerinde yol değişir. Bu yüzden tepsi ajanı sabit bir yoldan başlatılır (`%ProgramFiles%\AlterDNS\alterdns-tray.exe`, sürüm dizinine yönlendiren küçük bir başlatıcı) ya da GUID yerine `uID` kullanılır. Spike'ta seçilir.

## 3. Depo ve crate yapısı

```
alterdns/
├─ LICENSE                    # GPL-3.0 (kanonik metin)
├─ TRADEMARK.md               # ad ve logo kullanım notu
├─ PRIVACY.md                 # gizlilik politikası (Faz 4)
├─ CLAUDE.md / CLAUDE.local.md  # proje belleği / git dışı yerel ortam
├─ .github/workflows/         # ci.yml, release.yml (SHA sabit action'lar)
├─ Cargo.toml                 # workspace; ortak lint ve profil ayarları
├─ rust-toolchain.toml        # stable, sabitlenmiş sürüm
├─ deny.toml                  # lisans izin listesi, advisories, kaynaklar
├─ about.toml / about.hbs     # THIRD_PARTY_NOTICES üretimi (cargo-about)
├─ errors/catalog.toml        # hata kataloğu (tek kaynak)
├─ data/
│  ├─ strategies/*.toml       # DPI strateji kataloğu (sürümlü, imzalı veri beslemesi)
│  ├─ probes/*.toml           # hedef prob listeleri (Discord, Roblox, kontrol siteleri)
│  └─ feeds/*.toml            # itibar kaynağı tanımları (URL, lisans, aralık, atıf)
├─ crates/
│  ├─ core/                   # tipler, yollar, config şeması, sürüm, i18n anahtarları
│  ├─ errors/                 # AdnsError + build.rs ile katalogdan kod üretimi
│  ├─ ipc/                    # zarf, pipe sunucu/istemci, ACL, kimlik doğrulama
│  ├─ journal/                # sistem değişikliği işlemleri (kaydet → uygula → geri al)
│  ├─ winsys/                 # unsafe Win32 sarmalayıcıları (tek yerde, SAFETY yorumlu)
│  ├─ net-observe/            # ağ olayları, parmak izi, NLM, WLAN, hop limit okuma
│  ├─ dpi/                    # paket motoru: filtre kurucu, teknikler, akış izleme
│  ├─ dns/                    # alan sınırlı DoH aktarıcı, sistem DNS yönetimi (journal)
│  ├─ probe/                  # TCP/TLS/HTTP/DoH/QUIC probları, zaman sınırları
│  ├─ tuner/                  # aday üretimi, deneme, puanlama, profil belleği
│  ├─ reputation/             # besleme indirme, normalize, sıkıştırılmış DB, sorgu
│  ├─ scan-engine/            # tarama hattı (işçide çalışır; platformdan bağımsız kısım)
│  ├─ scan-broker/            # işçi başlatma (AppContainer/Job), handle aktarımı, zaman aşımı
│  ├─ av-bridge/              # Defender MpCmdRun, AMSI, WSC (yüklü AV tespiti)
│  ├─ quarantine/             # şifreli karantina deposu, geri yükleme, kalıcı silme
│  ├─ update/                 # manifest, Ed25519, indirme, staging, A/B, geri dönüş
│  ├─ components/             # bileşen kayıt defteri, kur/kaldır, kalıntı tarayıcı
│  └─ ui/                     # Slint bileşenleri, tema, yazı tipleri, simgeler
├─ apps/ svc/ tray/ ui/ scan/ nmh/ maint/ setup/
├─ extension/                 # MV3 TypeScript (Chrome + Edge; paketlenmemiş dağıtım), INSTALL.md
├─ xtask/                     # check, test, perf, dev-install, lab, package
├─ assets/brand/              # logo kopyaları ve türetilmiş simgeler
├─ lab/                       # araştırma senaryoları (sonuçlar git dışı: lab/results/)
└─ docs/
```

Platformdan bağımsız crate'ler (`scan-engine`, `reputation` ayrıştırıcıları, `dpi` paket kurucuları, `errors`, `update` doğrulaması) Linux'ta da derlenir. Böylece WSL ve Linux CI'da fuzz edilebilir.

## 4. IPC

- **Taşıma:** Named pipe `\\.\pipe\AlterDNS.Svc.v1`. DACL: SYSTEM ve Administrators tam erişim; `INTERACTIVE` bağlan/oku/yaz. `FILE_FLAG_FIRST_PIPE_INSTANCE` ile pipe squatting önlenir. İstemci tarafında sunucu işleminin servis olduğu doğrulanır (`GetNamedPipeServerProcessId` → imaj yolu).
- **Çağıran doğrulaması:** `GetNamedPipeClientProcessId` ile imaj yolu ve kurulum dizini kontrol edilir; imza varsa doğrulanır. Ayrıcalıklı yöntemler (motor yapılandırması, sistem değişikliği, güncelleme uygula) yalnızca AlterDNS imajlarından ve etkileşimli oturumdan kabul edilir. Kullanıcı onayı gerektiren yöntemler bir onay belirteci ister; bu belirteç tepsi veya UI onay ekranından üretilir.
- **Çerçeve:** 4 bayt uzunluk + JSON. Mesaj başına üst sınır 1 MB.
  ```json
  {"v":1,"id":"…","method":"conn.status","params":{}}
  {"v":1,"id":"…","ok":true,"result":{…}}
  {"v":1,"id":"…","ok":false,"error":{"code":"ADNS-IPC-002","params":{…}}}
  ```
- **Olaylar:** İstemci `events.subscribe` çağırır; servis durum değişikliklerini iter (bağlantı durumu, engelleme sayacı, güncelleme, hata).
- **Sürümleme:** `v` alanı uyuşmazsa `ADNS-IPC-004` döner ve UI güncelleme önerir.

## 5. Veri ve yapılandırma yolları

| Yol | İçerik | ACL / not |
|---|---|---|
| `%ProgramFiles%\AlterDNS\<sürüm>\` | İkililer (A/B). `current` işaretçisi: `HKLM\SOFTWARE\AlterDNS\CurrentVersion` | Yönetici yazar, herkes okur |
| `%ProgramFiles%\AlterDNS\alterdns-tray.exe` | Sabit başlatıcı (tepsi GUID'i için) | |
| `%ProgramFiles%\AlterDNS\browser-extension\` | Paketlenmemiş MV3 uzantısı (sabit yol; A/B dışında) | Yalnız yönetici yazar, kullanıcı okur (tarayıcı okuyabilsin) |
| `%ProgramData%\AlterDNS\` | `config/` (makine), `journal/`, `reputation/`, `rules/` (YARA), `strategies/`, `staging/`, `logs/` | SYSTEM + Administrators yazar; Users yalnızca gerekli alt dizinleri okur |
| `%LOCALAPPDATA%\AlterDNS\` | `config/` (kullanıcı), `reports/`, `quarantine/`, `logs/` | Kullanıcı + SYSTEM. Karantina dizininde çalıştırma izni yok, kalıtım kapalı |
| `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.alterdns.host` ve Edge karşılığı | Native host kaydı | Journal'a yazılır |
| `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\AlterDNS` | Kaldırma kaydı (Publisher: Furkan Okan, ModifyPath, UninstallString) | Journal'a yazılır |

Yapılandırma TOML biçimindedir ve şeması sürümlüdür (`schema = N`). Geçişler ileri yönlüdür ve geçişten önce yedek alınır.

## 6. Journal — sistem değişiklikleri

Her sistem değişikliği bir `ChangeRecord` olarak önce diske yazılır (fsync), sonra uygulanır:

```
ChangeRecord { id, txn_id, component, kind, target, before: Snapshot, after: Value,
               state: Pending|Applied|Reverted|Failed, created_by: "alterdns", ts }
```

- **Türler:** `ServiceCreate/Config`, `DriverService` (yalnızca AlterDNS oluşturduysa), `RegistryValue`, `DnsServers(if)`, `DohTemplate`, `ScheduledTask`, `NativeHostRegistration`, `FirewallRule` (gerekirse), `StartupEntry`, `FileInstall`, `UninstallEntry`, `ExternalExtensionHint`.
- **İşlem (txn):** Ayarlayıcı denemesi, bileşen kurulumu ve güncelleme birer txn'dir. Txn başarısız olursa kayıtları ters sırayla geri alınır.
- **Açılışta kurtarma:** `Pending` durumundaki kayıtlar kontrol edilir; uygulanmışsa geri alınır, uygulanmamışsa silinir. Sonuç loglanır.
- **Sahiplik kuralı:** Journal'da `created_by = alterdns` kaydı bulunmayan hiçbir sistem öğesine kaldırma sırasında dokunulmaz. Örnekler: kullanıcının hop limiti, başka uygulamaların kurduğu WinDivert servisi, kullanıcının DNS ayarları. Kurulum öncesi değerler `before` alanında saklanır. Geri yüklemeden önce mevcut değerin hâlâ AlterDNS'in yazdığı değer olduğu kontrol edilir; kullanıcı arada değiştirdiyse değer ezilmez ve kullanıcıya sorulur.
- **Bekçi:** Lab ve ayarlayıcı denemelerinde ayrı bir işlem (`alterdns-maint watchdog --txn <id> --deadline 60s`) çalışır. Süre dolduğunda txn hâlâ "commit" edilmemişse geri alır. Ana işlem ölse bile bekçi çalışır.

## 7. Modüller

### 7.1 Bağlantı yönetimi

```
net-observe ──olay──▶ tuner ──config──▶ dpi engine (WinDivert) ◀──▶ dns relay
     ▲                   │  ▲                     │
     │                   ▼  │                     ▼
 parmak izi         probe suite            sayaçlar / sağlık
                         │
                    profil belleği (ProgramData/strategies/profiles.toml, hash anahtarlı)
```

- **Filtre stratejisi:** Yalnızca gereken paketler kullanıcı moduna yönlendirilir. Örnekler:
  - Giden TCP 443 ve TLS ClientHello (`tcp.Payload[0]==0x16 and tcp.Payload[5]==0x01`)
  - Giden TCP 80 ve HTTP istek başı
  - Hedef alan adı listesine ait DNS sorguları
  - Strateji gerektiriyorsa ilk N UDP paketi (QUIC Initial, Discord IP keşfi)
  - Pasif DPI tespiti için gelen RST ve yönlendirme paketleri

  Kararlı durumdaki veri akışı yönlendirilmez. Filtre dili ve performansı: WinDivert belgesi (seçici filtre, toplu `WinDivertRecvEx`/`SendEx`).
- **Teknik arayüzü:**
  ```rust
  trait Technique {
      fn id(&self) -> &'static str;
      fn applies(&self, pkt: &ParsedPacket, flow: &FlowState) -> bool;
      fn transform(&self, pkt: ParsedPacket, flow: &mut FlowState, out: &mut Vec<OutPacket>) -> Result<(), AdnsError>;
  }
  ```
  `transform` saf bir fonksiyondur: paket girer, paketler çıkar. Bu sayede pcap fikstürleriyle birim test edilir. Checksum hesabı `WinDivertHelperCalcChecksums` ile veya saf Rust ile yapılır (karşılaştırmalı test).
- **DNS aktarıcı:** Hedef alan adlarının giden UDP/53 sorguları yakalanır. DoH (HTTPS, sertifika doğrulamalı, rustls) ile çözülür ve yanıt gelen yönde enjekte edilir. Sistem ayarı değişmez; motor durunca kendiliğinden devre dışı kalır. DoH sağlayıcısı listesi yapılandırılabilir.
- **Ayarlayıcı durum makinesi:** `Idle → Fingerprint → Classify → Trial(n) → Score → Commit → Monitor`. Hata yolu: `Rollback → Backoff`. Her `Trial` bir journal txn'idir ve bekçiyle korunur. `Commit` olayında profil yazılır.
- **Diğer DPI araçlarının tespiti:** `GoodbyeDPI` servisi, `winws.exe`, bilinen WinDivert istemcileri ve `WINDIVERT_LAYER_REFLECT` ile diğer handle'lar kontrol edilir. Çakışma varsa kullanıcıya gösterilir ve onayla geçici durdurma önerilir. AlterDNS başka bir aracı kendiliğinden kaldırmaz.
- **Ayrıntılı araştırma ve uyumluluk:** `research/CONNECTIVITY_RESEARCH.md`.

### 7.2 Dosya güvenliği

```
tetik (sağ tık "AlterDNS ile tara", sürükle-bırak, indirme izleyici, uzantıdan indirme olayı)
  → scan-broker: dosyayı FILE_SHARE_READ ve reparse izlemeden aç, kimlik (hash) ve MOTW oku
  → alterdns-scan (sandbox): tür tespiti → [açıcılar ↻] → analizörler → YARA-X → bulgular
  → broker: hash itibarı (onaylı çevrim içi), Defender MpCmdRun/AMSI (yüklü ve etkinse)
  → karar motoru → rapor (JSON) → UI / bildirim → kullanıcı eylemi (karantina/sil/izin ver)
  → doğrulama
```

- **Açıcılar:** zip, 7z, rar (UnRAR kullanılmaz; libarchive ile RAR4/RAR5 okuma doğrulanacak, olmazsa "incelenemedi"), tar/gz/bz2/xz, cab, iso/udf, img, vhd(x) (salt okunur ayrıştırma; bağlama yok), OLE/CFB, OOXML, MSI, PDF akışları, e-posta (.eml/.msg). Her açıcıya bütçe verilir (bayt, öğe, derinlik, süre).
- **Analizörler (örnekler):**
  - PE: imza durumu (broker'da `WinVerifyTrust`), imzalayan, içe aktarımlar, bölüm entropisi, paketleyici izleri, overlay, TLS callback, manifest `requestedExecutionLevel`, sürüm bilgisi tutarlılığı
  - .NET meta verisi
  - LNK: hedef, argümanlar, LOLBin kullanımı
  - Betikler: gizleme göstergeleri (base64/`-enc`, `IEX`, `DownloadString`, karakter kodu birleştirme); AMSI'ye tampon olarak gönderim
  - Office: makrolar (`vbaProject.bin`), AutoOpen ve benzerleri, harici şablon, DDE
  - PDF: `/JavaScript`, `/OpenAction`, `/Launch`, `/EmbeddedFile`
  - HTML/SVG: betik, HTML smuggling kalıpları
  - Görsel, ses ve video: biçim geçerliliği, son işaretçiden sonra gelen veri (polyglot), gömülü arşiv veya PE, boyut bombası
  - Dosya adı: RTLO, çift uzantı, uzantı-içerik uyuşmazlığı
- **Karar motoru:** Her bulgunun şu alanları vardır: `severity ∈ {info, low, medium, high, critical}`, `confidence ∈ {low, medium, high}`, `source`. Kurallar:
  - Yüksek güvenli AV/YARA zararlı tespiti veya bilinen kötü hash → `Riskli`
  - Orta düzey sezgisel bulgular veya eksik inceleme → `Şüpheli`
  - Hiç bulgu yoksa ve bütün kritik parçalar incelendiyse → `Güvenli` ("bilinen tehdit bulunamadı")

  Kurallar tablo halinde test edilir.
- **Karantina:**
  - Dosya AES-GCM ile şifrelenir. Kurulum başına anahtar DPAPI ile korunur.
  - Meta veri saklanır: özgün yol, hash, MOTW, rapor kimliği.
  - Geri yükleme onay ister ve özgün yolun güvenli olduğu kontrol edilir.
  - Kalıcı silme iki adımlıdır.
- **Dinamik inceleme (deneysel):**
  - Windows Sandbox `.wsb`: ağ kapalı, vGPU kapalı, salt okunur eşleme, `ProtectedClient`.
  - Yalnızca Pro ve üzeri sürümlerde; kullanıcı başlatır.
  - Davranış izleme kapsamı sınırlıdır ve raporda böyle belirtilir.

### 7.3 Tarayıcı koruması

- **Besleme hattı (servis):**
  - Her kaynak `data/feeds/*.toml` ile tanımlanır: URL, biçim, lisans, ticari kullanım koşulu, atıf metni, güncelleme aralığı, beklenen boyut aralığı.
  - Hat: indir → doğrula (TLS, boyut, biçim, değişim oranı) → normalize et (IDN/punycode, küçük harf, kanonik URL) → birleştir (kaynak etiketiyle) → sıkıştırılmış DB.
  - Sorgu biçimi: alan adı için 64 bit hash sıralı dizi veya FST + URL önekleri. Sorgu süresi < 1 ms.
- **Native host protokolü:**
  - `hello{ext_version, schema}` → `{app_version, db_version}`
  - `lookup{url}` → `{verdict, categories[], sources[{name, list_date}]}`
  - `rules.sync{since_version}` → DNR kural parçaları
  - `scan.download{path}` (isteğe bağlı)
  - `report.false_positive{url}` → yalnızca yerel kayıt; gönderim her seferinde kullanıcı onayıyla yapılır
- **Uzantı:**
  - Servis çalışanı durum tutmaz (`chrome.storage`).
  - DNR dinamik kuralları: `main_frame` + `requestDomains` → `redirect: { extensionPath: "/blocked.html?…" }`.
  - "Yine de devam et" seçeneği, yalnızca o sekme için daha yüksek öncelikli `allow` kuralı ekler (oturum kuralı, süreli).
  - DNR kapasitesine sığmayan URL düzeyindeki girdiler `webNavigation.onBeforeNavigate` → host sorgusu → `tabs.update` yolundan gider. Bu yolun sınırlaması belgelenir.
- **Uygulama yoksa:** Uzantı en son DNR kurallarıyla çalışmaya devam eder ve "Uygulama bağlantısı yok — liste tarihi X" uyarısı gösterir.
- **Dağıtım ve kurulum (ADR-011; mağaza yok):**
  - **Sabit yol:** Uzantı `%ProgramFiles%\AlterDNS\browser-extension\` altındadır. Tarayıcı paketlenmemiş uzantıyı bu yoldan okur; A/B dizini değişse de yol aynı kalır.
  - **Sabit kimlik:** Manifestte `key` (açık anahtar) bulunur. Kimlik derlemede hesaplanır ve native host manifestinin `allowed_origins` alanına yazılır. Özel anahtar gerekmez ve depoya girmez.
  - **Tarayıcı tespiti:** `HKLM/HKCU\SOFTWARE\Clients\StartMenuInternet` ve `App Paths` kayıtları; Chrome, Edge, Brave, Opera ve Vivaldi.
  - **Native host kaydı:** Her tespit edilen tarayıcının kendi `NativeMessagingHosts` anahtarına yazılır (journal'lı). Chromium türevlerinin anahtar yolları doğrulanacak.
  - **"Tarayıcıda kur" akışı:**
    1. Tarayıcı başlatılır.
    2. `chrome://extensions` adresi panoya kopyalanır (komut satırından açılıyorsa doğrudan açılır; doğrulanacak).
    3. Klasör Explorer'da seçili gösterilir.
    4. Ekran görüntülü yönerge sunulur.
    5. `hello` mesajı beklenir (en fazla 10 dk); gelince başarı ekranı gösterilir.
  - **Kalp atışı:** Uzantı `alarms` ile periyodik `ping` gönderir; ayrıca tarayıcı açılışında `hello` yapar. Tarayıcı çalışırken 24 saattir `hello` gelmemişse tepsi `ADNS-WEB-005` bildirir. Tarayıcının çalışıp çalışmadığı işlem listesinden anlaşılır.
  - **Güncelleme:** `maint` uzantı dosyalarını atomik olarak değiştirir (geçici dizin → yeniden adlandırma). Sonraki `hello`'da host yeni sürümü bildirir, uzantı `chrome.runtime.reload()` çağırır. Paketlenmemiş uzantıda davranış Faz 4'te doğrulanacak; çalışmazsa kullanıcıdan "Yenile" düğmesine basması istenir.
  - **Kaldırma:** Dosyalar ve native host kayıtları silinir. Tarayıcıdaki giriş kullanıcı tarafından kaldırılır; yönerge gösterilir.

## 8. Kurulum, bileşenler, güncelleme, kaldırma

- **Bileşen manifesti** (`release-manifest.json`, Ed25519 imzalı):
  ```json
  {"schema":1,"channel":"stable","version":"1.0.0","expires":"2026-12-31T00:00:00Z",
   "min_updater":"1.0.0",
   "components":[{"id":"core","version":"1.0.0","url":"…/core-1.0.0.zip","sha256":"…","size":123,
                  "requires":[],"requires_admin":true,"system_changes":["service","uninstall-entry"]}],
   "next_key":null}
  ```
- **Manifest konumu (D1):** `https://github.com/fxrkqnplus/alterdns/releases/latest/download/release-manifest.json` ve `release-manifest.json.sig`. Ön sürüm kanalı için ayrı adlandırılmış manifest kullanılır. API çağrısı yapılmaz; hız sınırı davranışı doğrulanacak.
- **Bileşenler:** `core` (zorunlu), `file-security`, `file-security.yara-rules`, `file-security.clamav` (isteğe bağlı), `file-security.sandbox` (deneysel), `browser`, `connection` (WinDivert dahil).
- **Kurulum txn'i:** indir → hash ve imza doğrula → `%ProgramFiles%\AlterDNS\<sürüm>\` altına aç → journal kayıtlarıyla sistem değişiklikleri → sağlık kontrolü → commit. Her adımın hata kodu vardır.
- **Güncelleme:** Servis indirip doğrular; `maint` yeni sürümü A/B dizinine kurar → `current` değişir → servis ve tepsi yeniden başlar → 60 sn sağlık kontrolü yapılır (IPC ping, motor öz-testi, UI açılabiliyor mu). Başarısızlıkta önceki sürüme dönülür ve `ADNS-UPD-0xx` kodu gösterilir. Bir önceki sürüm, bir sonraki başarılı güncellemeye kadar saklanır.
- **Kaldırma:**
  - **(A) Tam temizlik:** Journal ters sırada geri alınır; bütün dizinler silinir; servis, native host ve uzantı ipucu kayıtları kaldırılır. Kullanıcı uzantıyı tarayıcıdan kendisi kaldırır; yönergeler gösterilir.
  - **(B) Raporları koruyarak kaldırma:** `%LOCALAPPDATA%\AlterDNS\reports` dizini kalır, gerisi (A) ile aynıdır.
  - Son adım bir kalıntı taraması yapar: dosya, kayıt defteri, servis, sürücü, görev, başlangıç, native host, güvenlik duvarı. Sonuç ekranda gösterilir.
  - `maint` geçici kopyadan çalışır ve kendini `FILE_FLAG_DELETE_ON_CLOSE` ile siler.
- **WinDivert sürücüsü:**
  - Yalnızca AlterDNS kurduysa ve başka bir WinDivert istemcisi yoksa durdurulur ve silinir.
  - Aksi halde dosyalar silinir; sürücü bir sonraki yeniden başlatmada boşalır (WinDivert belgesi). Kullanıcı bilgilendirilir.

## 8.1 Yayın hattı (ADR-012)

```
tag vX.Y.Z (korumalı) ─▶ GitHub Actions: release.yml (windows-latest)
  1. checkout (SHA sabit), rust-toolchain.toml, cargo build --locked --release (deterministik bayraklar)
  2. testler + cargo deny + lisans/başlık denetimi
  3. paketleme: bileşen zip'leri, SBOM (CycloneDX), THIRD_PARTY_NOTICES, kaynak arşivi (GPL)
  4. [SignPath onaylıysa] imzalama isteği → elle onay → Authenticode imzalı ikililer
  5. SHA256SUMS; actions/attest-build-provenance (+ SBOM attestation)
  6. release-manifest.json üretimi → Ed25519 imzası (release ortamı veya yerel imza adımı)
  7. GitHub Release taslağı → kullanıcı onayı → yayın
```

- **Ortam koruması:** `release` ortamı, gerekli onaylayıcı olarak kullanıcıyı ister. Gizli değişkenler (SignPath belirteci, gerekirse Ed25519 anahtarı) yalnız bu ortamdadır.
- **Doğrulama yönergesi** (sürüm notları ve README): `gh attestation verify <dosya> -R fxrkqnplus/alterdns`, `SHA256SUMS`, imza durumu.
- **Yeniden üretilebilirlik kontrolü:** CI çıktısının imzasız hash'i, yerel `cargo xtask package --reproducible` çıktısıyla karşılaştırılır (T-REL-001).

## 9. Arayüz işlemleri

Ayrıntılar `UI_SPEC.md`'dedir. Teknik notlar:

- Flyout tepsi işleminde çalışır; ilk gösterimde oluşturulur.
- Kapandıktan sonra pencere ve renderer serbest bırakılır. Bellek iadesi spike'ta ölçülür; yeterli değilse flyout da kısa ömürlü ayrı bir işleme taşınır.
- Toast bildirimleri için AUMID'li Başlat menüsü kısayolu oluşturulur (journal kaydıyla).

## 10. Log ve gözlemlenebilirlik

- `tracing` + `tracing-appender`; JSON satırları, döngüsel dosyalar. Toplam ≤ 10 MB.
- Varsayılan düzey `info`. "Tanılama modu" 30 dk süreyle `debug` açar ve sonra kendiliğinden kapanır.
- Kişisel veri en aza indirilir: URL'ler yalnız alan adı olarak tutulur, dosya yolları kullanıcı adı maskelenerek yazılır. Ağ parmak izleri hash'lenir.
- "Tanılama paketi oluştur": loglar, sürümler ve parmak izi özeti tek bir ZIP'e konur. Kullanıcı önce içeriği görür; otomatik gönderim yoktur.

## 11. Dil desteği

- Türkçe varsayılan, İngilizce ikinci dildir. Slint `@tr()` + gettext `.po` ile ya da Fluent ile; spike'ta seçilir.
- Hata kataloğu ve UI aynı dil anahtarı sistemini kullanır. Eksik çeviri CI'da hata verir.
