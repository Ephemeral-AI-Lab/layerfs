//! Real kernel mount: `rewinddir` on an open directory retires the offsets
//! `telldir` returned before it. A `seekdir` to one of them is answered
//! `EINVAL`; positions taken since still resume strictly after their name.
//! Ordinary libc calls of a process the daemon never registered.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use mutating::{harness, quiet, unmounted};
use nix::{errno::Errno, libc};
use std::{
    ffi::{CStr, CString},
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
};

/// Names of the listed directory: several READDIR replies of 64 names.
const NAMES: usize = 300;

struct Stream(*mut libc::DIR);
impl Stream {
    fn open(path: &Path) -> Self {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path; the stream is closed in Drop.
        let stream = unsafe { libc::opendir(path.as_ptr()) };
        assert!(!stream.is_null(), "opendir: {}", Errno::last());
        Self(stream)
    }
    /// The next name that is not a dot entry, or the error of the call.
    fn next(&mut self) -> Result<Option<Vec<u8>>, Errno> {
        loop {
            Errno::clear();
            // SAFETY: the stream is open; the entry is copied before the
            // next call on it.
            let entry = unsafe { libc::readdir(self.0) };
            if entry.is_null() {
                return match Errno::last() {
                    Errno::UnknownErrno => Ok(None),
                    error => Err(error),
                };
            }
            // SAFETY: readdir returned a valid entry with a terminated name.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name != b"." && name != b".." {
                return Ok(Some(name.to_vec()));
            }
        }
    }
    fn rest(&mut self) -> Vec<Vec<u8>> {
        let mut names = Vec::new();
        while let Some(name) = self.next().unwrap() {
            names.push(name);
        }
        names
    }
    fn tell(&mut self) -> libc::c_long {
        // SAFETY: the stream is open.
        unsafe { libc::telldir(self.0) }
    }
    fn seek(&mut self, position: libc::c_long) {
        // SAFETY: the stream is open.
        unsafe { libc::seekdir(self.0, position) }
    }
    fn rewind(&mut self) {
        // SAFETY: the stream is open.
        unsafe { libc::rewinddir(self.0) }
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: opened by `open`, closed once.
        unsafe { libc::closedir(self.0) };
    }
}
fn child(index: usize) -> Vec<u8> {
    format!("n-{index:04}").into_bytes()
}

#[test]
fn rewinddir_retires_the_positions_telldir_gave_before_it() {
    let (f, h) = harness("-rewind");
    let ready = h.mount(2);
    let directory = Path::new(&ready.directory).join("listed");
    fs::create_dir(&directory).unwrap();
    for index in 0..NAMES {
        fs::write(
            directory.join(std::str::from_utf8(&child(index)).unwrap()),
            b"",
        )
        .unwrap();
    }
    let mut wanted: Vec<Vec<u8>> = (0..NAMES).map(child).collect();

    let mut stream = Stream::open(&directory);
    // A position inside the listing, as `telldir` gives it after 100 names.
    let mut listed = Vec::new();
    for _ in 0..100 {
        listed.push(stream.next().unwrap().unwrap());
    }
    let before = stream.tell();
    assert!(before > 2, "a published offset: {before}");
    // A seek back without a rewind resumes strictly after that name.
    listed.extend(stream.rest());
    assert_eq!(listed, wanted);
    stream.seek(before);
    assert_eq!(stream.next().unwrap(), Some(child(100)));

    // A name that sorts before every other, then the rewind: the listing is
    // read again from offset 0 and is complete.
    fs::write(directory.join("a-early"), b"").unwrap();
    wanted.insert(0, b"a-early".to_vec());
    stream.rewind();
    let mut listed = Vec::new();
    for _ in 0..100 {
        listed.push(stream.next().unwrap().unwrap());
    }
    let since = stream.tell();
    listed.extend(stream.rest());
    assert_eq!(listed, wanted);

    // The position from before the rewind is an invalid argument now; one
    // taken since resumes strictly after its name, and so does the handle
    // after the refusal.
    stream.seek(before);
    assert_eq!(stream.next(), Err(Errno::EINVAL));
    stream.seek(since);
    assert_eq!(stream.next().unwrap(), Some(wanted[100].clone()));
    stream.seek(before);
    assert_eq!(stream.next(), Err(Errno::EINVAL));
    stream.rewind();
    assert_eq!(stream.rest(), wanted);
    drop(stream);

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    println!("MOUNTED_REWIND names={NAMES} work={work:?}");
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}
