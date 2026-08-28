use super::*;

static DELETE_SERIAL: AtomicU64 = AtomicU64::new(0);

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
