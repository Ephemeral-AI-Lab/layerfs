//! Which bytes of a pack one acquisition of the mark's walk reads.
//!
//! Kept in a submodule so `objects.rs` stays within the product source-line
//! ceiling.
use layerfs_storage::{
    location::PackInfo,
    pack::layout::{self, PackLane},
    port::{PackRange, PackReadChoice, PackReadPlan, PersistenceError, PACK_READ_PREFIX_BYTES},
};

/// The complete encoded groups one mark walk demands from one pack.
///
/// A metadata-only walk needs one record header per object and one delta edge
/// per record, so the demand is sparse; the plan selects those exact group
/// ranges and the backend reads only them, never the body behind the
/// directory. A dense demand or a cache that already holds another group of
/// the pack still materializes the whole body once.
pub(super) struct MarkGroups<'a> {
    pub(super) pack_id: i64,
    pub(super) groups: &'a [usize],
    pub(super) whole: bool,
}

impl PackReadPlan for MarkGroups<'_> {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        if info.pack_id != self.pack_id || self.groups.is_empty() {
            return Err(PersistenceError::Malformed);
        }
        let header = layout::parse_directory_header(prefix, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        if self.whole || header.lane == PackLane::Singleton || info.length <= PACK_READ_PREFIX_BYTES
        {
            return Ok(PackReadChoice::Whole);
        }
        let views = layout::directory_group_views(prefix, header)
            .map_err(|_| PersistenceError::Malformed)?;
        let mut ranges: Vec<PackRange> = Vec::new();
        let mut useful = 0usize;
        let mut previous = None;
        for &group in self.groups {
            if previous.is_some_and(|prior| prior >= group) {
                return Err(PersistenceError::Malformed);
            }
            previous = Some(group);
            let view = *views.get(group).ok_or(PersistenceError::Malformed)?;
            useful = useful
                .checked_add(view.end - view.start)
                .ok_or(PersistenceError::Malformed)?;
            if let Some(prior) = ranges
                .last_mut()
                .filter(|prior| prior.offset + prior.length == view.start)
            {
                prior.length += view.end - view.start;
            } else {
                ranges.push(PackRange {
                    offset: view.start,
                    length: view.end - view.start,
                });
            }
        }
        if useful >= info.length.div_ceil(2) || ranges.len() > 4 {
            Ok(PackReadChoice::Whole)
        } else {
            Ok(PackReadChoice::Ranges(ranges))
        }
    }
}
