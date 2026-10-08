//! Bounded disposal after native admission and its consumers are revoked.
use crate::{
    db::unsigned, maintenance::Item, NativeMount, Overlay, OverlayError, OverlayResult,
    StatementKind,
};
impl Overlay {
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
