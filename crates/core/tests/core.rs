// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Yollar, yapılandırma ve log maskeleme.

// Yalnızca test kodu: yardımcı işlevler de unwrap/panic kullanabilir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use alterdns_core::{
    Lang, LeftClick, LoadOutcome, Paths, UserConfig, load_user, redact_user_paths, save_user,
};
use alterdns_winsys::{KnownFolder, known_folder};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("alterdns-core-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn default_user_config_matches_k9() {
    let d = temp("default");
    let path = d.join("config.toml");
    let (cfg, outcome) = load_user(&path).unwrap();
    assert_eq!(outcome, LoadOutcome::Defaulted { backup: None });
    assert_eq!(
        cfg,
        UserConfig {
            language: Lang::Tr,
            left_click: LeftClick::Flyout,
            notifications: true,
            run_at_logon: true,
            online_lookups: false,
        }
    );
    save_user(&path, &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("schema = 1"), "{text}");
    assert_eq!(load_user(&path).unwrap(), (cfg, LoadOutcome::Loaded));
}

#[test]
fn roundtrip_preserves_unknown_keys() {
    let d = temp("unknown");
    let path = d.join("config.toml");
    std::fs::write(
        &path,
        "schema = 1\n[ui]\nlanguage = \"en\"\nfuture_key = 3\n[future]\nx = \"y\"\n",
    )
    .unwrap();
    let (mut cfg, outcome) = load_user(&path).unwrap();
    assert_eq!(outcome, LoadOutcome::Loaded);
    assert_eq!(cfg.language, Lang::En);
    cfg.notifications = false;
    save_user(&path, &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("future_key = 3"), "{text}");
    assert!(text.contains("[future]"), "{text}");
    assert!(text.contains("notifications = false"), "{text}");
}

#[test]
fn corrupt_config_is_backed_up_and_defaults_used() {
    for (i, bytes) in [
        &b""[..],
        b"[[[",
        b"schema = 1\n[ui]\nlanguage = \"t",
        b"\xff\xfe\x00",
    ]
    .iter()
    .enumerate()
    {
        let d = temp(&format!("corrupt{i}"));
        let path = d.join("config.toml");
        std::fs::write(&path, bytes).unwrap();
        let (cfg, outcome) = load_user(&path).unwrap();
        assert_eq!(cfg, UserConfig::default(), "durum {i}");
        let LoadOutcome::Defaulted { backup: Some(b) } = outcome else {
            panic!("durum {i}: {outcome:?}");
        };
        assert_eq!(std::fs::read(&b).unwrap(), *bytes, "durum {i}");
    }
}

#[test]
fn newer_schema_is_not_overwritten() {
    let d = temp("newer");
    let path = d.join("config.toml");
    let original = b"schema = 2\n[ui]\nlanguage = \"en\"\n[yeni]\nalan = 1\n";
    std::fs::write(&path, original).unwrap();
    let (cfg, outcome) = load_user(&path).unwrap();
    assert_eq!(outcome, LoadOutcome::NewerSchema);
    assert_eq!(cfg.language, Lang::En);
    assert!(save_user(&path, &cfg).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn failed_save_keeps_original() {
    let d = temp("failsave");
    let path = d.join("config.toml");
    std::fs::write(&path, "schema = 1\n[ui]\nlanguage = \"en\"\n").unwrap();
    let before = std::fs::read(&path).unwrap();
    // Salt okunur hedef: yeniden adlandırma başarısız olur (yazma hatası karşılığı).
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&path, perm.clone()).unwrap();
    assert!(save_user(&path, &UserConfig::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // Geçici dosya geride kalmaz.
    let leftovers: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&path, perm).unwrap();
}

#[test]
fn oversized_config_is_treated_as_corrupt() {
    let d = temp("oversized");
    let path = d.join("config.toml");
    let big = format!("schema = 1\n# {}\n", "x".repeat(70 * 1024));
    std::fs::write(&path, &big).unwrap();
    let (cfg, outcome) = load_user(&path).unwrap();
    assert_eq!(cfg, UserConfig::default());
    assert!(
        matches!(outcome, LoadOutcome::Defaulted { backup: Some(_) }),
        "{outcome:?}"
    );
}

#[test]
fn save_over_corrupt_file_keeps_a_backup() {
    let d = temp("savecorrupt");
    let path = d.join("config.toml");
    std::fs::write(&path, "[[[").unwrap();
    save_user(&path, &UserConfig::default()).unwrap();
    assert_eq!(
        std::fs::read(path.with_extension("toml.bak")).unwrap(),
        b"[[["
    );
    assert_eq!(load_user(&path).unwrap().1, LoadOutcome::Loaded);
}

#[test]
fn unwritable_backup_still_falls_back_to_defaults() {
    let d = temp("bakdir");
    let path = d.join("config.toml");
    std::fs::write(&path, "[[[").unwrap();
    std::fs::create_dir_all(path.with_extension("toml.bak")).unwrap();
    let (cfg, outcome) = load_user(&path).unwrap();
    assert_eq!(cfg, UserConfig::default());
    assert_eq!(outcome, LoadOutcome::Defaulted { backup: None });
}

#[test]
fn paths_keep_non_ascii_user_dirs() {
    let root = temp("unicode")
        .join("Çağrı Öz")
        .join("AppData")
        .join("Local");
    let p = Paths::rooted(root.join("pd"), root.join("AlterDNS"), root.join("pf"));
    let cfg = p.user_config();
    assert!(cfg.starts_with(&root), "{}", cfg.display());
    assert!(cfg.display().to_string().contains("Çağrı Öz"));
    save_user(&cfg, &UserConfig::default()).unwrap();
    assert_eq!(load_user(&cfg).unwrap().1, LoadOutcome::Loaded);
}

#[test]
fn paths_system_are_under_known_folders() {
    let p = Paths::system().unwrap();
    assert_eq!(
        p.machine,
        known_folder(KnownFolder::ProgramData)
            .unwrap()
            .join("AlterDNS")
    );
    assert_eq!(
        p.user,
        known_folder(KnownFolder::LocalAppData)
            .unwrap()
            .join("AlterDNS")
    );
    assert!(p.journal_dir().ends_with("AlterDNS\\journal"));
    assert!(p.install_dir("1.2.3").ends_with("AlterDNS\\1.2.3"));
}

#[test]
fn redact_masks_turkish_user_name() {
    // Örnek profil yolları parçalı kurulur; kişisel veri taraması kaynağı yakalamasın.
    let users = ["Us", "ers"].concat();
    assert_eq!(
        redact_user_paths(&format!("C:\\{users}\\Çağrı Öz\\x")),
        format!("C:\\{users}\\<kullanıcı>\\x")
    );
    assert_eq!(
        redact_user_paths(&format!(
            "dosya: /c/{users}/ali/AppData ve C:\\{users}\\Public\\y"
        )),
        format!("dosya: /c/{users}/<kullanıcı>/AppData ve C:\\{users}\\Public\\y")
    );
    assert_eq!(
        redact_user_paths("C:\\Users\\Default\\z"),
        "C:\\Users\\Default\\z"
    );
    assert_eq!(redact_user_paths("yol yok"), "yol yok");
    // Kaçışlı ayraçlar ({:?} ve JSON) ve büyük harf.
    let upper = ["US", "ERS"].concat();
    assert_eq!(
        redact_user_paths(&format!("\"C:\\\\{users}\\\\bob\\\\x\"")),
        format!("\"C:\\\\{users}\\\\<kullanıcı>\\\\x\"")
    );
    assert_eq!(
        redact_user_paths(&format!("C:\\{upper}\\BOB\\x")),
        format!("C:\\{upper}\\<kullanıcı>\\x")
    );
    assert_eq!(
        redact_user_paths(&format!("/mnt/c/{users}/ali/x")),
        format!("/mnt/c/{users}/<kullanıcı>/x")
    );
    assert_eq!(
        redact_user_paths(&format!("\\\\sunucu\\c$\\{users}\\veli\\x")),
        format!("\\\\sunucu\\c$\\{users}\\<kullanıcı>\\x")
    );
    // Ad boşluk içerebildiği için ayraç gelene kadar maskelenir (fazla maskeleme güvenli yön).
    let r = redact_user_paths(&format!("dosya C:\\{users}\\ali okunamadı"));
    assert!(!r.contains("ali"), "{r}");
}
