//! The changing procedures over the kernel mutation ports: one decided plan
//! per engine visit, and every publication ticket released only after the
//! call's single reply attempt.
//!
//! Each procedure runs [`KernelMutation::perform`], the same orchestration
//! the FUSE connection uses: the serial of a creating operation is reserved
//! there (an exact allocator contention is the declined `Contended`, served
//! as `NFS3ERR_JUKEBOX` so the client retries later with nothing created),
//! need rounds read base facts, and the one publishing visit hands back a
//! [`KernelMutation`] that still owns its ticket. The procedure puts it in
//! the call's [`Tickets`]; the connection releases them with
//! [`Tickets::replied`] after `write_record` returned, whether or not the
//! reply reached the client. A failure that keeps custody is retained, never
//! released or undone on a guess; a fenced step or a failed base demand of
//! this request alone is relinquished, as the FUSE connection does.
use crate::{
    attributes::{refusal, Status},
    block::wait,
    kernel::Kernel,
    names,
};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_fuse::operations::{
    create, link, remove, rename,
    write::{self, AttributeChange, Stamp},
    Declined, KernelMutation, MutationFailure, MutationInput, MutationRequest,
};
use layerfs_overlay::WRITE_WINDOW;
use layerfs_workspace::{Time, ViewStat};
use std::{io, sync::atomic::Ordering, time::SystemTime};

/// The page a client reads a symlink target into; a longer target could not
/// be read back whole and is refused at creation, as on the FUSE side.
pub const LINK_PAGE: u32 = 4096;
/// Modes when a CREATE or MKDIR names none (clients normally send one).
const FILE_MODE: u32 = 0o644;
const DIRECTORY_MODE: u32 = 0o755;
/// EXCLUSIVE CREATE carries no attributes; the client sets them right after.
const EXCLUSIVE_MODE: u32 = 0o600;

/// Mutations decided while serving one call, each owed exactly one release
/// of its publication ticket after that call's reply attempt. A declined or
/// unchanged mutation holds no ticket; its release issues no job.
#[derive(Default)]
#[must_use = "every decided mutation is owed its post-reply release"]
pub struct Tickets(Vec<KernelMutation>);
impl Tickets {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Called only after the reply's single send attempt returned. A release
    /// that fails is retained with its remaining custody.
    pub fn replied(self, kernel: &Kernel) {
        for mutation in self.0 {
            if let Err(failure) = wait(mutation.replied()) {
                kernel.retain(failure);
            }
        }
    }
}

/// The settable attributes of `sattr3`. Access time is accepted and not
/// stored (the Workspace keeps one timestamp; the mount is `noatime`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sattr {
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub size: Option<u64>,
    pub mtime: Option<Stamp>,
}
impl Sattr {
    fn change(self) -> AttributeChange {
        AttributeChange {
            mode: self.mode,
            owner: self.uid,
            group: self.gid,
            size: self.size,
            mtime: self.mtime,
        }
    }
    fn is_empty(self) -> bool {
        self.change() == AttributeChange::default()
    }
}
/// `createhow3`.
#[derive(Clone, Copy, Debug)]
pub enum How {
    Unchecked(Sattr),
    Guarded(Sattr),
    Exclusive([u8; 8]),
}
/// What a published result must carry for its reply to be composed.
#[derive(Clone, Copy)]
enum Expect {
    Nothing,
    /// The published inode, with this serial when given.
    Stat(Option<u64>),
}
/// A declined mutation as the status the client sees.
pub fn declined(reason: Declined) -> Status {
    match reason {
        Declined::Refused(value) => refusal(value),
        Declined::NoSpace => Status::NoSpc,
        Declined::Contended => Status::Jukebox,
        Declined::TargetTooLong => Status::NameTooLong,
    }
}

impl Kernel {
    /// One kernel mutation. Its decided [`KernelMutation`] goes to `tickets`
    /// whatever the answer; a failure is relinquished or retained.
    fn mutate(
        &self,
        protected: u64,
        handle: Option<u64>,
        expect: Expect,
        tickets: &mut Tickets,
        build: impl FnOnce(Time) -> Result<MutationInput, Declined>,
    ) -> Result<Option<ViewStat>, Status> {
        let now = write::time(SystemTime::now()).ok_or(Status::Io)?;
        let input = build(now).map_err(declined)?;
        let (services, request) = self.request()?;
        let request = MutationRequest {
            mount: self.mount(),
            request,
            protected,
            handle,
            input,
            open: None,
            now,
        };
        let mutation = match wait(KernelMutation::perform(services, request)) {
            Ok(mutation) => mutation,
            Err(failure) => {
                let status = if crate::kernel::failure_is_stale(&failure.reason) {
                    Status::Stale
                } else {
                    Status::Io
                };
                self.failed(failure);
                return Err(status);
            }
        };
        let stat = match mutation.value() {
            Err(reason) => {
                tickets.0.push(mutation);
                return Err(declined(reason));
            }
            Ok(published) => published.stat.clone(),
        };
        if let Expect::Stat(serial) = expect {
            let composable = stat
                .as_ref()
                .is_some_and(|stat| serial.is_none_or(|serial| serial == stat.serial));
            if !composable {
                // The reply cannot be composed from the published result:
                // an error for the client and a retained mutation.
                let reason = io::Error::other("NFS reply composition").into();
                self.retain(mutation.retain(reason));
                return Err(Status::Io);
            }
        }
        tickets.0.push(mutation);
        Ok(stat)
    }
    fn failed(&self, failure: MutationFailure) {
        if failure.fenced() || failure.base_demand().is_some() {
            if let Err(failure) = wait(failure.relinquish()) {
                self.retain(failure);
            }
        } else {
            self.retain(failure);
        }
    }
    /// A name a changing procedure may bind (`creating`) or remove.
    fn checked(&self, name: &[u8], creating: bool) -> Result<PathName, Status> {
        match name {
            b"" => return Err(Status::Inval),
            b"." | b".." => {
                return Err(if creating {
                    Status::Exist
                } else {
                    Status::Inval
                })
            }
            _ if name.len() > 255 => return Err(Status::NameTooLong),
            _ if creating && self.refuses_apple_double() && names::apple_double(name) => {
                self.names_refused.fetch_add(1, Ordering::Relaxed);
                return Err(Status::Access);
            }
            _ => {}
        }
        PathName::from_bytes(name).map_err(|_| Status::Inval)
    }
    /// The entry a name binds now, or none.
    fn existing(&self, parent: u64, name: &[u8]) -> Result<Option<ViewStat>, Status> {
        match self.lookup(parent, name) {
            Ok(stat) => Ok(Some(stat)),
            Err(Status::NoEnt) => Ok(None),
            Err(status) => Err(status),
        }
    }
    /// CREATE of a regular file.
    pub fn create(
        &self,
        parent: u64,
        name: &[u8],
        how: How,
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let checked = self.checked(name, true)?;
        let key = (parent, name.to_vec());
        if let How::Exclusive(verifier) = how {
            // A retransmitted EXCLUSIVE CREATE finds its own file.
            let known = self.table().verifiers.get(&key).copied();
            if let Some((stored, serial, _)) = known {
                if stored == verifier {
                    if let Some(stat) = self.existing(parent, name)? {
                        if stat.serial == serial {
                            return Ok(stat);
                        }
                    }
                }
            }
        }
        let (mode, rest) = match how {
            How::Unchecked(sattr) | How::Guarded(sattr) => (
                sattr.mode.unwrap_or(FILE_MODE),
                Sattr {
                    mode: None,
                    ..sattr
                },
            ),
            How::Exclusive(_) => (EXCLUSIVE_MODE, Sattr::default()),
        };
        let created = self.mutate(parent, None, Expect::Stat(None), tickets, |_| {
            Ok(MutationInput::Named(create::create(parent, checked, mode)))
        });
        let stat = match (created, how) {
            (Ok(stat), _) => stat.ok_or(Status::ServerFault)?,
            (Err(Status::Exist), How::Unchecked(sattr)) => {
                // UNCHECKED over an existing regular file: only its size is
                // applied, as an open with truncation would.
                let stat = self.existing(parent, name)?.ok_or(Status::Exist)?;
                if stat.kind != InodeKind::RegularFile {
                    return Err(Status::Exist);
                }
                return match sattr.size {
                    Some(size) if size != stat.logical_len => {
                        let sized = Sattr {
                            size: Some(size),
                            ..Sattr::default()
                        };
                        self.setattr(stat.serial, sized, None, tickets)
                    }
                    _ => Ok(stat),
                };
            }
            (Err(status), _) => return Err(status),
        };
        self.entry_taken(&stat, parent, name);
        if let How::Exclusive(verifier) = how {
            let mut table = self.table();
            let tick = table.tick();
            if table.verifiers.len() >= crate::kernel::VERIFIERS_HELD {
                let oldest = table
                    .verifiers
                    .iter()
                    .min_by_key(|(_, entry)| entry.2)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    table.verifiers.remove(&oldest);
                }
            }
            table.verifiers.insert(key, (verifier, stat.serial, tick));
        }
        self.settle(stat, rest, tickets)
    }
    /// Applies what a creating call's attributes ask beyond the mode.
    fn settle(
        &self,
        stat: ViewStat,
        rest: Sattr,
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let rest = Sattr {
            size: rest.size.filter(|size| *size != stat.logical_len),
            ..rest
        };
        if rest.is_empty() {
            return Ok(stat);
        }
        self.setattr(stat.serial, rest, None, tickets)
    }
    /// MKDIR.
    pub fn mkdir(
        &self,
        parent: u64,
        name: &[u8],
        sattr: Sattr,
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let checked = self.checked(name, true)?;
        let mode = sattr.mode.unwrap_or(DIRECTORY_MODE);
        let stat = self
            .mutate(parent, None, Expect::Stat(None), tickets, |_| {
                Ok(MutationInput::Named(create::mkdir(parent, checked, mode)))
            })?
            .ok_or(Status::ServerFault)?;
        self.entry_taken(&stat, parent, name);
        let rest = Sattr {
            mode: None,
            size: None,
            ..sattr
        };
        self.settle(stat, rest, tickets)
    }
    /// SYMLINK. A symlink's mode is fixed; its attributes are not applied.
    pub fn symlink(
        &self,
        parent: u64,
        name: &[u8],
        target: &[u8],
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let checked = self.checked(name, true)?;
        let stat = self
            .mutate(parent, None, Expect::Stat(None), tickets, |_| {
                create::symlink(parent, checked, target, LINK_PAGE).map(MutationInput::Named)
            })?
            .ok_or(Status::ServerFault)?;
        self.entry_taken(&stat, parent, name);
        Ok(stat)
    }
    /// The silly-named serial `name` denotes in `parent`, if any.
    fn silly_entry(&self, parent: u64, name: &[u8]) -> Option<u64> {
        self.table().silly.get(&(parent, name.to_vec())).copied()
    }
    /// Drops one silly name; the inode goes back once no silly name keeps it.
    fn drop_silly(&self, parent: u64, name: &[u8]) {
        let serial = {
            let mut table = self.table();
            let serial = table.silly.remove(&(parent, name.to_vec()));
            serial.filter(|serial| !table.pinned(*serial))
        };
        if let Some(serial) = serial {
            self.unlinked(serial);
        }
    }
    /// REMOVE of a non-directory name.
    pub fn remove(&self, parent: u64, name: &[u8], tickets: &mut Tickets) -> Result<(), Status> {
        if self.silly_entry(parent, name).is_some() {
            self.drop_silly(parent, name);
            return Ok(());
        }
        let checked = self.checked(name, false)?;
        let target = self.existing(parent, name)?.ok_or(Status::NoEnt)?;
        self.mutate(parent, None, Expect::Nothing, tickets, |_| {
            Ok(MutationInput::Named(remove::unlink(parent, checked)))
        })?;
        if target.namespace_refs <= 1 && !self.table().pinned(target.serial) {
            self.unlinked(target.serial);
        }
        Ok(())
    }
    /// RMDIR.
    pub fn rmdir(&self, parent: u64, name: &[u8], tickets: &mut Tickets) -> Result<(), Status> {
        let checked = self.checked(name, false)?;
        let target = self.existing(parent, name)?.ok_or(Status::NoEnt)?;
        self.mutate(parent, None, Expect::Nothing, tickets, |_| {
            Ok(MutationInput::Named(remove::rmdir(parent, checked)))
        })?;
        self.unlinked(target.serial);
        Ok(())
    }
    /// RENAME, replacing the destination. A rename onto a client silly name
    /// is the source's unlink with the inode kept alive by the export.
    pub fn rename(
        &self,
        from: u64,
        name: &[u8],
        to: u64,
        new_name: &[u8],
        tickets: &mut Tickets,
    ) -> Result<(), Status> {
        if let Some(serial) = self.silly_entry(from, name) {
            // Moving a silly name to another silly name keeps it ours;
            // giving it a real name again would need a link of an unlinked
            // inode, which the Workspace refuses.
            if !names::silly(new_name) {
                return Err(Status::Access);
            }
            let mut table = self.table();
            table.silly.remove(&(from, name.to_vec()));
            table.silly.insert((to, new_name.to_vec()), serial);
            return Ok(());
        }
        let source_name = self.checked(name, false)?;
        let destination_name = self.checked(new_name, true)?;
        let source = self.existing(from, name)?.ok_or(Status::NoEnt)?;
        if names::silly(new_name) && source.kind == InodeKind::RegularFile {
            // Keep a writable descriptor before the last name may go.
            self.open_file(source.serial, true)?;
            self.mutate(from, None, Expect::Nothing, tickets, |_| {
                Ok(MutationInput::Named(remove::unlink(from, source_name)))
            })?;
            self.silly_renamed.fetch_add(1, Ordering::Relaxed);
            let replaced = {
                let mut table = self.table();
                let replaced = table.silly.insert((to, new_name.to_vec()), source.serial);
                replaced.filter(|serial| !table.pinned(*serial))
            };
            if let Some(serial) = replaced {
                self.unlinked(serial);
            }
            return Ok(());
        }
        let destination = self.existing(to, new_name)?;
        self.mutate(from, None, Expect::Nothing, tickets, |_| {
            rename::rename(from, source_name, to, destination_name, 0).map(MutationInput::Named)
        })?;
        if source.kind == InodeKind::Directory {
            self.saw_directory(source.serial, to);
        }
        self.renamed(source.serial, to, new_name);
        if let Some(replaced) = destination.filter(|stat| stat.serial != source.serial) {
            let last = replaced.kind == InodeKind::Directory || replaced.namespace_refs <= 1;
            if last && !self.table().pinned(replaced.serial) {
                self.unlinked(replaced.serial);
            }
        }
        Ok(())
    }
    /// LINK: one more name for `serial`.
    pub fn link(
        &self,
        serial: u64,
        parent: u64,
        name: &[u8],
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let checked = self.checked(name, true)?;
        let stat = self
            .mutate(parent, None, Expect::Stat(Some(serial)), tickets, |_| {
                Ok(MutationInput::Named(link::link(serial, parent, checked)))
            })?
            .ok_or(Status::ServerFault)?;
        // The engine took one more lookup reference; the export holds one.
        self.entry_taken(&stat, parent, name);
        Ok(stat)
    }
    /// SETATTR. `guard` is the client's expected ctime (seconds,
    /// nanoseconds), compared with a fresh GETATTR first: the check is not
    /// atomic with the change.
    pub fn setattr(
        &self,
        serial: u64,
        sattr: Sattr,
        guard: Option<(u32, u32)>,
        tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        if let Some(expected) = guard {
            let current = self.getattr(serial)?;
            if crate::attributes::ctime(&current) != expected {
                return Err(Status::NotSync);
            }
        }
        if sattr.is_empty() || self.refreshed(serial, &sattr) {
            return self.getattr(serial);
        }
        let identity = (self.identity().uid, self.identity().gid);
        let change = sattr.change();
        // A silly-named inode has no name left: a size change goes through
        // its descriptor, as an ftruncate would.
        let pinned = self.table().pinned(serial);
        let handle = if pinned && change.size.is_some() {
            Some(self.open_file(serial, true)?.handle)
        } else {
            None
        };
        let stat = self
            .mutate(serial, handle, Expect::Stat(Some(serial)), tickets, |now| {
                write::set_attributes(serial, handle.is_some(), identity, change, now)
            })?
            .ok_or(Status::ServerFault)?;
        self.observed_size(&stat);
        Ok(stat)
    }
    /// WRITE through the implicit writable descriptor, in engine windows of
    /// at most `WRITE_WINDOW` bytes, each its own publication. A failure after
    /// the first window is a short write of the windows already published.
    pub fn write(
        &self,
        serial: u64,
        offset: u64,
        data: &[u8],
        tickets: &mut Tickets,
    ) -> Result<(u32, ViewStat), Status> {
        let open = self.open_file(serial, true)?;
        if data.is_empty() {
            return Ok((0, self.getattr(serial)?));
        }
        let mut written = 0usize;
        let mut last = None;
        for window in data.chunks(WRITE_WINDOW) {
            let at = offset.checked_add(written as u64).ok_or(Status::FBig)?;
            let published = self.mutate(
                serial,
                Some(open.handle),
                Expect::Stat(Some(serial)),
                tickets,
                |_| write::write(at, window, false),
            );
            match published {
                Ok(stat) => {
                    written += window.len();
                    last = stat;
                }
                Err(status) if written == 0 => return Err(status),
                Err(_) => break,
            }
        }
        let stat = last.ok_or(Status::ServerFault)?;
        self.observed_size(&stat);
        Ok((u32::try_from(written).unwrap_or(u32::MAX), stat))
    }
}
