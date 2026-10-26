//! Positive-progress writes, no retries. The exclusive temporary-file
//! claim is the shared installer's (`crate::shared_install`).
use crate::install_types::InstallWork;
pub(super) use crate::shared_install::{absent, claim, paths};
use std::{
    fs::File,
    io::{self, Write},
};

pub(super) fn write(file: &mut File, mut bytes: &[u8], work: &mut InstallWork) -> io::Result<()> {
    while !bytes.is_empty() {
        work.write_calls = work.write_calls.saturating_add(1);
        let size = file.write(bytes)?;
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Store install write",
            ));
        }
        work.written = work.written.saturating_add(size as u64);
        bytes = &bytes[size..];
    }
    Ok(())
}
