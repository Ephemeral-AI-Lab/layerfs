//! The owned, decoded, unfinished representation of one edit operation.
//!
//! There is exactly one frontier per operation. It holds decoded extents and page
//! summaries, never encoded draft nodes: retained slices are appended directly and
//! replacement runs are chunked straight into the same builder, so no draft has to
//! be decoded, pruned and re-encoded later. Pages are sealed as soon as a later
//! edit cannot reach them, which bounds the retained entries by the tree height
//! and the page capacity instead of by the file length or the edit count.

use crate::error::{ContentError, ContentResult};
use crate::file::cdc::FastCdc;
use crate::file::edit::input::{EditSource, ReplacementReader};
use crate::file::edit::split::slice_of;
use crate::file::mapping::{ExtentBuilder, MappingBuild};
use crate::object::{FinalizedConsumer, ObjectId};
use crate::policy::ConstructionCapacities;

/// Decoded unfinished output of one known edit.
pub struct EditFrontier {
    builder: ExtentBuilder,
    capacities: ConstructionCapacities,
    retained_extents: u64,
    replacement_runs: u64,
    replacement_bytes: u64,
    /// Payload of the most recently retained slice: the declared correspondence a
    /// replacement run continues. It is a hint for physical selection only.
    previous_payload: Option<ObjectId>,
}

impl EditFrontier {
    /// Empty frontier under the declared capacities.
    pub fn new(capacities: &ConstructionCapacities) -> Self {
        Self {
            builder: ExtentBuilder::new(capacities),
            capacities: *capacities,
            retained_extents: 0,
            replacement_runs: 0,
            replacement_bytes: 0,
            previous_payload: None,
        }
    }

    /// Appends the retained part `from..to` of a base extent starting at `origin`.
    pub fn retain(
        &mut self,
        extent: crate::file::mapping::ExtentSlice,
        origin: u64,
        from: u64,
        to: u64,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<()> {
        let slice = slice_of(extent, origin, from, to)?;
        self.builder.push_extent(slice, consumer)?;
        self.retained_extents = self.retained_extents.saturating_add(1);
        self.previous_payload = Some(slice.payload_object_id());
        Ok(())
    }

    /// Chunks one replacement run into the same builder.
    pub fn replace(
        &mut self,
        source: &dyn EditSource,
        index: usize,
        length: u64,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<()> {
        if source.replacement_len(index) != length {
            return Err(ContentError::InvalidEdit {
                what: "replacement length",
            });
        }
        self.replacement_runs = self.replacement_runs.saturating_add(1);
        if length == 0 {
            return Ok(());
        }
        self.replacement_bytes = self.replacement_bytes.saturating_add(length);
        let reader = ReplacementReader::new(source, index, length);
        let mut chunks = 0_u64;
        let predecessor = self.previous_payload;
        let counters = FastCdc::new().scan(reader, |chunk| {
            chunks = chunks.saturating_add(1);
            self.builder
                .push_chunk(chunk, predecessor, consumer)
                .map(|_| ())
        })?;
        if counters.bytes_scanned != length || chunks == 0 {
            return Err(ContentError::InvalidEdit {
                what: "replacement bytes",
            });
        }
        Ok(())
    }

    /// Logical bytes the frontier would emit.
    pub fn logical_len(&self) -> u64 {
        self.builder.logical_len()
    }

    /// Largest number of decoded entries retained at once.
    pub fn peak_pending(&self) -> usize {
        self.builder.peak_pending()
    }

    /// Retained slices appended so far.
    pub const fn retained_extents(&self) -> u64 {
        self.retained_extents
    }

    /// Replacement runs chunked so far.
    pub const fn replacement_runs(&self) -> u64 {
        self.replacement_runs
    }

    /// Replacement bytes chunked so far.
    pub const fn replacement_bytes(&self) -> u64 {
        self.replacement_bytes
    }

    /// Declared capacities of this operation.
    pub const fn capacities(&self) -> &ConstructionCapacities {
        &self.capacities
    }

    /// Seals every remaining page, child first, and returns the build facts.
    pub fn finish(self, consumer: &mut dyn FinalizedConsumer) -> ContentResult<MappingBuild> {
        self.builder.finish(consumer)
    }
}
