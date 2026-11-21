// AlterDNS — Copyright (C) 2026 Furkan Okan
// SPDX-License-Identifier: GPL-3.0-or-later

//! Korumalı veri dizinleri (K20): `%ProgramData%\AlterDNS` ve alt dizinleri yalnızca SYSTEM ve
//! Administrators yazabilir. Dizin önceden varsa (ör. standart bir kullanıcı kurulumdan önce
//! oluşturduysa) sahibi ve DACL'i doğrulanır; reparse noktası reddedilir.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce,
    GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, IsWellKnownSid,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
};
use windows::core::{BOOL, PCWSTR};

/// `ACCESS_ALLOWED_ACE_TYPE` ve `ACCESS_DENIED_ACE_TYPE` (winnt.h). Diğer türler (nesne,
/// geri çağırmalı ACE'ler) değerlendirilmez; DACL'de bulunurlarsa dizin reddedilir.
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;
/// Yazma sayılan haklar: veri yazma/ekleme, EA ve öznitelik yazma, alt öğe silme, silme,
/// DACL ve sahip değiştirme, genel yazma ve genel tüm haklar.
const WRITE_RIGHTS: u32 = 0x0000_0002
    | 0x0000_0004
    | 0x0000_0010
    | 0x0000_0040
    | 0x0000_0100
    | 0x0001_0000
    | 0x0004_0000
    | 0x0008_0000
    | 0x1000_0000
    | 0x4000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirAccess {
    /// Yalnızca SYSTEM ve Administrators (ör. `journal`).
    AdminOnly,
    /// Yönetici yazar, Users okur (ör. `config`).
    AdminWriteUsersRead,
}

impl DirAccess {
    /// Korumalı (`P`: kalıtım kapalı) DACL.
    pub fn sddl(self) -> &'static str {
        match self {
            DirAccess::AdminOnly => "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)",
            DirAccess::AdminWriteUsersRead => {
                "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FRFX;;;BU)"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirProblem {
    Missing,
    ReparsePoint,
    UntrustedOwner,
    WritableByOthers,
    Unreadable(String),
}

pub(crate) fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// `LocalAlloc` ile ayrılmış güvenlik tanımlayıcısı; düşünce `LocalFree` ile serbest bırakılır.
pub(crate) struct LocalSd(pub(crate) PSECURITY_DESCRIPTOR);

impl Drop for LocalSd {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: İşaretçi API'nin `LocalAlloc` ile ayırdığı bellektir ve bir kez serbest bırakılır.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0)));
            }
        }
    }
}

pub(crate) fn sd_from_sddl(sddl: &str) -> std::io::Result<LocalSd> {
    let text: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sd = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
    // SAFETY: `text` NUL ile biten geçerli bir UTF-16 dizesidir ve çağrı boyunca yaşar; çıktı
    // işaretçisi `LocalSd` ile serbest bırakılır.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(text.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
    }
    .map_err(std::io::Error::other)?;
    Ok(LocalSd(sd))
}

fn trusted(sid: PSID) -> bool {
    // SAFETY: `sid` geçerli bir güvenlik tanımlayıcısının içindeki SID'e işaret eder.
    unsafe {
        IsWellKnownSid(sid, WinLocalSystemSid).as_bool()
            || IsWellKnownSid(sid, WinBuiltinAdministratorsSid).as_bool()
    }
}

/// Sahip SYSTEM ya da Administrators olmalı; DACL bu ikisi dışında kimseye yazma hakkı vermemeli.
/// NULL DACL herkese tam erişim demektir.
fn evaluate_sd(sd: PSECURITY_DESCRIPTOR) -> Result<(), DirProblem> {
    let mut owner = PSID(std::ptr::null_mut());
    let mut defaulted = BOOL(0);
    // SAFETY: `sd` geçerli bir güvenlik tanımlayıcısıdır; çıktılar yerel değişkenlerdir.
    unsafe { GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) }
        .map_err(|e| DirProblem::Unreadable(e.to_string()))?;
    if owner.0.is_null() || !trusted(owner) {
        return Err(DirProblem::UntrustedOwner);
    }
    let mut present = BOOL(0);
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: `sd` geçerli; çıktılar yerel değişkenlerdir.
    unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) }
        .map_err(|e| DirProblem::Unreadable(e.to_string()))?;
    if !present.as_bool() || dacl.is_null() {
        return Err(DirProblem::WritableByOthers);
    }
    // SAFETY: `dacl` boş olmayan, güvenlik tanımlayıcısına ait geçerli bir ACL'dir.
    let count = unsafe { (*dacl).AceCount };
    for i in 0..u32::from(count) {
        let mut ace: *mut c_void = std::ptr::null_mut();
        // SAFETY: `i < AceCount`; `ace` ACL içindeki bir ACE'ye işaret eder.
        unsafe { GetAce(dacl, i, &mut ace) }.map_err(|e| DirProblem::Unreadable(e.to_string()))?;
        // SAFETY: Her ACE bir `ACE_HEADER` ile başlar.
        let header = unsafe { &*(ace as *const ACE_HEADER) };
        match header.AceType {
            ACCESS_ALLOWED_ACE_TYPE => {}
            ACCESS_DENIED_ACE_TYPE => continue,
            other => {
                return Err(DirProblem::Unreadable(format!(
                    "desteklenmeyen ACE türü: {other}"
                )));
            }
        }
        // SAFETY: Tür izin ACE'sidir; düzeni `ACCESS_ALLOWED_ACE`'dir.
        let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
        // SAFETY: SID, ACE içinde `SidStart` konumunda başlar ve ACE'nin sonuna kadar uzanır;
        // işaretçi ACE'nin başından türetilir ki bütün SID'i kapsasın.
        let sid = PSID(unsafe {
            ace.cast::<u8>()
                .add(std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart))
                .cast::<c_void>()
        });
        if allowed.Mask & WRITE_RIGHTS != 0 && !trusted(sid) {
            return Err(DirProblem::WritableByOthers);
        }
    }
    Ok(())
}

/// SDDL metniyle verilen güvenlik tanımlayıcısını değerlendirir (test ve tanılama için).
pub fn evaluate_security(sddl: &str) -> Result<(), DirProblem> {
    let sd = sd_from_sddl(sddl).map_err(|e| DirProblem::Unreadable(e.to_string()))?;
    evaluate_sd(sd.0)
}

pub fn is_reparse_point(path: &Path) -> std::io::Result<bool> {
    let w = wide(path.as_os_str());
    // SAFETY: `w` NUL ile biten geçerli bir UTF-16 yoldur ve çağrı boyunca yaşar.
    let attrs = unsafe { GetFileAttributesW(PCWSTR(w.as_ptr())) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(std::io::Error::last_os_error());
    }
    Ok(attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0)
}

/// Dizin yoksa korumalı DACL ile oluşturur (üst dizin var olmalı). Varsa dokunmaz ama
/// güvenilir olmasını ister: değilse `PermissionDenied` döner.
pub fn create_protected_dir(path: &Path, access: DirAccess) -> std::io::Result<()> {
    if path.exists() {
        return verify_protected_dir(path).map_err(|p| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("dizin korumalı değil ({p:?}): {}", path.display()),
            )
        });
    }
    // Sahip açıkça Administrators: yükseltilmemiş bir işlem bunu veremez ve oluşturma başarısız
    // olur (kapalı yönde hata).
    let sd = sd_from_sddl(&format!("O:BA{}", access.sddl()))?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
        lpSecurityDescriptor: sd.0.0,
        bInheritHandle: BOOL(0),
    };
    let w = wide(path.as_os_str());
    // SAFETY: Yol ve güvenlik öznitelikleri çağrı boyunca geçerlidir; `sd` sonra serbest kalır.
    unsafe { CreateDirectoryW(PCWSTR(w.as_ptr()), Some(&sa)) }.map_err(std::io::Error::other)
}

/// Var olan dizinin güvenilir olup olmadığını denetler: reparse noktası değil, sahibi
/// SYSTEM/Administrators, başkasına yazma hakkı yok.
pub fn verify_protected_dir(path: &Path) -> Result<(), DirProblem> {
    if !path.exists() {
        return Err(DirProblem::Missing);
    }
    if is_reparse_point(path).map_err(|e| DirProblem::Unreadable(e.to_string()))? {
        return Err(DirProblem::ReparsePoint);
    }
    let w = wide(path.as_os_str());
    let mut sd = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
    // SAFETY: Yol geçerli; çıktı güvenlik tanımlayıcısı `LocalSd` ile serbest bırakılır.
    let err = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(w.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut sd,
        )
    };
    let sd = LocalSd(sd);
    if err.0 != 0 {
        return Err(DirProblem::Unreadable(format!("Win32 hata {}", err.0)));
    }
    evaluate_sd(sd.0)
}

/// Açık tutulduğu sürece dizini sabitler: tutamaç `FILE_SHARE_DELETE` olmadan açılır, bu yüzden
/// dizin taşınamaz ya da silinemez. Reparse noktası ve dizin olmayan yol reddedilir.
pub struct DirPin {
    _handle: std::fs::File,
}

pub fn pin_dir(path: &Path) -> std::io::Result<DirPin> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    /// `FILE_SHARE_READ | FILE_SHARE_WRITE` (silme paylaşımı yok).
    const SHARE: u32 = 0x1 | 0x2;
    /// `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`.
    const FLAGS: u32 = 0x0200_0000 | 0x0020_0000;
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE)
        .custom_flags(FLAGS)
        .open(path)?;
    let meta = handle.metadata()?;
    if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("sabitlenecek yol düz bir dizin değil: {}", path.display()),
        ));
    }
    Ok(DirPin { _handle: handle })
}

/// `root`'tan `leaf`'e kadar her dizini denetler (ikisi dahil). Yalnızca yaprağı denetlemek
/// yetmez: üst dizinin sahibi bir kullanıcıysa yaprağı denetimden sonra bir bağlantıyla
/// değiştirebilir. `root`'un üstü (ör. `%ProgramData%`) denetlenmez; orada kullanıcılar dizin
/// oluşturabilir ama başkasının dizinini taşıyamaz.
pub fn verify_protected_chain(root: &Path, leaf: &Path) -> Result<(), DirProblem> {
    let rest = leaf
        .strip_prefix(root)
        .map_err(|_| DirProblem::Unreadable(format!("{} kökün altında değil", leaf.display())))?;
    let mut dir = root.to_path_buf();
    verify_protected_dir(&dir)?;
    for c in rest.components() {
        match c {
            std::path::Component::Normal(name) => {
                dir.push(name);
                verify_protected_dir(&dir)?;
            }
            _ => {
                return Err(DirProblem::Unreadable(format!(
                    "geçersiz yol bileşeni: {}",
                    leaf.display()
                )));
            }
        }
    }
    Ok(())
}
