//! Store-owned immutable segments; failure retains named physical custody.
use super::{
    connection::SqlWork,
    segment_io as io,
    segment_layout::{self, Segment},
};
use crate::{backend::records::BackendError, SqlitePersistenceProfile};
use layerfs_storage::{policy, port::PublishedPack};
use std::{
    cell::RefCell,
    fs::{File, OpenOptions, Permissions},
    io::Write,
    os::unix::fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Instant,
};
pub(crate) struct SegmentOwner {
    path: PathBuf,
    device: i64,
    inode: i64,
    durable: bool,
}
impl SegmentOwner {
    pub(crate) fn open(
        database: &Path,
        create: bool,
        profile: SqlitePersistenceProfile,
        work: &RefCell<SqlWork>,
    ) -> Result<Self, BackendError> {
        let absolute = if database.is_absolute() {
            database.to_owned()
        } else {
            std::env::current_dir()
                .map_err(io::filesystem)?
                .join(database)
        };
        let parent = absolute
            .parent()
            .ok_or(BackendError::Integrity)?
            .canonicalize()
            .map_err(io::filesystem)?;
        let mut name = absolute
            .file_name()
            .ok_or(BackendError::Integrity)?
            .to_os_string();
        name.push(".payload");
        let path = parent.join(name);
        if create {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(io::filesystem)?;
        }
        let m = std::fs::symlink_metadata(&path).map_err(io::filesystem)?;
        if !m.is_dir() {
            return Err(BackendError::Integrity);
        }
        let (device, inode) = segment_layout::identity(m.dev(), m.ino())?;
        let owner = Self {
            path,
            device,
            inode,
            durable: profile == SqlitePersistenceProfile::Durable,
        };
        if create && owner.durable {
            owner.with_directory(work, |directory| io::synchronize(directory, true, work))?;
            let parent = directory(&parent)?;
            let result = io::synchronize(&parent, true, work);
            io::finish(result, parent, work)?;
        }
        Ok(owner)
    }
    pub(crate) fn identity(&self) -> (i64, i64) {
        (self.device, self.inode)
    }
    fn check_directory(&self, file: &File) -> Result<(), BackendError> {
        let fd = file.metadata().map_err(io::filesystem)?;
        let path = std::fs::symlink_metadata(&self.path).map_err(io::filesystem)?;
        if !fd.is_dir()
            || !path.is_dir()
            || fd.dev() != self.device as u64
            || fd.ino() != self.inode as u64
            || fd.dev() != path.dev()
            || fd.ino() != path.ino()
        {
            return Err(BackendError::Integrity);
        }
        Ok(())
    }
    fn with_directory<T>(
        &self,
        work: &RefCell<SqlWork>,
        operation: impl FnOnce(&File) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let file = directory(&self.path)?;
        let result = (|| {
            self.check_directory(&file)?;
            let value = operation(&file)?;
            self.check_directory(&file)?;
            Ok(value)
        })();
        io::finish(result, file, work)
    }
    pub(crate) fn prepare(
        &self,
        packs: &[&PublishedPack],
        work: &RefCell<SqlWork>,
    ) -> Result<Segment, BackendError> {
        let id = packs
            .iter()
            .map(|p| p.info.pack_id)
            .min()
            .ok_or(BackendError::Integrity)?;
        let length = packs
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(p.body.len()))
            .ok_or(BackendError::Capacity)?;
        if id <= 0 || length > policy::SINGLETON_PACK_LIMIT {
            return Err(BackendError::Capacity);
        }
        self.with_directory(work, |directory| {
            let name = format!("{id:016x}.segment");
            let mut file = io::open(directory, &name, true)?;
            let result = (|| {
                let m = file.metadata().map_err(io::filesystem)?;
                let (device, inode) = segment_layout::identity(m.dev(), m.ino())?;
                if device != self.device {
                    return Err(BackendError::Integrity);
                }
                io::custody(directory, &name, &file, device, inode, 0)?;
                for pack in packs {
                    work.borrow_mut().segment_write_calls += 1;
                    let start = Instant::now();
                    let wrote = file.write_all(&pack.body).map_err(io::filesystem);
                    let mut w = work.borrow_mut();
                    w.segment_write_ns += start.elapsed().as_nanos() as u64;
                    if wrote.is_ok() {
                        w.segment_write_bytes += pack.body.len() as u64;
                    }
                    drop(w);
                    wrote?;
                }
                file.set_permissions(Permissions::from_mode(0o400))
                    .map_err(io::filesystem)?;
                io::custody(directory, &name, &file, device, inode, length)?;
                if self.durable {
                    io::synchronize(&file, false, work)?;
                    io::synchronize(directory, true, work)?;
                }
                io::custody(directory, &name, &file, device, inode, length)?;
                Ok(Segment {
                    id,
                    device,
                    inode,
                    length,
                })
            })();
            // Neither definite nor uncertain failures unlink this exclusively
            // named file. Consumed physical IDs cannot be reused on a guess.
            io::finish(result, file, work)
        })
    }
    pub(crate) fn read<T>(
        &self,
        segment: &Segment,
        offset: usize,
        length: usize,
        work: &RefCell<SqlWork>,
        acquire: impl FnOnce(
            &mut dyn FnMut(usize, &mut [u8]) -> Result<(), BackendError>,
        ) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        if offset
            .checked_add(length)
            .filter(|end| *end <= segment.length)
            .is_none()
        {
            return Err(BackendError::Integrity);
        }
        self.with_directory(work, |directory| {
            let name = format!("{:016x}.segment", segment.id);
            let file = io::open(directory, &name, false)?;
            let result = (|| {
                io::custody(
                    directory,
                    &name,
                    &file,
                    segment.device,
                    segment.inode,
                    segment.length,
                )?;
                let got = acquire(&mut |start, bytes| {
                    if start
                        .checked_add(bytes.len())
                        .filter(|end| *end <= length)
                        .is_none()
                    {
                        return Err(BackendError::Integrity);
                    }
                    work.borrow_mut().segment_read_calls += 1;
                    let clock = Instant::now();
                    let result = file
                        .read_exact_at(bytes, (offset + start) as u64)
                        .map_err(io::filesystem);
                    let mut w = work.borrow_mut();
                    w.segment_read_ns += clock.elapsed().as_nanos() as u64;
                    if result.is_ok() {
                        w.segment_read_bytes += bytes.len() as u64;
                    }
                    drop(w);
                    result
                })?;
                io::custody(
                    directory,
                    &name,
                    &file,
                    segment.device,
                    segment.inode,
                    segment.length,
                )?;
                Ok(got)
            })();
            io::finish(result, file, work)
        })
    }
}
fn directory(path: &Path) -> Result<File, BackendError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)
        .map_err(io::filesystem)
}
