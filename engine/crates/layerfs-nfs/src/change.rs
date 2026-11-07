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
    kernel::{Kernel, Table},
    names,
};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_mount_ports::operations::{
    create, link, remove, rename,
    write::{self, AttributeChange, Stamp},
    Declined, KernelMutation, MutationFailure, MutationInput, MutationRequest, Published,
};

/// Pre- and post-operation attributes of one inode or directory.
pub type Wcc = (Option<ViewStat>, Option<ViewStat>);
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

/// The descriptor the export holds on `serial`: its implicit file
/// descriptor, or an enumeration of the directory.
fn custody(table: &Table, serial: u64) -> Option<u64> {
    table
        .files
        .get(&serial)
        .map(|held| held.handle)
        .or_else(|| {
            table
                .directories
                .iter()
                .find(|(_, (held, _))| *held == serial)
                .map(|(handle, _)| *handle)
        })
}

/// What an unlinking or replacing mutation may retire in its own publishing
/// job: the serial `name` was last bound to by this export, and the descriptor
/// that custody lives in. Both absent: the export gives it back afterwards.
#[derive(Clone, Copy, Default)]
struct Retire {
    serial: Option<u64>,
    handle: Option<u64>,
}
impl Retire {
    /// The request selects a publishing job that retires the target.
    fn open(self) -> Option<bool> {
        self.serial.map(|_| false)
    }
}

impl Kernel {
    /// One kernel mutation. Its decided [`KernelMutation`] goes to `tickets`
    /// whatever the answer; a failure is relinquished or retained.
    fn mutate(
        &self,
        protected: u64,
        handle: Option<u64>,
        open: Option<bool>,
        expect: Expect,
        tickets: &mut Tickets,
        build: impl FnOnce(Time) -> Result<MutationInput, Declined>,
    ) -> Result<Published, Status> {
        let now = write::time(SystemTime::now()).ok_or(Status::Io)?;
        let input = build(now).map_err(declined)?;
        let (services, request) = self.request()?;
        let request = MutationRequest {
            mount: self.mount(),
            request,
            protected,
            handle,
            input,
            open,
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
        let published = match mutation.value() {
            Err(reason) => {
                tickets.0.push(mutation);
                return Err(declined(reason));
            }
            Ok(published) => Published {
                stat: published.stat.clone(),
                file: published.file,
                changed: published.changed,
                wcc: published.wcc.clone(),
                detached: published.detached.clone(),
                moved: published.moved.clone(),
            },
        };
        if let Expect::Stat(serial) = expect {
            let composable = published
                .stat
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
        // The publishing visit is where a directory's own facts come from:
        // remember them, so a companion change that names the directory
        // without touching it can answer its `wcc_data` (see
        // [`Kernel::directory_wcc`]).
        for after in published.wcc.iter().filter_map(|wcc| wcc.1.as_ref()) {
            self.observed(after);
        }
        Ok(published)
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
    ) -> Result<(ViewStat, Wcc), Status> {
        if self.refuses_apple_double() && names::apple_double(name) && name.len() <= 255 {
            return self.companion_create(parent, name, how, tickets);
        }
        let checked = self.checked(name, true)?;
        let key = (parent, name.to_vec());
        if let How::Exclusive(verifier) = how {
            // A retransmitted EXCLUSIVE CREATE finds its own file.
            let known = self.table().verifiers.get(&key).copied();
            if let Some((stored, serial, _)) = known {
                if stored == verifier {
                    if let Some(stat) = self.existing(parent, name)? {
                        if stat.serial == serial {
                            return Ok((stat, (None, None)));
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
        let created = self.mutate(
            parent,
            None,
            Some(true),
            Expect::Stat(None),
            tickets,
            |_| Ok(MutationInput::Named(create::create(parent, checked, mode))),
        );
        let (stat, file, directory) = match (created, how) {
            (Ok(published), _) => (
                published.stat.ok_or(Status::ServerFault)?,
                published.file,
                published.wcc[0].clone(),
            ),
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
                        let (stat, _) = self.setattr(stat.serial, sized, None, tickets)?;
                        Ok((stat, (None, None)))
                    }
                    _ => Ok((stat, (None, None))),
                };
            }
            (Err(status), _) => return Err(status),
        };
        self.entry_taken(&stat, parent, name);
        if let Some(file) = file {
            self.file_taken(stat.serial, file.owner_id(), &stat, true);
        }
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
        let stat = self.settle(stat, rest, tickets)?;
        Ok((stat, directory))
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
            .map(|(stat, _)| stat)
    }
    /// MKDIR.
    pub fn mkdir(
        &self,
        parent: u64,
        name: &[u8],
        sattr: Sattr,
        tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        let checked = self.checked(name, true)?;
        let mode = sattr.mode.unwrap_or(DIRECTORY_MODE);
        let published = self.mutate(parent, None, None, Expect::Stat(None), tickets, |_| {
            Ok(MutationInput::Named(create::mkdir(parent, checked, mode)))
        })?;
        let stat = published.stat.ok_or(Status::ServerFault)?;
        self.entry_taken(&stat, parent, name);
        let rest = Sattr {
            mode: None,
            size: None,
            ..sattr
        };
        let stat = self.settle(stat, rest, tickets)?;
        Ok((stat, published.wcc[0].clone()))
    }
    /// SYMLINK. A symlink's mode is fixed; its attributes are not applied.
    pub fn symlink(
        &self,
        parent: u64,
        name: &[u8],
        target: &[u8],
        tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        let checked = self.checked(name, true)?;
        let published = self.mutate(parent, None, None, Expect::Stat(None), tickets, |_| {
            create::symlink(parent, checked, target, LINK_PAGE).map(MutationInput::Named)
        })?;
        let stat = published.stat.ok_or(Status::ServerFault)?;
        self.entry_taken(&stat, parent, name);
        Ok((stat, published.wcc[0].clone()))
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
    /// Selects whether an unlinking mutation can retire synthetic kernel
    /// custody on a detached (`nlink == 0`) target in the publishing job.
    /// A name that a silly name may keep alive is retired afterwards instead.
    fn prepare_unlink(&self, parent: u64, name: &[u8], rmdir: bool) -> Retire {
        let table = self.table();
        let known = table.by_name.get(&(parent, name.to_vec())).copied();
        if !rmdir
            && (known.is_some_and(|serial| table.pinned(serial))
                || (known.is_none() && !table.silly.is_empty()))
        {
            return Retire::default();
        }
        Retire {
            serial: known,
            handle: known.and_then(|serial| custody(&table, serial)),
        }
    }
    /// A replacing RENAME detaches its destination when that was its last
    /// name, and retires the export's custody of it in the publishing job as
    /// an unlink does. The destination's kind is not known here, so the
    /// descriptor is whichever one the export holds; a name the export never
    /// bound holds no reference of ours to give back.
    fn prepare_replace(&self, parent: u64, name: &[u8]) -> Retire {
        let table = self.table();
        let serial = table.by_name.get(&(parent, name.to_vec())).copied();
        if serial.is_some_and(|serial| table.pinned(serial)) {
            return Retire::default();
        }
        Retire {
            serial,
            handle: serial.and_then(|serial| custody(&table, serial)),
        }
    }
    /// Drops the name an unlinking mutation removed, and gives back what the
    /// export held for the inode it detached. Only the inode the publishing
    /// job itself retired skips the release jobs.
    fn finish_unlink(&self, parent: u64, name: &[u8], retire: Retire, detached: Option<ViewStat>) {
        self.table().by_name.remove(&(parent, name.to_vec()));
        let Some(target) = detached else {
            return;
        };
        let last = target.kind == InodeKind::Directory || target.namespace_refs == 0;
        if !last || self.table().pinned(target.serial) {
            return;
        }
        if retire.serial == Some(target.serial) {
            self.unlinked_retired(target.serial, retire.handle);
        } else {
            self.unlinked(target.serial);
        }
    }
    /// REMOVE of a non-directory name.
    pub fn remove(&self, parent: u64, name: &[u8], tickets: &mut Tickets) -> Result<Wcc, Status> {
        if let Some(wcc) = self.companion_remove(parent, name) {
            return Ok(wcc);
        }
        if self.silly_entry(parent, name).is_some() {
            self.drop_silly(parent, name);
            return Ok((None, None));
        }
        let checked = self.checked(name, false)?;
        let retire = self.prepare_unlink(parent, name, false);
        let published = self.mutate(
            parent,
            retire.handle,
            retire.open(),
            Expect::Nothing,
            tickets,
            |_| Ok(MutationInput::Named(remove::unlink(parent, checked))),
        )?;
        self.finish_unlink(parent, name, retire, published.detached);
        Ok(published.wcc[0].clone())
    }
    /// RMDIR.
    pub fn rmdir(&self, parent: u64, name: &[u8], tickets: &mut Tickets) -> Result<Wcc, Status> {
        let checked = self.checked(name, false)?;
        let retire = self.prepare_unlink(parent, name, true);
        let published = self.mutate(
            parent,
            retire.handle,
            retire.open(),
            Expect::Nothing,
            tickets,
            |_| Ok(MutationInput::Named(remove::rmdir(parent, checked))),
        )?;
        self.finish_unlink(parent, name, retire, published.detached);
        Ok(published.wcc[0].clone())
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
    ) -> Result<[Wcc; 2], Status> {
        if let Some(renamed) = self.companion_rename(from, name, to, new_name) {
            return renamed.map(|()| [(None, None), (None, None)]);
        }
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
            return Ok([(None, None), (None, None)]);
        }
        let source_name = self.checked(name, false)?;
        let destination_name = self.checked(new_name, true)?;
        if names::silly(new_name) {
            // The client renamed a file it still has open: only a regular
            // file keeps such a name, so this case reads the entry once.
            let source = self.existing(from, name)?.ok_or(Status::NoEnt)?;
            if source.kind == InodeKind::RegularFile {
                // Keep a writable descriptor before the last name may go.
                self.open_file(source.serial, true)?;
                let unpublished =
                    self.mutate(from, None, None, Expect::Nothing, tickets, |_| {
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
                return Ok([unpublished.wcc[0].clone(), (None, None)]);
            }
        }
        let retire = self.prepare_replace(to, new_name);
        let published = self.mutate(
            from,
            retire.handle,
            retire.open(),
            Expect::Nothing,
            tickets,
            |_| {
                rename::rename(from, source_name, to, destination_name, 0).map(MutationInput::Named)
            },
        )?;
        self.finish_unlink(to, new_name, retire, published.detached.clone());
        self.renamed_taken(from, name, to, new_name, published.moved.as_ref());
        Ok(published.wcc.clone())
    }
    /// Records where a completed RENAME left the moved inode: the facts the
    /// publishing job carried, or - for a rename between two names of one
    /// inode, which publishes no binding - the export's index of the source.
    fn renamed_taken(
        &self,
        from: u64,
        name: &[u8],
        to: u64,
        new_name: &[u8],
        moved: Option<&ViewStat>,
    ) {
        let known = match moved {
            Some(moved) => Some((moved.serial, moved.kind == InodeKind::Directory)),
            None => {
                let table = self.table();
                let serial = table.by_name.get(&(from, name.to_vec())).copied();
                serial.map(|serial| (serial, table.parents.contains_key(&serial)))
            }
        };
        let Some((serial, directory)) = known else {
            return;
        };
        if directory {
            self.saw_directory(serial, to);
        }
        self.renamed(serial, to, new_name);
    }
    /// LINK: one more name for `serial`.
    pub fn link(
        &self,
        serial: u64,
        parent: u64,
        name: &[u8],
        tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        if crate::apple_double::companion(serial) {
            return Err(Status::Access);
        }
        let checked = self.checked(name, true)?;
        let published = self.mutate(
            parent,
            None,
            None,
            Expect::Stat(Some(serial)),
            tickets,
            |_| Ok(MutationInput::Named(link::link(serial, parent, checked))),
        )?;
        let stat = published.stat.ok_or(Status::ServerFault)?;
        // The engine took one more lookup reference; the export holds one.
        self.entry_taken(&stat, parent, name);
        Ok((stat, published.wcc[0].clone()))
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
    ) -> Result<(ViewStat, Wcc), Status> {
        if crate::apple_double::companion(serial) {
            return self.companion_setattr(serial, sattr, guard, tickets);
        }
        if let Some(expected) = guard {
            let current = self.getattr(serial)?;
            if crate::attributes::ctime(&current) != expected {
                return Err(Status::NotSync);
            }
        }
        if sattr.is_empty() || self.refreshed(serial, &sattr) {
            let stat = self.getattr(serial)?;
            return Ok((stat.clone(), (Some(stat.clone()), Some(stat))));
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
        let published = self.mutate(
            serial,
            handle,
            None,
            Expect::Stat(Some(serial)),
            tickets,
            |now| write::set_attributes(serial, handle.is_some(), identity, change, now),
        )?;
        let stat = published.stat.ok_or(Status::ServerFault)?;
        self.observed(&stat);
        Ok((stat, published.wcc[0].clone()))
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
    ) -> Result<(u32, Wcc), Status> {
        if crate::apple_double::companion(serial) {
            return self.companion_write(serial, offset, data);
        }
        let open = self.open_file(serial, true)?;
        if data.is_empty() {
            let stat = self.getattr(serial)?;
            return Ok((0, (Some(stat.clone()), Some(stat))));
        }
        let mut written = 0usize;
        let mut before = None;
        let mut last = None;
        for window in data.chunks(WRITE_WINDOW) {
            let at = offset.checked_add(written as u64).ok_or(Status::FBig)?;
            let published = self.mutate(
                serial,
                Some(open.handle),
                None,
                Expect::Stat(Some(serial)),
                tickets,
                |_| write::write(at, window, false),
            );
            match published {
                Ok(published) => {
                    if written == 0 {
                        before = published.wcc[0].0.clone();
                    }
                    written += window.len();
                    last = published.stat;
                }
                Err(status) if written == 0 => return Err(status),
                Err(_) => break,
            }
        }
        let stat = last.ok_or(Status::ServerFault)?;
        self.observed(&stat);
        Ok((
            u32::try_from(written).unwrap_or(u32::MAX),
            (before, Some(stat)),
        ))
    }
    /// COMMIT. Every WRITE was answered `FILE_SYNC`, so nothing is
    /// uncommitted and no work is owed. The attributes are the ones this
    /// export last published or observed for the file, so a client's `fsync`
    /// costs no owner job; a file this export holds no descriptor for is read
    /// as any other GETATTR.
    pub fn commit(&self, serial: u64) -> Result<ViewStat, Status> {
        if crate::apple_double::companion(serial) {
            return self.companion_stat(serial);
        }
        match self.held_stat(serial) {
            Some(stat) => Ok(stat),
            None => self.getattr(serial),
        }
    }
}
