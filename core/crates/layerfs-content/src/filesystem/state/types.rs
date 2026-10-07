//! Private dropped-parent view shared by the resident and backed state paths.
use crate::ContentResult;
use std::collections::BTreeMap;

/// The existing dropped-new-parent decision, never a reachability certificate.
pub(crate) trait DroppedParents {
    fn contains(&self, serial: u64) -> ContentResult<bool>;
}
impl DroppedParents for BTreeMap<u64, ()> {
    fn contains(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.contains_key(&serial))
    }
}
