use super::*;

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
