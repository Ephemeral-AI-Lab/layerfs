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

/// Objects one authenticated read wave may demand through a filesystem operation.
///
/// The storage side declares the same figure, and the pool that feeds both is
/// bounded by it, so a wave is always a bounded amount of provider and decode
/// work rather than an unchecked slice length.
pub const MAXIMUM_READ_DEMANDS: usize = 4_096;

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
    ///
    /// The group is bounded by [`MAXIMUM_READ_DEMANDS`]: one wave is one grouped
    /// provider call and one decode workspace, so its size is a declared resource
    /// rather than whatever slice a caller happens to pass. The provider's own
    /// ceiling is the same figure, so a wave this boundary accepts is one the
    /// storage side accepts too.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > MAXIMUM_READ_DEMANDS {
            return Err(crate::error::ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_READ_DEMANDS,
                actual: ids.len(),
            });
        }
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

/// Coarse phase scopes for one filesystem operation.
///
/// A phase is recorded as a child of the caller's running scope, so an enabled
/// recording reports where a complete operation spent its time without one trace
/// node per inode. Disabled phases run the identical body and read no clock: the
/// result, the errors and the algorithm are unchanged either way.
pub struct FilesystemPhases<'a> {
    scope: Option<&'a layerfs_telemetry::timer::TimingScope<'a, layerfs_telemetry::timer::Active>>,
}

impl<'a> FilesystemPhases<'a> {
    /// Phases that measure nothing.
    pub const fn disabled() -> Self {
        Self { scope: None }
    }

    /// Phases below one running scope.
    pub const fn new(
        scope: &'a layerfs_telemetry::timer::TimingScope<'a, layerfs_telemetry::timer::Active>,
    ) -> Self {
        Self { scope: Some(scope) }
    }

    /// True when these phases actually record.
    pub fn is_recording(&self) -> bool {
        self.scope.is_some_and(|scope| scope.is_recording())
    }

    /// Runs one phase, recording it when the caller enabled measurement.
    pub fn phase<T>(
        &self,
        name: &'static str,
        body: impl FnOnce() -> ContentResult<T>,
    ) -> ContentResult<T> {
        match self.scope {
            Some(scope) => scope.child(name).run(|_| body()),
            None => body(),
        }
    }
}
