//! Fallible initial inode rows; no complete backed count/final-row collection.
use super::{RebuiltRoots, SerialState};
use crate::filesystem::rows::{view::OperationInput, SerialRowSource};
use crate::object::inode_leaf::InodeValue;
use crate::{ContentError, ContentResult};

type InitialRow = (u64, Option<InodeValue>);
pub(crate) struct InitialRows<'a, 's, 'b> {
    source: Source<'a, 's, 'b>,
}
enum Source<'a, 's, 'b> {
    Memory(std::vec::IntoIter<InitialRow>),
    Backed(InitialCursor<'a, 's, 'b>),
}
struct InitialCursor<'a, 's, 'b> {
    input: &'a dyn OperationInput,
    state: &'a SerialState<'b>,
    roots: &'a RebuiltRoots<'s, 'b>,
    serials: Box<dyn SerialRowSource + 'a>,
    position: usize,
    ended: bool,
}
impl<'a, 's, 'b> InitialRows<'a, 's, 'b> {
    pub fn new(
        input: &'a dyn OperationInput,
        state: &'a SerialState<'b>,
        roots: &'a RebuiltRoots<'s, 'b>,
    ) -> ContentResult<Self> {
        let cursor = InitialCursor {
            input,
            state,
            roots,
            serials: input.new_inodes()?,
            position: 0,
            ended: false,
        };
        let source = if state.backed() {
            Source::Backed(cursor)
        } else {
            // The resident compatibility route checks/collects before the
            // existing inode-construction phase, just as its old row Vec did.
            Source::Memory(cursor.collect::<ContentResult<Vec<_>>>()?.into_iter())
        };
        Ok(Self { source })
    }
}
impl InitialCursor<'_, '_, '_> {
    fn row(&mut self) -> ContentResult<Option<InitialRow>> {
        loop {
            let Some(serial) = self.serials.next_row()? else {
                return if self.position == self.input.new_rows() {
                    Ok(None)
                } else {
                    Err(ContentError::InvalidRecord("new inode row count"))
                };
            };
            if self.position >= self.input.new_rows() {
                return Err(ContentError::InvalidRecord("new inode row count"));
            }
            let position = self.position;
            self.position += 1;
            if self.state.dropped(self.input, serial)? {
                continue;
            }
            let count = self.state.count(serial, position)?;
            if count == 0 && serial != self.input.root_serial() {
                return Err(ContentError::InvalidRecord("new inode without binding"));
            }
            let value = self
                .input
                .value_for(serial)?
                .ok_or(ContentError::InvalidRecord("new inode value"))?;
            // The same single-binding invariant the reducer enforces on an
            // update's final values; a build takes its rows from here instead.
            if value.kind != crate::object::inode_leaf::InodeKind::RegularFile && count > 1 {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
            let root = self.roots.get(serial)?;
            if value.kind == crate::object::inode_leaf::InodeKind::Directory && root.is_none() {
                return Err(ContentError::InvalidRecord(
                    "filesystem rebuilt root missing",
                ));
            }
            return Ok(Some((
                serial,
                Some(InodeValue {
                    namespace_ref_count: count,
                    content_root: root.unwrap_or(value.content_root),
                    ..value
                }),
            )));
        }
    }
}
impl Iterator for InitialCursor<'_, '_, '_> {
    type Item = ContentResult<InitialRow>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let result = self.row().transpose();
        if result.as_ref().is_none_or(|result| result.is_err()) {
            self.ended = true;
        }
        result
    }
}
impl Iterator for InitialRows<'_, '_, '_> {
    type Item = ContentResult<InitialRow>;
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.source {
            Source::Memory(rows) => rows.next().map(Ok),
            Source::Backed(rows) => rows.next(),
        }
    }
}
