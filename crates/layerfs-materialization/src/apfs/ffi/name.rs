use super::*;

pub(super) fn cname(name: &[u8]) -> io::Result<CString> {
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    CString::new(name).map_err(Into::into)
}
