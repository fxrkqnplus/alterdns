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
   Adaptör kalkınca `lab clean-bin` çalışabilir.
4. **Hop limit:** Yalnızca okunur. Değer S0 envanterindeki başlangıç değerinden farklıysa lab onu
   değiştirmemiştir. Kullanıcıya bildirin; geri yazmayın.
5. Bağlantıyı doğrulayın: `target\xtask\debug\xtask.exe lab preflight`. Kontrol probu geçmelidir.
