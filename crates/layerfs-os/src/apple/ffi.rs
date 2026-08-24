//! Reviewed unsafe syscall boundary.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Component;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use layerfs_vfs::driver::NativeKind;

static DELETE_SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn try_lock_exclusive(file: &File) -> io::Result<bool> {
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

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

pub fn list_xattrs_file(file: &File) -> io::Result<Vec<Vec<u8>>> {
    let size = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    if size as usize > layerfs_vfs::driver::MAX_NATIVE_XATTR_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "xattr name list exceeds AppleWorkspaceV1 ceiling",
        ));
    }
    let mut bytes = vec![0_u8; size as usize];
    if size > 0 {
        let read = unsafe {
            libc::flistxattr(file.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len(), 0)
        };
        if read < 0 {
            return Err(io::Error::last_os_error());
        }
        bytes.truncate(read as usize);
    }
    Ok(bytes
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(<[u8]>::to_vec)
        .collect())
}

pub fn get_xattr_file(file: &File, name: &[u8]) -> io::Result<Vec<u8>> {
    let name = CString::new(name)?;
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    if size as usize > 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "xattr exceeds AppleWorkspaceV1 buffer ceiling",
        ));
    }
    let mut bytes = vec![0_u8; size as usize];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            0,
            0,
        )
    };
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(read as usize);
    Ok(bytes)
}

pub fn set_xattr_file(file: &File, name: &[u8], value: &[u8]) -> io::Result<()> {
    let name = CString::new(name)?;
    if unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn remove_xattr_file(file: &File, name: &[u8]) -> io::Result<()> {
    let name = CString::new(name)?;
    if unsafe { libc::fremovexattr(file.as_raw_fd(), name.as_ptr(), 0) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn flags_file(file: &File) -> io::Result<u32> {
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { stat.assume_init() }.st_flags)
}

pub fn set_flags_file(file: &File, flags: u32) -> io::Result<()> {
    if unsafe { libc::fchflags(file.as_raw_fd(), flags) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn set_mode_file(file: &File, mode: libc::mode_t) -> io::Result<()> {
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn set_mode_at(parent: &File, name: &[u8], mode: libc::mode_t) -> io::Result<()> {
    let name = cname(name)?;
    if unsafe { libc::fchmodat(parent.as_raw_fd(), name.as_ptr(), mode, 0) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
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

fn cpath(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(Into::into)
}

pub fn open_directory_at(parent: &File, name: &[u8]) -> io::Result<File> {
    open_at(
        parent,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
    )
}

pub fn open_directory_path_nofollow(path: &Path) -> io::Result<File> {
    let mut directory = File::open(if path.is_absolute() { "/" } else { "." })?;
    for component in path.components() {
        directory = match component {
            Component::RootDir | Component::CurDir => directory,
            Component::ParentDir => open_directory_at(&directory, b"..")?,
            Component::Normal(name) => open_directory_at(&directory, name.as_bytes())?,
            Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unsupported path prefix",
                ))
            }
        };
    }
    Ok(directory)
}

pub fn open_regular_at(parent: &File, name: &[u8], writable: bool) -> io::Result<File> {
    open_at(
        parent,
        name,
        (if writable {
            libc::O_RDWR
        } else {
            libc::O_RDONLY
        }) | libc::O_CLOEXEC
            | libc::O_NOFOLLOW,
    )
}

pub fn open_entry_at(parent: &File, name: &[u8]) -> io::Result<File> {
    open_at(
        parent,
        name,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_SYMLINK,
    )
}

pub fn available_bytes(path: &Path) -> io::Result<u64> {
    let path = cpath(path)?;
    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    stat.f_bavail
        .checked_mul(u64::from(stat.f_bsize))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "available-space overflow"))
}

pub fn create_regular_at(parent: &File, name: &[u8]) -> io::Result<File> {
    let name = cname(name)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

pub fn clone_file_at(source: &File, staging: &File, name: &[u8]) -> io::Result<File> {
    let name = cname(name)?;
    if unsafe { libc::fclonefileat(source.as_raw_fd(), staging.as_raw_fd(), name.as_ptr(), 0) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    open_at(
        staging,
        name.as_bytes(),
        libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
    )
}

pub fn unlink_if_identity_at(parent: &File, name: &[u8], expected: &[u8]) -> io::Result<()> {
    match stable_token_at(parent, name) {
        Ok(actual) if actual == expected => {}
        Ok(_) => return Err(io::Error::from_raw_os_error(libc::ESTALE)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(()),
        Err(error) => return Err(error),
    }
    let name = cname(name)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn open_at(parent: &File, name: &[u8], flags: libc::c_int) -> io::Result<File> {
    let name = cname(name)?;
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

pub fn mkdir_at(parent: &File, name: &[u8]) -> io::Result<()> {
    let name = cname(name)?;
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn remove_directory_at(parent: &File, name: &[u8]) -> io::Result<()> {
    let name = cname(name)?;
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn remove_directory_if_identity_at(
    parent: &File,
    name: &[u8],
    expected: &[u8],
) -> io::Result<()> {
    if stable_token_at(parent, name)? != expected {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    remove_directory_at(parent, name)
}

pub fn remove_owned_tree(
    root: &File,
    parent: &File,
    name: &[u8],
    expected: &[u8],
) -> io::Result<()> {
    let mut entries = directory_entries(root)?;
    for entry in entries.by_ref() {
        let (child_name, kind, _, _, stable) = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if stable_token_at(root, &child_name)? != stable {
            return Err(io::Error::from_raw_os_error(libc::ESTALE));
        }
        if kind != NativeKind::Symlink {
            let mode = if kind == NativeKind::Directory {
                0o700
            } else {
                0o600
            };
            if let Err(error) = set_mode_at(root, &child_name, mode) {
                if error.raw_os_error() != Some(libc::EPERM) {
                    return Err(error);
                }
            }
        }
        let control = open_cleanup_entry_at(root, &child_name, kind)
            .map_err(|error| io_context("open private child", error))?;
        if file_stable_token(&control)? != stable {
            return Err(io::Error::from_raw_os_error(libc::ESTALE));
        }
        set_flags_file(&control, 0).map_err(|error| io_context("clear child flags", error))?;
        set_acl_file(&control, None).map_err(|error| io_context("clear child ACL", error))?;
        if kind == NativeKind::Directory {
            set_mode_file(&control, 0o700)
                .map_err(|error| io_context("restore child directory mode", error))?;
            let child = open_directory_at(root, &child_name)?;
            if file_stable_token(&child)? != stable {
                return Err(io::Error::from_raw_os_error(libc::ESTALE));
            }
            let tombstone = quarantine_at(root, &child_name, &stable)?;
            remove_owned_tree(&child, root, &tombstone, &stable)?;
        } else {
            if kind == NativeKind::RegularFile {
                set_mode_file(&control, 0o600)?;
            }
            let tombstone = quarantine_at(root, &child_name, &stable)?;
            unlink_if_identity_at(root, &tombstone, &stable)?;
        }
    }
    drop(entries);
    remove_directory_if_identity_at(parent, name, expected)
}

fn open_cleanup_entry_at(parent: &File, name: &[u8], kind: NativeKind) -> io::Result<File> {
    open_at(
        parent,
        name,
        libc::O_CLOEXEC
            | if kind == NativeKind::Symlink {
                libc::O_RDONLY | libc::O_SYMLINK
            } else {
                libc::O_EVTONLY | libc::O_NOFOLLOW
            },
    )
}

fn quarantine_at(parent: &File, name: &[u8], expected: &[u8]) -> io::Result<Vec<u8>> {
    for _ in 0..64 {
        let tombstone = format!(
            ".layerfs-child-tombstone-{}-{}",
            std::process::id(),
            DELETE_SERIAL.fetch_add(1, Ordering::Relaxed)
        )
        .into_bytes();
        match rename_at(parent, name, parent, &tombstone) {
            Ok(()) => match stable_token_at(parent, &tombstone) {
                Ok(actual) if actual == expected => return Ok(tombstone),
                _ => {
                    let _ = rename_at(parent, &tombstone, parent, name);
                    return Err(io::Error::from_raw_os_error(libc::ESTALE));
                }
            },
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "owned child tombstone collision",
    ))
}

pub fn detach_and_remove_owned_tree(
    root: &File,
    parent: &File,
    name: &[u8],
    tombstone: &[u8],
    expected: &[u8],
) -> io::Result<()> {
    if file_stable_token(root)? != expected || stable_token_at(parent, name)? != expected {
        return Err(io::Error::from_raw_os_error(libc::ESTALE));
    }
    let flags = flags_file(root)?;
    let acl = acl_file(root)?;
    set_flags_file(root, 0).map_err(|error| io_context("clear root flags", error))?;
    set_acl_file(root, None).map_err(|error| io_context("clear root ACL", error))?;
    if let Err(error) = rename_at(parent, name, parent, tombstone) {
        let _ = set_flags_file(root, flags);
        let _ = set_acl_file(root, acl.as_deref());
        return Err(error);
    }
    match stable_token_at(parent, tombstone) {
        Ok(actual) if actual == expected => {
            set_mode_file(root, 0o700).map_err(|error| io_context("restore root mode", error))?;
            set_acl_file(root, None)
                .map_err(|error| io_context("clear detached root ACL", error))?;
            remove_owned_tree(root, parent, tombstone, expected)
        }
        _ => {
            let _ = rename_at(parent, tombstone, parent, name);
            let _ = set_flags_file(root, flags);
            let _ = set_acl_file(root, acl.as_deref());
            Err(io::Error::from_raw_os_error(libc::ESTALE))
        }
    }
}

fn io_context(step: &'static str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{step}: {error}"))
}

pub fn read_link_at(parent: &File, name: &[u8]) -> io::Result<Vec<u8>> {
    let name = cname(name)?;
    let mut bytes = vec![0_u8; libc::PATH_MAX as usize];
    let read = unsafe {
        libc::readlinkat(
            parent.as_raw_fd(),
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(read as usize);
    Ok(bytes)
}

pub fn token_at(parent: &File, name: &[u8]) -> io::Result<Vec<u8>> {
    let name = cname(name)?;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(stat_token(&unsafe { stat.assume_init() }))
}

pub fn stable_token_at(parent: &File, name: &[u8]) -> io::Result<Vec<u8>> {
    let name = cname(name)?;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(stat_identity(&unsafe { stat.assume_init() }))
}

pub fn file_stable_token(file: &File) -> io::Result<Vec<u8>> {
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(stat_identity(&unsafe { stat.assume_init() }))
}

pub fn file_token(file: &File) -> io::Result<Vec<u8>> {
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(stat_token(&unsafe { stat.assume_init() }))
}

pub fn entry_at(parent: &File, name: &[u8]) -> io::Result<(NativeKind, u64, Vec<u8>, Vec<u8>)> {
    let name = cname(name)?;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let kind = match stat.st_mode & libc::S_IFMT {
        libc::S_IFDIR => NativeKind::Directory,
        libc::S_IFREG => NativeKind::RegularFile,
        libc::S_IFLNK => NativeKind::Symlink,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "unsupported entry kind",
            ))
        }
    };
    Ok((
        kind,
        u64::from(stat.st_nlink),
        stat_token(&stat),
        stat_identity(&stat),
    ))
}

fn stat_identity(stat: &libc::stat) -> Vec<u8> {
    let mut token = Vec::with_capacity(16);
    token.extend_from_slice(&stat.st_dev.to_be_bytes());
    token.extend_from_slice(&stat.st_ino.to_be_bytes());
    token
}

fn stat_token(stat: &libc::stat) -> Vec<u8> {
    let mut token = Vec::with_capacity(80);
    token.extend_from_slice(&stat.st_dev.to_be_bytes());
    token.extend_from_slice(&stat.st_ino.to_be_bytes());
    token.extend_from_slice(&stat.st_mode.to_be_bytes());
    token.extend_from_slice(&stat.st_nlink.to_be_bytes());
    token.extend_from_slice(&stat.st_size.to_be_bytes());
    token.extend_from_slice(&stat.st_mtime.to_be_bytes());
    token.extend_from_slice(&stat.st_mtime_nsec.to_be_bytes());
    token.extend_from_slice(&stat.st_ctime.to_be_bytes());
    token.extend_from_slice(&stat.st_ctime_nsec.to_be_bytes());
    token.extend_from_slice(&stat.st_flags.to_be_bytes());
    token
}

pub struct DirectoryEntries {
    directory: *mut libc::DIR,
    parent: File,
    done: bool,
}

impl Iterator for DirectoryEntries {
    type Item = io::Result<(Vec<u8>, NativeKind, u64, Vec<u8>, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            unsafe { *libc::__error() = 0 };
            let entry = unsafe { libc::readdir(self.directory) };
            if entry.is_null() {
                self.done = true;
                let errno = unsafe { *libc::__error() };
                return (errno != 0).then(|| Err(io::Error::from_raw_os_error(errno)));
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            return Some(
                entry_at(&self.parent, name).map(|(kind, links, token, stable)| {
                    (name.to_vec(), kind, links, token, stable)
                }),
            );
        }
    }
}

impl Drop for DirectoryEntries {
    fn drop(&mut self) {
        unsafe { libc::closedir(self.directory) };
    }
}

pub fn directory_entries(parent: &File) -> io::Result<DirectoryEntries> {
    let fd = unsafe { libc::fcntl(parent.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let directory = unsafe { libc::fdopendir(fd) };
    if directory.is_null() {
        unsafe { libc::close(fd) };
        return Err(io::Error::last_os_error());
    }
    unsafe { libc::rewinddir(directory) };
    Ok(DirectoryEntries {
        directory,
        parent: parent.try_clone()?,
        done: false,
    })
}

pub fn hard_link_at(
    source_parent: &File,
    source: &[u8],
    target_parent: &File,
    target: &[u8],
) -> io::Result<()> {
    let source = cname(source)?;
    let target = cname(target)?;
    if unsafe {
        libc::linkat(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            target_parent.as_raw_fd(),
            target.as_ptr(),
            0,
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn symlink_at(parent: &File, name: &[u8], target: &[u8]) -> io::Result<()> {
    let name = cname(name)?;
    let target = CString::new(target)?;
    if unsafe { libc::symlinkat(target.as_ptr(), parent.as_raw_fd(), name.as_ptr()) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn rename_at(
    source_parent: &File,
    source: &[u8],
    target_parent: &File,
    target: &[u8],
) -> io::Result<()> {
    let source = cname(source)?;
    let target = cname(target)?;
    const RENAME_EXCL_NOFOLLOW_ANY: u32 = 0x04 | 0x10;
    if unsafe {
        libc::renameatx_np(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            target_parent.as_raw_fd(),
            target.as_ptr(),
            RENAME_EXCL_NOFOLLOW_ANY,
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn replace_at(
    source_parent: &File,
    source: &[u8],
    target_parent: &File,
    target: &[u8],
) -> io::Result<()> {
    let source = cname(source)?;
    let target = cname(target)?;
    const RENAME_NOFOLLOW_ANY: u32 = 0x10;
    if unsafe {
        libc::renameatx_np(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            target_parent.as_raw_fd(),
            target.as_ptr(),
            RENAME_NOFOLLOW_ANY,
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn set_symlink_mtime_at(
    parent: &File,
    name: &[u8],
    seconds: i64,
    nanoseconds: u32,
) -> io::Result<()> {
    let name = cname(name)?;
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: seconds,
            tv_nsec: nanoseconds.into(),
        },
    ];
    if unsafe {
        libc::utimensat(
            parent.as_raw_fd(),
            name.as_ptr(),
            times.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } < 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn cname(name: &[u8]) -> io::Result<CString> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    CString::new(name).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    #[test]
    fn entry_token_changes_for_an_in_place_live_writer() {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-token-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("file"), b"before").unwrap();
        let parent = File::open(&directory).unwrap();
        let before = token_at(&parent, b"file").unwrap();
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(directory.join("file"))
            .unwrap()
            .write_all(b"after-content")
            .unwrap();
        let after = token_at(&parent, b"file").unwrap();
        assert_ne!(before, after);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn verified_tombstone_cleanup_clears_immutable_and_append_flags() {
        let base = std::env::temp_dir().join(format!(
            "layerfs-flags-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        fs::create_dir(base.join("owned")).unwrap();
        fs::write(base.join("owned/file"), b"content").unwrap();
        let parent = File::open(&base).unwrap();
        let root = open_directory_at(&parent, b"owned").unwrap();
        let child = open_entry_at(&root, b"file").unwrap();
        set_flags_file(&child, 0x0000_0006).unwrap();
        set_flags_file(&root, 0x0000_0006).unwrap();
        let identity = file_stable_token(&root).unwrap();
        detach_and_remove_owned_tree(&root, &parent, b"owned", b"private-tombstone", &identity)
            .unwrap();
        assert!(!base.join("owned").exists());
        assert!(!base.join("private-tombstone").exists());
        fs::remove_dir(base).unwrap();
    }

    #[test]
    fn apfs_clone_temp_supports_independent_same_offset_patch() {
        let base = std::env::temp_dir().join(format!(
            "layerfs-clone-patch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        fs::create_dir(base.join("staging")).unwrap();
        fs::write(base.join("source"), vec![0x5a; 1024 * 1024]).unwrap();
        let source = File::open(base.join("source")).unwrap();
        let staging = File::open(base.join("staging")).unwrap();
        let mut cloned = clone_file_at(&source, &staging, b"clone").unwrap();
        use std::io::{Seek, SeekFrom};
        cloned.seek(SeekFrom::Start(4096)).unwrap();
        cloned.write_all(b"PATCH").unwrap();
        cloned.sync_all().unwrap();
        assert_eq!(
            &fs::read(base.join("source")).unwrap()[4096..4101],
            &[0x5a; 5]
        );
        assert_eq!(
            &fs::read(base.join("staging/clone")).unwrap()[4096..4101],
            b"PATCH"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn private_cleanup_neutralizes_mode_zero_and_deny_delete_acl() {
        let base = std::env::temp_dir().join(format!(
            "layerfs-restrictive-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&base).unwrap();
        fs::create_dir_all(base.join("owned/child")).unwrap();
        fs::write(base.join("owned/child/file"), b"content").unwrap();
        assert!(Command::new("chmod")
            .args(["+a", "everyone deny delete"])
            .arg(base.join("owned"))
            .status()
            .unwrap()
            .success());
        assert!(Command::new("chmod")
            .args(["+a", "everyone deny delete_child"])
            .arg(base.join("owned/child"))
            .status()
            .unwrap()
            .success());
        let parent = File::open(&base).unwrap();
        let root = open_directory_at(&parent, b"owned").unwrap();
        fs::set_permissions(
            base.join("owned/child/file"),
            fs::Permissions::from_mode(0o000),
        )
        .unwrap();
        fs::set_permissions(base.join("owned/child"), fs::Permissions::from_mode(0o000)).unwrap();
        fs::set_permissions(base.join("owned"), fs::Permissions::from_mode(0o000)).unwrap();
        let identity = file_stable_token(&root).unwrap();
        detach_and_remove_owned_tree(&root, &parent, b"owned", b"private-tombstone", &identity)
            .unwrap();
        assert!(!base.join("owned").exists());
        assert!(!base.join("private-tombstone").exists());
        fs::remove_dir(base).unwrap();
    }
}
