//! Private routing of one canonical reducer meaning to resident or indexed state.
use super::{indexed::IndexedReducer, indexed_release, indexed_rows::IndexedRows};
use super::{
    FinalChange, FinalRows, OrderingBacking, ReferenceReducer, ReferenceWork, ReleaseWork,
};
use crate::filesystem::{
    inode::read::InodeTable,
    objects::FilesystemObjects,
    rows::view::OperationInput,
    state::{DroppedParents, SerialState},
};
use crate::object::inode_leaf::InodeValue;
use crate::{AuthenticatedObjects, ContentResult};

pub(crate) struct OperationReducer<'s, 'p, 'r, 'b> {
    memory: Option<ReferenceReducer<'r, 'b>>,
    indexed: Option<IndexedReducer<'s, 'p>>,
    ordering: Option<&'r mut (dyn OrderingBacking + 'b)>,
}
impl<'s, 'p, 'r, 'b> OperationReducer<'s, 'p, 'r, 'b> {
    pub fn new(
        state: &'s SerialState<'p>,
        input: &'s dyn OperationInput,
        dropped: &'s dyn DroppedParents,
        backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
    ) -> Self {
        if state.backed() {
            Self {
                memory: None,
                indexed: Some(IndexedReducer::new(state, input, dropped)),
                ordering: backing,
            }
        } else {
            let resources = input.resources();
            Self {
                memory: Some(ReferenceReducer::new(
                    resources.maximum_pending_records,
                    backing,
                    resources.merge_buffer_bytes,
                    resources.ordering_bytes,
                )),
                indexed: None,
                ordering: None,
            }
        }
    }
    pub fn memory(&mut self) -> Option<&mut ReferenceReducer<'r, 'b>> {
        self.memory.as_mut()
    }
    pub fn check_backing_capacity(&self) -> ContentResult<()> {
        match &self.memory {
            Some(memory) => memory.check_backing_capacity(),
            None => Ok(()),
        }
    }
    pub fn declare_new(&mut self, serial: u64) -> ContentResult<()> {
        match (&mut self.memory, &mut self.indexed) {
            (Some(memory), _) => memory.declare_new(serial),
            (_, Some(indexed)) => indexed.declare_new(serial),
            _ => unreachable!("one reducer route"),
        }
    }
    pub fn note_retained_binding(&mut self, serial: u64) -> ContentResult<()> {
        match (&mut self.memory, &mut self.indexed) {
            (Some(memory), _) => memory.note_retained_binding(serial),
            (_, Some(indexed)) => indexed.note_retained_binding(serial),
            _ => unreachable!("one reducer route"),
        }
    }
    pub fn note_removed_binding(&mut self, serial: u64) -> ContentResult<()> {
        match (&mut self.memory, &mut self.indexed) {
            (Some(memory), _) => memory.note_removed_binding(serial),
            (_, Some(indexed)) => indexed.note_removed_binding(serial),
            _ => unreachable!("one reducer route"),
        }
    }
    pub fn note_value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
        match (&mut self.memory, &mut self.indexed) {
            (Some(memory), _) => memory.note_value(serial, value),
            (_, Some(indexed)) => indexed.note_value(serial, value),
            _ => unreachable!("one reducer route"),
        }
    }
    pub fn release_indexed(
        &mut self,
        objects: &FilesystemObjects<'_>,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<(ReleaseWork, u64)> {
        indexed_release::release(
            objects,
            table,
            self.indexed.as_mut().expect("indexed route"),
            batch,
            root,
        )
    }
    pub fn finish<'a>(
        &'a mut self,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<OperationRows<'a, 'p>> {
        match (&mut self.memory, &self.indexed) {
            (Some(memory), _) => Ok(OperationRows {
                route: RowRoute::Memory(Box::new(memory.finish(reader, table, batch, root)?)),
            }),
            (_, Some(indexed)) => Ok(OperationRows {
                route: RowRoute::Indexed(Box::new(IndexedRows::new(
                    indexed, reader, table, batch, root,
                ))),
            }),
            _ => unreachable!("one reducer route"),
        }
    }
    pub fn release(&mut self) -> ContentResult<()> {
        if let Some(memory) = &mut self.memory {
            memory.release()?;
        }
        if let Some(ordering) = &mut self.ordering {
            ordering.release()?;
        }
        Ok(())
    }
}

pub(crate) struct OperationRows<'a, 'p> {
    route: RowRoute<'a, 'p>,
}
enum RowRoute<'a, 'p> {
    Memory(Box<FinalRows<'a>>),
    Indexed(Box<IndexedRows<'a, 'p, 'a>>),
}
impl OperationRows<'_, '_> {
    pub fn next_change(&mut self) -> ContentResult<Option<FinalChange>> {
        match &mut self.route {
            RowRoute::Memory(rows) => rows.next_change(),
            RowRoute::Indexed(rows) => rows.next_change(),
        }
    }
    pub fn work(&self) -> ReferenceWork {
        match &self.route {
            RowRoute::Memory(rows) => rows.work(),
            RowRoute::Indexed(rows) => rows.work(),
        }
    }
}
