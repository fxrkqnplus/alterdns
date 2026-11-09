# AlterDNS

Windows için modüler güvenlik ve bağlantı uygulaması. Üç modülden oluşur:

- **Dosya güvenliği:** İndirilen dosyaları ve gömülü içeriklerini çalıştırmadan, korumalı ortamda inceler. Sonucu Güvenli / Şüpheli / Riskli olarak raporlar. Karantina ve silme işlemlerini yaptıktan sonra sonucu doğrular.
- **Tarayıcı koruması:** Uygulamayla gelen tarayıcı uzantısıyla çalışır. Riskli olarak işaretlenmiş adreslere girilmeden önce AlterDNS engelleme sayfasını gösterir. Uzantı, uygulamadaki "Tarayıcıda kur" yönergesiyle tarayıcının geliştirici modundan yüklenir.
- **Bağlantı yönetimi:** VPN veya proxy kullanmadan, kullanıcının kendi bağlantısındaki DPI kaynaklı erişim kısıtlamalarını aşmak için yerel paket işleme yapar.

**Durum:** Geliştirme başlangıcı. Belgeler `docs/` altındadır. Geliştirme yönergeleri `CLAUDE.md` ve `docs/CLAUDE_CODE_PROMPT.md` dosyalarındadır.

## İndirme ve doğrulama

Resmi sürümler yalnızca bu deponun [Releases](https://github.com/fxrkqnplus/alterdns/releases) sayfasında yayımlanır. Bir dosyanın bu depodaki derleme hattında üretildiğini şöyle doğrulayabilirsiniz:

```
gh attestation verify <dosya> -R fxrkqnplus/alterdns
```

Ayrıca her sürümde `SHA256SUMS` dosyası bulunur.

**Önemli:** AlterDNS hiçbir zaman antivirüsünüzü veya Windows Akıllı Uygulama Denetimi'ni kapatmanızı istemez. Bunu isteyen bir yönerge veya kopya sahtedir.

## Kod imzalama politikası

İlk ön sürümler kod imzası taşımaz; bu yüzden Windows SmartScreen uyarı gösterebilir. Proje, açık kaynak projelere ücretsiz imza sağlayan SignPath Foundation programına başvuracaktır. İmza alındığında bu bölüm şu ifadeyle güncellenecektir: "Free code signing provided by SignPath.io, certificate by SignPath Foundation". Rol listesi (Yazar / İnceleyen / Onaylayan) de o zaman eklenecektir.

## Lisans

Copyright © 2026 Furkan Okan

AlterDNS özgür yazılımdır: **GNU General Public License v3.0 veya (seçiminize göre) daha sonraki bir sürümün** koşulları altında yeniden dağıtabilir ve/veya değiştirebilirsiniz. Ayrıntılar için `LICENSE` dosyasına bakın. Bu program faydalı olması umuduyla dağıtılmaktadır, ancak HİÇBİR GARANTİ VERİLMEMEKTEDİR.

**Ek izin (GPL-3.0 §7):** `LICENSE` dosyasının sonunda, AlterDNS kodunun Wintun projesinin dağıttığı değiştirilmemiş, imzalı `wintun.dll` ile birleştirilmesine izin veren bir ek izin bulunur. Ek izin yalnızca AlterDNS telif sahibinin (ve bu izni kabul eden katkıcıların) kodunu kapsar. Katkılar GPL-3.0-or-later ve bu ek izinle kabul edilir.

"AlterDNS" adı ve logosu bu lisansın kapsamı dışındadır; bkz. `TRADEMARK.md`.

Üçüncü taraf bileşenler kendi lisanslarına tabidir; bkz. `docs/research/THIRD_PARTY.md` (dağıtımda: `THIRD_PARTY_NOTICES`).
