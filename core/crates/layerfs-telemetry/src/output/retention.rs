//! Exclusive operational namespaces and bounded restart-safe segment retention.
use nix::fcntl::{Flock, FlockArg};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
pub(crate) struct Local {
    directory: PathBuf,
    slots: Vec<Option<SystemTime>>,
    active: Option<File>,
    index: usize,
    length: usize,
    segment: usize,
    expiry: Duration,
    _ownership: Flock<File>,
}
fn regular(path: &Path) -> io::Result<()> {
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::other("unowned segment type"));
    }
    Ok(())
}
impl Local {
    pub(crate) fn new(
        path: &Path,
        count: usize,
        segment: usize,
        expiry: Duration,
    ) -> io::Result<Self> {
        let fresh = match std::fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => false,
            Err(e) => return Err(e),
        };
        if !std::fs::symlink_metadata(path)?.is_dir()
            || std::fs::symlink_metadata(path)?.permissions().mode() & 0o077 != 0
        {
            return Err(io::Error::other("output namespace type"));
        }
        let marker = path.join(".layerfs-operational-v1");
        let expected = format!("layerfs operational telemetry v1\n{count} {segment}\n");
        if fresh {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&marker)?
                .write_all(expected.as_bytes())?;
        }
        regular(&marker)?;
        let mut actual = String::new();
        File::open(&marker)?.take(128).read_to_string(&mut actual)?;
        if actual != expected {
            return Err(io::Error::other("output namespace ownership/profile"));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(path.join(".writer-lock"))?;
        let ownership = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|(_, e)| io::Error::other(e))?;
        let mut local = Self {
            directory: path.to_owned(),
            slots: vec![None; count],
            active: None,
            index: 0,
            length: 0,
            segment,
            expiry,
            _ownership: ownership,
        };
        for i in 0..count {
            match std::fs::symlink_metadata(local.path(i)) {
                Ok(meta) => {
                    if !meta.is_file() || meta.len() > segment as u64 {
                        return Err(io::Error::other("retained segment bound/type"));
                    }
                    local.slots[i] = Some(meta.modified()?);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        local.maintain()?;
        local.index = local
            .slots
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                local
                    .slots
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, born)| **born)
                    .map_or(0, |(i, _)| i)
            });
        Ok(local)
    }
    fn path(&self, index: usize) -> PathBuf {
        self.directory.join(format!("segment-{index:02}.jsonl"))
    }
    pub(crate) fn maintain(&mut self) -> io::Result<()> {
        if self.active.is_some()
            && self.slots[self.index]
                .is_some_and(|born| born.elapsed().is_ok_and(|age| age >= self.expiry))
        {
            self.active.take();
            self.length = 0;
        }
        for i in 0..self.slots.len() {
            if i == self.index && self.active.is_some() {
                continue;
            }
            if self.slots[i].is_some_and(|born| born.elapsed().is_ok_and(|age| age >= self.expiry))
            {
                regular(&self.path(i))?;
                std::fs::remove_file(self.path(i))?;
                self.slots[i] = None;
            }
        }
        Ok(())
    }
    pub(crate) fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > self.segment {
            return Err(io::Error::other("segment record bound"));
        }
        self.maintain()?;
        if self.active.is_some() && self.length + bytes.len() > self.segment {
            self.active.take();
            self.index = (self.index + 1) % self.slots.len();
            self.length = 0;
        }
        if self.active.is_none() {
            if self.slots[self.index].is_some() {
                regular(&self.path(self.index))?;
                std::fs::remove_file(self.path(self.index))?;
                self.slots[self.index] = None;
            }
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.path(self.index))?;
            self.slots[self.index] = Some(SystemTime::now());
            self.active = Some(file);
        }
        self.active
            .as_mut()
            .ok_or_else(|| io::Error::other("segment closed"))?
            .write_all(bytes)?;
        self.length += bytes.len();
        Ok(())
    }
}
