// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Yerel git kancası: push öncesi kişisel veri ve politika taraması (Faz 0 S-5).
//! Kanca `.git/hooks` altına yazılır; depoya girmez.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

/// Push edilecek commit'in ağacını tarar.
pub fn script() -> &'static str {
    "#!/bin/sh\n# AlterDNS: push öncesi kişisel veri ve politika taraması (yerel kanca).\nexec cargo xtask check --tree HEAD\n"
}

/// `.git/hooks/pre-push`'u yazar. Başka bir kanca varsa üzerine yazmaz.
pub fn install(root: &Path) -> anyhow::Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "hooks"])
        .current_dir(root)
        .output()
        .context("git çalıştırılamadı")?;
    if !out.status.success() {
        bail!("git kanca dizini bulunamadı");
    }
    let dir = root.join(String::from_utf8_lossy(&out.stdout).trim());
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("pre-push");
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == script() => return Ok(path),
        Ok(_) => bail!(
            "{} zaten var ve başka bir kanca; üzerine yazılmadı",
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::fs::write(&path, script())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_script_runs_tree_scan() {
        assert!(script().starts_with("#!/bin/sh\n"));
        assert!(script().contains("cargo xtask check --tree HEAD"));
        assert!(!script().contains("--no-verify"));
    }
}
