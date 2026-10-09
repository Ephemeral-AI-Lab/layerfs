//! Live bounded cleanup of a closed directory handle that held more
//! published replies than its RELEASEDIR deletes in its own job.
use crate::{
    db::unsigned, lifetime::INLINE_PAGES, maintenance::Item, Overlay, OverlayError, OverlayResult,
    StatementKind,
};
impl Overlay {
    /// One turn deletes one indexed window of at most `INLINE_PAGES` reply
    /// rows after the persisted cursor; the turn that finds none deletes the
    /// header and finishes the item.
    pub(crate) fn retire_native_directory(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let closed = self.query(
            StatementKind::Lease,
            "SELECT closed FROM native_directory WHERE ns=?1 AND owner=?2",
            &[&item.ns, &item.resource],
            16,
            |r| r.get::<_, bool>(0),
        )?;
        if closed != [true] {
            return Err(OverlayError::Stale);
        }
        let rows = self.query(
            StatementKind::Lease,
            crate::sql::COOKIE_PAGES,
            &[&item.ns, &item.resource, &item.cursor],
            24,
            |r| Ok((r.get::<_, i64>(0)?, unsigned(r, 1)?)),
        )?;
        let rows = &rows[..rows.len().min(INLINE_PAGES)];
        if let Some((last, _)) = rows.last() {
            self.execute(
                StatementKind::Lease,
                "DELETE FROM native_cookie WHERE ns=?1 AND owner=?2 AND first_cookie<=?3",
                &[&item.ns, &item.resource, last],
                24,
            )?;
            self.advance_item(item, 0, *last, -1, &[])?;
            return Ok((
                rows.len() as u64,
                rows.iter().map(|(_, bytes)| bytes).sum(),
                false,
            ));
        }
        self.execute(
            StatementKind::Lease,
            "DELETE FROM native_directory WHERE ns=?1 AND owner=?2",
            &[&item.ns, &item.resource],
            16,
        )?;
        self.finish_item(item)?;
        self.queue_closed(self.route_for_ns(item.ns)?)?;
        Ok((1, 0, true))
    }
}
