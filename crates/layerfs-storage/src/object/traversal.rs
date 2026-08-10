//! Shared bounded traversal laws for canonical typed-object graphs.

use crate::format::{MAX_PATH_DEPTH, MAX_TREE_PAGE_DEPTH};
use crate::{CoreError, CoreResult};

/// A canonical path may contain at most 256 child components. At every path
/// level, traversal may retain one directory frame, up to two index-page
/// frames, and one leaf/child frame. The extra root level covers the root
/// directory and its page chain without permitting a 257th path component.
pub(crate) const MAX_CANONICAL_TRAVERSAL_FRAMES_V1: usize =
    (MAX_PATH_DEPTH + 1) * (MAX_TREE_PAGE_DEPTH as usize + 2);

pub(crate) fn require_canonical_traversal_depth_v1(depth: usize) -> CoreResult<()> {
    if depth < MAX_CANONICAL_TRAVERSAL_FRAMES_V1 {
        Ok(())
    } else {
        Err(CoreError::CountCap)
    }
}
