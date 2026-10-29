//! The provider-neutral driver: a remote holds files by name.
use crate::error::{local, RemoteError};
use crate::limits::Limits;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// A file's user metadata: lowercase `[a-z0-9-]` keys, printable ASCII values.
pub type Metadata = BTreeMap<String, String>;

/// A byte range of a file: `length` bytes from `offset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub offset: u64,
    pub length: u64,
}

/// A remote file's size, user metadata and last change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stat {
    pub bytes: u64,
    pub metadata: Metadata,
    /// When the file was last written, in milliseconds since the Unix
    /// epoch, when the remote says.
    pub modified: Option<u64>,
    /// The version of the file the remote holds (its entity tag), when the
    /// remote says; a conditional read pins it (#510).
    pub etag: Option<String>,
}

/// What a conditional ranged read requires of the remote file (#510).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Condition {
    /// The file is still the version with this entity tag (`If-Match`).
    Match(String),
    /// The file was not written after this time, in milliseconds since the
    /// Unix epoch (`If-Unmodified-Since`, second resolution).
    UnmodifiedSince(u64),
}

/// What a conditional ranged read wrote.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ranged {
    pub bytes: u64,
    /// The entity tag of the version read, when the remote says.
    pub etag: Option<String>,
}

/// One file of a listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Listed {
    /// Its name, as the other calls take it.
    pub name: String,
    pub bytes: u64,
    /// Milliseconds since the Unix epoch, when the remote says.
    pub modified: Option<u64>,
    /// Its user metadata when the listing carries it; `None` when a stat
    /// is needed to read it.
    pub metadata: Option<Metadata>,
}

/// One remote. Each call sends each of its requests once; a sent request
/// with an unknown outcome is `Uncertain` and never resent.
pub trait Driver: Send + Sync {
    /// What this remote allows.
    fn limits(&self) -> Limits;

    /// Uploads `path` (or `span` of it) as `name`, replacing any file of that
    /// name, with `metadata`. Refuses a body larger than `limits().max_file`.
    fn upload_file(
        &self,
        name: &str,
        path: &Path,
        span: Option<Span>,
        metadata: &Metadata,
    ) -> Result<(), RemoteError>;

    /// Writes `name` (or `span` of it) to `out`; returns the bytes written.
    fn download_file(
        &self,
        name: &str,
        span: Option<Span>,
        out: &mut dyn Write,
    ) -> Result<u64, RemoteError>;

    /// Writes `span` of `name` to `out` only while `condition` holds; a file
    /// that no longer meets it is [`RemoteError::Changed`]. One request,
    /// never resent. A driver without conditional reads refuses `Unsupported`.
    fn download_range(
        &self,
        name: &str,
        span: Span,
        condition: &Condition,
        out: &mut dyn Write,
    ) -> Result<Ranged, RemoteError> {
        let _ = (name, span, condition, out);
        Err(RemoteError::Unsupported(
            "conditional ranged reads on this remote".to_owned(),
        ))
    }

    /// The file's size and metadata, or `None` when it is absent.
    fn stat(&self, name: &str) -> Result<Option<Stat>, RemoteError>;

    /// Removes `name`. An absent file is not an error.
    fn delete(&self, name: &str) -> Result<(), RemoteError>;

    /// Every file whose name starts with `prefix`, in name order, in one
    /// paged pass. A remote that cannot list refuses `Unsupported`.
    fn list(&self, prefix: &str) -> Result<Vec<Listed>, RemoteError>;
}

/// Most pages one listing reads (1000 names each on S3) before it refuses.
pub const MAX_LIST_PAGES: usize = 10_000;

/// Longest file name a driver accepts.
pub const MAX_NAME: usize = 512;

/// Refuses a name that is not slash-separated segments of `[A-Za-z0-9._-]`,
/// none empty or starting with a dot.
pub fn check_name(name: &str) -> Result<(), RemoteError> {
    let segment = |s: &str| {
        !s.is_empty()
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    if name.len() > MAX_NAME || !name.split('/').all(segment) {
        return Err(RemoteError::Invalid(format!("remote file name {name:?}")));
    }
    Ok(())
}

/// Refuses metadata a provider could not carry as headers unchanged.
pub fn check_metadata(metadata: &Metadata) -> Result<(), RemoteError> {
    for (key, value) in metadata {
        if key.is_empty()
            || key.len() > 64
            || !key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || value.len() > 256
            || !value.bytes().all(|b| (0x21..0x7f).contains(&b))
        {
            return Err(RemoteError::Invalid(format!("metadata {key:?}")));
        }
    }
    Ok(())
}

/// Opens `path` and checks `span` (default: the whole file) lies within it.
pub fn open_span(path: &Path, span: Option<Span>) -> Result<(File, Span), RemoteError> {
    let file = File::open(path).map_err(|e| local("open upload source", e))?;
    let bytes = file
        .metadata()
        .map_err(|e| local("stat upload source", e))?
        .len();
    let span = span.unwrap_or(Span {
        offset: 0,
        length: bytes,
    });
    if span
        .offset
        .checked_add(span.length)
        .is_none_or(|end| end > bytes)
    {
        return Err(RemoteError::Invalid(format!(
            "span {span:?} beyond a {bytes}-byte file"
        )));
    }
    Ok((file, span))
}
