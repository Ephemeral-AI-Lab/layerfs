//! Files under `root/objects/<name>`, metadata under `root/metadata/<name>`.
//! An upload writes a `.uploading` sibling and renames it into place.
use crate::driver::{check_metadata, check_name, open_span, Driver, Metadata, Span, Stat};
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
            let n = io::copy(&mut source.take(span.length), file)?;
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
        let bytes = match fs::metadata(&data) {
            Ok(found) if found.is_file() => found.len(),
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(local("stat", error)),
        };
        let text = match fs::read_to_string(&meta) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(local("read metadata", error)),
        };
        let metadata = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Ok(Some(Stat { bytes, metadata }))
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
