//! Independent source/cookie custody and one bounded local directory page.
use crate::{
    db::{integer, unsigned},
    NativeDirectory, NativeDirectoryCursor, NativeDirectoryPage, NativeDirectoryRead, NativeMount,
    Overlay, OverlayError, OverlayResult, StatementKind,
};
impl Overlay {
    pub(crate) fn native_directory_cursor(
        &self,
        directory: NativeDirectory,
        offset: u64,
    ) -> OverlayResult<NativeDirectoryCursor> {
        match offset {
            0 => Ok(NativeDirectoryCursor::Start),
            1 => Ok(NativeDirectoryCursor::AfterDot),
            2 => Ok(NativeDirectoryCursor::Names(None)),
            _ => self
                .query(
                    StatementKind::Lease,
                    "SELECT name FROM native_cookie WHERE ns=?1 AND owner=?2 AND cookie=?3",
                    &[
                        &directory.mount.route.ns,
                        &integer(directory.owner)?,
                        &integer(offset)?,
                    ],
                    24,
                    |r| r.get::<_, Vec<u8>>(0),
                )?
                .pop()
                .map(|name| NativeDirectoryCursor::Names(Some(name)))
                .ok_or(OverlayError::Stale),
        }
    }
    pub fn acquire_native_directory_read(
        &self,
        directory: NativeDirectory,
        request: u64,
        offset: u64,
    ) -> OverlayResult<NativeDirectoryRead> {
        self.atomic(|| {
            let state = self.check_native_mount(directory.mount)?;
            self.native_directory(directory.mount, directory.serial, directory.owner)?;
            let cursor = self.native_directory_cursor(directory, offset)?;
            let parent = self.native_parent(directory.mount, directory.serial)?;
            let source = self.retain_native_source(directory.mount, state, request, directory.serial)?;
            self.execute(StatementKind::Lease,
                "INSERT INTO native_directory_read(ns,owner,directory,offset,parent) VALUES(?1,?2,?3,?4,?5)",
                &[&directory.mount.route.ns, &integer(source.owner)?, &integer(directory.owner)?, &integer(offset)?, &integer(parent)?], 40)?;
            Ok(NativeDirectoryRead { source, directory, parent, cursor })
        })
    }
    pub fn retained_native_directory_read(
        &self,
        mount: NativeMount,
        request: u64,
    ) -> OverlayResult<Option<NativeDirectoryRead>> {
        let Some(source) = self.retained_native_source(mount, request)? else {
            return Ok(None);
        };
        let row = self.query(StatementKind::Lease,
            "SELECT r.directory,r.offset,r.parent,d.serial FROM native_directory_read r JOIN native_directory d ON d.ns=r.ns AND d.owner=r.directory WHERE r.ns=?1 AND r.owner=?2 AND d.mount=?3",
            &[&mount.route.ns, &integer(source.owner)?, &integer(mount.owner)?], 24,
            |r| Ok((unsigned(r,0)?,unsigned(r,1)?,unsigned(r,2)?,unsigned(r,3)?)))?.pop();
        row.map(|(owner, offset, parent, serial)| {
            let directory = NativeDirectory {
                mount,
                owner,
                serial,
            };
            Ok(NativeDirectoryRead {
                source,
                directory,
                parent,
                cursor: self.native_directory_cursor(directory, offset)?,
            })
        })
        .transpose()
    }
    pub(crate) fn check_native_directory_read(
        &self,
        read: &NativeDirectoryRead,
    ) -> OverlayResult<()> {
        self.check_native_attached(read.directory.mount)?;
        if read.source.class != 2 || read.source.route != read.directory.mount.route {
            return Err(OverlayError::Stale);
        }
        self.source_state(read.source)?;
        let row = self.query(StatementKind::Lease,
            "SELECT r.offset FROM native_directory_read r JOIN native_directory d ON d.ns=r.ns AND d.owner=r.directory WHERE r.ns=?1 AND r.owner=?2 AND r.directory=?3 AND r.parent=?4 AND d.mount=?5 AND d.serial=?6",
            &[&read.source.route.ns, &integer(read.source.owner)?, &integer(read.directory.owner)?, &integer(read.parent)?, &integer(read.directory.mount.owner)?, &integer(read.directory.serial)?], 48,
            |r| unsigned(r,0))?.pop().ok_or(OverlayError::Stale)?;
        if self.native_directory_cursor(read.directory, row)? != read.cursor {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// One read-only owner step: current local names and their local inode kinds.
    /// Existing sources continue after RELEASEDIR/logical Workspace close.
    pub fn native_directory_page(
        &self,
        read: &NativeDirectoryRead,
        after: Option<&[u8]>,
    ) -> OverlayResult<NativeDirectoryPage> {
        self.check_native_directory_read(read)?;
        if after.is_some_and(|name| name.len() > 255)
            || read
                .cursor
                .after_name()
                .is_some_and(|minimum| after.is_none_or(|name| name < minimum))
        {
            return Err(OverlayError::Invalid("native directory continuation"));
        }
        let local =
            self.source_directory_entry_window(read.source, read.directory.serial, after)?;
        let mut serials: Vec<_> = local
            .active
            .iter()
            .chain(&local.captured)
            .filter_map(|entry| entry.serial)
            .collect();
        serials.sort_unstable();
        serials.dedup();
        let mut local_kinds = Vec::with_capacity(serials.len());
        for serial in serials {
            if let Some(inode) = self.source_inode(read.source, serial)? {
                local_kinds.push((serial, inode.kind));
            }
        }
        Ok(NativeDirectoryPage {
            read: read.clone(),
            after: after.map(<[u8]>::to_vec),
            local,
            local_kinds,
        })
    }
}
