//! The Windows privacy boundary of homes, discovery records and Stores:
//! owner-only DACLs, the parity of unix's 0700 directories and 0600 files.
//!
//! This is the crate's one owner-authorized module of raw Win32 calls,
//! allowed by the boundary guard and compiled on Windows only; no other
//! module in this crate may use unsafe Rust. A fresh entry is restricted to
//! its owner by a protected DACL whose single allow-ACE names the OWNER
//! RIGHTS SID (S-1-3-4): whoever owns the entry keeps full control and no
//! inherited grant survives, so a home created under a world-readable
//! parent leaks nothing (the unix arm reads the same intent from mode
//! bits). An existing entry is refused when its DACL grants access to
//! Everyone, Builtin\Users or Authenticated Users, or when it carries no
//! DACL at all (a null DACL is everyone-full).
#![allow(unsafe_code)]
use std::io;
use std::path::Path;

/// A freshly built well-known SID, `SECURITY_MAX_SID_SIZE` bytes.
fn well_known(kind: windows_sys::Win32::Security::WELL_KNOWN_SID_TYPE) -> io::Result<[u8; 68]> {
    use windows_sys::Win32::Security::{CreateWellKnownSid, PSID, SECURITY_MAX_SID_SIZE};
    let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = sid.len() as u32;
    if unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            sid.as_mut_ptr() as PSID,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}

/// Restricts `path` — file or directory — to its owner: one allow-ACE for
/// OWNER RIGHTS under a protected DACL; nothing is inherited.
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Authorization::{
        SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS, SE_FILE_OBJECT,
        TRUSTEE_IS_SID, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        WinCreatorOwnerRightsSid, ACL, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let mut sid = well_known(WinCreatorOwnerRightsSid)?;
    // The concrete all-file-rights mask, not GENERIC_ALL: a generic bit
    // written raw into the ACE is not mapped for the file object type, and
    // the owner's own write lands denied.
    const FILE_ALL_ACCESS: u32 = 0x001F_01FF;
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: 0,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: 0,
            // With TRUSTEE_IS_SID the name pointer is the SID itself.
            ptstrName: sid.as_mut_ptr().cast(),
        },
    };
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let built = unsafe { SetEntriesInAclW(1, &entry, std::ptr::null(), &mut dacl) };
    if built != 0 || dacl.is_null() {
        return Err(io::Error::other(format!(
            "the owner-only ACL cannot be built (win32 error {built:#x})"
        )));
    }
    // A verbatim spelling: the named-security calls accept \\?\ paths, and a
    // home beyond MAX_PATH must still be restrictable.
    let mut wide = layerfs_persistence::verbatim_wide(path);
    let set = unsafe {
        SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        )
    };
    unsafe { LocalFree(dacl as HLOCAL) };
    if set != 0 {
        return Err(io::Error::other(format!(
            "the owner-only ACL cannot be set on {}: win32 error {set:#x}",
            path.display()
        )));
    }
    Ok(())
}

/// Refuses `path` when another local user can read it: its DACL allows
/// Everyone, Builtin\Users or Authenticated Users, or it has no DACL.
pub fn require_owner_only(path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorDacl,
        WinAuthenticatedUserSid, WinBuiltinUsersSid, WinWorldSid, ACL, DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID,
    };
    let shared = |path: &Path| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "{} grants access beyond its owner; the Store's privacy \
                 boundary needs an owner-only entry",
                path.display()
            ),
        )
    };
    let mut wide = layerfs_persistence::verbatim_wide(path);
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let read = unsafe {
        GetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if read != 0 {
        return Err(io::Error::other(format!(
            "the ACL of {} cannot be read: win32 error {read:#x}",
            path.display()
        )));
    }
    // The ACL points into the descriptor; read every ACE before freeing it.
    let verdict = (|| {
        let mut present = 0;
        let mut entry: *mut ACL = std::ptr::null_mut();
        let mut defaulted = 0;
        if unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut entry, &mut defaulted)
        } == 0
        {
            return Err(io::Error::other(
                "the security descriptor has no DACL field",
            ));
        }
        if present == 0 || entry.is_null() {
            return Err(shared(path));
        }
        let mut size = windows_sys::Win32::Security::ACL_SIZE_INFORMATION::default();
        if unsafe {
            GetAclInformation(
                entry,
                &mut size as *mut windows_sys::Win32::Security::ACL_SIZE_INFORMATION
                    as *mut core::ffi::c_void,
                std::mem::size_of::<windows_sys::Win32::Security::ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let broad = [
            well_known(WinWorldSid)?,
            well_known(WinBuiltinUsersSid)?,
            well_known(WinAuthenticatedUserSid)?,
        ];
        for index in 0..size.AceCount {
            let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
            if unsafe { GetAce(entry, index, &mut ace) } == 0 || ace.is_null() {
                return Err(io::Error::last_os_error());
            }
            // repr(C) ACCESS_ALLOWED_ACE: 4-byte header, u32 mask, then the
            // SID; only an allow ACE (type 0) can widen access.
            let header = unsafe { *(ace as *const windows_sys::Win32::Security::ACE_HEADER) };
            if header.AceType != 0 {
                continue;
            }
            let sid = unsafe { ace.byte_add(8) } as PSID;
            if broad
                .iter()
                .any(|bytes| unsafe { EqualSid(sid, bytes.as_ptr() as PSID) } != 0)
            {
                return Err(shared(path));
            }
        }
        Ok(())
    })();
    unsafe { LocalFree(descriptor as HLOCAL) };
    verdict
}
