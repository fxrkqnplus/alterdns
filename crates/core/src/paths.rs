// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Veri ve yapılandırma yolları (`docs/ARCHITECTURE.md` §5). Kökler ortam değişkenlerinden
//! değil, bilinen klasör API'sinden alınır. Yollar `PathBuf` olarak taşınır; ASCII varsayılmaz.

use std::path::PathBuf;

use alterdns_errors::{AdnsError, AdnsErrorCode};
use alterdns_winsys::{KnownFolder, known_folder};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// `%ProgramData%\AlterDNS`
    pub machine: PathBuf,
    /// `%LOCALAPPDATA%\AlterDNS`
    pub user: PathBuf,
    /// `%ProgramFiles%\AlterDNS`
    pub program_files: PathBuf,
}

impl Paths {
    /// Geçerli kullanıcı için gerçek yollar.
    pub fn system() -> Result<Paths, AdnsError> {
        let get = |f: KnownFolder| {
            known_folder(f)
                .map(|p| p.join("AlterDNS"))
                .map_err(|e| AdnsError::new(AdnsErrorCode::GenUnexpected).with_source(e))
        };
        Ok(Paths {
            machine: get(KnownFolder::ProgramData)?,
            user: get(KnownFolder::LocalAppData)?,
            program_files: get(KnownFolder::ProgramFiles)?,
        })
    }

    /// Verilen köklerle (testler ve geliştirme kurulumu için).
    pub fn rooted(machine: PathBuf, user: PathBuf, program_files: PathBuf) -> Paths {
        Paths {
            machine,
            user,
            program_files,
        }
    }

    pub fn machine_config(&self) -> PathBuf {
        self.machine.join("config").join("machine.toml")
    }

    pub fn user_config(&self) -> PathBuf {
        self.user.join("config").join("config.toml")
    }

    pub fn journal_dir(&self) -> PathBuf {
        self.machine.join("journal")
    }

    pub fn machine_logs(&self) -> PathBuf {
        self.machine.join("logs")
    }

    pub fn user_logs(&self) -> PathBuf {
        self.user.join("logs")
    }

    /// Bir sürümün ikili dizini (`%ProgramFiles%\AlterDNS\<sürüm>`).
    pub fn install_dir(&self, version: &str) -> PathBuf {
        self.program_files.join(version)
    }
}
