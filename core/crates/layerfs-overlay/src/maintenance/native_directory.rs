//! Live bounded cookie cleanup after RELEASEDIR and last read-source release.
use crate::{db::unsigned, maintenance::Item, Overlay, OverlayError, OverlayResult, StatementKind};
impl Overlay {
    pub(crate) fn retire_native_directory(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        let safe=self.query(StatementKind::Lease,
            "SELECT closed=1 AND NOT EXISTS(SELECT 1 FROM native_directory_read WHERE ns=?1 AND directory=?2) FROM native_directory WHERE ns=?1 AND owner=?2",
            &[&item.ns, &item.resource], 16, |r| r.get::<_,bool>(0))?.pop().unwrap_or(false);
        if !safe {
            return Err(OverlayError::Stale);
        }
        let rows=self.query(StatementKind::Lease,
            "SELECT cookie,length(name) FROM native_cookie WHERE ns=?1 AND owner=?2 AND cookie>?3 ORDER BY cookie LIMIT 64",
            &[&item.ns, &item.resource, &item.cursor], 24, |r| Ok((r.get::<_,i64>(0)?,unsigned(r,1)?)))?;
        for (cookie, _) in &rows {
            self.execute(
                StatementKind::Lease,
                "DELETE FROM native_cookie WHERE ns=?1 AND owner=?2 AND cookie=?3",
                &[&item.ns, &item.resource, cookie],
                24,
            )?;
        }
        if let Some((cookie, _)) = rows.last() {
            self.advance_item(item, 0, *cookie, -1, &[])?;
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
