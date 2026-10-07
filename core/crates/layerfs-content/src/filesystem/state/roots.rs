//! Rebuilt roots: resident map or points replayed through sealed parent headers.
use super::SerialState;
use crate::filesystem::rows::view::{OperationDirectories, OperationInput};
use crate::{ContentError, ContentResult, ObjectId};
use std::collections::BTreeMap;

pub(crate) struct RebuiltRoots<'s, 'b> {
    state: &'s SerialState<'b>,
    memory: BTreeMap<u64, ObjectId>,
}
impl<'s, 'b> RebuiltRoots<'s, 'b> {
    pub fn new(state: &'s SerialState<'b>) -> Self {
        Self {
            state,
            memory: BTreeMap::new(),
        }
    }
    pub fn insert(&mut self, serial: u64, root: ObjectId) -> ContentResult<()> {
        if self.state.backed() {
            self.state.put_root(serial, root)
        } else {
            self.memory.insert(serial, root);
            Ok(())
        }
    }
    pub fn get(&self, serial: u64) -> ContentResult<Option<ObjectId>> {
        if self.state.backed() {
            self.state.root(serial)
        } else {
            Ok(self.memory.get(&serial).copied())
        }
    }
    pub fn rows<'a>(
        &'a self,
        input: &'a dyn OperationInput,
    ) -> ContentResult<RootRows<'a, 's, 'b>> {
        let source = if self.state.backed() {
            Source::Backed {
                roots: self,
                headers: input.directories()?,
                input,
                seen: 0,
                previous: 0,
            }
        } else {
            Source::Memory(self.memory.iter())
        };
        Ok(RootRows {
            source,
            ended: false,
        })
    }
}
enum Source<'a, 's, 'b> {
    Memory(std::collections::btree_map::Iter<'a, u64, ObjectId>),
    Backed {
        roots: &'a RebuiltRoots<'s, 'b>,
        headers: Box<dyn OperationDirectories<'a> + 'a>,
        input: &'a dyn OperationInput,
        seen: usize,
        previous: u64,
    },
}
pub(crate) struct RootRows<'a, 's, 'b> {
    source: Source<'a, 's, 'b>,
    ended: bool,
}
impl Iterator for RootRows<'_, '_, '_> {
    type Item = ContentResult<(u64, ObjectId)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let result = match &mut self.source {
            Source::Memory(rows) => rows.next().map(|(serial, root)| Ok((*serial, *root))),
            Source::Backed {
                roots,
                headers,
                input,
                seen,
                previous,
            } => {
                let next = (|| loop {
                    let Some(header) = headers.next_row()? else {
                        return if *seen == input.directory_rows() {
                            Ok(None)
                        } else {
                            Err(ContentError::InvalidRecord("directory row count"))
                        };
                    };
                    header.check_header()?;
                    if *seen >= input.directory_rows() {
                        return Err(ContentError::InvalidRecord("directory row count"));
                    }
                    if *seen > 0 && *previous >= header.header.parent {
                        return Err(ContentError::NonCanonicalOrdering);
                    }
                    *seen += 1;
                    *previous = header.header.parent;
                    if let Some(root) = roots.get(header.header.parent)? {
                        return Ok(Some((header.header.parent, root)));
                    }
                    if !roots.state.dropped(*input, header.header.parent)? {
                        return Err(ContentError::InvalidRecord(
                            "filesystem rebuilt root missing",
                        ));
                    }
                })();
                next.transpose()
            }
        };
        if result.as_ref().is_none_or(|result| result.is_err()) {
            self.ended = true;
        }
        result
    }
}
