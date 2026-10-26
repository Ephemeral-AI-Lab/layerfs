//! Paged deletion of reference pieces: stale rows after a shrink and the
//! rows of a retired or folded layer.
use crate::{
    maintenance::{Item, STALE_PIECES},
    sql, Overlay, OverlayResult, StatementKind,
};

impl Overlay {
    /// Drops the lowest pieces of one layer, at most `rows`, in one
    /// statement, and returns how many it dropped.
    pub(crate) fn drop_pieces(&self, serial: i64, gen: i64, rows: usize) -> OverlayResult<u64> {
        let last = rows.saturating_sub(1) as i64;
        self.execute(
            StatementKind::Reclaim,
            sql::PIECES_DROP,
            &[&gen, &serial, &last],
            32,
        )
    }
    pub(crate) fn drop_piece(&self, (gen, serial): (i64, i64), offset: i64) -> OverlayResult<()> {
        self.execute(
            StatementKind::Reclaim,
            sql::PIECE_DROP,
            &[&gen, &serial, &offset],
            32,
        )?;
        Ok(())
    }
    /// One step of the stale-piece item a shrink queued: one page of the
    /// layer's pieces beyond the item's cursor, each deleted if a shrink hid
    /// it. A live piece beyond the cursor is one a later claim wrote.
    pub(crate) fn clean_pieces(&self, item: &Item) -> OverlayResult<(u64, u64, bool)> {
        debug_assert_eq!(item.kind, STALE_PIECES);
        let layer = self
            .layers(item.resource, item.target, item.target - 1)?
            .pop();
        let key = (item.target, item.resource);
        let rows = self.query(
            StatementKind::Reclaim,
            sql::PIECE_PAGE,
            &[&item.target, &item.resource, &item.cursor],
            32,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (offset, stamp) in &rows {
            let stale = match layer {
                Some(ref layer) => self.stale(item.resource, layer, *offset, *stamp)?,
                None => true,
            };
            if stale {
                self.drop_piece(key, *offset)?;
            }
        }
        match rows.last() {
            Some((offset, _)) => self.advance_item(item, item.phase, *offset, -1, &[])?,
            None => self.finish_item(item)?,
        }
        Ok((rows.len() as u64, 0, rows.is_empty()))
    }
    /// One page of the pieces of a retired generation, in key order from the
    /// item's cursor: every row is deleted unless an orphan still reads its
    /// layer. Returns the rows visited.
    pub(crate) fn retire_pieces(&self, item: &Item, phase: i64) -> OverlayResult<usize> {
        let gen = item.target;
        let rows = self.query(
            StatementKind::Reclaim,
            sql::PIECE_GENERATION_PAGE,
            &[&gen, &item.cursor, &item.aux],
            32,
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for (serial, offset) in &rows {
            if !self.orphan_holds(*serial, gen)? {
                self.drop_piece((gen, *serial), *offset)?;
            }
        }
        if let Some((serial, offset)) = rows.last() {
            self.advance_item(item, phase, *serial, *offset, &[])?;
        }
        Ok(rows.len())
    }
}
