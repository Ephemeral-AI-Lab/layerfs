use super::*;

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
