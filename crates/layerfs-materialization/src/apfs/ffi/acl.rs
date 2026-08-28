use super::*;

const ACL_TYPE_EXTENDED: i32 = 0x100;
const ACL_FIRST_ENTRY: i32 = 0;
const ACL_NEXT_ENTRY: i32 = -1;
const ACL_ENTRY_FLAGS: &[i32] = &[1 << 4, 1 << 5, 1 << 6, 1 << 7, 1 << 8];
const ACL_DEFER_INHERIT: i32 = 1;
const ACL_NO_INHERIT: i32 = 1 << 17;
const ACL_RIGHTS_MASK: u64 = 0x0010_3ffe;
const ACL_FLAGS_MASK: u64 = 0x0002_01f0;

type Acl = *mut libc::c_void;
type AclEntry = *mut libc::c_void;
type AclFlagset = *mut libc::c_void;

unsafe extern "C" {
    fn acl_free(object: *mut libc::c_void) -> libc::c_int;
    fn acl_init(count: libc::c_int) -> Acl;
    fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> Acl;
    fn acl_set_fd_np(fd: libc::c_int, acl: Acl, kind: libc::c_int) -> libc::c_int;
    fn acl_get_entry(acl: Acl, entry_id: libc::c_int, entry: *mut AclEntry) -> libc::c_int;
    fn acl_create_entry(acl: *mut Acl, entry: *mut AclEntry) -> libc::c_int;
    fn acl_get_tag_type(entry: AclEntry, tag: *mut libc::c_int) -> libc::c_int;
    fn acl_set_tag_type(entry: AclEntry, tag: libc::c_int) -> libc::c_int;
    fn acl_get_qualifier(entry: AclEntry) -> *mut libc::c_void;
    fn acl_set_qualifier(entry: AclEntry, qualifier: *const libc::c_void) -> libc::c_int;
    fn acl_get_permset_mask_np(entry: AclEntry, mask: *mut u64) -> libc::c_int;
    fn acl_set_permset_mask_np(entry: AclEntry, mask: u64) -> libc::c_int;
    fn acl_get_flagset_np(object: *mut libc::c_void, flags: *mut AclFlagset) -> libc::c_int;
    fn acl_get_flag_np(flags: AclFlagset, flag: libc::c_int) -> libc::c_int;
    fn acl_clear_flags_np(flags: AclFlagset) -> libc::c_int;
    fn acl_add_flag_np(flags: AclFlagset, flag: libc::c_int) -> libc::c_int;
    fn acl_set_flagset_np(object: *mut libc::c_void, flags: AclFlagset) -> libc::c_int;
}

struct OwnedAcl(Acl);

impl Drop for OwnedAcl {
    fn drop(&mut self) {
        unsafe { acl_free(self.0) };
    }
}

pub fn acl_file(file: &File) -> io::Result<Option<Vec<u8>>> {
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
    encode_acl(acl)
}

fn encode_acl(acl: Acl) -> io::Result<Option<Vec<u8>>> {
    if acl.is_null() {
        let error = io::Error::last_os_error();
        return if matches!(error.raw_os_error(), Some(code) if code == libc::ENOENT || code == libc::ENOATTR)
        {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let acl = OwnedAcl(acl);
    let mut acl_flags = std::ptr::null_mut();
    if unsafe { acl_get_flagset_np(acl.0, &mut acl_flags) } < 0 {
        return Err(io::Error::last_os_error());
    }
    match unsafe { acl_get_flag_np(acl_flags, ACL_DEFER_INHERIT) } {
        0 => {}
        1 => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported deferred-inherit ACL flag",
            ))
        }
        _ => return Err(io::Error::last_os_error()),
    }
    let no_inherit = match unsafe { acl_get_flag_np(acl_flags, ACL_NO_INHERIT) } {
        0 => false,
        1 => true,
        _ => return Err(io::Error::last_os_error()),
    };
    let mut entries: Vec<AclTuple> = Vec::new();
    let mut entry = std::ptr::null_mut();
    let mut selector = ACL_FIRST_ENTRY;
    loop {
        match unsafe { acl_get_entry(acl.0, selector, &mut entry) } {
            0 => {}
            _ => {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::EINVAL) {
                    break;
                }
                return Err(error);
            }
        }
        selector = ACL_NEXT_ENTRY;
        if entries.len() == 128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ACL exceeds 128 entries",
            ));
        }
        let mut tag = 0;
        if unsafe { acl_get_tag_type(entry, &mut tag) } < 0 || !matches!(tag, 1 | 2) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported ACL tag",
            ));
        }
        let qualifier = unsafe { acl_get_qualifier(entry) };
        if qualifier.is_null() {
            return Err(io::Error::last_os_error());
        }
        let uuid: [u8; 16] = unsafe { std::slice::from_raw_parts(qualifier.cast::<u8>(), 16) }
            .try_into()
            .unwrap();
        unsafe { acl_free(qualifier) };
        let mut rights = 0_u64;
        if unsafe { acl_get_permset_mask_np(entry, &mut rights) } < 0
            || rights & !ACL_RIGHTS_MASK != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported ACL rights",
            ));
        }
        let mut flagset = std::ptr::null_mut();
        if unsafe { acl_get_flagset_np(entry, &mut flagset) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut flags = if no_inherit { ACL_NO_INHERIT as u64 } else { 0 };
        for &flag in ACL_ENTRY_FLAGS {
            match unsafe { acl_get_flag_np(flagset, flag) } {
                1 => flags |= flag as u64,
                0 => {}
                _ => return Err(io::Error::last_os_error()),
            }
        }
        entries.push((tag as u8, flags, rights, uuid));
    }
    if entries.is_empty() {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(12 + 36 * entries.len());
    bytes.extend_from_slice(b"LFS4ACL\0");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for (tag, flags, rights, uuid) in entries {
        bytes.extend_from_slice(&[tag, 1, 0, 0]);
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes.extend_from_slice(&rights.to_be_bytes());
        bytes.extend_from_slice(&uuid);
    }
    Ok(Some(bytes))
}

pub fn set_acl_file(file: &File, canonical: Option<&[u8]>) -> io::Result<()> {
    if canonical.is_none() && acl_file(file)?.is_none() {
        return Ok(());
    }
    let acl = build_acl(canonical)?;
    if unsafe { acl_set_fd_np(file.as_raw_fd(), acl.0, ACL_TYPE_EXTENDED) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn validate_acl(canonical: Option<&[u8]>) -> io::Result<()> {
    if canonical.is_some() {
        drop(build_acl(canonical)?);
    }
    Ok(())
}

fn build_acl(canonical: Option<&[u8]>) -> io::Result<OwnedAcl> {
    let entries = canonical.map(decode_acl).transpose()?.unwrap_or_default();
    let acl = unsafe { acl_init(entries.len() as libc::c_int) };
    if acl.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut acl = OwnedAcl(acl);
    let no_inherit = entries
        .first()
        .is_some_and(|entry| entry.1 & ACL_NO_INHERIT as u64 != 0);
    if entries
        .iter()
        .any(|entry| (entry.1 & ACL_NO_INHERIT as u64 != 0) != no_inherit)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "inconsistent ACL-level no-inherit flag",
        ));
    }
    for (tag, flags, rights, uuid) in entries {
        let mut entry = std::ptr::null_mut();
        if unsafe { acl_create_entry(&mut acl.0, &mut entry) } < 0
            || unsafe { acl_set_tag_type(entry, tag as libc::c_int) } < 0
            || unsafe { acl_set_qualifier(entry, uuid.as_ptr().cast()) } < 0
            || unsafe { acl_set_permset_mask_np(entry, rights) } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut flagset = std::ptr::null_mut();
        if unsafe { acl_get_flagset_np(entry, &mut flagset) } < 0
            || unsafe { acl_clear_flags_np(flagset) } < 0
        {
            return Err(io::Error::last_os_error());
        }
        for &flag in ACL_ENTRY_FLAGS {
            if flags & flag as u64 != 0 && unsafe { acl_add_flag_np(flagset, flag) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if unsafe { acl_set_flagset_np(entry, flagset) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let mut flags = std::ptr::null_mut();
    if unsafe { acl_get_flagset_np(acl.0, &mut flags) } < 0
        || unsafe { acl_clear_flags_np(flags) } < 0
        || (no_inherit && unsafe { acl_add_flag_np(flags, ACL_NO_INHERIT) } < 0)
        || unsafe { acl_set_flagset_np(acl.0, flags) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(acl)
}

type AclTuple = (u8, u64, u64, [u8; 16]);

fn decode_acl(bytes: &[u8]) -> io::Result<Vec<AclTuple>> {
    if bytes.len() < 12
        || &bytes[..8] != b"LFS4ACL\0"
        || &bytes[8..10] != 1_u16.to_be_bytes().as_slice()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid ACL framing",
        ));
    }
    let count = usize::from(u16::from_be_bytes([bytes[10], bytes[11]]));
    if count == 0 || count > 128 || bytes.len() != 12 + 36 * count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid ACL length",
        ));
    }
    bytes[12..]
        .chunks_exact(36)
        .map(|entry| {
            let tag = entry[0];
            let flags = u64::from_be_bytes(entry[4..12].try_into().unwrap());
            let rights = u64::from_be_bytes(entry[12..20].try_into().unwrap());
            if !matches!(tag, 1 | 2)
                || entry[1] != 1
                || entry[2..4] != [0, 0]
                || flags & !ACL_FLAGS_MASK != 0
                || rights & !ACL_RIGHTS_MASK != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsupported ACL entry",
                ));
            }
            Ok((tag, flags, rights, entry[20..36].try_into().unwrap()))
        })
        .collect()
}
