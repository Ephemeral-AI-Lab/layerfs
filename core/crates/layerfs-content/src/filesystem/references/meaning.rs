//! One meaning for resident runs and indexed reference rows.
use super::{
    record::Row,
    reduce::{FinalChange, PendingState, ReferenceWork},
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{ContentError, ContentResult};

pub(super) fn retained(row: &mut Row) -> ContentResult<()> {
    match row {
        Row::Count { count, .. } => {
            *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?
        }
        Row::Effect { delta, .. } => {
            *delta = delta.checked_add(1).ok_or(ContentError::LengthOverflow)?
        }
    }
    Ok(())
}
pub(super) fn removed(row: &mut Row) -> ContentResult<()> {
    match row {
        Row::Count { .. } => return Err(ContentError::InvalidRecord("new inode loses a binding")),
        Row::Effect { delta, .. } => {
            *delta = delta.checked_sub(1).ok_or(ContentError::LengthOverflow)?
        }
    }
    Ok(())
}
pub(super) fn value(row: &mut Row, value: InodeValue) {
    match row {
        Row::Count { value: before, .. } | Row::Effect { value: before, .. } => {
            *before = Some(value)
        }
    }
}
pub(super) fn state(row: Row) -> PendingState {
    match row {
        Row::Count { value, count, .. } => PendingState::New { value, count },
        Row::Effect { value, delta, .. } => PendingState::Existing { value, delta },
    }
}
pub(super) fn with_base(row: Row, base: Option<InodeValue>) -> ContentResult<Row> {
    match row {
        Row::Effect {
            serial,
            value,
            delta,
        } => {
            let base = base.ok_or(ContentError::InvalidRecord("effect inode record"))?;
            Ok(Row::Effect {
                serial,
                delta,
                value: Some(InodeValue {
                    kind: value.map_or(base.kind, |value| value.kind),
                    namespace_ref_count: base.namespace_ref_count,
                    content_root: value.map_or(base.content_root, |value| value.content_root),
                    metadata_root: value.map_or(base.metadata_root, |value| value.metadata_root),
                }),
            })
        }
        row => Ok(row),
    }
}
pub(crate) fn derived_count(base: u64, delta: i64) -> ContentResult<u64> {
    let count = (i128::from(base) + i128::from(delta)).max(0);
    u64::try_from(count).map_err(|_| ContentError::LengthOverflow)
}
pub(super) fn count(row: Row, base: Option<InodeValue>) -> ContentResult<u64> {
    match row {
        Row::Count { count, .. } => Ok(count),
        Row::Effect { delta, .. } => {
            let base = base.ok_or(ContentError::InvalidRecord("effect inode record"))?;
            derived_count(base.namespace_ref_count, delta)
        }
    }
}
pub(super) fn finish(row: Row, root: u64, work: &mut ReferenceWork) -> ContentResult<FinalChange> {
    let serial = row.serial();
    let value = match row {
        Row::Count { value, count, .. } => {
            if count == 0 && serial != root {
                return Err(ContentError::InvalidRecord("new inode without binding"));
            }
            let value = value.ok_or(ContentError::InvalidRecord("new inode value"))?;
            Some(InodeValue {
                namespace_ref_count: count,
                ..value
            })
        }
        Row::Effect { value, delta, .. } => {
            let base = value.ok_or(ContentError::InvalidRecord("effect inode record"))?;
            let count = derived_count(base.namespace_ref_count, delta)?;
            if count == 0 && serial != root {
                None
            } else {
                Some(InodeValue {
                    namespace_ref_count: count,
                    ..base
                })
            }
        }
    };
    // The leaf grammar gives a directory or a symlink exactly one binding. The
    // count is derived from the bindings the merge observed, so a second parent
    // is refused here, where the value is produced, on every reducer route.
    if value
        .is_some_and(|value| value.kind != InodeKind::RegularFile && value.namespace_ref_count > 1)
    {
        return Err(ContentError::InvalidRecord("multiple parents"));
    }
    if value.is_some() {
        work.final_values = work.final_values.saturating_add(1);
    } else {
        work.final_removals = work.final_removals.saturating_add(1);
    }
    Ok(FinalChange { serial, value })
}
