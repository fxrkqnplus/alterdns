// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Komut satırı: `--tray [--logon]` tepsi kipi; argümansız ya da `--page <ad>` ana pencere.

use alterdns_core::UserConfig;

/// Ana pencerenin sayfaları (`--page` adı ve tek örnek iletisindeki kod).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageName {
    Overview,
    File,
    Browser,
    Connection,
    Components,
    Updates,
    Settings,
    About,
}

impl PageName {
    pub const ALL: [PageName; 8] = [
        PageName::Overview,
        PageName::File,
        PageName::Browser,
        PageName::Connection,
        PageName::Components,
        PageName::Updates,
        PageName::Settings,
        PageName::About,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PageName::Overview => "overview",
            PageName::File => "file",
            PageName::Browser => "browser",
            PageName::Connection => "connection",
            PageName::Components => "components",
            PageName::Updates => "updates",
            PageName::Settings => "settings",
            PageName::About => "about",
        }
    }

    pub fn parse(name: &str) -> Option<PageName> {
        PageName::ALL.into_iter().find(|p| p.as_str() == name)
    }

    /// Yayın iletisindeki kod: 1'den başlar (0 = sayfa yok).
    pub fn code(self) -> u32 {
        PageName::ALL
            .iter()
            .position(|p| *p == self)
            .map_or(0, |i| i as u32 + 1)
    }

    pub fn from_code(code: u32) -> Option<PageName> {
        let index = usize::try_from(code.checked_sub(1)?).ok()?;
        PageName::ALL.get(index).copied()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Tray { logon: bool },
    Window { page: Option<PageName> },
}

/// Tanınmayan komut satırı (çıkış kodu 2).
#[derive(Debug, PartialEq, Eq)]
pub struct UsageError;

pub fn parse_args(args: &[String]) -> Result<Mode, UsageError> {
    let (mut tray, mut logon, mut page) = (false, false, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--tray" if !tray => tray = true,
            "--logon" if !logon => logon = true,
            "--page" if page.is_none() => {
                let name = it.next().ok_or(UsageError)?;
                page = Some(PageName::parse(name).ok_or(UsageError)?);
            }
            _ => return Err(UsageError),
        }
    }
    match (tray, logon, page) {
        (true, logon, None) => Ok(Mode::Tray { logon }),
        (false, false, page) => Ok(Mode::Window { page }),
        _ => Err(UsageError),
    }
}

/// Oturum açılışında (`--logon`) başlatılan tepsi, kullanıcı bunu kapattıysa hemen çıkar.
/// Başlatma kaydı makine genelinde olduğu için kullanıcı tercihi burada uygulanır.
pub fn exit_at_logon(mode: &Mode, cfg: &UserConfig) -> bool {
    matches!(mode, Mode::Tray { logon: true }) && !cfg.run_at_logon
}
