//! Open-file reference counts.
use crate::{OverlayError, OverlayResult, ReferenceKind};

#[derive(Clone, Copy, Default)]
pub(crate) struct FileReferences {
    pub opens: u64,
    pub lookups: u64,
    pub readers: u64,
}
impl FileReferences {
    pub fn total(self) -> u64 {
        self.opens + self.lookups + self.readers
    }
    pub fn change(&mut self, kind: ReferenceKind, add: bool) -> OverlayResult<()> {
        let value = match kind {
            ReferenceKind::File => &mut self.opens,
            ReferenceKind::Lookup => &mut self.lookups,
            ReferenceKind::Reader => &mut self.readers,
        };
        *value = if add {
            value.checked_add(1)
        } else {
            value.checked_sub(1)
        }
        .ok_or(OverlayError::Stale)?;
        Ok(())
    }
}
