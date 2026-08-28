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

use crate::driver::NativeKind;

mod acl;
mod cleanup;
mod file_metadata;
mod identity;
mod lock;
mod mutation;
mod name;
mod open;
mod xattr;

pub use acl::{acl_file, set_acl_file, validate_acl};
pub use cleanup::{detach_and_remove_owned_tree, remove_owned_tree};
pub use file_metadata::{flags_file, set_flags_file};
#[allow(unused_imports)]
pub use identity::{
    directory_entries, entry_at, file_stable_token, file_token, read_link_at, stable_token_at,
    token_at, DirectoryEntries,
};
pub use lock::try_lock_exclusive;
pub use mutation::{hard_link_at, rename_at, replace_at, set_symlink_mtime_at, symlink_at};
#[allow(unused_imports)]
pub use open::{
    clone_file_at, create_regular_at, mkdir_at, open_directory_at, open_directory_path_nofollow,
    open_entry_at, open_regular_at, remove_directory_at, remove_directory_if_identity_at,
    unlink_if_identity_at,
};
pub use xattr::{get_xattr_file, list_xattrs_file, remove_xattr_file, set_xattr_file, XattrNames};

use file_metadata::{set_mode_at, set_mode_file};
use name::cname;
use open::open_at;

#[cfg(test)]
mod tests;
