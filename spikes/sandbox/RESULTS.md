# Sandbox spike'ı — sonuçlar (T-SPK-003)

Tarih: 2026-11-09 · Ortam: geliştirici makinesi, Windows 11 26H2 x64, debug derleme · Plan: `docs/superpowers/plans/2026-11-09-faz-0.md` Görev 5

## Yapılandırma

- **Kimlik:** AppContainer profili `AlterDNS.Spike.Scan`, yetenek yok.
- **İşlem oluşturma:** `STARTUPINFOEXW` öznitelikleri:
  - `SECURITY_CAPABILITIES`
  - `HANDLE_LIST` (yalnızca girdi dosyası + stdout pipe'ı)
  - `CHILD_PROCESS_POLICY = RESTRICTED`
- **Bayraklar:** `CREATE_SUSPENDED | DETACHED_PROCESS`.
- **Job Object:** `KILL_ON_JOB_CLOSE`, `ACTIVE_PROCESS = 1`, `PROCESS_MEMORY = 256 MiB`, işlem CPU süresi 10 sn, `DIE_ON_UNHANDLED_EXCEPTION`, UI kısıtları.
- **Girdi:** `GENERIC_READ`, `FILE_SHARE_READ`, `FILE_FLAG_OPEN_REPARSE_POINT` ile açılan, devralınabilir handle. İşçiye yalnızca handle değeri verilir; yol verilmez.

## Testler

`cargo test --manifest-path spikes/sandbox/Cargo.toml -- --test-threads=1` → **10/10 geçti.** Testler profili ve exe ACL'sini paylaştığı için sırayla koşar.

Her olumsuz test önce işçinin gerçekten çalıştığını doğrular (`no_output` yok, çıkış kodu 0, prob adı geri geldi). İlk denemede işçi hiç başlamadığı hâlde olumsuz testler "geçiyordu"; bu boşluk kapatıldı.

| Prob | Sonuç | Ayrıntı |
|---|---|---|
| `identity` | ✅ AppContainer, Low IL | `is_appcontainer = true`, bütünlük RID `0x1000` |
| `read-handle` | ✅ okundu | SHA-256 broker'ın hesapladığıyla aynı |
| `open-user-file` | ✅ engellendi | Hata 5 (erişim reddedildi); `%LOCALAPPDATA%` altındaki dosya |
| `write-temp` | ✅ engellendi | Hata 5; kullanıcının temp dizini |
| `write-own-temp` | ℹ️ yazılabilir | Kapsayıcının özel deposu (`%LOCALAPPDATA%\Packages\<profil>\AC\Temp`) |
| `connect-loopback` | ✅ engellendi | 3 sn zaman aşımı (loopback yalıtımı; reddetme değil, düşürme) |
| `connect-external` | ✅ engellendi | WSAEACCES (10013); `internetClient` yeteneği yok |
| `spawn-child` | ✅ engellendi | Hata 367 (`ERROR_CHILD_PROCESS_BLOCKED`) |
| `alloc` (300 MiB) | ✅ engellendi | Ayırma başarısız (iş nesnesi bellek sınırı); işlem sonlandırılmadı |

## Ölçüm

İşçi başlatma (askıda oluşturma → iş nesnesi → sürdürme → çıktının okunması), `noop` probu, 30 tekrar, debug derleme: **p50 9,1 ms, p95 9,6 ms** (en az 8,1, en çok 9,7). Profil oluşturma ve exe ACL'si bu süreye dahil değildir.

## Bulgular (Faz 3 için)

1. **Konsol ve iş nesnesi:** `CREATE_NO_WINDOW` ile işçi `0xC0000142` (`STATUS_DLL_INIT_FAILED`) koduyla başlamadan ölüyordu. Muhtemel neden, konsol ana işleminin (conhost) iş nesnesinin süreç sınırına takılması. `DETACHED_PROCESS` ile konsol oluşturulmuyor ve stdout pipe'ı çalışıyor. Ürün işçisi konsolsuz başlatılmalı.
2. **Kapsayıcının özel deposu yazılabilir:** AppContainer içinde `GetTempPath` kapsayıcının kendi `AC\Temp` dizinini döndürüyor. İşçi buraya yazabiliyor ve iş nesnesi disk kullanımını sınırlamıyor. Faz 3'te tarama başına temizlik veya disk kotası gerekir.
3. **Exe ACL'si:** AppContainer, "ALL APPLICATION PACKAGES" (`S-1-15-2-1`) için RX izni olmayan bir exe'yi çalıştıramaz. Spike, `target/` altındaki exe'ye `icacls` ile bu izni verdi. `%ProgramFiles%` bu izni varsayılan olarak verir; kurulum dizini ACL'si korunmalı.
4. **Komut satırı tırnaklama:** Tırnaklı argümanın sonundaki `\` kapanan tırnağı kaçırıyor (`\"`) ve sonraki argümanları bozuyordu. Ürün işçisine parametreler komut satırı yerine devralınan pipe üzerinden verilmeli.
5. **Bellek sınırı:** Sınır aşımı işlemi sonlandırmıyor; ayırma hatası olarak dönüyor. Rust'ta hata döndürmeyen ayırmalar bu durumda süreci durdurur (abort). Broker bunu çökme olarak sınıflandırıp "incelenemedi" sonucu vermeli (T-SCN-009).
6. **Loopback:** Bağlantı reddedilmiyor, zaman aşımına uğruyor. Broker zaman sınırları buna göre ayarlanmalı.
7. **Profil yaşam döngüsü:** `CreateAppContainerProfile` kullanıcı profilinde `Packages\alterdns.spike.scan` dizinini oluşturuyor. `sandbox-spike cleanup` (`DeleteAppContainerProfile`) dizini kaldırdı; doğrulandı. Ürün: profil kurulumda oluşturulur (journal'lı), kaldırmada silinir.

## Kapsam dışı (Faz 3'te T-SBX-001 genişletilecek)

- UI kısıtlarının ve CPU süresi sınırının etkisi ayrıca sınanmadı.
- Liste dışındaki handle'ların devralınmadığı ayrıca sınanmadı.
- Kayıt defteri, adlandırılmış nesneler ve pano erişimi sınanmadı.
