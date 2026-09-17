//! The operation's bounded object boundary: authenticated reads plus final output.
//!
//! Every filesystem entry point takes one [`FilesystemObjects`] value instead of
//! a store. Reads go to the caller's authenticated provider, exactly as file
//! construction and logical reads already do, and every finalized tree page moves
//! to the caller's bounded consumer. Nothing here opens a database, a pack or a
//! file, and nothing retries: a refused object ends the operation.
//!
//! The counters describe work this operation actually did. They are not a
//! durability or storage claim: acknowledged persistence belongs to the consumer.

use crate::error::ContentResult;
use crate::object::{AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId};

/// Work one filesystem operation performed on canonical objects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObjectWork {
    /// Canonical objects read.
    pub objects_read: u64,
    /// Grouped demand waves issued.
    pub read_waves: u64,
    /// Canonical bytes read.
    pub bytes_read: u64,
    /// Finalized objects emitted.
    pub objects_emitted: u64,
    /// Canonical bytes emitted.
    pub bytes_emitted: u64,
}

/// Authenticated reads and finalized output for one operation.
pub struct FilesystemObjects<'a> {
    reader: &'a dyn AuthenticatedObjects,
    consumer: &'a mut dyn FinalizedConsumer,
    work: ObjectWork,
}

impl<'a> FilesystemObjects<'a> {
    /// Empty operation state over `reader`, publishing to `consumer`.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
    ) -> Self {
        Self {
            reader,
            consumer,
            work: ObjectWork::default(),
        }
    }

    /// Work performed so far.
    pub const fn work(&self) -> ObjectWork {
        self.work
    }

    /// The authenticated provider, with its own lifetime.
    ///
    /// A caller that needs to read while this boundary is mutably borrowed
    /// elsewhere keeps this reference: it borrows the provider, not the boundary.
    pub fn reader(&self) -> &'a dyn AuthenticatedObjects {
        self.reader
    }

    /// Reads one authenticated canonical object and charges it.
    pub fn read(&mut self, id: ObjectId) -> ContentResult<Vec<u8>> {
        let canonical = self.reader.read_canonical(id)?;
        self.note_read(1, canonical.len() as u64);
        Ok(canonical)
    }

    /// Reads one authenticated canonical object without mutating the boundary.
    pub fn read_shared(reader: &dyn AuthenticatedObjects, id: ObjectId) -> ContentResult<Vec<u8>> {
        reader.read_canonical(id)
    }

    /// Reads one bounded group as a single authenticated demand wave.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let values = self.reader.read_canonical_batch(ids)?;
        if values.len() != ids.len() {
            return Err(crate::error::ContentError::BatchCardinality {
                requested: ids.len(),
                returned: values.len(),
            });
        }
        self.note_read(
            ids.len() as u64,
            values.iter().map(|value| value.len() as u64).sum(),
        );
        self.work.read_waves = self.work.read_waves.saturating_add(1);
        Ok(values)
    }

    /// Moves one finalized object to the consumer and charges it.
    pub fn emit(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
        let id = object.id();
        let bytes = object.canonical_len() as u64;
        self.consumer.accept(object)?;
        self.work.objects_emitted = self.work.objects_emitted.saturating_add(1);
        self.work.bytes_emitted = self.work.bytes_emitted.saturating_add(bytes);
        Ok(id)
    }

    fn note_read(&mut self, objects: u64, bytes: u64) {
        self.work.objects_read = self.work.objects_read.saturating_add(objects);
        self.work.bytes_read = self.work.bytes_read.saturating_add(bytes);
        self.work.read_waves = self.work.read_waves.saturating_add(1);
    }
}
