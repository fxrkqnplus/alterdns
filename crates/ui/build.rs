// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! `.slint` dosyalarını derler ve `lang/<dil>/LC_MESSAGES/alterdns-ui.po` çevirilerini ikiliye
//! gömer. Varsayılan çeviri bağlamı kapalıdır: anahtar yalnızca İngilizce metindir
//! (`xtask check` adımı `i18n` aynı kuralla denetler).

fn main() {
    // Başsız testlerin öğe arama API'si (ElementHandle) üretilen kodda hata ayıklama bilgisi
    // ister; yalnızca `debug` profilinde açılır, sürüm ikilisine girmez.
    let debug = std::env::var("PROFILE").is_ok_and(|p| p == "debug");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None)
        .with_debug_info(debug);
    if let Err(e) = slint_build::compile_with_config("ui/app.slint", config) {
        eprintln!("slint derlemesi başarısız: {e}");
        std::process::exit(1);
    }
}
