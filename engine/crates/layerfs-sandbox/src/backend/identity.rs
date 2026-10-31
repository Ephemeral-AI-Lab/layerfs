//! A fresh random identity in the Engine's form (64 lowercase hex, not all
//! zero), so records, names and the cgroup directory treat a local Sandbox
//! like a container.
use crate::{ContainerId, RuntimeError};
use std::{fs::File, io::Read};

pub(crate) fn random() -> Result<ContainerId, RuntimeError> {
    let mut bytes = [0u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let text: String = (bytes.iter())
        .flat_map(|b| [HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]])
        .map(char::from)
        .collect();
    ContainerId::parse(&text)
}
