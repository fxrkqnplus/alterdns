# Lab işletim kılavuzu (`xtask lab`)

Bağlantı araştırma düzeneğinin arıza durumları ve elle kurtarma adımları. Kurallar: CLAUDE.md
"Değişmez kurallar" 1, 2 ve 5. Sistem hop limiti **hiçbir adımda yazılmaz** (ADR-010).

## 1. Nasıl çalıştırılır

```powershell
cargo build -p xtask --target-dir target/xtask
target\xtask\debug\xtask.exe lab preflight
```

- `lab preflight` salt okunurdur. Şunları gösterir: yönetici yetkisi, `AlterDNS-Spike` adaptörü,
  bekçinin iş nesnesi durumu, bayat journal, lab dışı DPI aracı (`goodbyedpi.exe`, `winws.exe`),
  WinDivert sürücüsü ve kontrol probu.
- **Bekçi ve iş nesnesi:** `cargo xtask ...` altında cargo, alt işlemleri "kapanınca öldür"
  (kill-on-close) iş nesnesine koyar ve ayrılmaya izin vermez. Cargo kapanınca bekçi de ölebilir.
  - Sistem değiştiren senaryolar (Görev 7 Wintun, Görev 9) bu durumda **başlamaz**.
  - Bunları ve Ctrl+C denemesini, derlenmiş ikiliyle (`target\xtask\debug\xtask.exe lab ...`)
    normal bir terminalden çalıştırın.
  - `lab preflight` çıktısında "Bekçi: iş nesnesi dışında" yazmalıdır.
- S1 (`lab baseline`) ve S2-U (`lab techniques`) sistem değiştirmez. Bu yüzden `cargo xtask` altında
  da çalışırlar; yalnızca bir uyarı basılır.
- S5-soket (`lab ttl`) da sistem değiştirmez: yalnızca probun kendi soketinin taban `IP_TTL`'i
  64 / 65 / 128 olur. Sistem hop limiti hiçbir senaryoda yazılmaz (ADR-010, ADR-014).
  - Telefon hotspot'unda TTL 64 ve 128 ile giden prob paketleri operatöre paylaşım trafiği gibi
    görünebilir; onay ekranı bunu belirtir.
- `--only <önek>` yalnızca adı o önekle başlayan hedefleri ölçer; kontrol probları her zaman
  kalır. Örnek: `lab baseline --only wattpad`, `lab techniques --only wattpad`.
- **Korunan değerler (ADR-014):** Her senaryo başında ve sonunda kullanıcıya ait değerlerin
  anlık görüntüsü alınır (`lab/scripts/korunan-degerler.ps1`, salt okunur): hop limit, bağlı
  arayüzlerin MTU'su ve otomatik metrik ayarı, varsayılan rotalar, DNS, DoH, NRPT, WinINET ve
  WinHTTP proxy'si, güvenlik duvarı profilleri.
  - Sonuç `lab/results/<koşu>/korunan-degerler.json` dosyasına yazılır.
  - Fark varsa koşu başarısız sayılır. AlterDNS bu değerlere yazmaz ve düzeltmeye çalışmaz;
    farkı başka bir program veya ağ (ör. DHCP) yapmış olabilir.

## 2. Bayat journal: `lab recover`

**Belirti:** "bitmemiş journal var" hatası, ya da `lab preflight` "eski journal" gerekçesiyle durur.

**Neden:** Önceki koşu çökmüştür, bekçi geri alamamıştır, ya da çözülmemiş bir çakışma vardır.

**Ne yapılır:** `target\xtask\debug\xtask.exe lab recover`

Her journal dosyası ayrı ele alınır:
- **Geri alınan kayıtlar** kapanır.
- **Geri alınamayan kayıt** (okuma veya yazma hatası) açık kalır. Nedeni giderin (çoğunlukla
  yönetici terminali gerekir) ve komutu yeniden çalıştırın.
- **Çakışma:** Değer ne öncekine ne lab'ın yazdığına eşittir, ya da öğe lab'a ait değildir.
  Lab bu öğeye dokunmaz.
  - Kayıt ayrıntısı gösterilir.
  - `evet` yazarsanız yalnızca journal kaydı kapanır; sistem değerine dokunulmaz.
  - Onay verilmezse yeni koşu başlamaz.

## 3. Bozuk journal

Journal dosyaları `lab/results/journal/<koşu>.jsonl` yolundadır (git dışı). Her satır bir durum
değişimidir; aynı kimliğin son satırı geçerlidir.

- **Yarım kalmış son satır** (satır sonu yok) otomatik olarak yok sayılır, ilk yazmada kırpılır.
  Yazma-önce kuralı gereği o satırın anlattığı sistem değişikliği başlamamıştır.
- **Ara satır bozuksa** hata iletisi dosya yolunu ve satır numarasını verir. Böyle bir dosya, güvenli
  tarafta kalmak için bayat sayılır.
  1. Dosyanın bir kopyasını alın.
  2. Bozuk satırı inceleyin. Satır sonlu ama geçersizse elle düzeltin ya da silin.
  3. Silmeden önce o kimliğin önceki satırlarındaki `before` değerini not edin.
  4. `lab recover` çalıştırın.
- **Değer doğrulaması:** Arayüz adı, önek, adres ve dosya yolu izin listesi dışındaysa dosya
  reddedilir. Bunu elle "düzeltmeyin"; böyle bir dosyayı lab yazmamıştır, kaynağını araştırın.

## 4. Bekçi raporu: `lab/results/<koşu>/watchdog-report.json`

| Alan | Anlamı |
|---|---|
| `outcome: CleanExit` | Ebeveyn `DONE` gönderdi. Bekçi yine de motorları ve açık kayıtları denetledi. |
| `outcome: ParentGone` | Ebeveyn öldü veya Ctrl+C. Motorlar durduruldu, journal geri alındı. |
| `outcome: DeadlineExpired` | Kira (≤ 60 sn) yenilenmedi. Ebeveyn takılmış olabilir; koşu "bekçiye ulaşılamadı" ile durur. |
| `engines[].result` | `taskkill` sonucu. "PID başka bir işleme geçmiş" ise o işleme dokunulmamıştır. |
| `revert.reverted` / `conflicts` / `failed` | Geri alınan, çakışan ve geri alınamayan kayıtlar. `failed` varsa bölüm 2'yi uygulayın. |
| `revert.error` | Journal açılamadı: bölüm 3. |

Rapor hiç yoksa bekçi büyük olasılıkla iş nesnesiyle birlikte ölmüştür (bölüm 1). `lab recover`
çalıştırın.

## 5. Elle ağ geri yükleme (son çare)

Önce `lab recover` deneyin. Elle müdahale yalnızca lab'ın oluşturduğu öğeler içindir; başka bir
adaptöre, rotaya, DNS ayarına veya DPI aracına dokunmayın.

1. **Durumu okuyun** (salt okunur):
   ```powershell
   Get-NetAdapter -Name 'AlterDNS-Spike' -ErrorAction SilentlyContinue
   Get-NetRoute -InterfaceAlias 'AlterDNS-Spike' -ErrorAction SilentlyContinue
   Get-NetIPAddress -InterfaceAlias 'AlterDNS-Spike' -ErrorAction SilentlyContinue
   Get-NetIPv4Protocol | Select-Object DefaultHopLimit
   Get-NetIPv6Protocol | Select-Object DefaultHopLimit
   ```
2. **Rota** (yönetici). Lab rotaları yalnızca `ActiveStore`'dadır, yeniden başlatmada da kalkar:
   ```powershell
   Remove-NetRoute -InterfaceAlias 'AlterDNS-Spike' -PolicyStore ActiveStore -Confirm:$false
   ```
3. **Adaptör:** Aygıt Yöneticisi → Ağ bağdaştırıcıları → `AlterDNS-Spike` → Cihazı kaldır.
   Ya da (yönetici) PnP kimliğini okuyup kaldırın; kimlik `SWD\WINTUN\{…}` biçiminde olmalı:
   ```powershell
   (Get-NetAdapter -Name 'AlterDNS-Spike').PnPDeviceID
   pnputil /remove-device '<yukarıdaki kimlik>'
   ```
   Adaptör `Get-NetAdapter`'da görünmüyor ama yeni adaptör oluşturulamıyorsa gizli bir kalıntı
   olabilir: `pnputil /enum-devices /class Net`. Adaptör kalkınca `lab clean-bin` çalışabilir.
   **Wintun sürücü paketini (`pnputil /delete-driver`) silmeyin:** bu makinede paket başka bir
   yazılıma aittir; lab ona hiç dokunmaz.
4. **Hop limit:** Yalnızca okunur. Değer S0 envanterindeki başlangıç değerinden farklıysa lab onu
   değiştirmemiştir. Kullanıcıya bildirin; geri yazmayın.
5. Bağlantıyı doğrulayın: `target\xtask\debug\xtask.exe lab preflight`. Kontrol probu geçmelidir.

## 6. Wintun spike'ı (Görev 7)

Komutlar yönetici terminalinde, derlenmiş ikililerle çalıştırılır (`cargo run` ile değil):

```powershell
cargo build -p xtask --target-dir target/xtask
cargo build --release --manifest-path spikes/wintun/Cargo.toml
target\xtask\debug\xtask.exe lab preflight
spikes\wintun\target\release\wintun-spike.exe status
spikes\wintun\target\release\wintun-spike.exe kill-test
spikes\wintun\target\release\wintun-spike.exe run
spikes\wintun\target\release\wintun-spike.exe status
```

- **Sıra:** önce `kill-test`. Bu komut, kurtarma yolunu (bekçinin geri alması) gerçek
  adaptörle sınar; ancak o geçerse `run` çalıştırılır.
- **Ön koşullar:**
  - `lab preflight` "Bekçi: iş nesnesi dışında" demeli.
  - `status` "Yükseltilmiş çalıştırma ön koşulları: tamam" demeli.
  - `xtask.exe` kaynaklardan eskiyse spike çalışmaz; yeniden derleyin.
- **Ctrl+C güvenlidir:**
  - `evet` sorusunda ve doğrudan ölçümde henüz hiçbir şey değişmemiştir.
  - Tünel açıkken Ctrl+C'de spike işlemi ölür. Ayrı işlemdeki bekçi stdin kapanmasını görür,
    rotayı, adresi ve kalakaldıysa adaptörü geri alır; raporu `ParentGone`dur.
  - Sonra yaklaşık 10 sn bekleyip `wintun-spike status` çalıştırın. Bir şey kaldıysa
    `xtask lab recover`, o da yetmezse bölüm 5.
- **Korunan değerler (ADR-014):** `run` ve `kill-test` önce ve sonra kullanıcıya ait değerleri
  okur (bölüm 1). Fark varsa komut başarısız sayılır; değerlere dokunulmaz.
- **S4 rota planı:** `wintun-spike s4-plan` salt okunurdur; yönetici gerekmez. Kaynak:
  `data/s4/hedefler.toml`. Gösterdikleri:
  - sağlayıcının kendi önekleri (yalnızca Roblox AS22697);
  - alan adlarından çözülen tek IP'ler;
  - DNS önbelleğinden öğrenilen tek IP'ler.
  Paylaşımlı CDN'lerde (Discord: Cloudflare, Wattpad: CloudFront) önek kullanılmaz.
- **S4 gerçek istemciler (`hold`, Görev 9 K10):** yönetici terminalinde, `kill-test` geçtikten sonra:

  ```powershell
  spikes\wintun\target\release\wintun-spike.exe s4-plan
  spikes\wintun\target\release\wintun-spike.exe hold
  ```

  - Başlamadan önce Discord, Roblox ve Wattpad sekmelerini kapatın: açık bağlantılar rota
    eklenince kopar.
  - Seçenekler: `--tech tls-record-sni` (varsayılan) | `oob-sni` | `split-1`; `--max <sn>` (≤ 600).
  - Tünel hazır olunca uygulamaları deneyin: Discord masaüstü (giriş, metin, sesli kanal),
    Roblox (oyuna katılım), Wattpad (hikâye açma). Bitirince Enter.
  - Kontrol probu 20 sn'de bir geçmezse oturum kendiliğinden biter. DNS önbelleğinden 5 sn'de
    bir yeni tek IP'ler öğrenilir (en çok 128); Discord sesi ilk denemede bağlanmazsa birkaç
    saniye sonra yeniden deneyin.
  - Güvenlik: DNS sunucuları, ağ geçidi ve başlangıçta açık bağlantıların uzak adresleri (ör.
    bu oturum) hiçbir zaman TUN'a yönlendirilmez; öğrenilen adresler de bu denetimden geçer.
  - Motor durursa (döngü, hata) oturum hemen biter; kira beklenmez.
  - Söküm sırasında uygulamaları kapatın; rota sayısına göre söküm bir iki dakika sürebilir.
  - Sonda tünel sökülür ve kısa sorular sorulur (e / h / boş). Sonuç: `lab/results/wintun-hold-*/hold.json`.
- **Sonuçlar** `lab/results/wintun-*/` altındadır (git dışı):
  - `wintun-run.json`: başlangıç verisi baştan yazılır.
  - `kill-test.json`.
  - `watchdog-report.json`.
- **Gizlilik:** Çıktıda test IP'si ve yerel yollar görünür; paylaşmadan önce ayıklayın.

## 7. S6 — başka bir ağda tekrar (Görev 9 K12)

Amaç H2: aynı ölçümler başka bir ağda (ör. ev Wi-Fi'si veya Ethernet) farklı sonuç veriyor mu.
Önce bilgisayarı o ağa bağlayın. Hotspot ve başka VPN kapalı olmalı.

```powershell
cargo build -p xtask --target-dir target/xtask
target\xtask\debug\xtask.exe lab preflight
cargo xtask lab baseline
cargo xtask lab techniques
cargo xtask lab ttl
```

- **Tahmini veri:** S1 ≈ 1,0 MB; S2-U ≈ 14,3 MB; S5-soket ≈ 2,6 MB. Her komut başlamadan önce tahmini gösterir ve `evet` bekler.
- **Sistem değişikliği:** Yok. Korunan değerler başta ve sonda denetlenir.
- **S4 (isteğe bağlı, yönetici terminali):** Uygulamaları kapatıp başlatın, "Tünel hazır" yazınca deneyin.

  ```powershell
  spikes\wintun\target\release\wintun-spike.exe s4-plan
  spikes\wintun\target\release\wintun-spike.exe hold --tech oob-sni --max 300
  ```

- Sonuçlar `lab/results/` altındadır. Yerel özet `CLAUDE.local.md`'ye, genelleştirilmiş özet `CONNECTIVITY_RESEARCH.md` §7'ye yazılır; ağ adresi yazılmaz.
