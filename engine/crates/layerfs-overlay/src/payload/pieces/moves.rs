//! Moving the pieces of a lower layer into the layer above it, one bounded
//! step at a time, for a folded capture and for an orphan's migration.
use super::rows::Piece;
use crate::{db::integer, layers::Layer, sql, Overlay, OverlayResult, StatementKind, CELL_BYTES};

impl Overlay {
    /// The live pieces of one layer that hold a byte of `[from, to)`, a
    /// range of at most one payload row.
    pub(crate) fn live_pieces(
        &self,
        serial: i64,
        layer: &Layer,
        from: i64,
        to: i64,
    ) -> OverlayResult<Vec<Piece>> {
        let mut pieces = self.query(
            StatementKind::Payload,
            sql::PIECE_WINDOW,
            &[&layer.gen, &serial, &from, &to],
            48,
            Piece::decode,
        )?;
        let mut live = Vec::with_capacity(pieces.len());
        for piece in pieces.drain(..).filter(|piece| piece.end() > from) {
            if !self.stale(serial, layer, piece.offset, piece.epoch)? {
                live.push(piece);
            }
        }
        Ok(live)
    }
    /// The first piece of one layer that starts at or after byte `at`.
    pub(crate) fn piece_next(
        &self,
        gen: i64,
        serial: i64,
        at: i64,
    ) -> OverlayResult<Option<Piece>> {
        Ok(self
            .query(
                StatementKind::Payload,
                sql::PIECE_NEXT,
                &[&gen, &serial, &at],
                32,
                Piece::decode,
            )?
            .pop())
    }
    /// One step of moving the pieces of `lower` into `upper`, the layer
    /// directly above it, after every lower cell has moved. The lowest lower
    /// piece gives its leading part that the composed view still reads from
    /// it to a new upper row: bytes below both the lower size and the upper
    /// cutoff that no upper piece decides. Upper cells need no care: they win
    /// inside their layer. A step reads at most three rows and changes at
    /// most three, whatever the length of a piece. False when the lower layer
    /// holds no piece.
    pub(crate) fn transfer_piece(
        &self,
        serial: i64,
        lower: &Layer,
        upper: &Layer,
    ) -> OverlayResult<bool> {
        let Some(piece) = self.piece_next(lower.gen, serial, 0)? else {
            return Ok(false);
        };
        let (below, above) = ((lower.gen, serial), (upper.gen, serial));
        let end = piece.end().min(integer(lower.size.min(upper.cutoff))?);
        if piece.offset >= end || self.stale(serial, lower, piece.offset, piece.epoch)? {
            self.drop_piece(below, piece.offset)?;
            return Ok(true);
        }
        // Where the fragment that moves in this step ends.
        let mut to = end;
        if upper.referenced {
            let cover = self.piece_cover(upper.gen, serial, piece.offset)?;
            if let Some(own) = cover.filter(|own| own.end() > piece.offset) {
                if self.stale(serial, upper, own.offset, own.epoch)? {
                    // Garbage in the way of the row that moves in next.
                    self.drop_piece(above, own.offset)?;
                } else {
                    // The upper layer decides these bytes itself.
                    self.advance_piece(below, &piece, own.end(), end)?;
                }
                return Ok(true);
            }
            let next = self.piece_next(upper.gen, serial, piece.offset + 1)?;
            if let Some(own) = next.filter(|own| own.offset < end) {
                if self.stale(serial, upper, own.offset, own.epoch)? {
                    self.drop_piece(above, own.offset)?;
                    return Ok(true);
                }
                to = own.offset;
            }
        } else {
            self.execute(
                StatementKind::Inode,
                sql::LAYER_REFERENCED,
                &[&serial, &upper.gen],
                24,
            )?;
        }
        let moved = (
            piece.offset,
            to - piece.offset,
            upper.epoch,
            piece.source,
            piece.source_offset,
        );
        self.put_piece(above, moved)?;
        self.advance_piece(below, &piece, to, end)?;
        Ok(true)
    }
    /// Takes the bytes below `to` from a lower piece whose bytes at or above
    /// `end` nothing reads. What is left starts at `to`, or the row goes.
    fn advance_piece(
        &self,
        (gen, serial): (i64, i64),
        piece: &Piece,
        to: i64,
        end: i64,
    ) -> OverlayResult<()> {
        // A piece starts on a cell boundary. A row above ends inside a cell
        // only at its layer's cutoff, and nothing is read beyond that.
        if to >= end || to % CELL_BYTES as i64 != 0 {
            return self.drop_piece((gen, serial), piece.offset);
        }
        let moved = to - piece.offset;
        self.execute(
            StatementKind::Payload,
            sql::PIECE_START,
            &[
                &gen,
                &serial,
                &piece.offset,
                &to,
                &(piece.length - moved),
                &(piece.source_offset + moved),
            ],
            56,
        )?;
        Ok(())
    }
}
