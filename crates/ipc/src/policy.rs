// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Çağıran yetkisi (K10). Yetki çağıranın belirtecinden gelir. İmaj yolu ek bir katmandır:
//! aynı kullanıcıdaki kod bir AlterDNS ikilisine enjekte edilebilir, bu yüzden `Elevated`
//! ayrıca yönetici belirteci ister.

use std::path::{Path, PathBuf};

use alterdns_errors::{AdnsError, AdnsErrorCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerPolicy {
    Open,
    InteractiveUser,
    Elevated,
}

/// Bağlantıdaki çağıran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub sid: String,
    pub session_id: u32,
    pub elevated: bool,
    pub interactive: bool,
    pub image_path: Option<PathBuf>,
}

impl From<alterdns_winsys::CallerToken> for Caller {
    fn from(t: alterdns_winsys::CallerToken) -> Caller {
        Caller {
            sid: t.sid,
            session_id: t.session_id,
            elevated: t.elevated,
            interactive: t.interactive,
            image_path: t.image_path,
        }
    }
}

fn unauthorized(why: &str) -> AdnsError {
    AdnsError::new(AdnsErrorCode::IpcUnauthorized)
        .with_source(std::io::Error::other(why.to_string()))
}

/// İmaj `<kök>\<sürüm>\alterdns-*.exe` mi? İki yol da `canonicalize` ile (son yol, disk üzerindeki
/// harfler) karşılaştırılır; okunamayan yol geçmez.
fn in_install_dir(image: Option<&Path>, install_root: &Path) -> bool {
    let (Some(image), Ok(root)) = (image, std::fs::canonicalize(install_root)) else {
        return false;
    };
    let Ok(image) = std::fs::canonicalize(image) else {
        return false;
    };
    let name_ok = image
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|n| n.starts_with("alterdns-") && n.ends_with(".exe"));
    name_ok && image.parent().and_then(Path::parent) == Some(root.as_path())
}

pub fn authorize(
    policy: CallerPolicy,
    caller: &Caller,
    install_root: &Path,
) -> Result<(), AdnsError> {
    match policy {
        CallerPolicy::Open => Ok(()),
        CallerPolicy::InteractiveUser if caller.interactive || caller.elevated => Ok(()),
        CallerPolicy::InteractiveUser => Err(unauthorized("etkileşimli oturum değil")),
        CallerPolicy::Elevated if !caller.elevated => Err(unauthorized("yönetici değil")),
        CallerPolicy::Elevated if !in_install_dir(caller.image_path.as_deref(), install_root) => {
            Err(unauthorized(
                "kurulum dizinindeki bir AlterDNS ikilisi değil",
            ))
        }
        CallerPolicy::Elevated => Ok(()),
    }
}
