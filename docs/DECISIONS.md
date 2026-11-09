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

## ADR-003 — Paket motoru · Önerilen: kendi Rust motoru + resmi WinDivert 2.2.2-A

**Seçenekler:**
1. **Kendi motoru (Rust) + WinDivert:**
   - Tek işlem ve seçici filtreyle düşük CPU.
   - UDP/QUIC ve DNS aktarıcı aynı motorda.
   - Saf fonksiyonlu teknikler pcap ile test edilir.
   - GoodbyeDPI teknikleri (Apache-2.0) uyarlanabilir.
2. **GoodbyeDPI ikilisini sarmalamak:** Hızlı başlar. UDP yok, süreç denetimi kaba, ayrı bir ikili güvencesi gerekir.
3. **zapret winws'i sarmalamak:** En geniş teknik seti. Cygwin bağımlılığı var, lisans doğrulanmadı, iki ayrı motor mantığı oluşur.

**Karar (önerilen):** 1. Upstream ikililer yalnızca lab'da referans ölçüm için kullanılır (fark testi). Faz 0 ölçümleri kendi motorumuzla eşdeğerliği engelleyen bir durum gösterirse (ör. UDP tekniği), sarmalama seçeneği isteğe bağlı bir bileşen olarak yeniden değerlendirilir.

**Açık sorular (Faz 0 spike'ı):**
- 2.2.2-A sürücüsünün kendi servisiyle HVCI ve çekirdek yığın koruması altında yüklenmesi.
- Sürücü servisinin yaşam döngüsü: kurulma, handle'lar kapanınca boşalma, kaldırma.

**Faz 6'ya taşınan soru (2026-11-09):** Aynı adlı `WinDivert` servisi başka bir sürücü yoluna bağlıyken (ör. GoodbyeDPI kurulu sistem) AlterDNS'in davranışı. Geliştirici makinesindeki eski paket kaldırıldığı için bu soru Faz 6 uyumluluk matrisinde, GoodbyeDPI kurulu bir VM'de ölçülür.

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
| D7 | SignPath reddederse veya bağlantı modülünü kapsam dışı sayarsa | (a) İmzasız dağıtıma devam. (b) Bağlantı modülünü ayrı, imzasız bir proje olarak dağıtmak, çekirdeği imzalı tutmak. (c) Başka ücretsiz bir program çıkarsa değerlendirmek | Faz 6 | Kurulabilirlik (SmartScreen, SAC), mimari bölünme |
