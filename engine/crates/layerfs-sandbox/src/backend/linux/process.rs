//! Linux `/proc` process facts slot; implemented by the Linux native backend.
use super::super::native::process::Stat;
use std::io;

const UNSUPPORTED: &str = "Linux native process table";

pub(crate) fn boot() -> io::Result<u128> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn stat(_pid: u32) -> io::Result<Stat> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn processes() -> io::Result<Vec<(u32, Stat)>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}
