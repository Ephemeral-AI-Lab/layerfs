//! A remote file pinned to the version it had when it was opened (#510):
//! ranged reads of that version and no other, for a reader that never
//! downloads the whole file.
//!
//! Opening stats the file once (size, metadata, entity tag, last change).
//! A whole file is read with `If-Match: <its tag>`. A file stored as pieces
//! ([`crate::pieces`]) has its manifest read once, under the same
//! condition, and each byte range maps to `(piece, offset in piece)`; a
//! range across two pieces is two requests. Pieces are replaced in place by
//! the next upload, before its manifest, so a piece's first read requires
//! it unmodified since the manifest was written (`If-Unmodified-Since`)
//! and later reads match the tag that first read returned. A file that
//! changed is [`RemoteError::Changed`], never mixed bytes. Every request is
//! sent once; requests and bytes are counted.
use crate::driver::{Condition, Driver, Span, Stat};
use crate::error::RemoteError;
use crate::pieces::{self, Manifest, LAYOUT, MAX_MANIFEST};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// One piece of a pinned file.
struct Piece {
    name: String,
    bytes: u64,
    /// The tag its first read returned.
    etag: Mutex<Option<String>>,
}

enum Layout {
    Whole {
        etag: String,
    },
    Pieces {
        piece_size: u64,
        pieces: Vec<Piece>,
        written: u64,
    },
}

/// One version of a remote file, read by byte range.
pub struct PinnedFile {
    driver: Box<dyn Driver>,
    name: String,
    stat: Stat,
    bytes: u64,
    layout: Layout,
    requests: AtomicU64,
    fetched: AtomicU64,
}

impl PinnedFile {
    /// Stats `name` once and pins its version; a file stored as pieces has
    /// its manifest read too. A remote that gives no entity tag (or, for
    /// pieces, no last change) cannot pin a version: `Unsupported`.
    pub fn open(driver: Box<dyn Driver>, name: &str) -> Result<Self, RemoteError> {
        let requests = AtomicU64::new(1);
        let stat = driver.stat(name)?.ok_or(RemoteError::Missing)?;
        let etag = stat.etag.clone().ok_or_else(|| {
            RemoteError::Unsupported("this remote gives no version tag to pin".to_owned())
        })?;
        let (bytes, layout, fetched) = match stat.metadata.get(LAYOUT).map(String::as_str) {
            Some("pieces") => {
                if stat.bytes == 0 || stat.bytes > MAX_MANIFEST {
                    return Err(RemoteError::Integrity(format!(
                        "{name}: a {}-byte manifest",
                        stat.bytes
                    )));
                }
                let written = stat.modified.ok_or_else(|| {
                    RemoteError::Unsupported("this remote gives no time to pin pieces".to_owned())
                })?;
                let mut text = Vec::new();
                let span = Span {
                    offset: 0,
                    length: stat.bytes,
                };
                requests.fetch_add(1, Ordering::Relaxed);
                driver.download_range(name, span, &Condition::Match(etag), &mut text)?;
                let text = String::from_utf8(text)
                    .map_err(|_| RemoteError::Integrity(format!("{name}: manifest text")))?;
                let manifest = Manifest::decode(&text)?;
                let layout = Layout::Pieces {
                    piece_size: manifest.piece_size,
                    pieces: (0..manifest.pieces.len() as u64)
                        .zip(&manifest.pieces)
                        .map(|(index, piece)| Piece {
                            name: pieces::piece_name(name, index),
                            bytes: piece.bytes,
                            etag: Mutex::new(None),
                        })
                        .collect(),
                    written,
                };
                (manifest.bytes, layout, stat.bytes)
            }
            _ => (stat.bytes, Layout::Whole { etag }, 0),
        };
        Ok(Self {
            driver,
            name: name.to_owned(),
            stat,
            bytes,
            layout,
            requests,
            fetched: AtomicU64::new(fetched),
        })
    }

    /// The file's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The pinned version's size (the whole file's, for pieces).
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// What the open's stat returned: metadata, entity tag, last change.
    pub fn stat(&self) -> &Stat {
        &self.stat
    }

    /// Requests sent so far, the open's included.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Bytes received so far.
    pub fn fetched(&self) -> u64 {
        self.fetched.load(Ordering::Relaxed)
    }

    /// Fills `out` with the pinned version's bytes from `offset`.
    pub fn read(&self, offset: u64, out: &mut [u8]) -> Result<(), RemoteError> {
        let end = offset
            .checked_add(out.len() as u64)
            .filter(|end| *end <= self.bytes)
            .ok_or_else(|| {
                RemoteError::Invalid(format!("{} bytes at {offset} beyond the file", out.len()))
            })?;
        if out.is_empty() {
            return Ok(());
        }
        match &self.layout {
            Layout::Whole { etag } => {
                self.get(&self.name, offset, out, &Condition::Match(etag.clone()))?;
            }
            Layout::Pieces {
                piece_size,
                pieces,
                written,
            } => {
                let mut at = offset;
                while at < end {
                    let index = at / piece_size;
                    let piece = pieces
                        .get(index as usize)
                        .ok_or_else(|| RemoteError::Integrity("piece index".to_owned()))?;
                    let within = at - index * piece_size;
                    let take = (piece.bytes - within).min(end - at);
                    let part = &mut out[(at - offset) as usize..(at - offset + take) as usize];
                    let known = piece
                        .etag
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner())
                        .clone();
                    let condition = match known {
                        Some(tag) => Condition::Match(tag),
                        None => Condition::UnmodifiedSince(*written),
                    };
                    let tag = self.get(&piece.name, within, part, &condition)?;
                    if let (Condition::UnmodifiedSince(_), Some(tag)) = (condition, tag) {
                        *piece
                            .etag
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner()) = Some(tag);
                    }
                    at += take;
                }
            }
        }
        Ok(())
    }

    /// One conditional ranged GET into `out`; the tag it returned.
    fn get(
        &self,
        name: &str,
        offset: u64,
        out: &mut [u8],
        condition: &Condition,
    ) -> Result<Option<String>, RemoteError> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let span = Span {
            offset,
            length: out.len() as u64,
        };
        let mut sink: &mut [u8] = out;
        let ranged = self
            .driver
            .download_range(name, span, condition, &mut sink)?;
        self.fetched.fetch_add(ranged.bytes, Ordering::Relaxed);
        if ranged.bytes != span.length || !sink.is_empty() {
            return Err(RemoteError::Integrity(format!("{name}: a short range")));
        }
        Ok(ranged.etag)
    }
}
