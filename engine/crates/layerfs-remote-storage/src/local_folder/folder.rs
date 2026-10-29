//! Files under `root/objects/<name>`, metadata under `root/metadata/<name>`.
//! An upload writes a `.uploading` sibling and renames it into place.
use crate::driver::{
    check_metadata, check_name, open_span, Condition, Driver, Listed, Metadata, Ranged, Span, Stat,
};
use crate::error::{local, RemoteError};
use crate::limits::Limits;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// A folder remote with configurable limits (to exercise pieces).
#[derive(Debug)]
pub struct LocalFolder {
    root: PathBuf,
    limits: Limits,
}

impl LocalFolder {
    /// A remote over the existing directory `root`.
    pub fn new(root: &Path, limits: Limits) -> Result<Self, RemoteError> {
        limits.validate()?;
        if !root.is_dir() {
            return Err(RemoteError::Invalid(format!(
                "{} is not a directory",
                root.display()
            )));
        }
        Ok(Self {
            root: root.to_owned(),
            limits,
        })
    }

    fn paths(&self, name: &str) -> Result<(PathBuf, PathBuf), RemoteError> {
        check_name(name)?;
        Ok((
            self.root.join("objects").join(name),
            self.root.join("metadata").join(name),
        ))
    }
}

/// A file's last change, in milliseconds since the Unix epoch.
fn modified(found: &fs::Metadata) -> Option<u64> {
    let since = found
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    u64::try_from(since.as_millis()).ok()
}

/// A file's version tag: its size and last change to the nanosecond. An
/// upload renames a new file into place, so each upload gets a new tag.
fn etag(found: &fs::Metadata) -> Option<String> {
    let since = found
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!("\"{}-{}\"", found.len(), since.as_nanos()))
}

/// The metadata kept at `meta`; none when absent.
fn read_metadata(meta: &Path) -> Result<Metadata, RemoteError> {
    let text = match fs::read_to_string(meta) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(local("read metadata", error)),
    };
    Ok(text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect())
}

/// A writer that reports what it writes to the thread's transfer progress.
struct Counted<'a>(&'a mut File);

impl Write for Counted<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.0.write(bytes)?;
        crate::progress::add(n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Writes `fill` to `path` through a `.uploading` sibling claimed once.
fn replace(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(parent)?;
    let mut staging = path.as_os_str().to_owned();
    staging.push(".uploading");
    let staging = PathBuf::from(staging);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    fill(&mut file)?;
    drop(file);
    fs::rename(&staging, path)
}

impl Driver for LocalFolder {
    fn limits(&self) -> Limits {
        self.limits
    }

    fn upload_file(
        &self,
        name: &str,
        path: &Path,
        span: Option<Span>,
        metadata: &Metadata,
    ) -> Result<(), RemoteError> {
        let (data, meta) = self.paths(name)?;
        check_metadata(metadata)?;
        let (mut source, span) = open_span(path, span)?;
        if span.length > self.limits.max_file {
            return Err(RemoteError::TooLarge {
                bytes: span.length,
                max: self.limits.max_file,
            });
        }
        source
            .seek(SeekFrom::Start(span.offset))
            .map_err(|e| local("seek", e))?;
        let text = metadata.iter().fold(String::new(), |mut text, (k, v)| {
            text.push_str(&format!("{k}={v}\n"));
            text
        });
        replace(&meta, |file| file.write_all(text.as_bytes()))
            .map_err(|e| local("write metadata", e))?;
        replace(&data, |file| {
            let n = io::copy(&mut source.take(span.length), &mut Counted(file))?;
            if n != span.length {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source shrank",
                ));
            }
            Ok(())
        })
        .map_err(|e| local("write file", e))
    }

    fn download_file(
        &self,
        name: &str,
        span: Option<Span>,
        out: &mut dyn Write,
    ) -> Result<u64, RemoteError> {
        let (data, _) = self.paths(name)?;
        if !data.is_file() {
            return Err(RemoteError::Missing);
        }
        let (mut file, span) = open_span(&data, span)?;
        file.seek(SeekFrom::Start(span.offset))
            .map_err(|e| local("seek", e))?;
        let n = io::copy(&mut file.take(span.length), out).map_err(|e| local("copy", e))?;
        if n != span.length {
            return Err(RemoteError::Integrity(format!("{name} shrank while read")));
        }
        Ok(n)
    }

    fn stat(&self, name: &str) -> Result<Option<Stat>, RemoteError> {
        let (data, meta) = self.paths(name)?;
        let found = match fs::metadata(&data) {
            Ok(found) if found.is_file() => found,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(local("stat", error)),
        };
        Ok(Some(Stat {
            bytes: found.len(),
            metadata: read_metadata(&meta)?,
            modified: modified(&found),
            etag: etag(&found),
        }))
    }

    fn download_range(
        &self,
        name: &str,
        span: Span,
        condition: &Condition,
        out: &mut dyn Write,
    ) -> Result<Ranged, RemoteError> {
        let (data, _) = self.paths(name)?;
        let (mut file, span) = match open_span(&data, Some(span)) {
            Err(_) if !data.is_file() => return Err(RemoteError::Missing),
            opened => opened?,
        };
        let found = file.metadata().map_err(|e| local("stat", e))?;
        let held = match condition {
            Condition::Match(tag) => etag(&found).as_ref() == Some(tag),
            Condition::UnmodifiedSince(millis) => {
                modified(&found).is_some_and(|at| at / 1000 <= millis / 1000)
            }
        };
        if !held {
            return Err(RemoteError::Changed);
        }
        file.seek(SeekFrom::Start(span.offset))
            .map_err(|e| local("seek", e))?;
        let n = io::copy(&mut file.take(span.length), out).map_err(|e| local("copy", e))?;
        if n != span.length {
            return Err(RemoteError::Integrity(format!("{name} shrank while read")));
        }
        Ok(Ranged {
            bytes: n,
            etag: etag(&found),
        })
    }

    fn list(&self, prefix: &str) -> Result<Vec<Listed>, RemoteError> {
        let objects = self.root.join("objects");
        let mut found = Vec::new();
        let mut pending = vec![String::new()];
        while let Some(directory) = pending.pop() {
            let entries = match fs::read_dir(objects.join(&directory)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(local("list", error)),
            };
            for entry in entries {
                let entry = entry.map_err(|e| local("list", e))?;
                let file = entry.file_name().to_string_lossy().into_owned();
                let name = if directory.is_empty() {
                    file
                } else {
                    format!("{directory}/{file}")
                };
                let kind = entry.file_type().map_err(|e| local("list", e))?;
                if kind.is_dir() {
                    pending.push(name);
                } else if kind.is_file()
                    && name.starts_with(prefix)
                    && !name.ends_with(".uploading")
                    && check_name(&name).is_ok()
                {
                    let (_, meta) = self.paths(&name)?;
                    let stat = entry.metadata().map_err(|e| local("list", e))?;
                    found.push(Listed {
                        bytes: stat.len(),
                        modified: modified(&stat),
                        metadata: Some(read_metadata(&meta)?),
                        name,
                    });
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    fn delete(&self, name: &str) -> Result<(), RemoteError> {
        let (data, meta) = self.paths(name)?;
        for path in [data, meta] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(local("delete", error)),
            }
        }
        Ok(())
    }
}
