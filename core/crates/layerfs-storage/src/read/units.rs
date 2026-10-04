//! Storage-owned complete-group selection and bounded adjacent coalescing.
use crate::{
    location::{PackDomain, PackInfo},
    pack::layout::{self, PackLane},
    port::{PackRange, PackReadChoice, PackReadPlan, PersistenceError},
    StorageError, StorageResult,
};

pub(crate) struct GroupPlan<'a> {
    id: i64,
    groups: &'a [usize],
    pub(crate) error: Option<StorageError>,
    pub(crate) selected_info: Option<PackInfo>,
    pub(crate) selected_choice: Option<PackReadChoice>,
    pub(crate) whole_reason: Option<&'static str>,
}
impl<'a> GroupPlan<'a> {
    pub(crate) fn new(id: i64, groups: &'a [usize]) -> Self {
        Self {
            id,
            groups,
            error: None,
            selected_info: None,
            selected_choice: None,
            whole_reason: None,
        }
    }
    fn choice(&mut self, info: PackInfo, prefix: &[u8]) -> StorageResult<PackReadChoice> {
        if info.pack_id != self.id
            || self.groups.is_empty()
            || self.groups.len() > crate::policy::READ_OBJECT_LIMIT
        {
            return Err(StorageError::Integrity("pack group demand binding"));
        }
        let header = layout::parse_directory_header(prefix, info.length)?;
        if PackDomain::for_lane(header.lane) != info.domain {
            return Err(StorageError::Integrity("sealed pack length/domain"));
        }
        let views = layout::directory_group_views(prefix, header)?;
        let mut ranges: Vec<PackRange> = Vec::new();
        let mut previous = None;
        let mut useful = 0usize;
        for &group in self.groups {
            if previous.is_some_and(|prior| prior >= group) {
                return Err(StorageError::Integrity("pack group demand order"));
            }
            previous = Some(group);
            let view = *views
                .get(group)
                .ok_or(StorageError::Integrity("group ordinal"))?;
            useful = useful
                .checked_add(view.end - view.start)
                .ok_or(StorageError::Integrity("selected group bytes"))?;
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
        self.whole_reason = if header.lane == PackLane::Singleton {
            Some("singleton")
        } else if info.length <= crate::port::PACK_READ_PREFIX_BYTES {
            Some("small")
        } else if useful >= info.length.div_ceil(2) || ranges.len() > 4 {
            Some("density")
        } else {
            None
        };
        Ok(if self.whole_reason.is_some() {
            PackReadChoice::Whole
        } else {
            PackReadChoice::Ranges(ranges)
        })
    }
}
impl PackReadPlan for GroupPlan<'_> {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        if self.selected_info.is_some() {
            return Err(PersistenceError::Malformed);
        }
        self.choice(info, prefix)
            .inspect(|choice| {
                self.selected_info = Some(info);
                self.selected_choice = Some(choice.clone());
            })
            .map_err(|error| {
                self.error = Some(error);
                PersistenceError::Malformed
            })
    }
}
