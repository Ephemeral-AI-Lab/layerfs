use super::*;

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

pub(super) fn set_mode_file(file: &File, mode: libc::mode_t) -> io::Result<()> {
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(super) fn set_mode_at(parent: &File, name: &[u8], mode: libc::mode_t) -> io::Result<()> {
    let name = cname(name)?;
    if unsafe { libc::fchmodat(parent.as_raw_fd(), name.as_ptr(), mode, 0) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
