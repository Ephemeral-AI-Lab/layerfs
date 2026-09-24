//! Frozen #241 Linux LFS2/LFE2 carrier. The Workspace semantics live in Core.
use super::{payload, read_exact_at, Options};
use std::{
    fs::File,
    io::{Error, ErrorKind},
    os::{
        fd::AsRawFd,
        raw::{c_int, c_ulong},
        unix::fs::MetadataExt,
    },
};

const STATE: c_ulong = 0xc058_f540;
const EDIT: c_ulong = 0x5060_f541;
const MAX_REPLACEMENT: usize = 4096;

unsafe extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

#[derive(Clone)]
struct Stamp {
    serial: u64,
    incarnation: [u8; 32],
    generation: u64,
    revision: u64,
    length: u64,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
}

fn number(bytes: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(bytes[start..start + 8].try_into().unwrap())
}

fn state(file: &File) -> Result<Stamp, Error> {
    let mut bytes = [0u8; 88];
    bytes[..4].copy_from_slice(b"LFS2");
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
    if unsafe { ioctl(file.as_raw_fd(), STATE, bytes.as_mut_ptr()) } != 0 {
        return Err(Error::last_os_error());
    }
    if &bytes[..8] != b"LFS2\x02\0\0\0" || bytes[84..] != [0; 4] {
        return Err(Error::new(ErrorKind::InvalidData, "invalid STATE reply"));
    }
    let nanos = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
    if nanos >= 1_000_000_000 {
        return Err(Error::new(ErrorKind::InvalidData, "invalid STATE mtime"));
    }
    Ok(Stamp {
        serial: number(&bytes, 8),
        incarnation: bytes[16..48].try_into().unwrap(),
        generation: number(&bytes, 48),
        revision: number(&bytes, 56),
        length: number(&bytes, 64),
        mtime_seconds: i64::from_le_bytes(bytes[72..80].try_into().unwrap()),
        mtime_nanoseconds: nanos,
    })
}

fn request(options: &Options, stamp: &Stamp, replacement: &[u8]) -> [u8; 4192] {
    let mut bytes = [0u8; 4192];
    bytes[..4].copy_from_slice(b"LFE2");
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    bytes[8..16].copy_from_slice(&stamp.serial.to_le_bytes());
    bytes[16..48].copy_from_slice(&stamp.incarnation);
    bytes[48..56].copy_from_slice(&stamp.generation.to_le_bytes());
    bytes[56..64].copy_from_slice(&stamp.revision.to_le_bytes());
    bytes[64..72].copy_from_slice(&options.offset.to_le_bytes());
    bytes[72..80].copy_from_slice(&options.delete_length.to_le_bytes());
    bytes[80..84].copy_from_slice(&(replacement.len() as u32).to_le_bytes());
    bytes[96..96 + replacement.len()].copy_from_slice(replacement);
    bytes
}

fn unknown(phase: &str, before: &Stamp, error: impl std::fmt::Display) -> ! {
    eprintln!(
        "UNKNOWN phase={phase} serial={} generation={} revision={} cause={error}",
        before.serial, before.generation, before.revision
    );
    std::process::exit(75)
}

fn confirm(file: &File, before: &Stamp, expected_length: u64, expected: &[u8], read_start: u64) {
    let after = state(file).unwrap_or_else(|error| unknown("post-state", before, error));
    if after.serial != before.serial
        || after.incarnation != before.incarnation
        || after.generation != before.generation
        || Some(after.revision) != before.revision.checked_add(1)
        || after.length != expected_length
    {
        unknown(
            "post-stamp",
            before,
            "revision, identity or length mismatch",
        );
    }
    let stat = file
        .metadata()
        .unwrap_or_else(|error| unknown("fstat", before, error));
    if stat.len() != after.length
        || stat.mtime() != after.mtime_seconds
        || stat.mtime_nsec() != i64::from(after.mtime_nanoseconds)
    {
        unknown("fstat", before, "length or mtime mismatch");
    }
    let mut actual = vec![0; expected.len()];
    read_exact_at(file, read_start, &mut actual)
        .unwrap_or_else(|error| unknown("readback", before, error));
    if actual != expected {
        unknown("readback", before, "boundary bytes mismatch");
    }
}

pub(super) fn run(options: &Options, file: &File) -> Result<(), Error> {
    let replacement = if options.length == 0 {
        Vec::new()
    } else {
        payload(options)?
    };
    if replacement.len() > MAX_REPLACEMENT || (replacement.is_empty() && options.delete_length == 0)
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "invalid bounded range edit",
        ));
    }
    let tail_start = options
        .offset
        .checked_add(options.delete_length)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "edit bounds overflow"))?;
    if tail_start > options.expect_size {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "edit exceeds declared file",
        ));
    }
    let final_length = options.expect_size - options.delete_length;
    let final_length = final_length
        .checked_add(replacement.len() as u64)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "result length overflow"))?;
    let before = state(file)?;
    let stat = file.metadata()?;
    if before.length != options.expect_size
        || stat.len() != before.length
        || stat.mtime() != before.mtime_seconds
        || stat.mtime_nsec() != i64::from(before.mtime_nanoseconds)
    {
        return Err(Error::new(ErrorKind::InvalidData, "pre-state mismatch"));
    }
    let left_len = options.offset.min(16) as usize;
    let right_len = (options.expect_size - tail_start).min(16) as usize;
    let read_start = options.offset - left_len as u64;
    let mut expected = vec![0; left_len + replacement.len() + right_len];
    read_exact_at(file, read_start, &mut expected[..left_len])?;
    expected[left_len..left_len + replacement.len()].copy_from_slice(&replacement);
    read_exact_at(
        file,
        tail_start,
        &mut expected[left_len + replacement.len()..],
    )?;
    let mut bytes = request(options, &before, &replacement);
    if unsafe { ioctl(file.as_raw_fd(), EDIT, bytes.as_mut_ptr()) } != 0 {
        unknown("edit", &before, Error::last_os_error());
    }
    confirm(file, &before, final_length, &expected, read_start);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn frozen_request_bytes() {
        let options = Options {
            operation: "splice".into(),
            file: PathBuf::new(),
            expect_size: 8192,
            offset: 4093,
            delete_length: 0,
            length: 4,
            size: 0,
            direction: None,
            payload: None,
        };
        let stamp = Stamp {
            serial: 7,
            incarnation: [9; 32],
            generation: 3,
            revision: 11,
            length: 8192,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        };
        let bytes = request(&options, &stamp, b"ABCD");
        assert_eq!(&bytes[..8], b"LFE2\x02\0\0\0");
        assert_eq!(number(&bytes, 56), 11);
        assert_eq!(number(&bytes, 64), 4093);
        assert_eq!(&bytes[80..84], &4u32.to_le_bytes());
        assert_eq!(&bytes[96..100], b"ABCD");
        assert!(bytes[100..].iter().all(|byte| *byte == 0));
    }
}
