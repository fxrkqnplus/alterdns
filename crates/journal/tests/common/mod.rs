// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Testlerin ortak yardımcıları.

#![allow(dead_code, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

/// Test bitince (panikte de) silinen geçici dizin: `%TEMP%\alterdns-<önek>-<pid>-<ad>`.
pub struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

pub fn temp(prefix: &str, name: &str) -> TempDir {
    let d = std::env::temp_dir().join(format!("alterdns-{prefix}-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    TempDir(d)
}
