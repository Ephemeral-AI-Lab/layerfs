//! Bounded disposal after native admission and its consumers are revoked.
use crate::{
    db::unsigned, maintenance::Item, NativeDirectory, NativeMount, OpenFile, Overlay, OverlayError,
    OverlayResult, StatementKind,
};
/// Unreleased handle windows: at most 64 small ownership rows per turn.
pub(crate) const FILE_WINDOW: &str = "SELECT owner,serial,writable FROM file_handle WHERE ns=?1 AND mount=?2 ORDER BY request LIMIT 64";
pub(crate) const DIRECTORY_WINDOW: &str = "SELECT owner,serial FROM native_directory INDEXED BY native_directory_open WHERE ns=?1 AND mount=?2 AND closed=0 ORDER BY owner LIMIT 64";
impl Overlay {
    /// One turn retires one bounded page of one ownership class: unreleased
    /// file handles, then unreleased directory handles, then lookup counts,
    /// then the mount row. Retired handle rows leave their own predicate, so
    /// only the lookup page needs the persisted cursor.
    pub(crate) fn retire_native(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let route = self.route_for_ns(item.ns)?;
        let root = self
            .query(
                StatementKind::Lease,
                "SELECT root FROM native_mount WHERE ns=?1 AND owner=?2 AND revoked=1",
                &[&item.ns, &item.resource],
                16,
                |r| unsigned(r, 0),
            )?
            .pop()
            .ok_or(OverlayError::Stale)?;
        let mount = NativeMount {
            route,
            owner: item.resource as u64,
            root,
        };
        let files = self.query(
            StatementKind::Lease,
            FILE_WINDOW,
            &[&item.ns, &item.resource],
            16,
            |r| {
                Ok(OpenFile {
                    route,
                    owner: unsigned(r, 0)?,
                    serial: unsigned(r, 1)?,
                    writable: r.get(2)?,
                })
            },
        )?;
        if !files.is_empty() {
            for file in &files {
                self.close_file_inner(*file)?;
            }
            return Ok((files.len() as u64, 0, false));
        }
        let directories = self.query(
            StatementKind::Lease,
            DIRECTORY_WINDOW,
            &[&item.ns, &item.resource],
            16,
            |r| {
                Ok(NativeDirectory {
                    mount,
                    owner: unsigned(r, 0)?,
                    serial: unsigned(r, 1)?,
                })
            },
        )?;
        if !directories.is_empty() {
            for directory in &directories {
                // Its cookies are swept by the directory's own bounded item.
                self.close_native_directory_inner(*directory)?;
            }
            return Ok((directories.len() as u64, 0, false));
        }
        let rows = self.query(StatementKind::Lease,
            "SELECT serial,owner FROM native_lookup WHERE ns=?1 AND mount=?2 AND serial>?3 ORDER BY serial LIMIT 64",
            &[&item.ns, &item.resource, &item.cursor], 24,
            |r| Ok((unsigned(r, 0)?, unsigned(r, 1)?)))?;
        for (serial, owner) in &rows {
            self.drop_native_lookup(mount, *serial, *owner)?;
        }
        if let Some((serial, _)) = rows.last() {
            self.advance_item(item, 0, *serial as i64, -1, &[])?;
            return Ok((rows.len() as u64, 0, false));
        }
        self.execute(
            StatementKind::Lease,
            "DELETE FROM native_mount WHERE ns=?1 AND owner=?2 AND revoked=1",
            &[&item.ns, &item.resource],
            16,
        )?;
        self.finish_item(item)?;
        self.queue_closed(route)?;
        Ok((1, 0, true))
    }
}
