//! Whole files on a remote, verified by sha256: a file that fits the
//! provider's largest object goes up as one object; a larger one as
//! fixed-size pieces plus a manifest of each piece's size and sha256.
//!
//! The file's own name holds either the whole file or the manifest; its
//! metadata says which (`layerfs-layout`) and carries the whole file's
//! sha256 (`layerfs-sha256`). Pieces go up first and the manifest last, so a
//! manifest is published only after all its pieces. Nothing here is atomic
//! across requests: a download racing an upload fails its sha256 check
//! instead of returning mixed bytes. The whole file's size goes with it
//! (`layerfs-bytes`), so one stat tells it even for a manifest.
//!
//! Progress (#506 E4): an upload reports `hash` (bytes hashed of the file)
//! then `upload` (bytes sent, by the driver) and its pieces; a download
//! reports `download` (bytes received) and its pieces.
use crate::driver::{open_span, Driver, Metadata, Span};
use crate::error::{local, RemoteError};
use crate::progress;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Metadata key of the whole file's sha256 (lowercase hex).
pub const SHA256: &str = "layerfs-sha256";
/// Metadata key of the layout: `whole` or `pieces`.
pub const LAYOUT: &str = "layerfs-layout";
/// Metadata key of the whole file's size in bytes (decimal).
pub const BYTES: &str = "layerfs-bytes";
const WHOLE: &str = "whole";
const PIECES: &str = "pieces";
/// Largest manifest a download reads into memory.
pub const MAX_MANIFEST: u64 = 16 * 1024 * 1024;
const MANIFEST_HEADER: &str = "layerfs-pieces 1";

/// What an upload or download moved.
#[derive(Debug)]
pub struct Moved {
    pub bytes: u64,
    pub sha256: [u8; 32],
    /// 0 for a whole file.
    pub pieces: u64,
    /// An upload's failure to remove pieces an earlier, larger upload left
    /// beyond this one's; the new file itself is complete.
    pub cleanup: Option<RemoteError>,
}

/// One piece's entry in a manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Piece {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

/// The manifest of a file stored as pieces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub piece_size: u64,
    pub pieces: Vec<Piece>,
}

impl Manifest {
    /// The manifest's text form.
    pub fn encode(&self) -> String {
        let mut text = format!(
            "{MANIFEST_HEADER}\nbytes {}\nsha256 {}\npiece-size {}\n",
            self.bytes,
            hex(&self.sha256),
            self.piece_size
        );
        for piece in &self.pieces {
            text.push_str(&format!("piece {} {}\n", piece.bytes, hex(&piece.sha256)));
        }
        text
    }

    /// Parses and checks a manifest: every piece is `piece_size` but the
    /// last, and the sizes add up to `bytes`.
    pub fn decode(text: &str) -> Result<Self, RemoteError> {
        let bad = |why: &str| RemoteError::Integrity(format!("manifest: {why}"));
        let mut lines = text.lines();
        if lines.next() != Some(MANIFEST_HEADER) {
            return Err(bad("header"));
        }
        let mut field = |name: &str| {
            lines
                .next()
                .and_then(|line| line.strip_prefix(name)?.strip_prefix(' '))
                .map(str::to_owned)
                .ok_or_else(|| bad(name))
        };
        let number = |text: String| text.parse::<u64>().map_err(|_| bad("number"));
        let bytes = number(field("bytes")?)?;
        let sha256 = parse_hex32(&field("sha256")?).ok_or_else(|| bad("sha256"))?;
        let piece_size = number(field("piece-size")?)?;
        let mut pieces = Vec::new();
        for line in lines {
            let mut words = line
                .strip_prefix("piece ")
                .ok_or_else(|| bad("piece"))?
                .split(' ');
            let size = words.next().ok_or_else(|| bad("piece size"))?;
            let digest = words.next().ok_or_else(|| bad("piece sha256"))?;
            if words.next().is_some() {
                return Err(bad("piece"));
            }
            pieces.push(Piece {
                bytes: number(size.to_owned())?,
                sha256: parse_hex32(digest).ok_or_else(|| bad("piece sha256"))?,
            });
        }
        let count = pieces.len() as u64;
        let well_sized = piece_size > 0
            && count == bytes.div_ceil(piece_size)
            && pieces.iter().enumerate().all(|(index, piece)| {
                let expected = if index as u64 + 1 == count {
                    bytes - piece_size * (count - 1)
                } else {
                    piece_size
                };
                piece.bytes == expected
            });
        if !well_sized {
            return Err(bad("piece sizes"));
        }
        Ok(Self {
            bytes,
            sha256,
            piece_size,
            pieces,
        })
    }
}

/// The remote name of piece `index` of `name`.
pub fn piece_name(name: &str, index: u64) -> String {
    format!("{name}.piece-{index:06}")
}

/// Uploads `path` as `name` with `metadata` plus its sha256 and layout,
/// as pieces when it is larger than the driver's `max_file`.
pub fn upload(
    driver: &dyn Driver,
    name: &str,
    path: &Path,
    metadata: &Metadata,
) -> Result<Moved, RemoteError> {
    let limits = driver.limits();
    limits.validate()?;
    if [SHA256, LAYOUT, BYTES]
        .iter()
        .any(|key| metadata.contains_key(*key))
    {
        return Err(RemoteError::Invalid("reserved metadata key".to_owned()));
    }
    let size = fs::metadata(path)
        .map_err(|e| local("stat upload source", e))?
        .len();
    progress::begin("hash", Some(size));
    let (bytes, sha256) = hash_file(path, None, true)?;
    let mut tagged = metadata.clone();
    tagged.insert(SHA256.to_owned(), hex(&sha256));
    tagged.insert(BYTES.to_owned(), bytes.to_string());
    let count = limits.pieces(bytes);
    progress::begin("upload", Some(bytes));
    if count == 0 {
        tagged.insert(LAYOUT.to_owned(), WHOLE.to_owned());
        driver.upload_file(name, path, None, &tagged)?;
    } else {
        let mut manifest = Manifest {
            bytes,
            sha256,
            piece_size: limits.piece_size,
            pieces: Vec::new(),
        };
        progress::pieces(0, count);
        for index in 0..count {
            let offset = index * limits.piece_size;
            let span = Span {
                offset,
                length: limits.piece_size.min(bytes - offset),
            };
            let (length, digest) = hash_file(path, Some(span), false)?;
            let mut piece = Metadata::new();
            piece.insert(SHA256.to_owned(), hex(&digest));
            driver.upload_file(&piece_name(name, index), path, Some(span), &piece)?;
            manifest.pieces.push(Piece {
                bytes: length,
                sha256: digest,
            });
            progress::pieces(index + 1, count);
        }
        tagged.insert(LAYOUT.to_owned(), PIECES.to_owned());
        let staged = Staged::write(path, manifest.encode().as_bytes())?;
        // the manifest is not the file's bytes
        progress::quiet(|| driver.upload_file(name, &staged.0, None, &tagged))?;
    }
    Ok(Moved {
        bytes,
        sha256,
        pieces: count,
        cleanup: remove_pieces_from(driver, name, count).err(),
    })
}

/// Downloads `name` into `out` (written from its current position) and
/// verifies every piece and the whole file against their sha256.
pub fn download(driver: &dyn Driver, name: &str, out: &mut File) -> Result<Moved, RemoteError> {
    let stat = driver.stat(name)?.ok_or(RemoteError::Missing)?;
    let sha256 = stat
        .metadata
        .get(SHA256)
        .and_then(|text| parse_hex32(text))
        .ok_or_else(|| RemoteError::Integrity(format!("{name} has no {SHA256}")))?;
    let mut whole = Hashing::new(out);
    whole.report = true;
    let pieces = match stat.metadata.get(LAYOUT).map(String::as_str) {
        Some(WHOLE) => {
            progress::begin("download", Some(stat.bytes));
            let n = driver.download_file(name, None, &mut whole)?;
            if n != stat.bytes || whole.count != stat.bytes {
                return Err(RemoteError::Integrity(format!(
                    "{name}: {n} bytes received, {} described",
                    stat.bytes
                )));
            }
            0
        }
        Some(PIECES) => {
            if stat.bytes > MAX_MANIFEST {
                return Err(RemoteError::Integrity("manifest too large".to_owned()));
            }
            let mut text = Vec::new();
            driver.download_file(name, None, &mut text)?;
            let manifest = Manifest::decode(
                std::str::from_utf8(&text)
                    .map_err(|_| RemoteError::Integrity("manifest text".to_owned()))?,
            )?;
            if manifest.sha256 != sha256 {
                return Err(RemoteError::Integrity("manifest sha256".to_owned()));
            }
            let count = manifest.pieces.len() as u64;
            progress::begin("download", Some(manifest.bytes));
            progress::pieces(0, count);
            for (index, piece) in manifest.pieces.iter().enumerate() {
                let piece_remote = piece_name(name, index as u64);
                let mut one = Hashing::new(&mut whole);
                let n = driver.download_file(&piece_remote, None, &mut one)?;
                if n != piece.bytes || one.count != piece.bytes || one.finish() != piece.sha256 {
                    return Err(RemoteError::Integrity(format!(
                        "{piece_remote} does not match the manifest"
                    )));
                }
                progress::pieces(index as u64 + 1, count);
            }
            manifest.pieces.len() as u64
        }
        _ => return Err(RemoteError::Integrity(format!("{name} has no {LAYOUT}"))),
    };
    let bytes = whole.count;
    if whole.finish() != sha256 {
        return Err(RemoteError::Integrity(format!("{name}: sha256 mismatch")));
    }
    Ok(Moved {
        bytes,
        sha256,
        pieces,
        cleanup: None,
    })
}

/// Deletes `name`, then its pieces.
pub fn delete(driver: &dyn Driver, name: &str) -> Result<(), RemoteError> {
    driver.delete(name)?;
    remove_pieces_from(driver, name, 0)
}

/// Removes pieces `from`, `from + 1`, … of `name` while they exist.
fn remove_pieces_from(driver: &dyn Driver, name: &str, from: u64) -> Result<(), RemoteError> {
    let mut index = from;
    while driver.stat(&piece_name(name, index))?.is_some() {
        driver.delete(&piece_name(name, index))?;
        index += 1;
    }
    Ok(())
}

/// The byte count and sha256 of `path` (or `span` of it).
pub fn sha256_file(path: &Path, span: Option<Span>) -> Result<(u64, [u8; 32]), RemoteError> {
    hash_file(path, span, false)
}

/// The sha256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// [`sha256_file`], reporting the bytes hashed to this thread's progress
/// when `report`.
fn hash_file(
    path: &Path,
    span: Option<Span>,
    report: bool,
) -> Result<(u64, [u8; 32]), RemoteError> {
    let (mut file, span) = open_span(path, span)?;
    file.seek(SeekFrom::Start(span.offset))
        .map_err(|e| local("seek", e))?;
    let mut digest = Hashing::new(io::sink());
    digest.report = report;
    let n = io::copy(&mut file.take(span.length), &mut digest).map_err(|e| local("read", e))?;
    if n != span.length {
        return Err(RemoteError::Local(format!(
            "{} shrank while read",
            path.display()
        )));
    }
    Ok((n, digest.finish()))
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

/// Parses 64 lowercase hex digits.
pub fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    let digits = text.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let value = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut out = [0; 32];
    for (index, pair) in digits.chunks(2).enumerate() {
        out[index] = (value(pair[0])? << 4) | value(pair[1])?;
    }
    Some(out)
}

/// A writer that counts and hashes what passes through it, and reports
/// the bytes to this thread's progress when `report`.
struct Hashing<W> {
    inner: W,
    digest: Sha256,
    count: u64,
    report: bool,
}

impl<W: Write> Hashing<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
            count: 0,
            report: false,
        }
    }
    fn finish(self) -> [u8; 32] {
        self.digest.finalize().into()
    }
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.digest.update(&bytes[..n]);
        self.count += n as u64;
        if self.report {
            progress::add(n as u64);
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A manifest staged next to its source for upload, removed when dropped.
struct Staged(PathBuf);

impl Staged {
    fn write(source: &Path, bytes: &[u8]) -> Result<Self, RemoteError> {
        let mut name = source.as_os_str().to_owned();
        name.push(".layerfs-manifest");
        let path = PathBuf::from(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| local("stage manifest", e))?;
        let staged = Self(path);
        file.write_all(bytes)
            .map_err(|e| local("stage manifest", e))?;
        Ok(staged)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        // This process created the file a moment ago; it holds no data of record.
        let _ = fs::remove_file(&self.0);
    }
}
