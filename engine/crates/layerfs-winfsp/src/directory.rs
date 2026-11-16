//! The ports-backed directory face: one ReadDirectory window over the
//! DirectoryStream, encoded into WinFsp DIR_INFO entries.
use crate::services::{block_on, Context};
use layerfs_mount_ports::operations::{DirectoryStep, DirectoryStream};
use std::sync::atomic::{AtomicU64, Ordering};

/// Fresh request ids for the directory face.
static REQUEST: AtomicU64 = AtomicU64::new(1);

/// The DIR_INFO header size (winfsp.h static assert: 104).
pub const ENTRY_SIZE: usize = 104;

/// Reads the root first window (or the dots alone when the services
/// refuse - the static face). Answers the encoded buffer.
pub fn root_window(context: &Context, handle: u64, offset: u64) -> Result<Vec<u8>, String> {
    let services = context.request()?;
    let request = REQUEST.fetch_add(1, Ordering::Relaxed);
    let stream = block_on(DirectoryStream::prepare(
        services,
        context.mount,
        request,
        1, // the root serial: the minted mount mints it first
        handle,
        offset,
        false,
    ))
    .map_err(|failure| format!("directory prepare: {failure}"))?;
    let mut buffer = Vec::new();
    let mut dots = stream.dots().peekable();
    if dots.peek().is_some() {
        for (name, _serial, cookie) in dots {
            push_entry(&mut buffer, name, cookie);
        }
        return Ok(buffer);
    }
    match block_on(stream.next()) {
        Ok(DirectoryStep::Batch(mut batch)) => {
            for (entry, cookie) in batch.entries() {
                let name = String::from_utf8_lossy(&entry.name).into_owned();
                push_entry(&mut buffer, &name, cookie);
            }
            batch.adopt(buffer.len() / ENTRY_SIZE.max(1));
            Ok(buffer)
        }
        Ok(DirectoryStep::End(_)) => Ok(buffer),
        Err(failure) => Err(format!("directory next: {failure}")),
    }
}

/// Appends one DIR_INFO entry: header, attributes, the name; NextOffset
/// carries the entry cookie.
fn push_entry(buffer: &mut Vec<u8>, name: &str, cookie: u64) {
    let start = buffer.len();
    buffer.resize(start + ENTRY_SIZE + name.len() * 2 + 2, 0);
    let at = &mut buffer[start..];
    at[0..2].copy_from_slice(&(ENTRY_SIZE as u16).to_le_bytes());
    at[2..6].copy_from_slice(&0x10u32.to_le_bytes()); // directory
    at[74..82].copy_from_slice(&cookie.to_le_bytes()); // NextOffset
    let name_units: Vec<u16> = name.encode_utf16().collect();
    for (i, unit) in name_units.iter().enumerate() {
        let pos = ENTRY_SIZE + i * 2;
        at[pos..pos + 2].copy_from_slice(&unit.to_le_bytes());
    }
}
