// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Kurulum işlemleri: `FileInstall` kaynak dosyadan kopyalanır ve hash'i doğrulanır. Diğer bütün
//! işlemler `RealOps`'a devredilir; geri almada yalnızca `RealOps` gerekir (kendi dosyalarımız
//! yalnızca silinir).

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::model::{Kind, Snapshot};
use crate::ops::SystemOps;
use crate::real::{RealOps, install_target, sha256_file};

pub struct InstallOps {
    /// Hedef yolu → kaynak dosya.
    sources: BTreeMap<String, PathBuf>,
}

impl InstallOps {
    pub fn new(sources: BTreeMap<String, PathBuf>) -> InstallOps {
        InstallOps { sources }
    }
}

impl SystemOps for InstallOps {
    fn read(&self, kind: Kind, target: &str) -> Result<Snapshot, String> {
        RealOps.read(kind, target)
    }

    fn write(&self, kind: Kind, target: &str, value: &Snapshot) -> Result<(), String> {
        let (Kind::FileInstall, Snapshot::Present(want)) = (kind, value) else {
            return RealOps.write(kind, target, value);
        };
        let source = self
            .sources
            .get(target)
            .ok_or_else(|| format!("kaynak yok: {target}"))?;
        let dest = install_target(target)?;
        if std::fs::symlink_metadata(&dest).is_ok() {
            return Err(format!("hedef zaten var: {target}"));
        }
        std::fs::copy(source, &dest).map_err(|e| e.to_string())?;
        // Kaynak, hash'i alındıktan sonra değişmiş olabilir: kopyanın kendisi doğrulanır.
        match sha256_file(&dest) {
            Ok(got) if &got == want => Ok(()),
            other => {
                let _ = std::fs::remove_file(&dest);
                Err(format!("kopyanın hash'i beklenen değil: {other:?}"))
            }
        }
    }
}
