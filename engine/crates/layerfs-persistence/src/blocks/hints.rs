//! Binding hint blocks: the hints of one write for the 64 inode serials of
//! one group (`child >> 6`), appended as a new row `(group, seq)`. Rows are
//! never changed; a reader merges a group's rows and drops repeats.
//!
//! Body, format 1: the format byte `1`, a LEB128 entry count, the entries
//! in ascending `(child, parent, name)` order without repeats, then the
//! CRC32C of every byte before it. An entry is the child's LEB128
//! difference from the previous entry's (from `group << 6` for the first),
//! the parent's zig-zag LEB128 difference from the previous entry's (from 0),
//! the length of the name prefix it shares with the previous entry's name
//! (one byte), and the LEB128 length and bytes of the rest of the name.
use super::frame::{self, Reader, BODY_LIMIT, CRC_BYTES};
use crate::backend::records::BackendError;
use layerfs_history::records::BindingHint;

const FORMAT: u8 = 1;
/// Inode serials per group.
pub(crate) const GROUP_BITS: u32 = 6;

/// One encoded block of one group.
pub(crate) struct Block {
    pub(crate) group: i64,
    pub(crate) body: Vec<u8>,
}

/// The group of `child`.
pub(crate) fn group_of(child: u64) -> i64 {
    (child >> GROUP_BITS) as i64
}

fn valid(hint: &BindingHint) -> bool {
    hint.child != 0 && hint.parent != 0 && (1..=255).contains(&hint.name.len())
}

fn shared(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

struct Open {
    group: i64,
    count: u64,
    entries: Vec<u8>,
    previous: Option<(u64, u64, Vec<u8>)>,
}

impl Open {
    fn new(group: i64) -> Self {
        Self {
            group,
            count: 0,
            entries: Vec::new(),
            previous: None,
        }
    }
    fn size(&self, extra: usize, count: u64) -> usize {
        1 + frame::varint_len(count) + self.entries.len() + extra + CRC_BYTES
    }
    fn push(&mut self, hint: &BindingHint) -> bool {
        let (child, parent) = match &self.previous {
            Some((child, parent, _)) => (*child, *parent),
            None => ((self.group as u64) << GROUP_BITS, 0),
        };
        let name: &[u8] = self.previous.as_ref().map_or(&[], |(_, _, name)| name);
        let common = shared(name, &hint.name).min(255);
        let mut entry = Vec::with_capacity(hint.name.len() + 16);
        frame::put_varint(&mut entry, hint.child - child);
        frame::put_varint(
            &mut entry,
            frame::zigzag((hint.parent as i64).wrapping_sub(parent as i64)),
        );
        entry.push(common as u8);
        frame::put_varint(&mut entry, (hint.name.len() - common) as u64);
        entry.extend_from_slice(&hint.name[common..]);
        if self.count > 0 && self.size(entry.len(), self.count + 1) > BODY_LIMIT {
            return false;
        }
        self.entries.extend_from_slice(&entry);
        self.count += 1;
        self.previous = Some((hint.child, hint.parent, hint.name.clone()));
        true
    }
    fn finish(self) -> Block {
        let mut body = Vec::with_capacity(self.size(0, self.count));
        body.push(FORMAT);
        frame::put_varint(&mut body, self.count);
        body.extend_from_slice(&self.entries);
        frame::seal(&mut body);
        Block {
            group: self.group,
            body,
        }
    }
}

/// Packs one write's hints into blocks, by group, in ascending order and
/// without repeats; a full block continues in another of the same group.
pub(crate) fn encode(rows: &[BindingHint]) -> Result<Vec<Block>, BackendError> {
    let mut sorted: Vec<&BindingHint> = rows.iter().collect();
    if !sorted.iter().all(|hint| valid(hint)) {
        return Err(BackendError::Integrity);
    }
    sorted.sort_unstable();
    sorted.dedup();
    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    for hint in sorted {
        let group = group_of(hint.child);
        if let Some(block) = open.as_mut() {
            if block.group == group && block.push(hint) {
                continue;
            }
        }
        if let Some(block) = open.take() {
            blocks.push(block.finish());
        }
        let mut block = Open::new(group);
        if !block.push(hint) {
            return Err(BackendError::Integrity);
        }
        open = Some(block);
    }
    if let Some(block) = open {
        blocks.push(block.finish());
    }
    Ok(blocks)
}

/// Every hint of one block of `group`, in its stored order.
pub(crate) fn decode(group: i64, body: &[u8]) -> Result<Vec<BindingHint>, BackendError> {
    if body.len() > BODY_LIMIT || group < 0 {
        return Err(BackendError::Integrity);
    }
    let mut reader = Reader::new(frame::open(body)?);
    if reader.byte()? != FORMAT {
        return Err(BackendError::Integrity);
    }
    let count = reader.varint()?;
    if count == 0 || count > BODY_LIMIT as u64 {
        return Err(BackendError::Integrity);
    }
    let mut hints: Vec<BindingHint> = Vec::with_capacity(count as usize);
    let (mut child, mut parent) = ((group as u64) << GROUP_BITS, 0u64);
    for _ in 0..count {
        child = child
            .checked_add(reader.varint()?)
            .ok_or(BackendError::Integrity)?;
        parent = (parent as i64).wrapping_add(frame::unzigzag(reader.varint()?)) as u64;
        let common = usize::from(reader.byte()?);
        let rest = usize::try_from(reader.varint()?).map_err(|_| BackendError::Integrity)?;
        let previous: &[u8] = hints.last().map_or(&[], |hint| &hint.name);
        if common > previous.len() {
            return Err(BackendError::Integrity);
        }
        let mut name = previous[..common].to_vec();
        name.extend_from_slice(reader.take(rest)?);
        let hint = BindingHint {
            child,
            parent,
            name,
        };
        if !valid(&hint)
            || group_of(child) != group
            || hints.last().is_some_and(|last| *last >= hint)
        {
            return Err(BackendError::Integrity);
        }
        hints.push(hint);
    }
    reader.finished()?;
    Ok(hints)
}
