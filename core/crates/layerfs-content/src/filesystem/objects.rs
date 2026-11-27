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
    accepted: Option<&'a dyn AuthenticatedObjects>,
    work: ObjectWork,
    inode_read_ahead: bool,
    base_reader: Option<&'a dyn AuthenticatedObjects>,
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
            accepted: None,
            work: ObjectWork::default(),
            inode_read_ahead: false,
            base_reader: None,
        }
    }

    /// Publishes to `consumer` and explicitly reads objects it accepted during
    /// this same Save through `accepted`. The caller supplies the actual
    /// authorized capability; this constructor does not establish authority,
    /// topology closure or durability. A failed accepted read never falls back.
    pub fn new_with_accepted(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        accepted: &'a dyn AuthenticatedObjects,
    ) -> Self {
        Self {
            reader,
            consumer,
            accepted: Some(accepted),
            work: ObjectWork::default(),
            inode_read_ahead: false,
            base_reader: None,
        }
    }

    /// Allows the inode merge to batch reads of touched stored leaf pages.
    pub fn with_inode_read_ahead(mut self, enabled: bool) -> Self {
        self.inode_read_ahead = enabled;
        self
    }

    /// Supplies an authenticated read port for the exact immutable base.
    pub fn with_base_reader(mut self, base: Option<&'a dyn AuthenticatedObjects>) -> Self {
        self.base_reader = base;
        self
    }

    pub(crate) fn base_reader(&self) -> &'a dyn AuthenticatedObjects {
        self.base_reader.unwrap_or(self.reader)
    }

    pub(crate) const fn inode_read_ahead(&self) -> bool {
        self.inode_read_ahead
    }

    /// The explicitly supplied same-Save accepted-object capability.
    pub fn accepted_reader(&self) -> ContentResult<&'a dyn AuthenticatedObjects> {
        self.accepted.ok_or(crate::ContentError::ProviderFailure {
            what: "filesystem accepted-object reader unavailable",
        })
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

    /// Reads one bounded group as a single authenticated demand wave.
    ///
    /// The group is bounded by [`MAXIMUM_READ_DEMANDS`]: one wave is one grouped
    /// provider call and one decode workspace, so its size is a declared resource
    /// rather than whatever slice a caller happens to pass. The provider's own
    /// ceiling is the same figure, so a wave this boundary accepts is one the
    /// storage side accepts too.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_from(self.reader, ids)
    }

    fn read_from(
        &mut self,
        reader: &'a dyn AuthenticatedObjects,
        ids: &[ObjectId],
    ) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > MAXIMUM_READ_DEMANDS {
            return Err(crate::error::ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_READ_DEMANDS,
                actual: ids.len(),
            });
        }
        let values = reader.read_canonical_batch(ids)?;
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
        Ok(values)
    }

    /// Reads a bounded cohort of known base pages.
    pub(crate) fn read_base_pages(&mut self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_from(self.base_reader(), ids)
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

/// Reads namespace pages already authenticated and held for the exact base,
/// then sends only misses to the ordinary provider. It does not admit
/// canonical bytes into the resident cache. Accepted Save reads use a
/// separate capability.
pub struct BasePageReader<'a> {
    cache: &'a dyn AuthenticatedObjects,
    provider: &'a dyn AuthenticatedObjects,
}

impl<'a> BasePageReader<'a> {
    /// Both ports are already authorized to read this immutable base.
    pub fn new(
        cache: &'a dyn AuthenticatedObjects,
        provider: &'a dyn AuthenticatedObjects,
    ) -> Self {
        Self { cache, provider }
    }
}

const BASE_RESIDENT_BYTES: usize = 2 * 1024 * 1024;

impl AuthenticatedObjects for BasePageReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let bound = ids
            .len()
            .saturating_mul(crate::filesystem::limits::MAXIMUM_PAGE_BYTES)
            .min(BASE_RESIDENT_BYTES);
        let values = self.cache.cached_canonical_batch(ids, bound)?;
        if values.len() != ids.len() {
            return Err(crate::ContentError::BatchCardinality {
                requested: ids.len(),
                returned: values.len(),
            });
        }
        let missing: Vec<_> = ids
            .iter()
            .zip(&values)
            .filter_map(|(id, value)| value.is_none().then_some(*id))
            .collect();
        let mut fetched = if missing.is_empty() {
            Vec::new()
        } else {
            self.provider.read_canonical_batch(&missing)?
        }
        .into_iter();
        if fetched.len() != missing.len() {
            return Err(crate::ContentError::BatchCardinality {
                requested: missing.len(),
                returned: fetched.len(),
            });
        }
        let mut result = Vec::with_capacity(values.len());
        for value in values {
            result.push(
                value
                    .or_else(|| fetched.next())
                    .ok_or(crate::ContentError::InvalidRecord("cached base page reply"))?,
            );
        }
        Ok(result)
    }

    fn decoded_pages(&self) -> Option<&super::decoded::DecodedPages> {
        self.cache.decoded_pages()
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
