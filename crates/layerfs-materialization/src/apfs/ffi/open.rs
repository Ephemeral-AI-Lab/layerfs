use super::*;

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

pub(super) fn open_at(parent: &File, name: &[u8], flags: libc::c_int) -> io::Result<File> {
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
