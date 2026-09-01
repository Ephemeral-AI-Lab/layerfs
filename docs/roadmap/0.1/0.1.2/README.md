# LayerFS 0.1.2 proposals

> **Status:** Proposed compatibility-preserving follow-up; no release candidate
> exists.

## Problem statement

The v0.1.0 payload campaign proved exact prepend and small-edit results with
strong canonical reuse, but it also exposed remaining transfer and mixed-edit
work. These opportunities should be addressed only after v0.1.1 completes the
namespace benchmark and only through the existing real-FUSE and Docker path.

## Goal

Use v0.1.2 for measured, compatibility-preserving payload and edit
optimizations against the same sealed FUSE/Docker environment and public SDK
lifecycle used by v0.1.0 and v0.1.1.

Candidate inputs:

- [Large and mixed-edit capture resilience](capture-large-mixed-edit-resilience.md)
- [Extent-aware `copy_file_range` and prepend](copy-file-range-prepend.md)

Each item still needs public-path evidence and a compatibility decision.
Patch-compatible internal fixes may ship in 0.1.2. Any required Store-schema,
canonical-format, identity, SDK/CLI, or incompatible daemon-protocol change
moves only that item to 0.2.0.

## Files to read

- [0.1.x phase](../README.md)
- [0.1.x benchmark contract](../benchmarking.md)
- [v0.1.1 checklist](../0.1.1/README.md)
- [Large and mixed-edit capture resilience](capture-large-mixed-edit-resilience.md)
- [Extent-aware `copy_file_range` and prepend](copy-file-range-prepend.md)

## Acceptance criteria

- [ ] Start from retained v0.1.0/v0.1.1 benchmark evidence and admit only a
  measured defect or opportunity.
- [ ] Iterate with the smallest failing LayerFS-only row; run the paired
  Cloudflare payload campaign only after candidate stability.
- [ ] Keep real FUSE, Docker custody, fresh processes, timing boundaries,
  acknowledgement, integrity checks, and registered scenario meanings fixed.
- [ ] Preserve exact final bytes, canonical roots, fresh reopen results, and
  all 0.1.x compatibility boundaries.
- [ ] Add one focused regression check for every retained optimization.
- [ ] Rerun registered payload and namespace matrices and explain every
  regression.
- [ ] Move any incompatible mechanism to 0.2.0 rather than weakening the
  patch-line contract.
