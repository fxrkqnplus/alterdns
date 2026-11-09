# UI spike'ı — sonuçlar (T-SPK-001 → ADR-002)

Tarih: 2026-11-09 · Ortam: geliştirici makinesi, Windows 11 26H2 x64, tek monitör · Plan: `docs/superpowers/plans/2026-11-09-faz-0.md` Görev 6

## Prototipler

İki prototip de `UI_SPEC.md` §4'teki aynı taslağı çizer: başlık, üç kart, Türkçe glif satırı (`ğĞüÜşŞıİöÖçÇ`), "Ana pencereyi aç" düğmesi.

**Ortak katman** (`common`):
- Tepsi simgesi (`Shell_NotifyIcon`, NOTIFYICON_VERSION_4) ve gizli ileti penceresi.
- Flyout konumu: simgeye hizalı, görev çubuğu kenarına göre, çalışma alanı içinde, PerMonitor DPI.
- Sertleştirme (alt sınıf):
  - `WM_NCHITTEST → HTCLIENT`; `SC_MOVE`, `SC_SIZE` ve `SC_KEYMENU` yok sayılır.
  - Odak kaybında ve Esc ile kapanır.
  - `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, sistem menüsü yok.
  - `DWMWCP_ROUND` köşe, `DWMSBT_TRANSIENTWINDOW` arka plan.

**Varyantlar** (hepsi aynı release profili: `lto = true`, `codegen-units = 1`, `strip = true`):

| Etiket | Çatı | Çizim | Kapanış |
|---|---|---|---|
| `win32-d2d-hw` | Win32 | Direct2D donanım hedefi + DirectWrite | Pencere ve hedef yok edilir |
| `win32-d2d-sw` | Win32 | Direct2D yazılım hedefi + DirectWrite | Pencere ve hedef yok edilir |
| `slint-sw-hide` | Slint 1.18.1 (GPL-3.0 seçeneği), winit | Yazılım çizici | `hide()` |
| `slint-sw-drop` | 〃 | Yazılım çizici | Bileşen bırakılır, açılışta yeniden oluşturulur |
| `slint-gl-hide` | 〃 | FemtoVG (OpenGL) | `hide()` |
| `slint-gl-drop` | 〃 | FemtoVG (OpenGL) | Bileşen bırakılır |

## Ölçüm yöntemi

`ui-measure`, her varyant için:
- 5 soğuk başlatma yapar. İlkinde 10 sn boşta bekleyip belleği ölçer.
- İlk başlatmada flyout açıkken, kapandıktan 5 sn sonra ve 50 sıcak aç/kapa sonrasında belleği ölçer.
- Bellek: `GetProcessMemoryInfo` → `PrivateUsage` ve `WorkingSetSize`.

**Açılış süresi:** ölçüm aracının "aç" komutunu (`PostMessage`) göndermeden hemen önceki QPC ile uygulamanın yazdığı `READY <qpc>` arasındaki fark. `READY`, açılıştan sonraki ilk `WM_PAINT` işlendikten ve `DwmFlush` döndükten sonra yazılır.

- `WM_PAINT` kuyruktan alındığı için `WH_GETMESSAGE` ile yakalanır; ardından kuyruklanan bir ileti boyama bittikten sonra işlenir.
- Ölçüm kipinde odak kaybında kapanma kapalıdır; flyout yalnızca komutla açılır ve kapanır.

## Ölçümler (release, 2026-11-09)

Bellek: `PrivateUsage` (bütçe ölçütü, TESTING.md §5) / `WorkingSet`, MB. Süre: ms. Soğuk açılış = 5 ayrı başlatmada ilk açılış; sıcak = 50 aç/kapa.

| Varyant | İkili | Boşta | Flyout açık | Kapandıktan 5 sn sonra | 50 döngü sonra | Soğuk p50 / en çok | Sıcak p50 / p95 |
|---|---|---|---|---|---|---|---|
| `win32-d2d-hw` | 0,17 | 1,76 / 10,8 | **53,2** / 44,4 | **53,2** / 44,4 | 54,0 | 165 / 185 | 11,7 / 13,9 |
| `win32-d2d-sw` | 0,17 | 1,70 / — | 6,68 / — | 6,68 / — | **8,43** | 28 / 33 | 9,5 / 13,6 |
| `slint-sw-hide` | 9,11 | 3,02 / 27,2 | 4,06 / 29,1 | 4,06 / 29,1 | 4,06 | 18 / 22 | 10,0 / 12,7 |
| `slint-sw-drop` | 9,11 | 2,01 / 11,8 | 4,00 / 21,0 | 3,49 / 20,6 | 3,55 | 6\* / 6\* | 3,1\* / 8,0\* |
| `slint-gl-hide` | 8,85 | **113,6** / 129,6 | **125,4** / 140,1 | **125,4** / 140,1 | 126,4 | 34 / 41 | 11,3 / 13,1 |
| `slint-gl-drop` | 8,85 | 2,24 / 12,1 | **70,7** / 71,7 | **57,4** / 70,1 | 82,4 | 107 / 107 | 5,0 / 8,7 |

Bütçe (CLAUDE_CODE_PROMPT §10): tepsi ajanı flyout kapalıyken ≤ 8 MB, flyout açık ≤ 45 MB, açılış p95 ≤ 150 ms (sıcak) ve ≤ 400 ms (soğuk). **Kalın** değerler bütçeyi aşar.

\* `drop` kipinde yeni bir Slint penceresinin ilk `WM_PAINT`'inde içeriğin çizildiği doğrulanmadı. Bu süreler alt sınır olabilir. `hide` kipi ürün için esas alınır.

`win32-d2d-sw` ilk tam koşuda bir açılışta `READY` 20 sn içinde gelmedi; tekrar koşu sorunsuzdu (yaklaşık 110 açılışta 1 kayıp). Bu, ölçüm yönteminin bir sınırı olarak kaydedildi.

## Davranış denetimi (otomatik, ölçüm kipinde)

| Denetim | Win32 (D2D) | Slint (yazılım) |
|---|---|---|
| `WM_NCHITTEST` üst kenar → `HTCLIENT` | ✅ | ✅ (alt sınıf) |
| `SC_MOVE` ve `SC_KEYMENU` sonrası konum aynı | ✅ | ✅ |
| Esc ile kapanma | ✅ | ✅ |
| Başlık ve sistem menüsü stili yok | ✅ | ❌ winit stilleri geri yazıyor |
| `WS_EX_TOOLWINDOW` (görev çubuğu ve Alt+Tab'da görünmez) | ✅ | ❌ winit geri yazıyor |
| `WS_EX_TOPMOST` | ✅ | ✅ |
| UIA ağacı | ❌ tek `Pane` düğümü | ✅ 14 düğüm: `Group`, `Text`, `Button`, Türkçe adlarla |
| Türkçe glifler (ekran görüntüsü) | ✅ | ✅ |
| Yuvarlak köşe | ✅ | ✅ |
| Geçici arka plan malzemesi | Görünmüyor (opak arka plan) | Görünmüyor (opak arka plan) |

Otomatik denetim betiği (`scripts/behavior-check.ps1`) canlı masaüstünde güvenilir değil: `win32-d2d-sw` ve `slint-gl` için `READY` alınamadı. Ölçüm aracı aynı ikililerle sorunsuz çalıştığı için sorun betikte.

## Bulgular

1. **GPU çiziciler bütçeyi aşıyor.** D2D donanım hedefi ve FemtoVG (OpenGL), GPU sürücüsünü işleme yükler: 53–125 MB özel bellek; kapatınca iade yok. Yazılım çiziciler 4–7 MB'ta kalıyor. Ürün tepsi ajanında yazılım çizici kullanılmalı.
2. **Slint + yazılım çizici bütün bütçeleri karşılıyor:** boşta 3,0 MB, açık 4,1 MB, sıcak p95 12,7 ms, soğuk ≤ 22 ms. 50 döngüde bellek artışı yok.
3. **Win32 + D2D yazılım hedefi de bütçede**, ama 50 döngüde 6,7 → 8,4 MB artış var (sızıntı adayı). Erişilebilirlik (UIA) elle yazılmalı; büyük emek.
4. **Slint/winit pencere stillerini geri yazıyor.** `WS_EX_TOOLWINDOW` ve sistem menüsü kaldırma kalıcı olmuyor; alt sınıf iletileri (hit test, `SC_*`, Esc) çalışıyor. Ürün için winit'e özgü API (Slint `unstable-winit-030` → `WindowExtWindows::set_skip_taskbar` vb.) veya stil değişiminde yeniden uygulama gerekir. Faz 1'de doğrulanacak.
5. **İkili boyutu:** Slint ikilisi ~9 MB (Win32 0,17 MB). Tepsi ve ana pencere ayrı exe olursa ~18 MB olur; çekirdek kurulum bütçesi ≤ 25 MB'a yaklaşır. Öneri: tepsi ve ana pencere için tek exe, farklı kiplerle.
6. **Çalışma kümesi:** Slint-sw'de çalışma kümesi 27–29 MB (özel bellek 3–4 MB). Bütçe ölçütü özel bellek; çalışma kümesi bilgi olarak raporlanır.
7. **Ölçüm yöntemi:** `WH_CALLWNDPROCRET`, `WM_PAINT`'i göremez (kuyruktan alınır); `WH_GETMESSAGE` + kuyruklanan ileti gerekir. Canlı masaüstünde odak değişimleri ölçümü bozar; ölçüm kipi ve açık komutlar gerekli.

## Elle denetim (kullanıcıyla; bekliyor)

- Gerçek odak kaybında kapanma: masaüstüne tıklama, başka pencere, Win tuşu.
- Alt+Space ve fareyle sürükleme denemesi.
- Slint penceresinin görev çubuğunda ve Alt+Tab'da görünüp görünmediği.
- DPI: %125, %150 ve %200 ölçek; varsa ikinci monitöre taşıma.
- Görev çubuğu farklı kenardayken konum.
