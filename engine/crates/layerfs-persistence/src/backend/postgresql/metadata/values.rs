//! Inode value block reads, with the blocks a transaction holds and reads
//! ahead.
use super::super::session::transaction::Transaction;
use crate::backend::error::BackendError;
use layerfs_storage::{location::InodeValueRow, port::PersistenceError};
use std::collections::BTreeMap;

/// Every stored value among ascending distinct ordinals, in ordinal order,
/// in one statement that names the schema. Each run of consecutive ordinals
/// seeks the last block starting at or before it and takes the blocks from
/// there to its end; each block is returned once, in ordinal order.
pub(crate) fn inode_values(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    crate::storage::blocks::values::pick(ordinals, value_blocks(tx, ordinals, 0, &[])?, out)?;
    Ok(())
}

/// The same two bounded immutable statements, sent on one writer turn.
pub(crate) fn inode_values_pair(
    tx: &Transaction<'_>,
    ordinals: [&[u32]; 2],
    out: &mut [Vec<InodeValueRow>; 2],
) -> Result<(), PersistenceError> {
    let [(left_lo, left_hi), (right_lo, right_hi)] = ordinals.map(value_ranges);
    let skip: [i64; 0] = [];
    let sql = values_sql(tx.schema);
    let replies = tx.typed_pair(
        &sql,
        [
            &[&left_lo, &left_hi, &skip.as_slice()],
            &[&right_lo, &right_hi, &skip.as_slice()],
        ],
        [(left_lo.len() * 16) as u64, (right_lo.len() * 16) as u64],
    )?;
    for ((asked, out), answer) in ordinals.into_iter().zip(out).zip(replies) {
        let rows = answer?;
        crate::storage::blocks::values::pick(asked, rows.iter().map(value_block), out)?;
        crate::storage::metadata::values::check_answer(asked, out)?;
    }
    Ok(())
}

/// Bytes of value-block bodies one transaction keeps between
/// [`inode_values_ahead`] calls, in two generations: when the young one
/// has reached half of this, the next call drops the old one. A call never
/// drops a block it read, so what it keeps can exceed this by its own
/// statement.
const HELD_BLOCK_BYTES: usize = 16 * 1024 * 1024;

/// The value blocks one transaction has read, by first ordinal, each with
/// its last ordinal and body.
///
/// A block is written once and never changes, so a held block answers for
/// its ordinals as the server would. A walk that names the same ordinals
/// again and again, scattered over the blocks they were first stored in,
/// then pays one round trip per block and not one per visit.
#[derive(Default)]
pub(crate) struct HeldBlocks {
    young: BTreeMap<u32, (u32, Vec<u8>)>,
    old: BTreeMap<u32, (u32, Vec<u8>)>,
    young_bytes: usize,
}

impl HeldBlocks {
    /// The first ordinal of the held block that holds `ordinal`.
    fn find(&self, ordinal: u32) -> Option<u32> {
        [&self.young, &self.old].into_iter().find_map(|blocks| {
            let (first, (last, _)) = blocks.range(..=ordinal).next_back()?;
            (ordinal <= *last).then_some(*first)
        })
    }

    /// The first ordinals of the held blocks starting after `after` and no
    /// later than `through`.
    fn firsts(&self, after: u32, through: u32) -> Vec<i64> {
        let Some(from) = after.checked_add(1).filter(|from| *from <= through) else {
            return Vec::new();
        };
        [&self.young, &self.old]
            .into_iter()
            .flat_map(|blocks| blocks.range(from..=through))
            .map(|(first, _)| i64::from(*first))
            .collect()
    }

    fn get(&self, first: u32) -> Option<&(u32, Vec<u8>)> {
        self.young.get(&first).or_else(|| self.old.get(&first))
    }

    /// Ages the generations, at the start of a call: every block a call
    /// finds or reads stays held until that call has handed it back.
    fn turn(&mut self) {
        if self.young_bytes > HELD_BLOCK_BYTES / 2 {
            self.old = std::mem::take(&mut self.young);
            self.young_bytes = 0;
        }
    }

    fn keep(&mut self, first: u32, last: u32, body: Vec<u8>) {
        self.young_bytes += body.len();
        if let Some((_, replaced)) = self.young.insert(first, (last, body)) {
            self.young_bytes -= replaced.len();
        }
    }
}

/// [`inode_values`], and every other value of the blocks that hold the asked
/// ordinals. The blocks the transaction has not read yet come in one
/// statement, with those starting up to `ahead` ordinals past the last
/// missing one; inside a transaction every block read is kept
/// ([`HeldBlocks`]), so the blocks read ahead answer later calls and the
/// caller is handed only the blocks it asked for.
pub(crate) fn inode_values_ahead(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    ahead: u32,
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let mut held = tx.values.borrow_mut();
    held.turn();
    let missing: Vec<u32> = ordinals
        .iter()
        .copied()
        .filter(|ordinal| held.find(*ordinal).is_none())
        .collect();
    if let Some(last) = missing.last().copied() {
        // Blocks read ahead by an earlier call lie in this call's window too.
        let skip = held.firsts(last, last.saturating_add(ahead));
        let mut end = 0u32;
        for block in value_blocks(tx, &missing, ahead, &skip)? {
            let (first, last, body) = block?;
            // The same order rule `pick` applies: blocks ascend and never overlap.
            if first <= end {
                return Err(BackendError::Integrity.into());
            }
            end = last;
            // An autocommit read keeps nothing: each of its statements is
            // fenced on its own.
            if tx.unfenced() {
                held.keep(first, last, body);
            } else {
                push_block(first, last, &body, out)?;
            }
        }
        if !tx.unfenced() {
            return Ok(());
        }
    }
    // The blocks of the asked ordinals, each once, in ordinal order. An
    // ordinal no block holds has no value and is left out, as `pick` does.
    let mut end = 0u32;
    for ordinal in ordinals {
        if *ordinal <= end {
            continue;
        }
        let Some(first) = held.find(*ordinal) else {
            continue;
        };
        let (last, body) = held.get(first).ok_or(BackendError::Integrity)?;
        push_block(first, *last, body, out)?;
        end = *last;
    }
    Ok(())
}

/// Every value of one block, in ordinal order.
fn push_block(
    first: u32,
    last: u32,
    body: &[u8],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let values = crate::storage::blocks::values::decode(first, last, body)?;
    out.extend(
        (first..=last)
            .zip(values)
            .map(|(ordinal, value)| InodeValueRow { ordinal, value }),
    );
    Ok(())
}

/// One stored block: its first and last ordinal and its body.
type ValueBlock = Result<(u32, u32, Vec<u8>), BackendError>;

/// The value blocks covering `ordinals`, and those starting within `ahead`
/// ordinals past the last one, in ascending first-ordinal order. A block
/// whose first ordinal is in `held` is left out: the caller has it.
fn value_blocks(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    ahead: u32,
    held: &[i64],
) -> Result<Vec<ValueBlock>, PersistenceError> {
    let (lows, mut highs) = value_ranges(ordinals);
    let sql = values_sql(tx.schema);
    if let Some(high) = highs.last_mut() {
        *high += i64::from(ahead);
    }
    let rows = tx.typed(
        &sql,
        &[&lows, &highs, &held],
        (lows.len() as u64 * 2 + held.len() as u64) * 8,
    )?;
    Ok(rows.iter().map(value_block).collect())
}

fn value_ranges(ordinals: &[u32]) -> (Vec<i64>, Vec<i64>) {
    let (mut lows, mut highs) = (Vec::new(), Vec::new());
    for &ordinal in ordinals {
        match highs.last_mut() {
            Some(high) if *high + 1 == i64::from(ordinal) => *high += 1,
            _ => {
                lows.push(i64::from(ordinal));
                highs.push(i64::from(ordinal));
            }
        }
    }
    (lows, highs)
}

fn value_block(row: &postgres::Row) -> Result<(u32, u32, Vec<u8>), BackendError> {
    let ordinal = |value| u32::try_from(value).map_err(|_| BackendError::Integrity);
    let first = row
        .try_get::<_, i64>(0)
        .map_err(|_| BackendError::Integrity)?;
    let last = row
        .try_get::<_, i64>(1)
        .map_err(|_| BackendError::Integrity)?;
    let body = row.try_get(2).map_err(|_| BackendError::Integrity)?;
    Ok((ordinal(first)?, ordinal(last)?, body))
}

/// The inode value block reader's text (`inode_values_ahead`): the blocks
/// covering each requested range, without the ones the caller holds.
pub(crate) fn values_sql(schema: &str) -> String {
    format!(
        "SELECT b.first_ordinal,b.last_ordinal,CASE WHEN octet_length(b.body) BETWEEN 9 AND 4000 THEN b.body END FROM {s}.inode_value_block b WHERE b.first_ordinal IN (\
         SELECT x.first_ordinal FROM unnest($1::bigint[],$2::bigint[]) r(lo,hi) JOIN {s}.inode_value_block x \
         ON x.first_ordinal BETWEEN coalesce((SELECT max(y.first_ordinal) FROM {s}.inode_value_block y \
         WHERE y.first_ordinal<=r.lo),0) AND r.hi) AND NOT (b.first_ordinal = ANY($3::bigint[])) \
         ORDER BY b.first_ordinal",
        s = schema
    )
}
