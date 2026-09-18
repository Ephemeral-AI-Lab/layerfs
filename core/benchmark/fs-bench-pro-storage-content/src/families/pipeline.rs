//! `pipeline.*`: the integrated C1 to C2 handoff.
//!
//! Four rows, frozen by `CONTRACT.md` section 3. Integration only means something
//! where the handoff *is* the question, so it is not applied to `c2.read.*`,
//! `c2.pool.*` or `c2.footprint`. Decision D1 records `pipeline.filesystem` and
//! `pipeline.c2` as `NOT_RUN` for the comparative option; these rows are the
//! absolute, single-arm integrated cases and carry no comparative claim.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, PipelineOp, Shape, StoreState};

/// Registry-group identifier. Not a family: the twenty-family smoke lane counts it
/// separately, which is why `--smoke` is twenty cases and not twenty-one.
pub const GROUP: &str = "pipeline.*";

/// Four rows.
pub fn cases() -> Vec<Case> {
    [
        (PipelineOp::EditsSmall, "pipeline-edits-small"),
        (PipelineOp::EditsChunked, "pipeline-edits-chunked"),
        (
            PipelineOp::EditsLargeToSmall,
            "pipeline-edits-large-to-small",
        ),
        (
            PipelineOp::FilesystemBuild,
            "pipeline-filesystem-build",
        ),
    ]
    .iter()
    .map(|(op, id)| {
        let _ = leak(String::new());
        CaseSpec::new(id, GROUP, Shape::Pipeline(*op))
            .cache(CacheState::CreatedInSample)
            .store(StoreState::CreatedInSample)
            .smoke_if(*op == PipelineOp::EditsSmall)
            .build()
    })
    .collect()
}
