//! Frozen #241 Linux LFS2/LFE2 carrier. The Workspace semantics live in Core.
use super::{payload, read_exact_at, Options};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Error, ErrorKind, Read, Write},
    os::{
        fd::AsRawFd,
        raw::{c_int, c_ulong},
        unix::fs::{FileExt, MetadataExt},
    },
    time::Instant,
};

const STATE: c_ulong = 0xc058_f540;
const EDIT: c_ulong = 0x5060_f541;
const MAX_REPLACEMENT: usize = 4096;
const MAX_STREAM: usize = 8 * 1024 * 1024;
const BEGIN: c_ulong = 0xc080_f542;
const DATA: c_ulong = 0x5080_f543;
const APPLY: c_ulong = 0x4080_f544;
const ABORT: c_ulong = 0x4080_f545;

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

#[derive(Default)]
struct BatchTimes {
    state_ns: u128,
    stat_ns: u128,
    read_ns: u128,
    edit_ns: u128,
    total_ns: u128,
}

fn confirm(
    file: &File,
    before: &Stamp,
    expected_length: u64,
    expected: &[u8],
    read_start: u64,
    stats: Option<&mut BatchTimes>,
) -> (i64, u32) {
    let mut stats = stats;
    let started = stats.as_ref().map(|_| Instant::now());
    let after = state(file).unwrap_or_else(|error| unknown("post-state", before, error));
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.state_ns += started.elapsed().as_nanos();
    }
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
    let started = stats.as_ref().map(|_| Instant::now());
    let stat = file
        .metadata()
        .unwrap_or_else(|error| unknown("fstat", before, error));
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.stat_ns += started.elapsed().as_nanos();
    }
    if stat.len() != after.length
        || stat.mtime() != after.mtime_seconds
        || stat.mtime_nsec() != i64::from(after.mtime_nanoseconds)
    {
        unknown("fstat", before, "length or mtime mismatch");
    }
    let mut actual = vec![0; expected.len()];
    let started = stats.as_ref().map(|_| Instant::now());
    read_exact_at(file, read_start, &mut actual)
        .unwrap_or_else(|error| unknown("readback", before, error));
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.read_ns += started.elapsed().as_nanos();
    }
    if actual != expected {
        unknown("readback", before, "boundary bytes mismatch");
    }
    (after.mtime_seconds, after.mtime_nanoseconds)
}

fn run_with(
    options: &Options,
    file: &File,
    replacement: &[u8],
    stats: Option<&mut BatchTimes>,
) -> Result<(i64, u32), Error> {
    let mut stats = stats;
    let total_started = stats.as_ref().map(|_| Instant::now());
    if replacement.len() as u64 != options.length {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "replacement length mismatch",
        ));
    }
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
    let started = stats.as_ref().map(|_| Instant::now());
    let before = state(file)?;
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.state_ns += started.elapsed().as_nanos();
    }
    let started = stats.as_ref().map(|_| Instant::now());
    let stat = file.metadata()?;
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.stat_ns += started.elapsed().as_nanos();
    }
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
    let started = stats.as_ref().map(|_| Instant::now());
    read_exact_at(file, read_start, &mut expected[..left_len])?;
    expected[left_len..left_len + replacement.len()].copy_from_slice(replacement);
    read_exact_at(
        file,
        tail_start,
        &mut expected[left_len + replacement.len()..],
    )?;
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.read_ns += started.elapsed().as_nanos();
    }
    let mut bytes = request(options, &before, replacement);
    let started = stats.as_ref().map(|_| Instant::now());
    if unsafe { ioctl(file.as_raw_fd(), EDIT, bytes.as_mut_ptr()) } != 0 {
        unknown("edit", &before, Error::last_os_error());
    }
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), started) {
        stats.edit_ns += started.elapsed().as_nanos();
    }
    let result = confirm(
        file,
        &before,
        final_length,
        &expected,
        read_start,
        stats.as_deref_mut(),
    );
    if let (Some(stats), Some(started)) = (stats.as_deref_mut(), total_started) {
        stats.total_ns += started.elapsed().as_nanos();
    }
    Ok(result)
}

pub(super) fn run(options: &Options, file: &File) -> Result<(i64, u32), Error> {
    let replacement = if options.length == 0 {
        Vec::new()
    } else {
        payload(options)?
    };
    run_with(options, file, &replacement, None)
}

pub(super) fn run_batch(options: &Options, file: &File) -> Result<(i64, u32), Error> {
    if options.expect_size != 1_048_576
        || options.offset != 524_288
        || options.delete_length != 0
        || options.length != 4096
        || !matches!(options.count, 1 | 32 | 128)
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "invalid Phase 1C batch",
        ));
    }
    let replacement = payload(options)?;
    let width = 4096 / options.count as usize;
    let mut step = options.clone();
    let mut mtime = (0, 0);
    let mut stats = std::env::var_os("LAYERFS_LIVENESS_DIAGNOSTIC").map(|_| BatchTimes::default());
    for i in 0..options.count as usize {
        step.expect_size = options.expect_size + (i * width) as u64;
        step.offset = options.offset + (i * (256 + width)) as u64;
        step.length = width as u64;
        mtime = run_with(
            &step,
            file,
            &replacement[i * width..(i + 1) * width],
            stats.as_mut(),
        )?;
    }
    if let Some(stats) = stats {
        if let Ok(mut log) = File::options().append(true).open("/proc/1/fd/2") {
            let _ = writeln!(
                log,
                "LFS_LIVENESS_DIAG v=1 edits={} total_ns={} state_ns={} stat_ns={} read_ns={} edit_ns={} residual_ns={}",
                options.count,
                stats.total_ns,
                stats.state_ns,
                stats.stat_ns,
                stats.read_ns,
                stats.edit_ns,
                stats.total_ns.saturating_sub(
                    stats.state_ns + stats.stat_ns + stats.read_ns + stats.edit_ns
                )
            );
        }
    }
    Ok(mtime)
}

enum Stream {
    Empty,
    Bytes(Vec<u8>),
    Zero(usize),
}

impl Stream {
    fn parse(spec: &str) -> Result<Self, Error> {
        if spec == "empty" {
            return Ok(Self::Empty);
        }
        let (kind, value) = spec
            .split_once(':')
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "stream requires kind:value"))?;
        match kind {
            "empty" if value == "0" => Ok(Self::Empty),
            "zero" => {
                let len = value
                    .parse::<usize>()
                    .map_err(|_| Error::new(ErrorKind::InvalidInput, "invalid Zero length"))?;
                if len == 0 || len > MAX_STREAM {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Zero length outside cap",
                    ));
                }
                Ok(Self::Zero(len))
            }
            "bytes" => {
                let mut file = File::open(value)?;
                if file.metadata()?.len() > MAX_STREAM as u64 {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Bytes length outside cap",
                    ));
                }
                let mut data = Vec::new();
                file.read_to_end(&mut data)?;
                if data.is_empty() || data.len() > MAX_STREAM {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Bytes length outside cap",
                    ));
                }
                Ok(Self::Bytes(data))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                "unsupported stream element",
            )),
        }
    }

    fn logical(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Bytes(data) => data.len(),
            Self::Zero(len) => *len,
        }
    }

    fn literal(&self) -> usize {
        match self {
            Self::Bytes(data) => data.len(),
            _ => 0,
        }
    }

    fn digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        match self {
            Self::Empty => {}
            Self::Bytes(data) => hash.update(data),
            Self::Zero(len) => {
                let blank = [0u8; 4096];
                let mut left = *len;
                while left != 0 {
                    let take = left.min(blank.len());
                    hash.update(&blank[..take]);
                    left -= take;
                }
            }
        }
        hash.finalize().into()
    }

    fn boundary(&self, end: bool) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Bytes(data) if end => &data[data.len() - data.len().min(16)..],
            Self::Bytes(data) => &data[..data.len().min(16)],
            Self::Zero(len) => &[0u8; 16][..(*len).min(16)],
        }
    }
}

fn checked_bytes(file: &File, at: u64, expected: &[u8], before: &Stamp) {
    let mut actual = vec![0; expected.len()];
    read_exact_at(file, at, &mut actual).unwrap_or_else(|error| unknown("readback", before, error));
    if actual != expected {
        unknown("readback", before, "boundary bytes mismatch");
    }
}

/// One registered #232 semantic range replacement through inline or staged v3.
pub(super) fn run_generic(
    options: &Options,
    file: &File,
) -> Result<((i64, u32), u64, u64, u64), Error> {
    let stream = Stream::parse(
        options
            .stream
            .as_deref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "missing replacement stream"))?,
    )?;
    let logical = stream.logical();
    let literal = stream.literal();
    let end = options
        .offset
        .checked_add(options.delete_length)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "edit bounds overflow"))?;
    if end > options.expect_size || (logical == 0 && options.delete_length == 0) {
        return Err(Error::new(ErrorKind::InvalidInput, "invalid range"));
    }
    let final_length = options.expect_size - options.delete_length;
    let final_length = final_length
        .checked_add(logical as u64)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "result overflow"))?;
    if final_length > 4 * 1024 * 1024 * 1024 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "result beyond file cap",
        ));
    }
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
    let right_len = (options.expect_size - end).min(16) as usize;
    let mut left = vec![0; left_len];
    let mut right = vec![0; right_len];
    read_exact_at(file, options.offset - left_len as u64, &mut left)?;
    read_exact_at(file, end, &mut right)?;
    let ioctls = if let Stream::Bytes(data) = &stream {
        if data.len() <= MAX_REPLACEMENT {
            let mut frame = request(options, &before, data);
            if unsafe { ioctl(file.as_raw_fd(), EDIT, frame.as_mut_ptr()) } != 0 {
                unknown("edit", &before, Error::last_os_error());
            }
            1
        } else {
            staged(options, file, &before, &stream)?
        }
    } else if matches!(stream, Stream::Empty) {
        let mut frame = request(options, &before, &[]);
        if unsafe { ioctl(file.as_raw_fd(), EDIT, frame.as_mut_ptr()) } != 0 {
            unknown("edit", &before, Error::last_os_error());
        }
        1
    } else {
        staged(options, file, &before, &stream)?
    };
    let after = state(file).unwrap_or_else(|error| unknown("post-state", &before, error));
    if after.serial != before.serial
        || after.incarnation != before.incarnation
        || after.generation != before.generation
        || Some(after.revision) != before.revision.checked_add(1)
        || after.length != final_length
    {
        unknown(
            "post-stamp",
            &before,
            "revision, identity or length mismatch",
        );
    }
    let stat = file
        .metadata()
        .unwrap_or_else(|error| unknown("fstat", &before, error));
    if stat.len() != final_length
        || stat.mtime() != after.mtime_seconds
        || stat.mtime_nsec() != i64::from(after.mtime_nanoseconds)
    {
        unknown("fstat", &before, "length or mtime mismatch");
    }
    checked_bytes(file, options.offset - left_len as u64, &left, &before);
    checked_bytes(file, options.offset, stream.boundary(false), &before);
    if logical > 16 {
        checked_bytes(
            file,
            options.offset + logical as u64 - 16,
            stream.boundary(true),
            &before,
        );
    }
    checked_bytes(file, options.offset + logical as u64, &right, &before);
    if file
        .read_at(&mut [0u8; 1], final_length)
        .unwrap_or_else(|error| unknown("eof", &before, error))
        != 0
    {
        unknown("eof", &before, "read past final length");
    }
    Ok((
        (after.mtime_seconds, after.mtime_nanoseconds),
        logical as u64,
        literal as u64,
        ioctls,
    ))
}

fn staged(options: &Options, file: &File, before: &Stamp, stream: &Stream) -> Result<u64, Error> {
    let mut begin = [0u8; 128];
    begin[..4].copy_from_slice(b"LFB3");
    begin[4..6].copy_from_slice(&3u16.to_le_bytes());
    begin[8..16].copy_from_slice(&before.serial.to_le_bytes());
    begin[16..48].copy_from_slice(&before.incarnation);
    begin[48..56].copy_from_slice(&before.generation.to_le_bytes());
    begin[56..64].copy_from_slice(&before.revision.to_le_bytes());
    begin[64..72].copy_from_slice(&options.offset.to_le_bytes());
    begin[72..80].copy_from_slice(&options.delete_length.to_le_bytes());
    begin[80..88].copy_from_slice(&(stream.logical() as u64).to_le_bytes());
    begin[88..96].copy_from_slice(&(stream.literal() as u64).to_le_bytes());
    begin[96..128].copy_from_slice(&stream.digest());
    if unsafe { ioctl(file.as_raw_fd(), BEGIN, begin.as_mut_ptr()) } != 0 {
        return Err(Error::last_os_error());
    }
    if &begin[..8] != b"LFB3\x03\0\0\0"
        || begin[8..24] == [0; 16]
        || begin[24..].iter().any(|byte| *byte != 0)
    {
        unknown("begin-reply", before, "invalid token reply");
    }
    let token: [u8; 16] = begin[8..24].try_into().unwrap();
    let mut sent = 0usize;
    let mut calls = 0u64;
    while sent < stream.logical() {
        let mut frame = [0u8; 4224];
        frame[..4].copy_from_slice(b"LFD3");
        frame[4..6].copy_from_slice(&3u16.to_le_bytes());
        frame[8..24].copy_from_slice(&token);
        frame[24..32].copy_from_slice(&(sent as u64).to_le_bytes());
        let length = match stream {
            Stream::Bytes(data) => {
                let len = (data.len() - sent).min(4096);
                frame[6..8].copy_from_slice(&1u16.to_le_bytes());
                frame[40..44].copy_from_slice(&(len as u32).to_le_bytes());
                frame[128..128 + len].copy_from_slice(&data[sent..sent + len]);
                len
            }
            Stream::Zero(len) => {
                frame[6..8].copy_from_slice(&2u16.to_le_bytes());
                *len
            }
            Stream::Empty => unreachable!(),
        };
        frame[32..40].copy_from_slice(&(length as u64).to_le_bytes());
        if unsafe { ioctl(file.as_raw_fd(), DATA, frame.as_mut_ptr()) } != 0 {
            let error = Error::last_os_error();
            let mut abort = token_frame(b"LFX3", token);
            let _ = unsafe { ioctl(file.as_raw_fd(), ABORT, abort.as_mut_ptr()) };
            return Err(error);
        }
        sent += length;
        calls += 1;
    }
    let mut apply = token_frame(b"LFA3", token);
    if unsafe { ioctl(file.as_raw_fd(), APPLY, apply.as_mut_ptr()) } != 0 {
        unknown("apply", before, Error::last_os_error());
    }
    Ok(calls + 2)
}

fn token_frame(magic: &[u8; 4], token: [u8; 16]) -> [u8; 128] {
    let mut frame = [0u8; 128];
    frame[..4].copy_from_slice(magic);
    frame[4..6].copy_from_slice(&3u16.to_le_bytes());
    frame[8..24].copy_from_slice(&token);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn frozen_request_bytes() {
        let options = Options {
            operation: "splice".into(),
            output_version: 3,
            file: PathBuf::new(),
            expect_size: 8192,
            offset: 4093,
            delete_length: 0,
            length: 4,
            size: 0,
            count: 0,
            direction: None,
            payload: None,
            stream: None,
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

    #[test]
    fn v3_stream_and_token_frames_are_bounded() {
        let empty = Stream::parse("empty").unwrap();
        assert_eq!((empty.logical(), empty.literal()), (0, 0));
        let zero = Stream::parse("zero:65536").unwrap();
        assert_eq!((zero.logical(), zero.literal()), (65536, 0));
        assert!(Stream::parse("zero:8388609").is_err());
        let token = [7; 16];
        let frame = token_frame(b"LFA3", token);
        assert_eq!(&frame[..8], b"LFA3\x03\0\0\0");
        assert_eq!(&frame[8..24], &token);
        assert!(frame[24..].iter().all(|byte| *byte == 0));
    }
}
