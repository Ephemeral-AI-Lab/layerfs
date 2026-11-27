//! What one kernel read answers this export, and what a failed one means.
//!
//! A read is served by one owner job (see [`crate::kernel`]). Its answer is
//! the entry it names and, for a LOOKUP, the directory it was named in, which
//! that job had to read anyway. A refusal keeps the directory row its own job
//! read, when it read one, so a reply's `dir_attributes` are current facts and
//! never this export's last observation: nothing ages such an observation, so
//! a client that loaded it would restart its own `actimeo` on facts that do
//! not change until the export happens to observe the directory again.
use super::{mutation::Wcc, state::Kernel, wait::wait};
use crate::protocol::attributes::{refusal, Status};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_overlay::{OverlayError, READ_WINDOW};
use layerfs_vfs::{
    ports::mount::ServiceError,
    requests::{KernelData, KernelRead, ReadDataInput, ReadFailure},
};
use layerfs_vfs::{KernelReadOperation, ViewStat};

/// One kernel read's answer, as this export uses it.
pub(crate) struct Answer {
    pub stat: ViewStat,
    /// The directory a LOOKUP named the inode in: the same visit read it, so
    /// the reply's directory attributes cost no second owner job.
    pub parent: Option<ViewStat>,
    pub file: Option<u64>,
    pub directory: Option<u64>,
}

/// A read this export refused: the status its reply answers, and the fresh
/// directory facts the refusing job read, when it read one. A LOOKUP that
/// binds no name read its parent's row in the same job, so the failure's
/// `dir_attributes` are those facts and never a last observation that no
/// time bound ages (see [`crate::kernel::Kernel::lookup`]).
pub struct Refused {
    pub status: Status,
    pub directory: Option<ViewStat>,
}
impl Refused {
    /// A refusal no visit decided: no job read a directory row for it.
    pub(crate) const fn plain(status: Status) -> Self {
        Self {
            status,
            directory: None,
        }
    }
}

fn stale(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if matches!(
            error.downcast_ref::<OverlayError>(),
            Some(OverlayError::Stale)
        ) {
            return true;
        }
        current = error.source();
    }
    false
}

/// Whether a failed step's cause chain holds an engine `Stale`.
pub(crate) fn failure_is_stale(error: &ServiceError) -> bool {
    stale(error.as_ref())
}

/// A failed port step: an engine `Stale` is the client's stale handle; any
/// other failure is an I/O error of this request alone.
pub fn failure(error: ServiceError) -> Status {
    if stale(error.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}

pub(crate) fn read_failure(failure: ReadFailure) -> Status {
    if stale(failure.reason.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}

/// The read visits: one owner job each, and what they observed.
impl Kernel {
    pub(crate) fn observe(
        &self,
        protected: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
    ) -> Result<Answer, Refused> {
        let (services, request) = self.request().map_err(|status| Refused {
            status,
            directory: None,
        })?;
        let found = wait(KernelRead::prepare(
            services, self.mount, request, protected, handle, operation,
        ))
        .map_err(|failure| Refused {
            status: read_failure(failure),
            directory: None,
        })?;
        let answer = match found.value() {
            Ok(value) => Ok(Answer {
                stat: value.stat.clone(),
                parent: value.parent.clone(),
                file: value.file.map(|file| file.owner_id()),
                directory: value.directory.map(|directory| directory.owner_id()),
            }),
            // A refusal still carries the directory row its job read, when it
            // read one: fresh facts, not this export's last observation.
            Err(status) => Err(Refused {
                status: refusal(status),
                directory: found.refused_directory().cloned(),
            }),
        };
        wait(found.dispose()).map_err(|failure| Refused {
            status: read_failure(failure),
            directory: None,
        })?;
        answer
    }
    /// GETATTR. An inode that lost its last name while a silly name keeps it
    /// is observed through its descriptor; any other inode no name reaches
    /// any more is stale.
    pub fn getattr(&self, serial: u64) -> Result<ViewStat, Status> {
        if crate::apple_double::companions::companion(serial) {
            return self.companion_stat(serial);
        }
        let handle = {
            let table = self.table();
            if table.pinned(serial) {
                table.files.get(&serial).map(|held| held.handle)
            } else {
                None
            }
        };
        let stat = self
            .observe(serial, handle, KernelReadOperation::Getattr { serial })
            .map(|answer| answer.stat)
            .map_err(|refused| match refused.status {
                Status::NoEnt => Status::Stale,
                status => status,
            })?;
        self.observed(&stat);
        Ok(stat)
    }
    /// LOOKUP of `name` in directory `parent`, including `.`, `..` and the
    /// export's own silly names. One owner job answers both the entry and,
    /// when the name resolved, the directory itself: the same job had to read
    /// the directory's row to resolve it, so the reply's directory attributes
    /// cost nothing more.
    ///
    /// A failure answers the directory's own fresh facts only when the job
    /// that failed read its row, which is every name this export looked up:
    /// a name that binds nothing still read the parent's row. `.`, `..`, a
    /// silly name and a refusal decided before any visit read no directory
    /// row and answer `None`, so the client falls back to its own `actimeo`
    /// instead of restarting its timer on facts nothing refreshes.
    pub fn lookup(
        &self,
        parent: u64,
        name: &[u8],
    ) -> Result<(ViewStat, Option<ViewStat>), Refused> {
        match name {
            b"" => return Err(Refused::plain(Status::NoEnt)),
            b"." => {
                return self
                    .getattr(parent)
                    .map(|stat| (stat, None))
                    .map_err(Refused::plain)
            }
            b".." => {
                let up = if parent == self.root() {
                    parent
                } else {
                    let known = self.table().parents.get(&parent).copied();
                    known.ok_or(Refused::plain(Status::NoEnt))?
                };
                self.held_reference(up).map_err(Refused::plain)?;
                return self
                    .getattr(up)
                    .map(|stat| (stat, None))
                    .map_err(Refused::plain);
            }
            _ if name.len() > 255 => return Err(Refused::plain(Status::NameTooLong)),
            _ => {}
        }
        // A `._` name the view holds is the Workspace's; only one it does not
        // hold may be a companion (see `crate::apple_double`).
        if self.refuses_apple_double() && crate::kernel::names::apple_double(name) {
            return self.lookup_apple_double(parent, name);
        }
        let silly = self.table().silly.get(&(parent, name.to_vec())).copied();
        if let Some(serial) = silly {
            return self
                .getattr(serial)
                .map(|stat| (stat, None))
                .map_err(Refused::plain);
        }
        self.lookup_view(parent, name)
    }
    /// LOOKUP of `name` in the Workspace view alone, asking the engine.
    pub(crate) fn lookup_view(
        &self,
        parent: u64,
        name: &[u8],
    ) -> Result<(ViewStat, Option<ViewStat>), Refused> {
        let path = PathName::from_bytes(name).map_err(|_| Refused::plain(Status::Inval))?;
        let lookup = KernelReadOperation::Lookup { parent, name: path };
        let answer = self.observe(parent, None, lookup).inspect_err(|refused| {
            refused.directory.iter().for_each(|row| self.observed(row));
        })?;
        answer.parent.iter().for_each(|row| self.observed(row));
        // The deciding visit took one engine reference; keep exactly one.
        self.entry_taken(&answer.stat, parent, name);
        self.trace_main_file("LOOKUP", parent, name, answer.stat.serial, "Ok");
        Ok((answer.stat, answer.parent))
    }
    /// Records the facts a published or observed stat reports: READ's EOF,
    /// what a COMMIT answers, and a directory's attributes, which
    /// [`Kernel::directory_wcc`] answers from.
    pub(crate) fn observed(&self, stat: &ViewStat) {
        let mut table = self.table();
        if let Some(held) = table.files.get_mut(&stat.serial) {
            held.stat = stat.clone();
        }
        if stat.kind == InodeKind::Directory {
            table.directory_stats.insert(stat.serial, stat.clone());
        }
    }
    /// `wcc_data` of a directory a change named but could not have touched:
    /// an AppleDouble companion lives in the export alone, so creating,
    /// removing or moving one never changes the directory's own facts, and
    /// the pre- and post-operation halves are the same last known stat.
    /// Answering both keeps the client from invalidating its cached
    /// attributes of the directory, which a missing post-operation half makes
    /// it do, at the cost of one GETATTR per later entry of that directory.
    /// `(None, None)` when this export has never seen the directory: the
    /// client answers that itself.
    pub(crate) fn directory_wcc(&self, parent: u64) -> Wcc {
        let table = self.table();
        match table.directory_stats.get(&parent) {
            Some(stat) => (Some(stat.clone()), Some(stat.clone())),
            None => (None, None),
        }
    }
    /// READLINK: the raw target bytes.
    pub fn readlink(&self, serial: u64) -> Result<Vec<u8>, Status> {
        if crate::apple_double::companions::companion(serial) {
            return Err(Status::Inval);
        }
        let (services, request) = self.request()?;
        let data = wait(KernelData::read(
            services,
            self.mount,
            request,
            serial,
            None,
            ReadDataInput::Link,
        ))
        .map_err(read_failure)?
        .map_err(refusal)?;
        Ok(data.bytes().to_vec())
    }
    /// READ of at most one engine window. `eof` is set when the reply reaches
    /// the size last seen for the file, or comes back short.
    pub fn read(&self, serial: u64, offset: u64, count: u32) -> Result<(Vec<u8>, bool), Status> {
        let mut data = Vec::new();
        let eof = self.read_into(serial, offset, count, &mut data)?;
        Ok((data, eof))
    }
    /// One engine window appended to `out`, which is what a reply larger than
    /// one window collects: the window's bytes are copied straight into the
    /// reply buffer, and a call serving several windows takes one descriptor
    /// and one owner visit per window. `eof` means the same as in
    /// [`Kernel::read`].
    pub(crate) fn read_into(
        &self,
        serial: u64,
        offset: u64,
        count: u32,
        out: &mut Vec<u8>,
    ) -> Result<bool, Status> {
        let length = count.min(READ_WINDOW as u32);
        if crate::apple_double::companions::companion(serial) {
            return self.companion_read(serial, offset, length, out);
        }
        let open = self.open_file(serial, false)?;
        if offset >= open.size {
            return Ok(true);
        }
        let (services, request) = self.request()?;
        let data = wait(KernelData::read(
            services,
            self.mount,
            request,
            serial,
            Some(open.handle),
            ReadDataInput::File { offset, length },
        ))
        .map_err(read_failure)?
        .map_err(refusal)?;
        let bytes = data.bytes();
        let end = offset.saturating_add(bytes.len() as u64);
        let eof = bytes.len() < length as usize || end >= open.size;
        out.extend_from_slice(bytes);
        Ok(eof)
    }
}
