# AlterDNS — Hata Kataloğu

Bu belge kod şemasını ve kataloğu tanımlar. **Tek kaynak `errors/catalog.toml` dosyasıdır**; İngilizce metinler de oradadır. §3 o dosyadan üretilir.

## 1. Şema

```
ADNS-<ALAN>-<NNN>
```

| Alan | Kapsam |
|---|---|
| `GEN` | Genel / beklenmeyen |
| `IPC` | Servis ↔ istemci iletişimi |
| `SVC` | Windows servisi |
| `SYS` | Sistem değişiklikleri ve journal |
| `NET` | Ağ durumu ve otomatik ayar |
| `DPI` | Paket motoru ve sürücü |
| `DNS` | DNS çözümleme ve DoH |
| `SCN` | Dosya taraması |
| `QRN` | Karantina ve silme |
| `WEB` | Tarayıcı uzantısı ve native host |
| `REP` | İtibar verisi beslemeleri |
| `UPD` | Güncelleme |
| `INS` | Kurulum, bileşenler, kaldırma |
| `UI` | Arayüz |

**Numaralandırma:**
- `001–899` kullanıcıya gösterilen durumlardır. Önem (bilgi, uyarı, hata, kritik) ayrı bir alandır.
- `900–999` iç hatalardır ve kullanıcıya genel bir mesajla gösterilir.
- Bir kod yayımlandıktan sonra anlamı değiştirilmez.
- Kullanılmayan kod silinmez, `deprecated = true` olarak işaretlenir.

**Ek alanlar:**
- `name`: Rust'taki varyant adı, PascalCase ve benzersiz.
- `faz`: kodun ilk üretildiği faz. "Her kod en az bir testte üretilir" kuralı (T-ERR-003) yalnızca geçerli faza kadarki kodlara uygulanır.
- `params`: metinlerde `{ad}` biçiminde geçen yer tutucuların listesi. Doğrulayıcı ikisinin eşleştiğini denetler.

**Önem düzeyleri:** `Bilgi`, `Uyarı`, `Hata`, `Kritik`. Kritik hatalarda bildirim her zaman gösterilir ve ilgili modül güvenli duruma geçer.

## 2. TOML biçimi

```toml
[[error]]
code = "ADNS-DPI-001"
name = "DpiDriverOpenFailed"
severity = "error"            # info | warning | error | critical
area = "DPI"
faz = 2
params = []
retryable = true
user_action_required = true
docs = "errors#adns-dpi-001"

[error.tr]
title = "Paket sürücüsü açılamadı"
message = "Bağlantı koruması ağ sürücüsünü başlatamadı. İnternet bağlantınız etkilenmedi."
causes = ["Yönetici izni verilmedi", "Sürücü dosyası eksik veya değiştirilmiş", "Güvenlik yazılımı sürücüyü engelledi"]
solutions = ["AlterDNS'i yeniden başlatın", "Bileşenler sayfasından 'Bağlantı yönetimi'ni onarın", "Güvenlik yazılımınızın bildirimlerini kontrol edin; antivirüsü kapatmayın"]

[error.en]
title = "Packet driver could not be opened"
# …

[error.tech]                  # yalnızca log ve "Ayrıntıları kopyala"
fields = ["win32_error", "driver_path", "driver_version"]
```

**Kurallar:**
- Mesajlar suçlayıcı değildir.
- Ne olduğu, kullanıcıyı nasıl etkilediği ve ne yapılabileceği söylenir.
- Teknik ayrıntı (Win32 kodu, yol) yalnızca "Ayrıntıları kopyala" ile görünür.
- Çözümler uygulanabilir adımlardır. "Antivirüsü kapatın" gibi güvenliği azaltan öneriler **yasaktır**.
