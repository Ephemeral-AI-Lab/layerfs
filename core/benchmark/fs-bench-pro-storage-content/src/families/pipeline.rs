//! `pipeline.*`: the integrated C1 to C2 handoff.
//!
//! Six rows. `CONTRACT.md` section 3 freezes four and was one row behind the tree
//! before this round: the fifth row, `pipeline-namespace-10000`, was registered in
//! #219 and the frozen cardinality was never amended, so `registry::self_check`
//! reported `frozen cardinality array` and `runner.py self-check` failed. The sixth
//! row is `pipeline-namespace-100000`, and the constant now counts six.
//!
//! Integration only means something where the handoff *is* the question, so it is
//! not applied to `c2.read.*`, `c2.pool.*` or `c2.footprint`. Decision D1 records `pipeline.filesystem` and
//! `pipeline.c2` as `NOT_RUN` for the comparative option; these rows are the
//! absolute, single-arm integrated cases and carry no comparative claim.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, PipelineOp, Shape, StoreState};

/// Registry-group identifier. Not a family: the twenty-family smoke lane counts it
/// separately, which is why `--smoke` is twenty cases and not twenty-one.
pub const GROUP: &str = "pipeline.*";

/// Six rows.
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
        (
            PipelineOp::NamespaceScale,
            "pipeline-namespace-10000",
        ),
        (
            PipelineOp::NamespaceScaleLarge,
            "pipeline-namespace-100000",
        ),
    ]
    .iter()
    .map(|(op, id)| {
        let _ = leak(String::new());
        // Every pipeline row measures against a base that must already be
        // stored: the edit rows read their base through the Store and the
        // filesystem row saves into a Store opened over a prepared copy, so the
        // declaration is `prepared-dewarmed` / `opened-from-copy` and never
        // `created-in-sample`. The earlier declaration said otherwise and no
        // driver existed to contradict it.
        let spec = CaseSpec::new(id, GROUP, Shape::Pipeline(*op))
            .cache(CacheState::PreparedDewarmed)
            .store(StoreState::OpenedFromCopy)
            .smoke_if(*op == PipelineOp::EditsSmall);
        // The tier is the reference ladder's position for the shape, and it matches
        // `c1.fs.build-scale`'s own rung at the same entry count: tier 2 at 10,000
        // and tier 3 at 100,000, both `binary`.
        let spec = match *op {
            PipelineOp::NamespaceScale => spec.entry_tier(2, 10_000, "binary"),
            PipelineOp::NamespaceScaleLarge => spec.entry_tier(3, 100_000, "binary"),
            _ => spec,
        };
        spec.build()
    })
    .collect()
}
