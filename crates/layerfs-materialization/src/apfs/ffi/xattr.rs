use super::*;

pub struct XattrNames {
    pub(super) bytes: Vec<u8>,
    pub(super) offsets: Vec<Vec<u32>>,
    pub(super) count: usize,
}

impl XattrNames {
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        (0..self.count).map(|index| name_at(&self.bytes, self.offset(index)))
    }

    pub fn sort(&mut self) {
        let count = self.count;
        for root in (0..count / 2).rev() {
            self.sift_down(root, count);
        }
        for end in (1..count).rev() {
            self.swap(0, end);
            self.sift_down(0, end);
        }
    }

    fn offset(&self, index: usize) -> usize {
        const OFFSETS_PER_CHUNK: usize = crate::driver::MAX_NATIVE_XATTR_BYTES / 4;
        self.offsets[index / OFFSETS_PER_CHUNK][index % OFFSETS_PER_CHUNK] as usize
    }

    fn swap(&mut self, left: usize, right: usize) {
        const OFFSETS_PER_CHUNK: usize = crate::driver::MAX_NATIVE_XATTR_BYTES / 4;
        let left_value = self.offset(left) as u32;
        let right_value = self.offset(right) as u32;
        self.offsets[left / OFFSETS_PER_CHUNK][left % OFFSETS_PER_CHUNK] = right_value;
        self.offsets[right / OFFSETS_PER_CHUNK][right % OFFSETS_PER_CHUNK] = left_value;
    }

    fn sift_down(&mut self, mut root: usize, end: usize) {
        loop {
            let child = root * 2 + 1;
            if child >= end {
                return;
            }
            let mut maximum = child;
            if child + 1 < end
                && name_at(&self.bytes, self.offset(child))
                    < name_at(&self.bytes, self.offset(child + 1))
            {
                maximum = child + 1;
            }
            if name_at(&self.bytes, self.offset(root)) >= name_at(&self.bytes, self.offset(maximum))
            {
                return;
            }
            self.swap(root, maximum);
            root = maximum;
        }
    }
}

fn name_at(bytes: &[u8], offset: usize) -> &[u8] {
    let tail = &bytes[offset..];
    &tail[..tail
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(tail.len())]
}

pub fn list_xattrs_file(file: &File) -> io::Result<XattrNames> {
    let size = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    if size as usize > crate::driver::MAX_NATIVE_XATTR_BYTES {
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
    const OFFSETS_PER_CHUNK: usize = crate::driver::MAX_NATIVE_XATTR_BYTES / 4;
    let total_names = bytes
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .count();
    let mut offsets = Vec::new();
    let mut count = 0_usize;
    let mut start = 0_usize;
    while start < bytes.len() {
        let name = name_at(&bytes, start);
        if name.is_empty() {
            start += 1;
            continue;
        }
        if offsets
            .last()
            .is_none_or(|chunk: &Vec<u32>| chunk.len() == OFFSETS_PER_CHUNK)
        {
            offsets.push(Vec::with_capacity(
                total_names.saturating_sub(count).min(OFFSETS_PER_CHUNK),
            ));
        }
        offsets
            .last_mut()
            .unwrap()
            .push(u32::try_from(start).map_err(|_| io::ErrorKind::FileTooLarge)?);
        count += 1;
        start = start
            .checked_add(name.len() + 1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
    }
    Ok(XattrNames {
        bytes,
        offsets,
        count,
    })
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
