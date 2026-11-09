# AlterDNS — Mimari Kararlar (ADR) ve Açık Kararlar

Durumlar: **Kabul** · **Önerilen (spike bekliyor)** · **Reddedildi** · **Yerine geçti**. Her ADR değiştiğinde tarih ve gerekçe eklenir.

---

## ADR-001 — Programlama dili: Rust · Kabul (2026-11-09)

**Bağlam:** Güvenilmeyen dosyaları ayrıştırma, çekirdek sürücüsüyle paket işleme, SYSTEM yetkili servis, asgari RAM/CPU hedefi, Windows öncelikli.

| Seçenek | Artı | Eksi |
|---|---|---|
| **Rust** | Bellek güvenliği (ayrıştırıcı ve paket kodu); GC yok, düşük ve öngörülebilir RAM; `windows` crate'i (Microsoft); YARA-X yerel Rust; tek ikili; `cargo-deny`; Linux'ta fuzz | Win32 UI ekosistemi C#'tan zayıf; derleme süreleri |
| C++ | GoodbyeDPI ve zapret ile aynı dil; Win32 doğrudan | Bellek güvenliği yok; güvenilmeyen girdi işleyen bir güvenlik ürünü için yüksek risk |
| C# / .NET 8+ (WinUI 3) | Hızlı UI geliştirme, Fluent görünüm | Çalışma zamanı + JIT; boşta RAM onlarca MB; sürücü ve FFI işleri yine P/Invoke ister |
| Go | Basit eşzamanlılık | GC; Win32 GUI zayıf; daha büyük ikili |

**Karar:** Rust. Uzantı TypeScript (zorunlu). Kurulum betikleri de Rust (`xtask`, `setup`).

---

## ADR-002 — UI çatısı · Önerilen: Slint (Faz 0 spike'ı bekliyor)

**Seçenekler:**
1. **Slint (GPL-3.0 seçeneği):** Bildirimsel; yerel çizim (yazılım, femtovg veya skia çizici); animasyon ve tema desteği. D2 ve SignPath şartları nedeniyle yalnızca GPL-3.0 seçeneğiyle kullanılır; Royalty-free lisans kapalı bir lisans olduğu için uygun değil.
2. **Win32 + Direct2D/DirectWrite (elle):** En küçük ayak izi ve tam pencere kontrolü; en fazla emek.
3. **egui / iced:** Saf Rust ve MIT. egui anlık kip (immediate mode); iced'in pencere/tepsi entegrasyonu ve bellek davranışı ölçülmeli.
4. **Tauri 2 (WebView2):** Hızlı geliştirme; WebView2 işlemleri nedeniyle RAM bütçesini aşması beklenir. **Varsayılan değil**; spike ölçümü bunun yanlış olduğunu gösterirse yeniden değerlendirilir.

**Spike ölçütleri** (`CLAUDE_CODE_PROMPT.md` §14):
- Tepsi ajanı boşta RAM (UI yüklü / yüklü değil)
- Flyout açılış süresi (p95 sıcak, soğuk)
- Kapandıktan sonra bellek iadesi
- Odak kaybında kapanma, sürüklenemezlik
- DPI ve çoklu monitör davranışı
- Win11 malzeme ve köşe desteği
- Erişilebilirlik (UIA)
- Türkçe karakter ve yazı tipi çizimi
- İkili boyutu

**Karar kuralı:** Bütçeleri karşılayan en düşük emekli seçenek. Slint seçilirse GPL-3.0 koşulları uygulanır; Hakkında ekranında Slint'e teşekkür edilir.

---

## ADR-003 — Bağlantı motoru · Yeniden değerlendiriliyor (D8, 2026-11-09)

**Geçmiş:**
- İlk öneri: kendi Rust motoru + resmi WinDivert 2.2.2-A.
- 2026-11-09: Microsoft Defender, resmi sürüm sayfasından indirilen `WinDivert-2.2.2-A.zip` arşivini `Misleading:Win32/Malgent!AMTB` tespitiyle engelledi ve kaldırdı. Varsayılan Defender'lı kullanıcı sistemlerinde kurulumun engellenmesi riski doğdu. AV kapatılmadı, istisna eklenmedi (CLAUDE.md kural 4). Kullanıcı D8'i açtı; karar Faz 0 ölçümlerinden sonra kesinleşecek.

**Seçenekler (D8):**
1. **Aday — Wintun TUN adaptörü + kullanıcı modunda kendi motorumuz (Rust):**
   - Yalnızca engelli servislerin IP'leri adaptöre yönlendirilir; diğer trafik doğrudan gider.
   - TCP bağlantıları motorda sonlandırılır ve kendi soketlerimizle yeniden kurulur. Teknikler: ClientHello bölme, TLS kaydı bölme, `IP_TTL` ile ters sıra, sahte gönderim, OOB.
   - UDP datagramları kendi UDP soketlerimizle iletilir; gerekirse düşük TTL'li gürültü datagramı eklenir.
   - Sistem hop limitine dokunulmaz (ADR-010); TTL yalnızca motorun kendi soketlerinde ayarlanır.
   - Açık sorular: Wintun lisansı ve SignPath uyumu (`THIRD_PARTY.md` §1.1), Defender'ın `wintun.dll`'e tepkisi, rota yönetimi ve geri alma, servis çökünce rotaların bırakılması (T-REC-001), motorun kendi soketlerinin TUN'a geri dönmemesi (`IP_UNICAST_IF`), CPU/RAM/gecikme, kullanıcı modu TCP/IP yığını seçimi.
2. **Yedek mod — aynı motor, yerel aktarıcı (PAC):** Yönetici yetkisi gerektirmez; tarayıcı ve PAC'a uyan uygulamalar için. Sistem dışı uygulamaları (ör. oyun istemcisi) kapsamayabilir; sınırlama belgelenir.
3. **Askıda — WinDivert + kendi motor:** Ürün motoru olarak kullanılmaz. Defender durumu değişirse ileride isteğe bağlı "gelişmiş mod" adayı. Aynı adlı `WinDivert` servisi çakışması (T-CMP-001) bu mod geri gelirse Faz 6'da ölçülür.

**Elenenler (2026-11-09, kullanıcı kararı):**
- **Kendi çekirdek sürücümüz** (doğrulandı, 2026-11-09)**:** Yayın için Microsoft imzası ve EV kod imzalama sertifikası gerekir; EV sertifikası ücretlidir ve şirket adına alınır. D3 (yalnız ücretsiz yollar) ile çelişir. Kaynaklar:
  - Microsoft, *Driver code signing requirements* (learn.microsoft.com/windows-hardware/drivers/dashboard/code-signing-reqs): attestation imzası için ikili göndermek üzere Hardware Dev Center hesabında en az bir EV sertifikası kayıtlı olmalı; aynı şart WHCP sertifikasyonu için de geçerli.
  - Microsoft, *Attestation sign Windows drivers* (learn.microsoft.com/windows-hardware/drivers/dashboard/code-signing-attestation): attestation imzası EV kod imzalama sertifikası gerektirir.
  - "Şirket adına" kısmı Microsoft belgesinde geçmez. İkincil kaynak: OSR topluluk forumu yorumu (community.osr.com/discussion/comment/293800/, 2019-05-03): herhangi bir imza türü için şirketiniz adına EV sertifikası alınıp panele yüklenmeli.
- **Npcap** (lisans dosyasından doğrulandı)**:** Lisans dosyası yazılımın açık kaynak olmadığını, yeniden dağıtılamayacağını ve ücretsiz sürümün sınırlı sayıda sisteme kurulabildiğini söylüyor; dağıtım ücretli OEM lisansı ister. Ayrıca yakalama/enjeksiyon modelidir; özgün paketi durduramaz.
- **Windows Packet Filter (WinpkFilter)** (README ve lisans sayfasından doğrulandı)**:** Kullanıcı modu kütüphanesi (`ndisapi`) MIT, ama sürücü ayrı lisanslı: ücretsiz katmanda ticari yeniden dağıtım hakkı ve sürücü kaynak kodu yok; dağıtım için ücretli ikili lisans, kaynak için ayrı ücretli lisans gerekir. Kapalı bileşen yasağıyla (SignPath) ve D3 ile çelişir.
- **GoodbyeDPI veya zapret winws'i sarmalamak:** WinDivert'e bağlıdır; WinDivert askıda olduğu için düşer.

**Wintun lisans önlemleri (2026-11-09, kullanıcı kararı; ayrıntı `THIRD_PARTY.md` §1.1):**
1. **GPL-3.0 §7 ek izni:** AlterDNS kodunun değiştirilmemiş, Wintun projesinin dağıttığı imzalı `wintun.dll` ile birleştirilmesine izin veren ek izin `LICENSE`'a eklenir (metin kullanıcı onayından sonra). Ek izin yalnızca telif sahibinin kodunu kapsar.
2. **Yalnızca `alterdns-svc` yükler:** `wintun.dll`'i başka hiçbir işlem yüklemez. `alterdns-svc` bağımlılık ağacında GPL-3.0-only kod (ör. Slint) bulunamaz; `deny-svc.toml` ve `cargo xtask check` bunu crate düzeyinde zorunlu kılar.
3. **Yedek — kurulumda indirme:** DLL'i pakete koymayıp bileşen yöneticisiyle wintun.net'ten sabit hash'le indirmek. **Yalnızca SignPath DLL'in pakette bulunmasına itiraz ederse** uygulanır.
4. **SignPath ön sorusu:** Faz 6'yı beklemeden SignPath Foundation'a Wintun DLL'i ve bağlantı modülü ("hacking tool" maddesi) hakkında ön soru gönderilir; yanıt D7 ve D8'e işlenir.

**Ölçüm planı (Faz 0):** Sürücüsüz soket teknikleri (S2-U) ve Wintun spike'ı (`docs/superpowers/plans/2026-11-09-faz-0.md` Görev 7–8). Karar kuralı: hotspot ağında Discord ve Roblox web/API problarını geçen, bütçeleri karşılayan, lisans ve SignPath şartlarına uyan en az müdahaleci seçenek.

---

## ADR-004 — İşlem modeli · Kabul

Servis (LocalSystem) + tepsi ajanı (kullanıcı) + kısa ömürlü UI ve tarama işçisi + native host. **Gerekçe:** en az ayrıcalık, boşta düşük RAM, güvenilmeyen içeriğin izolasyonu. Ayrıntı: `ARCHITECTURE.md` §2.

---

## ADR-005 — Kurulum ve bileşen yönetimi · Önerilen: kendi Rust önyükleyicisi + bileşen yöneticisi

| Seçenek | Artı | Eksi |
|---|---|---|
| **Kendi önyükleyicisi + `maint` motoru** | Kurulum, sonradan ekleme/kaldırma ve güncelleme **tek motor** ve tek journal; yalnız seçilen bileşen indirilir; A/B ve geri dönüş tam kontrol altında | Windows Installer'ın hazır geri alma ve onarım özellikleri yeniden yazılmalı; kapsamlı test gerekir |
| WiX v5/v6 (MSI + Burn) | Olgun; kurumsal dağıtım; MSI işlem geri alması | Bileşen başına ayrı MSI ve Burn karmaşıklığı; uygulama içi bileşen yöneticisi yine gerekli; A/B güncelleme ile uyum zor |
| Inno Setup | Basit; indirme sayfası desteği | Sonradan bileşen ekleme ve güncelleme için yine kendi motorumuz gerekir |
| MSIX | Temiz kaldırma | Çekirdek sürücüsü ve sistem servisi kısıtları; native host yürütme sorunları |

**Karar (önerilen):** Kendi motoru. Kurumsal dağıtım ihtiyacı doğarsa MSI sarmalayıcısı ileride eklenebilir.

---

## ADR-006 — Tarayıcıda engelleme yöntemi · Önerilen (Faz 4 spike'ı)

DNR dinamik kurallarıyla `main_frame` yönlendirmesi yapılır (istek öncesi engelleme). URL düzeyindeki ve kapasite dışı girdiler için `webNavigation` + native host + `tabs.update` yedeği kullanılır; bu yolun sınırlaması belgelenir. **Spike:** gerçek liste boyutlarında `requestDomains` paketleme sınırı, bellek ve güncelleme süresi.

---

## ADR-007 — Güncelleme bütünlüğü · Kabul

Ed25519 imzalı manifest (açık anahtar uygulamaya derlenir, `next_key` ile devir) + bileşen başına SHA-256 + sürüm düşürme ve son geçerlilik koruması + HTTPS + Authenticode (D3'e bağlı). İstemciye GitHub token'ı gömülmez.

---

## ADR-008 — DNS yaklaşımı · Önerilen

Önce **motor içi, alan adına sınırlı DoH aktarıcısı** denenir: sistem ayarı değişmez, motor durunca kendiliğinden kapanır. Gerekirse journal'lı ve onaylı **sistem DoH** modu kullanılır. GoodbyeDPI'ın şifresiz standart dışı port yönlendirmesi varsayılan değildir (H3 sonucuna göre yeniden değerlendirilir).

---

## ADR-009 — Tarama izolasyonu · Kabul

Varsayılan: AppContainer (yetenek yok) + Low IL + Job Object + handle aktarımı. Windows Sandbox ile dinamik inceleme deneysel ve isteğe bağlı bir bileşendir (yalnızca Pro ve üzeri).

---

## ADR-010 — Hop limit politikası · Kabul

AlterDNS hop limit değerini **okur, asla yazmaz**. Yalnızca lab modunda, kullanıcının o oturumdaki açık onayıyla, bekçili ve ≤ 10 dk süren karşılaştırma yapılabilir (D5). Kaldırma sırasında kullanıcıya ait bu değere dokunulmaz.

---

## ADR-011 — Tarayıcı uzantısı dağıtımı: mağaza yok, geliştirici modu · Kabul (2026-11-09, kullanıcı kararı)

**Karar:**
- Uzantı uygulamayla birlikte `%ProgramFiles%\AlterDNS\browser-extension\` altında sabit bir yolda gelir (A/B sürüm dizinlerinin dışında; yalnız yönetici yazabilir).
- Kullanıcı tarayıcının geliştirici modundan "Paketlenmemiş öğe yükle" ile ekler.
- Uygulamada **"Tarayıcıda kur"** düğmesi bulunur: tarayıcıyı başlatır, uzantılar sayfası adresini ve klasör yolunu kopyalar, adım adım yönerge gösterir, native host bağlantısını doğrular.

**Gerekçe:** Kullanıcı kararı. Mağaza incelemesine ve hesabına bağımlılık yok.

**Sonuçlar ve önlemler:**
- Manifest `key` alanı ile sabit kimlik sağlanır; native host `allowed_origins` bu kimliğe bağlıdır.
- Otomatik kurulum mümkün değil: Chrome markalı sürüm `--load-extension` bayrağını reddediyor, kurumsal ilke yönetilmeyen cihazda mağaza dışı uzantıyı kuramıyor.
- Tarayıcı geliştirici modu uyarısı gösterebilir veya uzantıyı devre dışı bırakabilir. Önlem: native host kalp atışıyla izleme, `ADNS-WEB-005` bildirimi ve yeniden etkinleştirme yönergesi. Uyarıyı gizleyen yama veya ilke **önerilmez**.
- Uzantı güncellemesi: dosyalar sabit yolda değiştirilir, native host uzantıya haber verir, uzantı `runtime.reload()` çağırır (doğrulanacak).
- Mağaza belgesi (`CHROMEWEBSTORE.md`) gerekmez. `extension/INSTALL.md` ve `PRIVACY.md` (SignPath şartı) tutulur.
- Firefox ileride AMO "unlisted" imzalamayla eklenebilir (ücretsiz; doğrulanacak).

---

## ADR-012 — Güvenilir derleme ve imzalama: yalnız ücretsiz yollar · Kabul (2026-11-09, kullanıcı kararı)

**Katmanlar:**
1. **Yeniden üretilebilir derleme (GitHub Actions, public depo):**
   - Sabitlenmiş araç zinciri, `--locked`, deterministik bağlayıcı ve yol bayrakları.
   - SBOM (CycloneDX) ve THIRD_PARTY_NOTICES.
   - SHA ile sabitlenmiş action'lar, en az yetkili token.
   - Korumalı `release` ortamı (gerekli onaylayıcı: kullanıcı).
2. **Köken kanıtı:** `actions/attest-build-provenance` + SBOM attestation (Sigstore Public Good). Kullanıcı `gh attestation verify` ile doğrular. Ücretsizdir (public depo).
3. **Uygulama içi bütünlük:** Ed25519 imzalı `release-manifest.json` + SHA-256. Anahtarın yeri (çevrim dışı yerel veya `release` ortamı gizli değişkeni) Faz 5'te belirlenir.
4. **Authenticode:** SignPath Foundation (OSS) başvurusu.
   - İlk imzasız ön sürüm Faz 5 sonunda yayımlanır; başvuru Faz 6'da yapılır.
   - Onaydan sonra her sürüm elle onaylanarak imzalanır.
   - Yayıncı "SignPath Foundation" görünür. Dosya bilgilerindeki telif sahibi Furkan Okan kalır.

**Elenenler:**
- Microsoft Store imzası: yalnız MSIX; sürücü ve servis mimarisine uymuyor.
- Ücretli OV/EV sertifika ve Azure Artifact Signing: ücretli; bireysel kapsam ABD/Kanada ile sınırlı.
- Kendinden imzalı sertifika: güven sağlamaz. Kullanıcıya kök sertifika kurdurmak güvenlik açısından kabul edilemez.

**Riskler:**
- SignPath'in "hacking tool" maddesi bağlantı modülünü kapsam dışı sayabilir (D7).
- Onaya kadar SmartScreen uyarısı devam eder ve Smart App Control açık sistemlerde kurulum engellenir.

---

## ADR-013 — Public depoda kişisel veri · Kabul (2026-11-09)

- Geliştirici makinesine özgü ortam bilgisi, ağ adresleri, log alıntıları, kişisel paket ayrıntıları ve hash kayıtları `CLAUDE.local.md` dosyasında tutulur (git dışı).
- Lab sonuçları `lab/results/` altında kalır (git dışı).
- Public belgeler yalnız genelleştirilmiş bulguları içerir.
- `xtask check` gizli bilgi ve kişisel veri kalıplarını tarar (token, anahtar, özel ağ adresi, kullanıcı yolu).

---

## Kullanıcı kararları

### Verilenler

| # | Karar | Sonuç | Tarih |
|---|---|---|---|
| D1 | Güncelleme ve dağıtım kanalı | Ana depo `fxrkqnplus/alterdns` **public**; GitHub Releases | 2026-11-09 |
| D2 | Proje lisansı | **GPL-3.0-or-later**. Ad ve logo lisans dışı (`TRADEMARK.md`) | 2026-11-09 |
| D3 | Kod imzalama | **Yalnız ücretsiz yollar:** SignPath Foundation başvurusu + Artifact Attestations + Ed25519 (ADR-012) | 2026-11-09 |
| — | Uzantı dağıtımı | **Mağaza yok**; geliştirici modu + "Tarayıcıda kur" düğmesi (ADR-011) | 2026-11-09 |
| D6 | Makinedeki eski DPI paketi | **Kapandı:** kullanıcı paketi Faz 0 başında tamamen kaldırdı (servis, görev, sürücü, program klasörü). Aynı adlı servis çakışması Faz 6 matrisinde ölçülür | 2026-11-09 |

### Açık kararlar

| # | Karar | Seçenekler | Ne zaman | Etkisi |
|---|---|---|---|---|
| D4 | Ticari mi, ticari olmayan mı | (a) Ücretsiz / ticari olmayan. (b) Ticari | Faz 4 başı | Safe Browsing ↔ Web Risk, abuse.ch aboneliği, VirusTotal koşulları. SignPath ticari ikili lisansı zaten yasaklıyor |
| D5 | Lab'da TTL 128 karşılaştırması | (a) Evet, kısa ve bekçili. (b) Hayır | Faz 0 | H7 testi; operatörün tethering tespiti riski |
| D8 | Bağlantı motoru teknolojisi (ADR-003) | (a) Wintun TUN + kullanıcı modu motor, yedek olarak yerel aktarıcı (PAC). (b) Yalnız yerel aktarıcı. (c) WinDivert'e dönüş (Defender durumu değişirse). Elenenler ADR-003'te | Faz 0 sonu (S2-U ve Wintun spike ölçümlerinden sonra) | Kurulabilirlik (Defender), lisans ve SignPath uyumu (Wintun ön derlenmiş ikili lisansı), UDP/oyun kapsamı, yönetici yetkisi |
| D7 | SignPath reddederse veya bağlantı modülünü kapsam dışı sayarsa | (a) İmzasız dağıtıma devam. (b) Bağlantı modülünü ayrı, imzasız bir proje olarak dağıtmak, çekirdeği imzalı tutmak. (c) Başka ücretsiz bir program çıkarsa değerlendirmek | Faz 6 | Kurulabilirlik (SmartScreen, SAC), mimari bölünme |
