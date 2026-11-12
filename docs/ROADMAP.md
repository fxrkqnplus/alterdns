# AlterDNS — Yol Haritası

Her faz şu dört parçadan oluşur: **çıktılar**, **bağımlılıklar**, **testler** ve **çıkış ölçütleri**. Çıkış ölçütü sağlanmadan sonraki faza geçilmez. Her fazın başında `superpowers:writing-plans` ile ayrıntılı uygulama planı yazılır (`docs/superpowers/plans/`). Bu belge o planların neyi kapsaması gerektiğini tanımlar.

Test kimlikleri (T-…) `TESTING.md` belgesindedir.

---

## Faz 0 — Ortam, doğrulama, spike'lar, bağlantı araştırması

**Çıktılar**
1. **Araç zinciri doğrulaması.** Geçici bir projede `cargo build` başarılı olmalı. Windows SDK eksikse kullanıcıya kurulum adımı bildirilir. `cargo-deny`, `cargo-about`, `cargo-cyclonedx` ve (isteğe bağlı) `cargo-nextest` kurulur.
2. **Depo iskeleti (public depo).**
   - `git init` yapılır. `LICENSE` (GPL-3.0) ve `TRADEMARK.md` doğrulanır.
   - `.gitignore`'un `CLAUDE.local.md` ve `lab/results/` dosyalarını dışarıda bıraktığı kontrol edilir.
   - `rust-toolchain.toml`, boş workspace ve CI taslağı eklenir (`.github/workflows/ci.yml`: windows-latest, `cargo xtask check`, action'lar SHA ile sabit, en az yetki).
   - Logolar `assets/brand/` altına kopyalanır.
   - İlk push'tan önce `git ls-files` ile kişisel veri denetimi yapılır.
3. **GitHub güvenlik ayarları** (kullanıcı onayıyla):
   - Secret scanning ve push protection
   - Dependabot
   - Dal koruması
   - Actions salt okunur token
   - `release` ortamı (gerekli onaylayıcı)
   - MFA
   - CodeQL'in Rust desteği varsa etkinleştirilir
4. **`THIRD_PARTY.md` doğrulaması.** "Doğrulanacak" satırlarının her biri için kaynak bağlantısı ve tarih yazılır. WinDivert 2.2.2-A arşivinin SHA-256 değeri ve sürücü imzası kaydedilir.
5. **Spike: UI (ADR-002).** Slint ve Win32/D2D ile flyout prototipi hazırlanır. Ölçülecekler ADR-002'de listelidir. Rapor ölçüm tablosu içerir.
6. **Spike: Wintun (ADR-003 / D8).** WinDivert, Defender tespiti nedeniyle askıda (2026-11-09).
   - wintun.net'ten resmi arşiv: hash, imza ve Defender tepkisi.
   - Adaptör oluşturma, tek bir test IP'si için journal'lı rota.
   - Kullanıcı modunda küçük bir TCP/UDP iletimi; CPU, RAM ve gecikme ölçümü.
   - Bekçi rotayı ve adaptörü geri alır; spike sonunda ikisinin de kalktığı doğrulanır.
7. **Spike: sandbox.** AppContainer + Low IL + Job Object + handle aktarımı. İşçinin dosya sistemine, ağa ve alt işleme erişemediği kanıtlanır.
8. **Bağlantı araştırma düzeneği (`xtask lab`).** `research/CONNECTIVITY_RESEARCH.md` §5 protokolü uygulanır: S0, S1, sürücüsüz soket teknikleri (S2-U), Wintun spike'ından sonra gerçek istemciler (S4), D5'e bağlı S5, S6. WinDivert'e bağlı S2–S3 iptal edildi. Sonuçlar §7'ye yazılır.

Not (2026-11-09): Geliştirici makinesindeki eski DPI paketi Faz 0 başında kaldırıldı (D6 kapandı). Yerel paket hash karşılaştırması bu fazdan çıktı; aynı adlı `WinDivert` servisi çakışması Faz 6 uyumluluk matrisine taşındı.

Not (2026-11-09, D8): Microsoft Defender resmi `WinDivert-2.2.2-A.zip` arşivini engelledi. Bağlantı motoru D8 ile yeniden değerlendiriliyor (aday: Wintun TUN + kullanıcı modu motor; yedek: yerel aktarıcı/PAC; WinDivert askıda). Karar Faz 0 ölçümlerinden sonra kesinleşir (ADR-003).

**Kullanıcı onayı gereken noktalar:** Wintun spike'ı (indirme, adaptör ve rota), gerçek istemci testleri, kendi soketimizde TTL karşılaştırması (S5-soket; D5 kapandı, sistem hop limiti değişmez), push ve depo ayarları.

**Testler:** T-LAB-001…006, T-SPK-001…004

**Çıkış ölçütleri**
- [ ] `cargo build` çalışıyor; CI taslağı yeşil.
- [ ] Depoda kişisel veri yok (`CLAUDE.local.md` ve lab sonuçları git dışı); güvenlik ayarları açık.
- [ ] ADR-002 ölçümle kesinleşti. ADR-003 / D8 için ölçümler (S2-U, Wintun spike'ı) tamamlandı ve karar kullanıcıya sunuldu.
- [ ] TTL 65 hipotezleri için ölçüm tablosu var. Her hipotezin durumu destekleniyor, çürütüldü veya belirsiz olarak işaretlendi; belirsizlerin gerekçesi yazıldı.
- [ ] Ağ ayarları başlangıç durumunda; Wintun spike'ının adaptörü ve test rotası kaldırıldı. Hop limit değişmedi.

---

## Faz 1 — Çekirdek altyapı

**Çıktılar**
1. Workspace ve lint ayarları: `unwrap_used`, `expect_used`, `panic` deny; workspace düzeyinde `unsafe_code = "deny"`, FFI dışı crate'lerde `#![forbid(unsafe_code)]` (winsys hariç). `xtask check` şunları denetler: telif ve SPDX başlığı (`GPL-3.0-or-later`), `deny.toml` lisans politikası (`THIRD_PARTY.md` §0), gizli bilgi ve kişisel veri kalıpları.
2. `errors` crate'i:
   - `catalog.toml` → build.rs → `AdnsErrorCode` enum'u ile TR/EN metinler.
   - Katalog doğrulayıcı: benzersiz kod, boş alan yok, her iki dil mevcut.
   - `ERROR_CATALOG.md` içeriği TOML'a taşınır.
3. `core` (yollar, config şeması v1, sürüm bilgisi), `winsys` (Win32 sarmalayıcıları) ve `winresource` ile exe sürüm bilgileri (yapımcı, telif: Furkan Okan).
4. `ipc` (§4 ARCHITECTURE): ACL, squatting koruması, çağıran doğrulaması, zarf, olay aboneliği.
5. `journal` (§6): kayıt, uygulama, geri alma, açılışta kurtarma, sahiplik kuralı, bekçi işlemi (`maint watchdog`).
6. Servis iskeleti:
   - Kur, başlat, durdur.
   - Hata durumunda kurtarma ayarı (failure actions).
   - Durum ve sağlık yöntemleri.
7. Tepsi ajanı:
   - Tema uyumlu simge, durum rozetleri, sağ/sol tık.
   - Flyout kabuğu: modül kartları "Yüklü değil" durumunda gösterilir.
   - "Ana pencereyi aç" düğmesi.
   - Oturum açılışında otomatik başlatma (journal'lı).
8. Ana pencere kabuğu: gezinme, Hakkında ekranı (Furkan Okan, telif, GPL-3.0-or-later, kaynak kod bağlantısı, üçüncü taraf lisansları, imza politikası), Ayarlar iskeleti.
9. Logolardan türetilmiş varlıklar: kare ana görsel, saydam varyantlar, 16/20/24/32 px tepsi glifleri (açık ve koyu tema, 4 durum), uygulama ICO'su.
10. `xtask`: `check`, `test`, `perf`, `dev-install`, `dev-uninstall`.

**Bağımlılık:** Faz 0. (D2 verildi: GPL-3.0-or-later.)

**Testler:** T-ERR-*, T-IPC-*, T-JRN-*, T-SVC-001…004, T-UI-001…008, T-PERF-001…003, T-PERF-010

**Çıkış ölçütleri**
- [ ] Servis ve tepsi IPC üzerinden konuşuyor. Yetkisiz istemci ayrıcalıklı yöntemi çağıramıyor.
- [ ] Journal kurtarma testi geçiyor: `kill -9` simülasyonu sonrası açılışta geri alma.
- [ ] Flyout gereksinimleri karşılanıyor: odak kaybında kapanma, sürüklenememe, DPI, çoklu monitör, görev çubuğunun dört kenarı.
- [ ] Boştaki RAM ve CPU bütçeleri karşılanıyor (T-PERF).
- [ ] `dev-install` → `dev-uninstall` sonrası kalıntı taraması temiz.

---

## Faz 2 — Bağlantı yönetimi

**Çıktılar**
1. `net-observe`: olay kaynakları (IP arayüzü, rota, adres, NLM, WLAN, uyanma), debounce, parmak izi, ölçülü bağlantı tespiti, hotspot ipuçları, hop limitinin salt okunur okunması, VPN/proxy tespiti.
2. `dpi` motoru:
   - Filtre kurucu
   - Akış izleme (bellek sınırlı LRU)
   - Teknikler: segmentasyon, SNI segmentasyonu, ters sıra, sahte paketler (TTL, otomatik TTL, checksum, seq), sahte SNI, HTTP varyasyonları, pasif DPI engelleme
   - UDP/QUIC teknikleri: Faz 0 bulgularına göre
   - Toplu G/Ç
   - Sayaçlar
3. `dns`: alan sınırlı DoH aktarıcı (önerilen) ve journal'lı sistem DoH modu (onaylı).
4. `probe`: hedef listeleri (`data/probes`), sınıflandırıcı (DNS, SNI, IP, yok), zaman sınırları, tekrar sayısı.
5. `tuner`: aday üretimi, deneme txn'leri, bekçi, puanlama, commit, izleme, geri çekilme, profil belleği.
6. `data/strategies/*.toml`: başlangıç kataloğu, Faz 0 ölçümlerinden türetilir.
7. Çakışma tespiti: GoodbyeDPI, winws, diğer WinDivert handle'ları. Kullanıcıya gösterilir ve onayla duraklatma önerilir.
8. UI:
   - Flyout'ta bağlantı anahtarı, durum, ağ satırı ve profil.
   - Ana pencerede Bağlantı sayfası: profiller, "Yeniden ayarla", tanılama, gelişmiş/lab (onaylı).
9. Başka DPI araçlarından geçiş:
   - Bilinen DPI araçlarının kurulumları ve profilleri tespit edilir.
   - Kullanıcı isterse profil bilgisinden aday sırası beslenir (kodu kopyalanmaz; davranışı referans alınır).
   - AlterDNS doğrulandıktan sonra diğer aracın kendi kaldırıcısıyla kaldırılması **önerilir** (kullanıcı kararı; AlterDNS kendiliğinden kaldırmaz).

**Bağımlılık:** Faz 1 ve Faz 0 araştırma sonuçları.

**Testler:** T-DPI-* (pcap fikstürleriyle saf fonksiyon testleri), T-TUN-*, T-NET-*, T-DNS-*, T-PERF-004…006, T-LAB-010…015 (onaylı gerçek ağ testleri), T-REC-001 (servis öldürülünce trafik normale dönüyor)

**Çıkış ölçütleri**
- [ ] Kullanıcının hotspot ağında Discord web/API ve Roblox web/API probları geçiyor.
- [ ] Gerçek istemci testleri yapıldı: Discord masaüstü giriş + metin, Discord ses (UDP fizibilitesine göre), Roblox oyuna katılım. Sonuç her biri için "çalışıyor / çalışmıyor + neden" olarak raporlandı.
- [ ] Ağ değişimi (Wi-Fi kapat/aç, farklı ağ, uyku) sonrası ≤ 15 sn içinde yeniden uyum sağlanıyor.
- [ ] Başarısız deneme sonrası journal ve sistem durumu başlangıçla aynı. Hop limit 65.
- [ ] Performans bütçeleri karşılanıyor: iş hacmi kaybı, gecikme, boşta CPU.
- [ ] `incident-thinker` ve `security-reviewer` bulguları kapandı.

---

## Faz 3 — Dosya güvenliği

**Çıktılar**
1. `scan-broker`: işçi başlatma (AppContainer profili, Job sınırları), handle aktarımı, zaman aşımı, çökme yakalama (çöken işçi = "incelenemedi" + hata kodu).
2. `scan-engine`: içerik tabanlı tür tespiti, açıcılar (bütçeli), analizörler (ARCHITECTURE §7.2), YARA-X entegrasyonu, bulgu modeli, karar motoru.
3. `av-bridge`:
   - WSC ile etkin AV tespiti.
   - Defender `MpCmdRun` (çıkış kodları ve `-DisableRemediation` davranışı doğrulanarak).
   - AMSI.
4. Hash itibarı: yerel önbellek, MalwareBazaar (kullanıcının Auth-Key'i veya proje anahtarı; D4'e bağlı), isteğe bağlı VirusTotal (kullanıcı anahtarı). Hepsi onaylı ve yalnız hash gönderir.
5. YARA kural beslemesi: kaynaklar ve lisanslar `THIRD_PARTY.md`'de; derlenmiş kural önbelleği; FP izleme listesi.
6. Rapor: JSON şeması v1, HTML dışa aktarma, "İncelenemeyenler" bölümü.
7. `quarantine`: şifreli depo, geri yükleme, kalıcı silme, kilitli dosya için servis aracılığıyla yeniden başlatmada silme, eylem sonrası doğrulama.
8. Tetikleyiciler:
   - Explorer sağ tık menüsü ("AlterDNS ile tara"; Win11 modern menü için gerekirse sparse package/IExplorerCommand; spike).
   - Sürükle-bırak.
   - İndirme klasörü izleyicisi (`ReadDirectoryChangesW`; geçici uzantıları `.crdownload`, `.part` bekler).
9. UI:
   - Flyout'ta son tarama ve "Dosya tara…".
   - Ana pencerede Tara, Raporlar ve Karantina sayfaları.
   - Riskli sonuçta toast bildirimi.
10. Deneysel: Windows Sandbox ile dinamik inceleme (ayrı bileşen, varsayılan kapalı).

**Bağımlılık:** Faz 1. Faz 4 ile paralel yürüyebilir.

**Testler:** T-SCN-*, T-QRN-*, T-SBX-*, T-FUZZ-001…010, T-PERF-007…008

**Çıkış ölçütleri**
- [ ] Zararsız derlemde (≥ 2.000 dosya; Windows ikilileri, imzalı uygulamalar, ofis/PDF/görsel/ses/video) `Riskli` = 0 ve `Şüpheli` ≤ %1. Her `Şüpheli` sonucun gerekçesi incelendi.
- [ ] Sentetik şüpheli derlemde beklenen bulguların %100'ü çıkıyor.
- [ ] VM'de EICAR (düz, zip içinde, 3 seviye iç içe) `Riskli` olarak çıkıyor.
- [ ] Gerçek zararlı derlemiyle (VM, ağ kapalı) tespit oranı ölçüldü ve raporlandı. Garanti verilmiyor.
- [ ] Bomba ve yol aşımı testlerinde sınırlar korunuyor; ana işlem etkilenmiyor.
- [ ] Karantina, silme ve doğrulama akışı uçtan uca test edildi. Junction/symlink saldırı testleri geçiyor.
- [ ] Fuzz hedefleri belirlenen sürede (her hedef ≥ 1 saat, CI'da kısa tur) çökme olmadan çalışıyor.

---

## Faz 4 — Tarayıcı koruması

**Çıktılar**
1. `reputation`: besleme tanımları, indirme, doğrulama, normalize, DB, sorgu, son iyi bilinen kopya, kaynak bazında atıf ve tarih.
2. `alterdns-nmh`: şema doğrulama, kaynak (origin) kontrolü, pipe köprüsü, HKCU kaydı (journal).
3. `extension/`:
   - MV3 TypeScript; Chrome ve Edge.
   - DNR senkronizasyonu, engelleme sayfası, "Yine de devam et" (oturumluk izin), "Hatalı engel bildir".
   - Uygulama bağlantısı yoksa çalışma kipi.
   - İsteğe bağlı `downloads` izni ile taramayı tetikleme.
   - Manifest `key` ile sabit kimlik; `ping`/`hello` kalp atışı; sürüm bildirimiyle `runtime.reload()`.
4. **Paketlenmemiş uzantı dağıtımı (ADR-011):**
   - Sabit yol (`%ProgramFiles%\AlterDNS\browser-extension\`), atomik güncelleme.
   - Tarayıcı tespiti; tarayıcı başına native host kaydı.
   - **"Tarayıcıda kur" sihirbazı:** tarayıcıyı başlatma, adres ve yolu kopyalama, Explorer'da gösterme, ekran görüntülü yönerge, bağlantı doğrulama.
   - Devre dışı kalma tespiti (`ADNS-WEB-005`).
   - **Spike:**
     - `chrome://extensions` / `edge://extensions` dışarıdan açılabiliyor mu?
     - Edge `--load-extension` davranışı
     - Geliştirici modu uyarısı ve tarayıcı güncellemesi sonrası devre dışı kalma davranışı
     - Paketlenmemiş uzantıda `runtime.reload()`
     - Brave, Opera ve Vivaldi native host kayıt yolları
5. **Spike: DNR kapasitesi.** Gerçek liste boyutlarında `requestDomains` paketleme, bellek kullanımı ve güncelleme süresi ölçülür. Sonuç ADR-006'ya yazılır.
6. `extension/INSTALL.md` (izin gerekçeleri, adım adım kurulum, sorun giderme) ve kök dizinde `PRIVACY.md` (gizlilik politikası; SignPath şartı: kurulumda gösterilir, veri gönderen işlevler kapatılabilir).
7. UI:
   - Flyout'ta bugün engellenen sayısı ve uzantı bağlantı durumu.
   - Ana pencerede Tarayıcı Koruması sayfası: kaynaklar ve tarihleri, engellenenler, izin listesi, uzantıyı kur/aç yönlendirmesi.

**Bağımlılık:** Faz 1. **Karar:** D4 (ticari olup olmadığı) bu fazın başında sorulur.

**Testler:** T-REP-*, T-NMH-*, T-EXT-* (Playwright E2E; engellenen adres için yerel test sunucusu 0 istek alır; paketlenmemiş kurulum akışı, sabit kimlik, güncelleme sonrası yeniden yükleme, devre dışı kalma tespiti), T-PERF-009

**Çıkış ölçütleri**
- [ ] DNR kapsamındaki adreslerde engelleme isteğe girmeden önce oluyor (0 istek).
- [ ] Engelleme sayfası kaynakları, tarihi ve nedeni doğru gösteriyor. Erişilebilirlik denetimi geçti.
- [ ] Uygulama kapalıyken uzantı son kurallarla çalışıyor ve uyarı gösteriyor.
- [ ] Besleme hatasında son iyi bilinen kopyaya dönülüyor ve hata kodu gösteriliyor.
- [ ] Chrome ve Edge'de "Tarayıcıda kur" sihirbazıyla kurulum, el ile test listesiyle baştan sona geçiyor. Uzantı kimliği sabit; native host yalnız bu kimliği kabul ediyor.
- [ ] Uygulama güncellemesinden sonra uzantı yeni sürüme geçiyor (otomatik veya yönergeyle).
- [ ] `INSTALL.md` ve `PRIVACY.md` eksiksiz; `modern-web-guidance` kontrol listesinin MV3 maddeleri tamam.

---

## Faz 5 — Kurulum, bileşen yöneticisi, güncelleme, kaldırma, yayın hattı

**Çıktılar**
1. `components` ve `maint`: manifest, bağımlılık çözümü, kur/kaldır txn'leri, `ModifyPath` ile "Değiştir" akışı.
2. `alterdns-setup.exe`:
   - Bileşen seçim ekranı: boyut, izinler, sistem değişiklikleri.
   - Yalnızca seçilenleri indirir; doğrular; kurar.
   - Kurulum öncesi onay ekranı.
3. `update`: Ed25519 manifest doğrulaması, anahtar devri, sürüm düşürme koruması, son geçerlilik, ETag, ölçülü bağlantı politikası, A/B, sağlık kontrolü, otomatik geri dönüş.
4. **Yayın hattı (ADR-012, ARCHITECTURE §8.1).** `release.yml`:
   - Deterministik derleme, SBOM, THIRD_PARTY_NOTICES, GPL kaynak arşivi.
   - SHA256SUMS ve `attest-build-provenance`.
   - Ed25519 manifest. Anahtarın yeri kararlaştırılır: çevrim dışı yerel veya `release` ortamı.
   - Korumalı ortam onayı, GitHub Release.
   - SignPath adımı hazır ama devre dışı.
5. **İlk imzasız ön sürüm (v0.x).**
   - Sürüm notlarında SmartScreen açıklaması ve `gh attestation verify` yönergesi bulunur.
   - SAC'nin kapatılması önerilmez.
   - SignPath'in "daha önce yayımlanmış olma" şartını karşılar.
6. Kaldırma: A (tam temizlik) ve B (raporları koru); kalıntı taraması; `maint`'in kendini silmesi.
7. Çevrim dışı tam kurulum paketi (isteğe bağlı).

**Bağımlılık:** Faz 1–4. (D1 verildi: public depo, GitHub Releases.)

**Testler:** T-INS-* (2³ bileşen kombinasyonu × kur/değiştir/kaldır A/B; VM anlık görüntü karşılaştırması), T-REL-*, T-UPD-* (geçerli, bozuk, imzasız, süresi dolmuş, eski sürüm, yarıda kesilen indirme, geçiş sırasında güç kaybı simülasyonu, sağlık kontrolü başarısız → geri dönüş)

**Çıkış ölçütleri**
- [ ] Bütün kombinasyonlarda kur → güncelle → geri dön → kaldır döngüsü VM'de geçiyor.
- [ ] Kaldırma A sonrası sistem anlık görüntüsü kurulum öncesiyle aynı (izin verilen sapmalar listelendi).
- [ ] Kaldırma B sonrasında yalnızca rapor dizini kalıyor.
- [ ] Kullanıcıya ait ayarlar (hop limit, DNS, başka uygulamaların WinDivert'i) her senaryoda değişmeden kalıyor.
- [ ] Ön sürüm GitHub Releases'ta. `gh attestation verify` her varlık için başarılı. Uygulama içi güncelleme public manifestten çalışıyor.

---

## Faz 6 — Sağlamlaştırma ve uyumluluk

**Çıktılar**
1. Uyumluluk matrisi (`TESTING.md` §6): Hyper-V VM'leri (Win10 22H2, Win11 23H2/24H2/25H2/26H2; HVCI açık/kapalı; Smart App Control açık/kapalı; Defender veya üçüncü taraf AV) ve en az bir başka fiziksel bilgisayar. Aynı adlı `WinDivert` servisi çakışması (T-CMP-001) GoodbyeDPI kurulu bir VM'de ölçülür.
2. Ağ matrisi: ev Wi-Fi/Ethernet, iPhone hotspot, Android hotspot (varsa), USB tethering, IPv6-only/NAT64 (simülasyon), captive portal (simülasyon).
3. Güvenlik incelemesi: bütün crate'ler `security-reviewer` ile incelenir. Sonuç `SECURITY.md` → "Açık bulgular".
4. Uzun süreli fuzz turları, performans regresyon tabanı, 72 saatlik kararlılık testi (bellek sızıntısı ≤ %5 artış).
5. Erişilebilirlik ve UX metin denetimi.
6. **SignPath Foundation başvurusu (ADR-012).**
   - README ve indirme sayfasında "Code signing policy" bölümü ve zorunlu ifade.
   - Roller: Yazar / İnceleyen / Onaylayan.
   - GitHub ve SignPath'te MFA.
   - Gizlilik politikası bağlantısı.
   - Sistem değişikliği uyarılarının ve kaldırmanın belgelenmesi.
   - Bağlantı modülünün amacı ve sınırları şeffaf biçimde anlatılır ("hacking tool" maddesi riski).
   - Sonuç D7'ye göre işlenir.

**Çıkış ölçütleri**
- [ ] Matrisin her hücresi için sonuç (geçti / bilinen sınırlama + kullanıcıya gösterilen hata kodu) kayıtlı.
- [ ] Yüksek ve kritik bulgu yok.
- [ ] 72 saat testinde bütçeler korunuyor.
- [ ] SignPath başvurusu yapıldı. Sonuç ve gereken değişiklikler kayıtlı.

---

## Faz 7 — İmzalı v1.0 yayını

**Çıktılar:**
- SignPath onaylıysa yayın hattında imzalama adımı etkinleştirilir; her sürüm elle onaylanır. Onaylı değilse D7'ye göre ilerlenir.
- Uygulama içi doğrulamaya Authenticode kontrolü eklenir (imzalı sürümlerden itibaren).
- Gizlilik politikası ve code signing policy yayımlanır.
- (İsteğe bağlı) Firefox için AMO "unlisted" imzalama.
- THIRD_PARTY_NOTICES dosyası (`cargo-about` + uzantı bağımlılıkları + WinDivert/ClamAV kaynak teklifleri).
- Sürüm notları.
- v1.0 etiketi.

**Çıkış ölçütü:** GitHub Releases'tan temiz bir VM'ye (Smart App Control açık ve kapalı) kurulum, güncelleme ve kaldırma döngüsü geçiyor. İmza, attestation ve manifest doğrulamaları başarılı. İmza yoksa sınırlamalar belgelenmiş.
