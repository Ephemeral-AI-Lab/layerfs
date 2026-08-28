use super::artifact::display_error;
#[cfg(test)]
use super::artifact::io_error;
#[cfg(test)]
use super::limits::BUFFER_BYTES;
use super::limits::INITIAL_BYTES;
#[cfg(test)]
use crate::stage1_fixture;
use crate::stage1_fixture::EvalResult;
#[cfg(test)]
use std::io::Write;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditKind {
    Overwrite,
    Insert,
    Delete,
    Append,
    Truncate,
}
impl EditKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Overwrite => "overwrite",
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Append => "append",
            Self::Truncate => "truncate",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EditSpec {
    pub(crate) tag: String,
    pub(crate) serial: u8,
    pub(crate) epoch: u8,
    pub(crate) kind: EditKind,
    pub(crate) size_band: &'static str,
    pub(crate) offset: u64,
    pub(crate) delete_bytes: u64,
    pub(crate) insert_bytes: u64,
    pub(crate) before_bytes: u64,
    pub(crate) after_bytes: u64,
    pub(crate) replacement_offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BurstSpec {
    pub(crate) root: u8,
    pub(crate) pattern: &'static str,
    pub(crate) edits: Vec<EditSpec>,
}
pub(crate) type FrozenBurstEdit = (EditKind, u64, u64, u64, u64, u64);
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScheduledRow {
    pub(crate) row_index: usize,
    pub(crate) row_id: String,
    pub(crate) row_group: &'static str,
    pub(crate) sequence: u8,
    pub(crate) epoch: u8,
    pub(crate) direction: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) size_band: &'static str,
    pub(crate) edit_index: Option<usize>,
    pub(crate) burst_index: Option<usize>,
    pub(crate) history_session: Option<u8>,
    pub(crate) milestone_root: Option<u8>,
    pub(crate) transition_root: Option<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct FrozenSchedule {
    pub(crate) edits: Vec<EditSpec>,
    pub(crate) bursts: Vec<BurstSpec>,
    pub(crate) rows: Vec<ScheduledRow>,
    pub(crate) replacement_backing: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Piece {
    Original { offset: u64, length: u64 },
    Inserted { offset: usize, length: u64 },
}
impl Piece {
    pub(crate) fn length(&self) -> u64 {
        match self {
            Self::Original { length, .. } | Self::Inserted { length, .. } => *length,
        }
    }
    pub(crate) fn slice(&self, offset: u64, length: u64) -> EvalResult<Self> {
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.length())
        {
            return Err("piece slice exceeds source".to_owned());
        }
        match self {
            Self::Original { offset: source, .. } => Ok(Self::Original {
                offset: source
                    .checked_add(offset)
                    .ok_or_else(|| "original piece offset overflow".to_owned())?,
                length,
            }),
            Self::Inserted { offset: source, .. } => Ok(Self::Inserted {
                offset: source
                    .checked_add(usize::try_from(offset).map_err(display_error)?)
                    .ok_or_else(|| "inserted piece offset overflow".to_owned())?,
                length,
            }),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PieceTable {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) logical_length: u64,
}
impl PieceTable {
    pub(crate) fn initial() -> Self {
        Self {
            pieces: vec![Piece::Original {
                offset: 0,
                length: INITIAL_BYTES,
            }],
            logical_length: INITIAL_BYTES,
        }
    }
    pub(crate) fn splice(&mut self, edit: &EditSpec) -> EvalResult<()> {
        if self.logical_length != edit.before_bytes {
            return Err(format!(
                "{} before length {} != {}",
                edit.tag, self.logical_length, edit.before_bytes
            ));
        }
        let end = edit
            .offset
            .checked_add(edit.delete_bytes)
            .ok_or_else(|| format!("{} delete range overflow", edit.tag))?;
        if end > self.logical_length {
            return Err(format!("{} delete range exceeds file", edit.tag));
        }
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut logical = 0_u64;
        for piece in &self.pieces {
            let piece_end = logical
                .checked_add(piece.length())
                .ok_or_else(|| "piece-table length overflow".to_owned())?;
            if piece_end <= edit.offset {
                left.push(piece.clone());
            } else if logical >= end {
                right.push(piece.clone());
            } else {
                if logical < edit.offset {
                    left.push(piece.slice(0, edit.offset - logical)?);
                }
                if piece_end > end {
                    right.push(piece.slice(end - logical, piece_end - end)?);
                }
            }
            logical = piece_end;
        }
        if logical != self.logical_length {
            return Err("piece-table stored length mismatch".to_owned());
        }
        if edit.insert_bytes != 0 {
            left.push(Piece::Inserted {
                offset: edit.replacement_offset,
                length: edit.insert_bytes,
            });
        }
        left.extend(right);
        self.pieces = left;
        self.logical_length = self
            .logical_length
            .checked_sub(edit.delete_bytes)
            .and_then(|value| value.checked_add(edit.insert_bytes))
            .ok_or_else(|| "piece-table splice length overflow".to_owned())?;
        self.coalesce()?;
        if self.logical_length != edit.after_bytes {
            return Err(format!(
                "{} after length {} != {}",
                edit.tag, self.logical_length, edit.after_bytes
            ));
        }
        Ok(())
    }
    pub(crate) fn coalesce(&mut self) -> EvalResult<()> {
        let mut output: Vec<Piece> = Vec::with_capacity(self.pieces.len());
        for piece in self.pieces.drain(..) {
            if piece.length() == 0 {
                continue;
            }
            let merged = match (output.last_mut(), &piece) {
                (
                    Some(Piece::Original { offset, length }),
                    Piece::Original {
                        offset: next,
                        length: next_len,
                    },
                ) if offset.checked_add(*length) == Some(*next) => {
                    *length = length
                        .checked_add(*next_len)
                        .ok_or_else(|| "original coalesce overflow".to_owned())?;
                    true
                }
                (
                    Some(Piece::Inserted { offset, length }),
                    Piece::Inserted {
                        offset: next,
                        length: next_len,
                    },
                ) if offset.checked_add(usize::try_from(*length).map_err(display_error)?)
                    == Some(*next) =>
                {
                    *length = length
                        .checked_add(*next_len)
                        .ok_or_else(|| "inserted coalesce overflow".to_owned())?;
                    true
                }
                _ => false,
            };
            if !merged {
                output.push(piece);
            }
        }
        self.pieces = output;
        Ok(())
    }
    pub(crate) fn range(&self, start: u64, length: u64) -> EvalResult<Self> {
        let end = start
            .checked_add(length)
            .ok_or_else(|| "piece-table range overflow".to_owned())?;
        if end > self.logical_length {
            return Err("piece-table range exceeds logical length".to_owned());
        }
        let mut pieces = Vec::new();
        let mut logical = 0_u64;
        for piece in &self.pieces {
            let piece_end = logical
                .checked_add(piece.length())
                .ok_or_else(|| "piece-table range position overflow".to_owned())?;
            let overlap_start = logical.max(start);
            let overlap_end = piece_end.min(end);
            if overlap_start < overlap_end {
                pieces.push(piece.slice(overlap_start - logical, overlap_end - overlap_start)?);
            }
            logical = piece_end;
            if logical >= end {
                break;
            }
        }
        Ok(Self {
            pieces,
            logical_length: length,
        })
    }
    #[cfg(test)]
    pub(crate) fn stream<W: Write>(&self, backing: &[u8], output: &mut W) -> EvalResult<()> {
        let mut scratch = vec![0_u8; BUFFER_BYTES];
        for piece in &self.pieces {
            match piece {
                Piece::Original { offset, length } => {
                    stream_original(*offset, *length, &mut scratch, output)?;
                }
                Piece::Inserted { offset, length } => {
                    let length = usize::try_from(*length).map_err(display_error)?;
                    let end = offset
                        .checked_add(length)
                        .ok_or_else(|| "replacement range overflow".to_owned())?;
                    output
                        .write_all(
                            backing
                                .get(*offset..end)
                                .ok_or_else(|| "replacement range exceeds backing".to_owned())?,
                        )
                        .map_err(io_error)?;
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
pub(crate) fn stream_original<W: Write>(
    start: u64,
    length: u64,
    scratch: &mut [u8],
    output: &mut W,
) -> EvalResult<()> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| "original oracle range overflow".to_owned())?;
    if end > INITIAL_BYTES || scratch.len() != BUFFER_BYTES {
        return Err("original oracle request exceeds frozen fixture".to_owned());
    }
    let mut position = start;
    while position < end {
        let block = position / BUFFER_BYTES as u64 * BUFFER_BYTES as u64;
        stage1_fixture::fill_retained_buffer(scratch, block);
        let within = usize::try_from(position - block).map_err(display_error)?;
        let take = usize::try_from((end - position).min((BUFFER_BYTES - within) as u64))
            .map_err(display_error)?;
        output
            .write_all(&scratch[within..within + take])
            .map_err(io_error)?;
        position += take as u64;
    }
    Ok(())
}
pub(crate) fn replacement_bytes(serial: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| {
            serial
                .wrapping_mul(17)
                .wrapping_add((index as u8).wrapping_mul(31))
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_edit(
    edits: &mut Vec<EditSpec>,
    replacement_backing: &mut Vec<u8>,
    tag: String,
    serial: u8,
    epoch: u8,
    kind: EditKind,
    size_band: &'static str,
    offset: u64,
    delete_bytes: u64,
    insert_bytes: u64,
    before_bytes: u64,
    after_bytes: u64,
) -> EvalResult<()> {
    let replacement_offset = replacement_backing.len();
    replacement_backing.extend(replacement_bytes(
        serial,
        usize::try_from(insert_bytes).map_err(display_error)?,
    ));
    edits.push(EditSpec {
        tag,
        serial,
        epoch,
        kind,
        size_band,
        offset,
        delete_bytes,
        insert_bytes,
        before_bytes,
        after_bytes,
        replacement_offset,
    });
    Ok(())
}
