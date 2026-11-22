# AlterDNS — Mimari

Durum: taslak (2026-11-09). Faz 0 spike'larından sonra ADR'lerle kesinleşir.

## 1. İlkeler

- **En az ayrıcalık:** Yalnızca paket motoru, journal ve güncelleme uygulaması yönetici yetkisiyle çalışır. Güvenilmeyen içerik hiçbir zaman ayrıcalıklı bir işlemde ayrıştırılmaz.
- **Kapanınca güvenli:** Servis çökerse motor kaynakları bırakılır ve trafik normal akar (WinDivert'te handle kapanır; TUN tasarımında rota ve adaptörün bırakılması D8 spike'ında ölçülür). Yarım kalan sistem değişiklikleri açılışta journal'dan geri alınır.
- **Tembel yükleme:** Arayüz, tarama işçisi ve ana pencere ihtiyaç anında başlar ve işi bitince çıkar. Boştayken yalnızca servis ve küçük tepsi ajanı çalışır.
- **Olay güdümlü çalışma:** Yoklama (polling) yerine Windows bildirimleri kullanılır. Boştayken 60 sn'den sık zamanlayıcı yoktur.
- **Modülerlik:** Her modül ayrı bir bileşen paketidir. Kurulu olmayan modülün kodu diskte bulunmaz; arayüz bu modülü "Yüklü değil" olarak gösterir.

## 2. İşlem modeli

| İşlem | Bağlam | Yaşam süresi | Görev |
|---|---|---|---|
| `alterdns-svc.exe` | Windows servisi, LocalSystem. Hizmet SID'i tanımlı. `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 \| APPLICATION_DIR)` | Sürekli | Bağlantı motoru (teknolojisi D8'e bağlı; ADR-003; Wintun seçilirse `wintun.dll`'i yalnızca bu işlem yükler, ağacında GPL-3.0-only yok — `deny-svc.toml`), ağ izleyici, ayarlayıcı, itibar DB ve beslemeleri, güncelleyici (indirme + doğrulama), journal, IPC sunucusu, yeniden başlatmada silme |
| `alterdns-ui.exe --tray` | Etkileşimli kullanıcı, orta bütünlük | Oturum boyunca | Tepsi simgesi, flyout, toast bildirimleri, indirme klasörü izleyici, tarama aracısı (işçileri başlatır), karantina (kullanıcı dosyaları) |
| `alterdns-ui.exe` | Kullanıcı | İsteğe bağlı açılır; pencere kapanınca çıkar | Ana pencere. Tepsi kipiyle aynı ikilidir (Slint bir kez gömülür, ADR-002); ayrı işlem olarak çalışır |
| `alterdns-scan.exe` | AppContainer (yetenek yok), Low IL, Job Object | Tarama başına | Tür tespiti, açma, analizörler, YARA-X. Girdi: devralınmış salt okunur handle. Çıktı: pipe üzerinden JSON |
| `alterdns-nmh.exe` | Tarayıcının başlattığı kullanıcı işlemi | Tarayıcı bağlantısı boyunca | stdio native messaging ↔ servis pipe'ı. Mesajları şema doğrulamasından geçirir |
| `alterdns-maint.exe` | Yönetici (UAC); bekçi kipinde başlatan işlemin çocuğu | Kurulum, değiştirme, kaldırma, güncelleme uygulama; bekçi kipinde txn süresince | Bileşen yöneticisi motoru; `watchdog` alt komutu (§6) |
| `alterdns-setup.exe` | Önyükleyici; yönetici yetkisini kendisi ister | Kurulum | Bileşen seçimi, manifest indirme, `maint` motorunu çalıştırma |

**Servis:**
- Ad `AlterDNS`, görünen ad `AlterDNS`. LocalSystem, hizmet SID türü `unrestricted`, tırnaklı yol, otomatik başlangıç.
- Kurtarma: 5 sn / 30 sn / 60 sn sonra yeniden başlat, sayaç 86 400 sn'de sıfırlanır. `SERVICE_CONFIG_FAILURE_ACTIONS_FLAG` açıktır; hata koduyla duran servis de yeniden başlar.
- `SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO` ile yalnızca gereken ayrıcalıklar tutulur.
- Pipe başkası tarafından tutuluyorsa (`ADNS-IPC-005`) yeniden başlatma denemeleri boşa döner. Bu bilinçli olarak kabul edilmiştir.

**DLL arama yolu:** Bütün exe'ler `main`'in başında `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)` çağırır; servis ek olarak `APPLICATION_DIR` ekler. Ayrıca `/DEPENDENTLOADFLAG:0x800` ile bağlanırlar; böylece `main`'den önce çözülen yükleme zamanı içe aktarımları da yalnızca System32'den gelir.

**C çalışma zamanı statik bağlanır** (`+crt-static`, `.cargo/config.toml`):
- `vcruntime140.dll` Windows'un parçası değildir; VC++ Redistributable ile gelir.
- Yukarıdaki bayrak yüzünden uygulama dizininden de yüklenemez. Dinamik bağlansaydı yeniden dağıtılabilir paketi olmayan makinede ikililer hiç açılmazdı.
- Testler her exe'nin statik içe aktarmalarını PE'den okur. Servis ve bakım ikilisinde hepsi KnownDLL, `ntdll` ya da API kümesi olmalıdır. Arayüz ikilisi pencere sistemi için KnownDLL olmayan sistem DLL'lerini de (ör. `dwmapi`, `uxtheme`) içe aktarır; bunların hepsi System32'de bulunmalıdır ve `DEPENDENTLOADFLAG` yüklemeyi oraya sınırlar. Bu yüzden uygulama dizinine bırakılan bir DLL yükleme zamanında kullanılamaz.

**Tepsi simgesi kimliği:**
- `NOTIFYICONDATA.guidItem` exe yoluna bağlıdır ve A/B sürüm dizinlerinde yol değişir. Faz 1'de bu yüzden `uID` kullanılır.
- Faz 5'te sabit bir başlatıcıyla (`%ProgramFiles%\AlterDNS\alterdns-tray.exe`) GUID'e geçiş değerlendirilir.
- Görev çubuğunun "simgeyi göster" tercihi iki yöntemde de exe yoluna bağlıdır; kalıcı çözüm sabit başlatıcıdır.

## 3. Depo ve crate yapısı

```
alterdns/
├─ LICENSE                    # GPL-3.0 (kanonik metin)
├─ TRADEMARK.md               # ad ve logo kullanım notu
├─ PRIVACY.md                 # gizlilik politikası (Faz 4)
├─ CONTRIBUTING.md               # geliştirme kuralları
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
├─ apps/ svc/ ui/ scan/ nmh/ maint/ setup/   # ui: alterdns-ui.exe (tepsi ve ana pencere kipleri)
├─ extension/                 # MV3 TypeScript (Chrome + Edge; paketlenmemiş dağıtım), INSTALL.md
├─ xtask/                     # check, test, perf, dev-install, lab, package
├─ assets/brand/              # logo kopyaları ve türetilmiş simgeler
├─ lab/                       # araştırma senaryoları (sonuçlar git dışı: lab/results/)
└─ docs/
```

Platformdan bağımsız crate'ler (`scan-engine`, `reputation` ayrıştırıcıları, `dpi` paket kurucuları, `errors`, `update` doğrulaması) Linux'ta da derlenir. Böylece WSL ve Linux CI'da fuzz edilebilir.

## 4. IPC

- **Taşıma:** Named pipe `\\.\pipe\AlterDNS.Svc.v1`.
  - Güvenlik tanımlayıcısı `O:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008b;;;IU)`: sahibi açıkça SYSTEM (istemci bunu ister; servis belirtecinin varsayılan sahibi Administrators olabilir), SYSTEM ve Administrators tam erişim; `INTERACTIVE` yalnızca okur ve yazar (`FILE_GENERIC_READ | FILE_WRITE_DATA`).
    - `GENERIC_WRITE` verilmez: içindeki `FILE_APPEND_DATA` pipe'larda `FILE_CREATE_PIPE_INSTANCE`'tır. Verilseydi etkileşimli bir kullanıcı servis çalışırken aynı ada kendi sunucu örneğini açabilirdi. Bu makinede ölçüldü (Faz 1C).
    - İstemci `GENERIC_READ | FILE_WRITE_DATA` ile açar.
  - Sunucu, pipe squatting'e karşı `FILE_FLAG_FIRST_PIPE_INSTANCE` ve `PIPE_REJECT_REMOTE_CLIENTS` kullanır.
  - Bir istemci bağlanınca sonraki dinleyen örnek hemen açılır; böylece servis çalıştığı sürece ad boşa çıkmaz.
  - En çok 16 örnek. `svc.hello` 5 sn içinde gelmelidir; başlamış bir mesaj ve bir yazma da 5 sn içinde bitmelidir. Okumayan istemci düşürülür.
  - Hata yanıtından sonra sunucu, bağlantıyı kesmeden önce istemcinin kapatmasını bekler (en çok 5 sn). `DisconnectNamedPipe` okunmamış veriyi siler; hemen kesilseydi istemci hata kodunu göremezdi.
- **İstemcinin sunucuyu doğrulaması:**
  - İstemci pipe'ı `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION` ile açar.
  - Pipe nesnesinin sahibinin SYSTEM olduğunu `GetSecurityInfo` ile doğrular. Servis yeniden başlarken aynı adla pipe açan bir işlem böylece reddedilir.
  - İmaj yolu denetimi ek bir katmandır.
- **Çağıran yetkisi:** Yetki çağıranın belirtecinden gelir (`ImpersonateNamedPipeClient`: SID, oturum, yükseltme, `INTERACTIVE` grubu).
  - Kimliğe bürünme ancak pipe'tan veri okunduktan sonra çalışır. Bu yüzden ilk çerçevenin öneki okunduktan sonra yapılır.
  - İstemci tanımlama düzeyinde bağlandığı için belirteç `OpenThreadToken(…, OpenAsSelf = TRUE)` ile açılır.

  Yöntemler üç sınıftadır:
  - `Open`: `svc.hello`, `svc.ping`.
  - `InteractiveUser`: `svc.status`, `events.subscribe`.
  - `Elevated`: yönetici belirteci ve kurulum dizinindeki AlterDNS ikilisi. Faz 1'de `svc.reload_config`; sonraki fazlarda motor yapılandırması, sistem değişikliği ve güncelleme uygulama.
- **İşlem tutamacı ve imaj yolu:**
  - Bağlantı anında istemci işlemi `OpenProcess` ile açılır ve tutamaç bağlantı boyunca tutulur; böylece PID yeniden kullanımına karşı korunur.
  - İmaj yolu bu tutamaçtan `QueryFullProcessImageNameW` + `GetFinalPathNameByHandle` ile okunur ve ordinal olarak karşılaştırılır. İmza varsa doğrulanır.
  - Aynı kullanıcıdaki kod bir AlterDNS ikilisine enjekte edilebileceği için imaj yolu tek başına güvenlik sınırı değildir.
- **Onay belirteci:** Kullanıcı onayı gerektiren yöntemler bir onay belirteci ister. Belirteç tepsi veya UI onay ekranından üretilir; ilk onaylı yöntemle birlikte gelir (Faz 2).
- **Çerçeve:** 4 bayt uzunluk + JSON. Mesaj başına üst sınır 1 MB.
  ```json
  {"v":1,"id":"…","method":"svc.status","params":{}}
  {"v":1,"id":"…","ok":true,"result":{…}}
  {"v":1,"id":"…","ok":false,"error":{"code":"ADNS-IPC-002","params":{…}}}
  ```
- **El sıkışma ve sürümleme:**
  - İlk çağrı `svc.hello {client_version, min_server}` olur; yanıt `{server_version, min_client, instance}`. `min_client` servisin kendi sürümüdür; `instance` servisin başlangıç zamanıdır ve tepsi yeniden başlamayı buradan anlar (`ADNS-SVC-002`).
  - Sürümler uyuşmazsa ya da `v` alanı farklıysa `ADNS-IPC-004` döner ve UI güncelleme önerir.
  - Bilinmeyen yöntem `ADNS-IPC-002` döner; bilinmeyen alan ve olay yok sayılır.
- **Olaylar:**
  - İstemci `events.subscribe` çağırır; servis durum değişikliklerini iter (bağlantı durumu, engelleme sayacı, güncelleme, hata).
  - Abonelik, çağıranın belirtecindeki oturum kimliğiyle etiketlenir. Kullanıcıya özgü bir olay yalnızca o oturuma gider (Hızlı Kullanıcı Değiştirme, RDP).
  - Yayıncı beklemez: her abonenin 64 olaylık kuyruğu vardır, dolarsa olay o abone için düşer. Olayları okumayan abone yazma zaman aşımında düşürülür.
- **Dil:** Servis kullanıcıya metin üretmez, yalnızca hata kodu ve parametre gönderir. Metni istemci kendi dilinde gösterir.

## 5. Veri ve yapılandırma yolları

| Yol | İçerik | ACL / not |
|---|---|---|
| `%ProgramFiles%\AlterDNS\<sürüm>\` | İkililer (A/B). `current` işaretçisi: `HKLM\SOFTWARE\AlterDNS\CurrentVersion` | Yönetici yazar, herkes okur |
| `%ProgramFiles%\AlterDNS\alterdns-tray.exe` | Sabit başlatıcı (Faz 5; tepsi simgesi kimliği için, §2) | |
| `%ProgramFiles%\AlterDNS\browser-extension\` | Paketlenmemiş MV3 uzantısı (sabit yol; A/B dışında) | Yalnız yönetici yazar, kullanıcı okur (tarayıcı okuyabilsin) |
| `%ProgramData%\AlterDNS\` | `config/` (makine), `journal/`, `reputation/`, `rules/` (YARA), `strategies/`, `staging/`, `logs/` | SYSTEM + Administrators yazar; Users yalnızca gerekli alt dizinleri okur. Dizin korumalı DACL ile (kalıtım kapalı) oluşturulur. Önceden varsa sahibi ve DACL'i doğrulanır; reparse noktası reddedilir. Servis açılışta aynı denetimi yapar, tutmazsa başlamaz (`ADNS-SVC-001`) |
| `%LOCALAPPDATA%\AlterDNS\` | `config/` (kullanıcı), `reports/`, `quarantine/`, `logs/` | Kullanıcı + SYSTEM. Karantina dizininde çalıştırma izni yok, kalıtım kapalı. Servis bu dizine yazmaz |
| `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.alterdns.host` ve Edge karşılığı | Native host kaydı | Journal'a yazılır |
| `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run` değeri `AlterDNS` | Oturum açılışında tepsiyi başlatır (makine kapsamı). `run_at_logon = false` olan kullanıcıda tepsi açılıp hemen çıkar | Journal'a yazılır (`StartupEntry`); yalnızca kendi değerimiz |
| `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\AlterDNS` | Kaldırma kaydı (Publisher: Furkan Okan, ModifyPath, UninstallString) | Journal'a yazılır |

Yapılandırma TOML biçimindedir ve şeması sürümlüdür (`schema = N`).
- **Geçişler:** İleri yönlüdür; geçişten önce yedek alınır.
- **Bilinmeyen anahtarlar** korunur.
- **Bozuk dosya** `.bak` olarak saklanır ve varsayılanla devam edilir.
- **Daha yeni şema** okunur ama üzerine yazılmaz.

Şema v1:
- **Kullanıcı** (`%LOCALAPPDATA%\AlterDNS\config\config.toml`): `schema = 1`; `[ui] language = "tr"|"en"`, `left_click = "flyout"|"main_window"`, `notifications = true`; `[startup] run_at_logon = true`; `[privacy] online_lookups = false`.
- **Makine** (`%ProgramData%\AlterDNS\config\machine.toml`): `schema = 1`.

## 6. Journal — sistem değişiklikleri

Journal, txn başına bir JSON Lines dosyasıdır (`%ProgramData%\AlterDNS\journal\<txn>.jsonl`); ilk satırı `{"t":"Schema","schema":1}`'dir. Her sistem değişikliği bir `ChangeRecord` olarak önce diske yazılır (fsync), sonra uygulanır. Yazma başarısız olursa değişiklik uygulanmaz. Canlı bir txn'in dosyası, sahibi olan işlem tarafından kilitli tutulur; başka bir işlem onu okuyamaz ve geri alamaz.

```
ChangeRecord { id, txn_id, component, kind, target, before, after,
               state: Pending|Applied|Reverted|Conflict|Acknowledged|Failed,
               created_by: "alterdns", ts }
```

- **İşlem (txn):**
  - Sınırları `TxnBegin { txn_id, lifetime: Persistent|Session }` ve `TxnCommit { txn_id }` satırları belirler.
  - Bileşen kurulumu ve güncelleme `Persistent`; ayarlayıcı denemesi ve motorun ağ nesneleri `Session`.
  - Başarısız bir txn'in kayıtları ters sırayla geri alınır.
- **Türler:**
  - Faz 1: `ServiceCreate`, `ServiceConfig`, `RegistryValue`, `StartupEntry`, `FileInstall`, `UninstallEntry`.
  - Faz 2: `TunAdapter`, `UnicastAddress`, `Route`. Hedefleri ad yerine GUID/LUID taşır.
  - Sonraki fazlar: `DriverService` (yalnızca AlterDNS oluşturduysa), `ScheduledTask`, `NativeHostRegistration`, `FirewallRule` (gerekirse), `ExternalExtensionHint`.
  - Kullanıcıya ait değerler için tür yoktur. Hop limit, DNS sunucuları ve DoH tablosu yazılmaz (ADR-010, ADR-014).
  - Gerçek işlemler aşamalı gelir: `RegistryValue` 1B'de (yalnızca REG_SZ; başka türde bir değer hata sayılır, "yok" sayılıp silinmez), servis, kaldırma kaydı ve kurulum dosyaları 1C'de, başlangıç kaydı 1D'de.
- **Hedef izin listesi:**
  - Her türün hedefi yüklemede sınırlanır: servis adı `AlterDNS`, `HKLM\SOFTWARE\AlterDNS\…`, kendi Run değeri, kurulum dizini, kendi adaptörü.
  - Listenin dışındaki bir hedefe dokunulmaz. Kayıt defteri kökleri sabittir; gerçek sistem işlemleri bunu txn'in hedeflerinden bağımsız olarak bir kez daha denetler.
  - AlterDNS'in oluşturduğu nesneler geri almada yalnızca kaldırılır, yeniden eklenmez.
- **Açılışta kurtarma:**
  - Commit edilmemiş txn'ler ve `Session` txn'leri geri alınır. Commit edilmiş `Persistent` txn'lere yalnızca kaldırmada dokunulur.
  - `Pending` bir kayıt uygulanmışsa geri alınır, uygulanmamışsa kapatılır.
  - Txn'ler en yeniden en eskiye geri alınır.
  - Yarım kalmış son satır dosyadan kesilip atılır. Ortadaki bozuk bir satır kurtarmayı durdurur (`ADNS-SYS-002`).
  - Bilinmeyen tür "açık, dokunulmadı" sayılır; böylece eski bir ikili yeni bir journal'da kilitlenmez.
  - Canlı bir işlemin tuttuğu (kilitli) dosya atlanır. Bir dosyanın hatası diğerlerini durdurmaz.
  - Sonuç loglanır.
- **Sahiplik kuralı:**
  - Journal'da `created_by = alterdns` kaydı bulunmayan hiçbir sistem öğesine kaldırma sırasında dokunulmaz. Örnekler: kullanıcının hop limiti, başka uygulamaların kurduğu WinDivert servisi, kullanıcının DNS ayarları.
  - Kurulum öncesi değerler `before` alanında saklanır.
  - Geri yüklemeden önce mevcut değerin hâlâ AlterDNS'in yazdığı değer olduğu kontrol edilir. Kullanıcı arada değiştirdiyse değer ezilmez: kayıt `Conflict` olur ve kullanıcıya `ADNS-SYS-003` gösterilir. Kullanıcı onaylayınca kayıt `Acknowledged` olur.
- **Bekçi:** Ayrı bir işlemdir: `alterdns-maint watchdog --txn <id>`.
  - **Yollar:** Journal ve rapor yolu (`<txn>.watchdog.json`) `%ProgramData%\AlterDNS\journal\` altından türetilir; komut satırından keyfi yol alınmaz. Dizin korumalı değilse (K20) bekçi hiçbir şeye dokunmadan çıkar.
  - **Kira protokolü (stdin):** Ebeveyn kirayı `RENEW <sn>` ile yeniler (1–60 sn; ilk kira 60 sn) ve işi bitince `DONE` gönderir. İzlenecek motoru bildiren `ENGINE` Faz 2'de gelir.
  - **Geri alma:** Stdin kapanırsa ya da kira dolarsa bekçi txn'i açılış kurtarmasıyla aynı kuralla geri alır: commit edilmiş `Persistent` txn'e dokunulmaz. Ebeveyn dosyayı hâlâ tutuyorsa (askıda) bekçi en çok 10 sn bekler; bırakılmazsa geri alma açılış kurtarmasına kalır ve raporda `InUse` yazar. Faz 2'de motor ve askıdaki ebeveyn PID ile değil, devralınan tutamaçla izlenir ve sonlandırılır. Bekçi yalnızca Win32 API kullanır.
  - **Başlatma:** Bekçi, onu başlatan işlemin kardeş ikilisidir. Başlatan yükseltilmişse ikilinin dizini kullanıcıya yazılamaz olmalıdır.
  - **Servisle ilişkisi:** Servis kendi txn'leri için bekçiyi kendi çocuğu olarak başlatır. Servis çökerse bekçi geri alır, SCM servisi yeniden başlatır. Servis açılışta kurtarmayı çalıştırır; o sırada bekçinin geri aldığı (kilitli) dosya atlanır ve sonucu bekçi raporunda görünür.
- **Lab journal'ı:** Faz 0 bağlantı lab'ının journal'ı ve bekçisi (`xtask/src/lab/`) bu crate'e taşınmadı. Lab, Faz 2'ye kadar kendi kopyasıyla çalışır.

## 7. Modüller

### 7.1 Bağlantı yönetimi

> **D8 notu (2026-11-09):** Bu bölüm WinDivert tabanlı ilk tasarımı anlatır. Defender resmi WinDivert arşivini engellediği için WinDivert askıdadır. Aday tasarım: Wintun TUN adaptörü + kullanıcı modu motor (yalnızca engelli servislerin IP'leri adaptöre yönlendirilir; TCP motorda yeniden kurulup kendi soketlerimizle gönderilir; UDP kendi soketlerimizle iletilir); yedek: yerel aktarıcı (PAC). Bölüm, D8 kararından sonra yeniden yazılır (ADR-003).

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
- **Bileşenler:** `core` (zorunlu), `file-security`, `file-security.yara-rules`, `file-security.clamav` (isteğe bağlı), `file-security.sandbox` (deneysel), `browser`, `connection` (paket/TUN sürücüsü D8'e bağlı).
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

Ayrıntılar `UI_SPEC.md`'dedir. Faz 1'de gerçekleşen yapı:

- **İkili ve kipler:** `alterdns-ui.exe` (`apps/ui`), arayüz bileşenleri `crates/ui` (Slint, yazılım çizici, `fluent` stili; ADR-002), Win32 işleri `crates/winsys`. `--tray [--logon]` tepsi kipi; argümansız ya da `--page <ad>` ana pencere. Tanınmayan argüman çıkış 2; başlatma hatası katalog metinli ileti kutusu (`ADNS-UI-001`) ve çıkış 1.
- **Tek örnek:** `Local\AlterDNS.Tray` ve `Local\AlterDNS.Window` mutex'leri; her oturum kendi örneğini çalıştırır. İkinci ana pencere işlemi, var olan pencereye kayıtlı `AlterDNS.Broadcast.v1` iletisini (`alterdns_core::ui` kodları: ayarlar değişti, öne gel, kapan, sayfa) gönderip çıkar. Hedef `FindWindowW` ile yalnızca çağıranın masaüstünde aranır; başka oturumlara ileti gitmez.
- **Gizli ileti penceresi:** Görünmeyen üst düzey pencere (ileti penceresi değil; `TaskbarCreated` ve `WM_SETTINGCHANGE` yalnızca üst düzey pencerelere yayınlanır). Tepsi simgesi `Shell_NotifyIcon` sürüm 4, `uID` 1; Explorer yeniden başlayınca yeniden eklenir.
- **Olay sırası:** Kabuk olayları, Slint geri çağrıları ve durum iş parçacığı komutları Slint olay döngüsüne kuyruklanır ve sırayla işlenir; bir işleyici çalışırken iç içe çağrılar durumu ödünç almaz.
- **Flyout:** Tepsi işleminde, başlangıçta bileşen olarak oluşturulur ve gizlenip gösterilir. Konum, simge dikdörtgeni ve `ABM_GETTASKBARPOS` kenarından saf bir işlevle hesaplanır (otomatik gizli görev çubuğu ve taşma alanı dahil). Bellek ölçümü 1E'de yapılır.
- **Pencere sertleştirme:** Gösterilmeden önce `WH_CALLWNDPROCRET` ile `WM_SHOWWINDOW` yakalanır ve `GWLP_WNDPROC` zinciriyle alt sınıf kurulur. comctl32'nin `SetWindowSubclass`'ı kullanılmaz: sürüm 5.82'de yalnızca sıra numarasıyla dışa aktarılır ve bildirimsiz ikililer adla bağlanınca açılmaz. Çatı stili geri yazabildiği için kurallar `WM_STYLECHANGING` ve `WM_WINDOWPOSCHANGING`'de yeniden uygulanır. Ana pencere simgesi exe'nin çok boyutlu simgesinden (kaynak 1) pencerenin DPI'ına göre yüklenir.
- **Tema:** `HKCU` tema ve DWM vurgu değerleri ile `SPI_GETCLIENTAREAANIMATION` yalnızca okunur; `WM_SETTINGCHANGE("ImmersiveColorSet")` ve `WM_DWMCOLORIZATIONCOLORCHANGED` ile yenilenir.
- **Durum:** Ayrı iş parçacığında IPC istemcisi; kopunca 2, 4, 8, 16, 30 sn geri çekilmeyle yeniden bağlanır, bağlıyken boşta uyanmaz. Servis örneği değişince `ADNS-SVC-002` balonu gösterilir; gösterilemezse `ADNS-UI-002` loglanır.
- **Bildirim:** Faz 1'de tepsi balonu (`NIF_INFO`, sessiz saatlere uyar); Windows bunu bildirim olarak gösterir. AUMID'li Başlat menüsü kısayoluyla toast Faz 5'te kurulumla birlikte değerlendirilir.
- **Oturum açılışı:** `HKLM\…\Run\AlterDNS` = `"<kurulum>\<sürüm>\alterdns-ui.exe" --tray --logon` journal'daki `StartupEntry` ile yazılır; komut birebir bu biçimde doğrulanır. Kullanıcı başlatmayı kapattıysa `--logon` ile açılan tepsi hemen çıkar.
- **Kaldırma:** Kaldırma ve güncelleme öncesi açık arayüz işlemlerine kapanma iletisi gönderilir; kurulu exe açıkken silinemez.
- Tepsi çökerse onu yeniden başlatan bir mekanizma Faz 1'de yoktur.

## 10. Log ve gözlemlenebilirlik

- `tracing` + kendi boyut sınırlı yazıcı (`CappedWriter`, 2 × 5 MiB, kullanıcı adı maskeli); JSON satırları. Servis `%ProgramData%\AlterDNS\logs`, arayüz `%LOCALAPPDATA%\AlterDNS\logs\{tray,window}.log`.
- Varsayılan düzey `info`. "Tanılama modu" 30 dk süreyle `debug` açar ve sonra kendiliğinden kapanır.
- Kişisel veri en aza indirilir: URL'ler yalnız alan adı olarak tutulur, dosya yolları kullanıcı adı maskelenerek yazılır. Ağ parmak izleri hash'lenir.
- "Tanılama paketi oluştur": loglar, sürümler ve parmak izi özeti tek bir ZIP'e konur. Kullanıcı önce içeriği görür; otomatik gönderim yoktur.

## 11. Dil desteği

- Türkçe varsayılan, İngilizce ikinci dildir.
- Arayüz metinleri Slint `@tr()` ile yazılır:
  - msgid kararlı bir İngilizce anahtardır. Derleyicinin varsayılan bağlamı (bileşen adı) kapalıdır; bağlam yalnızca `@tr("bağlam" => "metin")` ile verilir.
  - `crates/ui/lang/{tr,en}/LC_MESSAGES/alterdns-ui.po` ikisi de zorunludur ve ikiliye gömülür. Dil çalışırken değişir; açık pencerelerin ve tepsi ipucunun metni hemen güncellenir.
  - Rust tarafında gereken metinler (tepsi ipucu) de `.slint` içindeki bir globalden okunur; kodda kullanıcıya görünen metin yazılmaz. Dil adları kendi dillerinde yazılır.
- Hata metinleri `errors/catalog.toml`'dan gelir. Arayüz ve hata metinleri aynı dil seçimini (`Lang`) kullanır.
- Arayüzde büyük/küçük harf dönüşümü (`to_uppercase`) yapılmaz (Türkçe i/İ).
- `cargo xtask check` adımı `i18n`: eksik, boş ya da bulanık çeviri, kullanılmayan anahtar ve yer tutucu sayısı farkı hatadır.
