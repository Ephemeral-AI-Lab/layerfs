//! Wide-directory placement in set windows (#818 I2): the algorithm both
//! backends run, with each backend's statements in its own module.
//!
//! A unit's children are cut into windows
//! as `independent` cuts entry windows: before a repeated name, a repeated
//! position or a repeated (device, inode), and before a child a statement
//! could refuse on its own (a position that does not convert to signed, which
//! also ends the window as `record_roots` does; a host path out of its
//! bounds; a bound child at position 0). A window of one child runs the
//! per-row bind and place. A longer window first finds its first child whose
//! row a stored row could refuse (D17: a stored identity at its position, a
//! stored directory at its position); that child runs per row after the
//! children before it. The rest bind and place together, and the first child
//! in order with no bound row, no placed row or a kind that disagrees with
//! its binding is the per-row loop's `Changed`. Nothing in a window can raise
//! a constraint error the loop would not reach first, so its first refusal,
//! rows and charges are the loop's.
use super::super::backend::sqlite::acquisition::accounting::{host_bytes, HOST_BYTES};
use super::super::backend::sqlite::acquisition::statements::{signed, unsigned, Failure};
use crate::backend::error::BackendError;
use layerfs_content::inode_leaf::InodeKind;
use layerfs_storage::port::acquisition::Placed;
use std::collections::BTreeSet;

/// The number of leading children (at most `limit`) one set window may bind
/// and place together; 0 when the first must run alone.
pub(crate) fn independent(placed: &[Placed], limit: usize) -> usize {
    let (mut names, mut positions, mut identities) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for (count, child) in placed.iter().take(limit).enumerate() {
        let refusable = signed(child.position).is_err()
            || child.host.as_ref().is_some_and(|(_, path)| {
                path.is_empty() || path.len() > 4096 || child.position == 0
            });
        if refusable
            || !names.insert(child.name.as_slice())
            || !positions.insert(child.position)
            || child
                .host
                .as_ref()
                .is_some_and(|(host, _)| !identities.insert((host.device, host.inode)))
        {
            return count;
        }
    }
    placed.len().min(limit)
}

/// What one unit's placements answer and charge, accumulated in child order.
#[derive(Default)]
pub(crate) struct Placement {
    /// Each bound child's canonical position, in child order.
    pub(crate) canonical: Vec<u64>,
    pub(crate) rows: i64,
    pub(crate) bytes: i64,
}
impl Placement {
    /// One bound child's canonical position: whether its path created the
    /// identity's row decides the charge.
    pub(crate) fn bound(&mut self, child: &Placed, first: i64) -> Result<(), Failure> {
        let Some((_, path)) = &child.host else {
            return Err(BackendError::Integrity.into());
        };
        self.canonical.push(unsigned(first)?);
        if first == signed(child.position)? {
            self.rows += 1;
            self.bytes += host_bytes(path);
        }
        self.bytes -= HOST_BYTES;
        Ok(())
    }

    /// The child must exist unpositioned, and be regular exactly when bound.
    pub(crate) fn placed(child: &Placed, kind: Option<i64>) -> Result<(), Failure> {
        let kind = kind.ok_or(Failure::Changed(child.position))?;
        let regular = kind == i64::from(InodeKind::RegularFile.code());
        if regular != child.host.is_some() {
            return Err(Failure::Changed(child.position));
        }
        Ok(())
    }

    /// One set window's outcome, child by child in the per-row loop's order:
    /// a bound child's binding, then its placement.
    pub(crate) fn window(
        &mut self,
        children: &[Placed],
        bound: &[Option<i64>],
        kinds: &[Option<i64>],
    ) -> Result<(), Failure> {
        for (index, child) in children.iter().enumerate() {
            if child.host.is_some() {
                let first = bound[index].ok_or(Failure::Changed(child.position))?;
                self.bound(child, first)?;
            }
            Self::placed(child, kinds[index])?;
        }
        Ok(())
    }
}

/// How a unit runs its children: set windows of `independent` children up to
/// the first dependent one, which runs alone.
pub(crate) fn plan<'p>(
    placed: &'p [Placed],
    limit: usize,
    mut dependent: impl FnMut(&'p [Placed]) -> Result<usize, Failure>,
    mut one: impl FnMut(&'p Placed) -> Result<(), Failure>,
    mut window: impl FnMut(&'p [Placed]) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let mut offset = 0;
    while offset < placed.len() {
        let children = &placed[offset..];
        let count = independent(children, limit);
        if count < 2 {
            one(&children[0])?;
            offset += 1;
            continue;
        }
        let free = dependent(&children[..count])?;
        if free > count {
            return Err(BackendError::Integrity.into());
        }
        match free {
            0 => {}
            1 => one(&children[0])?,
            _ => window(&children[..free])?,
        }
        offset += free;
        if free < count {
            one(&children[free])?;
            offset += 1;
        }
    }
    Ok(())
}
