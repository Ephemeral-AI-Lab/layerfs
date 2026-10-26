//! The piece row of one layer: lookup, put, extend and cut.
use crate::{
    db::integer, layers::Layer, sql, Changes, LocalRead, Overlay, OverlayError, OverlayResult,
    PayloadWrite, ReferenceRange, StatementKind, WriteClaims, CELL_BYTES, READ_REFERENCES,
    READ_WINDOW, RUN_BYTES, WRITE_CLAIMS,
};

const CELL: u64 = CELL_BYTES as u64;
const RUN: i64 = RUN_BYTES as i64;
/// One more than the pieces that can hold a byte of one read window; the
/// literal LIMIT of `sql::PIECE_WINDOW`.
const WINDOW_ROWS: usize = READ_WINDOW / CELL_BYTES + 2;
const _: () = assert!(WINDOW_ROWS == 34);

/// One stored piece: `length` bytes of its layer from `offset` are the bytes
/// of content root `source` from `source_offset`. It starts on a cell
/// boundary. Pieces of one layer never overlap, live or stale, and a piece
/// is stale by its first byte and its stamp.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece {
    pub offset: i64,
    pub length: i64,
    pub epoch: i64,
    pub source: [u8; 32],
    pub source_offset: i64,
}
impl Piece {
    pub fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let source: Vec<u8> = row.get(3)?;
        let piece = Self {
            offset: row.get(0)?,
            length: row.get(1)?,
            epoch: row.get(2)?,
            source: source
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            source_offset: row.get(4)?,
        };
        if piece.offset < 0
            || piece.offset % CELL as i64 != 0
            || piece.length < 1
            || piece.epoch < 0
            || piece.source_offset < 0
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(piece)
    }
    pub fn end(&self) -> i64 {
        self.offset + self.length
    }
    /// Whether this piece names `source_offset` of `source` at file byte `at`.
    fn names(&self, at: i64, source: &[u8; 32], source_offset: i64) -> bool {
        self.source == *source && self.source_offset + (at - self.offset) == source_offset
    }
}
/// Window and grammar checks of the claims of one compound job, after the
/// checks of its write and before any transaction.
pub(crate) fn check_claims(changes: &Changes) -> OverlayResult<()> {
    let Some(claims) = &changes.claims else {
        return Ok(());
    };
    let invalid = Err(OverlayError::Invalid("write claims"));
    let Some(write) = &changes.write else {
        return invalid;
    };
    let end = write.offset + write.data.len() as u64;
    if claims.ranges.is_empty() || claims.ranges.len() > WRITE_CLAIMS {
        return invalid;
    }
    // Bytes an earlier job stored are at most one chunk before the window
    // for the first range, and at most one chunk after it for the last.
    let mut at = write.offset.saturating_sub(RUN_BYTES as u64);
    let last = claims.ranges.len() - 1;
    for (index, range) in claims.ranges.iter().enumerate() {
        let to = range.offset.checked_add(range.length);
        let source = range.source_offset.checked_add(range.length).map(integer);
        let limit = match index == last {
            true => end.saturating_add(RUN_BYTES as u64),
            false => end,
        };
        if range.length == 0
            || range.offset < at
            || range.offset >= end
            || (index != 0 && range.offset < write.offset)
            || to.is_none_or(|to| to > limit || to <= write.offset)
            || !matches!(source, Some(Ok(_)))
        {
            return invalid;
        }
        at = range.offset + range.length;
    }
    Ok(())
}
impl Overlay {
    /// Publishes one written window with its claims inside the caller's
    /// transaction. Whole cells of a claimed range become one piece, or
    /// extend the piece that already names the bytes before them; every
    /// other byte of the window is stored as cells, as an unclaimed write
    /// stores it. Rows changed are the window's cells and a fixed number of
    /// piece rows for each claim.
    pub(crate) fn write_claimed(
        &self,
        serial: i64,
        layer: &Layer,
        write: &PayloadWrite,
        claims: &WriteClaims,
    ) -> OverlayResult<()> {
        let end = write.offset + write.data.len() as u64;
        let bytes = |from: u64, to: u64| {
            &write.data[(from - write.offset) as usize..(to - write.offset) as usize]
        };
        // Bytes before and after the window are an earlier job's: they are
        // still the claimed bytes only if no publication wrote this row
        // since.
        let carried = claims
            .after
            .is_some_and(|after| after >= 0 && after == layer.sequence);
        let mut referenced = layer.referenced;
        // The next written byte that is neither stored nor claimed yet.
        let mut at = write.offset;
        for range in &claims.ranges {
            let (mut from, mut to) = (range.offset, range.offset + range.length);
            let mut source = range.source_offset;
            if from < write.offset && !carried {
                source += write.offset - from;
                from = write.offset;
            }
            if to > end && !(carried && to <= layer.size) {
                to = end;
            }
            // A piece is whole cells; the ragged edges stay literal bytes.
            let (first, last) = (from.div_ceil(CELL) * CELL, to / CELL * CELL);
            if first >= last {
                continue;
            }
            // Only the window's own bytes are written by this job.
            if at < first.min(end) {
                self.write_cells(serial, layer, at, bytes(at, first.min(end)))?;
            }
            let named = (claims.source, integer(source + (first - from))?);
            let cells = (integer(first)?, integer(last)?);
            self.claim(serial, layer, &mut referenced, cells, named)?;
            at = at.max(last.min(end));
        }
        if at < end {
            self.write_cells(serial, layer, at, bytes(at, end))?;
        }
        Ok(())
    }
    /// Makes cells `[from, to)` of one layer the bytes of `source` from
    /// `source_offset`. Cells of the layer inside the range are removed, so
    /// no byte is held twice; an older piece gives up the range and keeps
    /// what it names before and after it.
    fn claim(
        &self,
        serial: i64,
        layer: &Layer,
        referenced: &mut bool,
        (from, to): (i64, i64),
        (source, source_offset): ([u8; 32], i64),
    ) -> OverlayResult<()> {
        self.clear_cells(serial, layer, from, to)?;
        let key = (layer.gen, serial);
        if !*referenced {
            self.execute(
                StatementKind::Inode,
                sql::LAYER_REFERENCED,
                &[&serial, &layer.gen],
                24,
            )?;
            *referenced = true;
            let piece = (from, to - from, layer.epoch, source, source_offset);
            return self.put_piece(key, piece);
        }
        // The row that takes the range, and what an older piece names past it.
        let (mut extended, mut tail) = (None, None);
        let before = match from {
            0 => None,
            _ => self.piece_cover(layer.gen, serial, from - 1)?,
        };
        if let Some(old) = before.filter(|old| old.end() >= from) {
            let live = !self.stale(serial, layer, old.offset, old.epoch)?;
            if live && old.names(from, &source, source_offset) {
                // The same content continues: one row names all of it.
                extended = Some(old.offset);
                if old.end() < to {
                    self.piece_length(key, old.offset, to - old.offset)?;
                }
            } else if old.end() > from {
                self.piece_length(key, old.offset, from - old.offset)?;
                tail = Some(old).filter(|old| live && old.end() > to);
            }
        }
        let taken = self.query(
            StatementKind::Payload,
            sql::PIECES_TAKE,
            &[&layer.gen, &serial, &from, &to],
            40,
            Piece::decode,
        )?;
        if let Some(last) = taken.into_iter().max_by_key(Piece::end) {
            if last.end() > to && !self.stale(serial, layer, last.offset, last.epoch)? {
                tail = Some(last);
            }
        }
        // The same content may go on in the piece that begins where this
        // range ends: then one row names all of it.
        let mut end = to;
        let next = match tail {
            None => self.piece_cover(layer.gen, serial, to)?,
            Some(_) => None,
        };
        if let Some(next) = next.filter(|next| next.offset == to) {
            if next.names(to, &source, source_offset + (to - from))
                && !self.stale(serial, layer, next.offset, next.epoch)?
            {
                self.query(
                    StatementKind::Payload,
                    sql::PIECES_TAKE,
                    &[&layer.gen, &serial, &to, &(to + 1)],
                    40,
                    Piece::decode,
                )?;
                end = next.end();
            }
        }
        match extended {
            Some(first) if end > to => self.piece_length(key, first, end - first)?,
            Some(_) => {}
            None => {
                let piece = (from, end - from, layer.epoch, source, source_offset);
                self.put_piece(key, piece)?;
            }
        }
        if let Some(old) = tail {
            let origin = old.source_offset + (to - old.offset);
            let piece = (to, old.end() - to, layer.epoch, old.source, origin);
            self.put_piece(key, piece)?;
        }
        Ok(())
    }
    /// Removes the stored cells `[from, to)` of one layer. A live wide row
    /// that crosses either edge keeps its cells outside the range.
    fn clear_cells(&self, serial: i64, layer: &Layer, from: i64, to: i64) -> OverlayResult<()> {
        let key = (serial, layer.gen);
        let cell = CELL as i64;
        // No row crosses a slot boundary: only an edge inside a slot can be
        // crossed, from the row over its first cell or over its last one.
        if from % RUN != 0 {
            if let Some((shape, _)) = self.covering(serial, layer.gen, from)? {
                if shape.offset < from {
                    if self.stale(serial, layer, shape.offset, shape.epoch)? {
                        self.drop_row(shape.row)?;
                    } else {
                        self.carve(key, &shape, from, shape.end().min(to))?;
                    }
                }
            }
        }
        if to % RUN != 0 {
            if let Some((shape, _)) = self.covering(serial, layer.gen, to - cell)? {
                if shape.end() > to {
                    if self.stale(serial, layer, shape.offset, shape.epoch)? {
                        self.drop_row(shape.row)?;
                    } else {
                        self.carve(key, &shape, shape.offset, to)?;
                    }
                }
            }
        }
        self.execute(
            StatementKind::Payload,
            sql::CELLS_DROP,
            &[&serial, &layer.gen, &from, &to],
            40,
        )?;
        Ok(())
    }
    /// The last piece of one layer that starts at or before byte `at`.
    pub(crate) fn piece_cover(
        &self,
        gen: i64,
        serial: i64,
        at: i64,
    ) -> OverlayResult<Option<Piece>> {
        Ok(self
            .query(
                StatementKind::Payload,
                sql::PIECE_COVER,
                &[&gen, &serial, &at],
                32,
                Piece::decode,
            )?
            .pop())
    }
    /// Inserts one piece row. From here a piece row may exist in this engine.
    pub(crate) fn put_piece(
        &self,
        (gen, serial): (i64, i64),
        (offset, length, epoch, source, source_offset): (i64, i64, i64, [u8; 32], i64),
    ) -> OverlayResult<()> {
        self.pieces_seen.set(true);
        self.execute(
            StatementKind::Payload,
            sql::PIECE_PUT,
            &[
                &gen,
                &serial,
                &offset,
                &length,
                &epoch,
                &source.as_slice(),
                &source_offset,
            ],
            96,
        )?;
        Ok(())
    }
    fn piece_length(
        &self,
        (gen, serial): (i64, i64),
        offset: i64,
        length: i64,
    ) -> OverlayResult<()> {
        self.execute(
            StatementKind::Payload,
            sql::PIECE_LENGTH,
            &[&gen, &serial, &offset, &length],
            40,
        )?;
        Ok(())
    }
    /// Ends the piece of one layer that crosses byte `to` at that byte, for
    /// a shrink of the layer to `to`. One statement; no other piece is read.
    pub(crate) fn cut_piece(&self, serial: i64, gen: i64, to: u64) -> OverlayResult<()> {
        self.execute(
            StatementKind::Payload,
            sql::PIECE_CUT,
            &[&gen, &serial, &integer(to)?],
            32,
        )?;
        Ok(())
    }
    /// Gives the still undecided bytes of window `[start, end)` that a live
    /// piece of one layer holds to that piece, as one reference range for
    /// each piece, and returns how many bytes that decided. One statement.
    pub(crate) fn compose_pieces(
        &self,
        serial: i64,
        layer: &Layer,
        (start, end): (u64, u64),
        decided: &mut [u8],
        read: &mut LocalRead,
    ) -> OverlayResult<usize> {
        let rows = self.query(
            StatementKind::Payload,
            sql::PIECE_WINDOW,
            &[&layer.gen, &serial, &integer(start)?, &integer(end)?],
            48,
            Piece::decode,
        )?;
        if rows.len() >= WINDOW_ROWS {
            return Err(OverlayError::Invalid("pieces of one read window"));
        }
        let mut taken = 0;
        for piece in rows {
            let from = start.max(piece.offset as u64);
            let to = end.min(piece.end() as u64);
            if from >= to || self.stale(serial, layer, piece.offset, piece.epoch)? {
                continue;
            }
            // A range is a run of the piece's cells that still hold an
            // undecided byte. The piece decides every such byte, so no other
            // piece of any layer supplies a byte of these cells.
            let (mut at, mut run) = (from, None::<u64>);
            while at < to {
                let next = to.min(at - at % CELL + CELL);
                let slots = &mut decided[(at - start) as usize..(next - start) as usize];
                if slots.iter().all(|slot| *slot != 0) {
                    (at, run) = (next, None);
                    continue;
                }
                if run.is_none() {
                    if read.references.len() == READ_REFERENCES {
                        return Err(OverlayError::Invalid("reference ranges of one read window"));
                    }
                    read.references.push(ReferenceRange {
                        offset: at,
                        length: 0,
                        source: piece.source,
                        source_offset: piece.source_offset as u64 + (at - piece.offset as u64),
                    });
                    run = Some(at);
                }
                let tag = (read.references.len() + 1) as u8;
                for slot in slots.iter_mut().filter(|slot| **slot == 0) {
                    *slot = tag;
                    taken += 1;
                }
                if let Some(range) = read.references.last_mut() {
                    range.length = next - range.offset;
                }
                at = next;
            }
        }
        Ok(taken)
    }
}
