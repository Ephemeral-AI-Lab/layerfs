# LayerFS 0.1.2 proposals

> **Status:** Proposed compatibility-preserving follow-up; no release candidate
> exists.

LayerFS 0.1.2 is the next patch bucket after the focused 0.1.1 lifecycle work.
Candidate inputs:

- [Large and mixed-edit capture resilience](capture-large-mixed-edit-resilience.md)
- [Extent-aware `copy_file_range` and prepend](copy-file-range-prepend.md)

Each item still needs public-path evidence and a compatibility decision.
Patch-compatible internal fixes may ship in 0.1.2. Any required Store-schema,
canonical-format, identity, SDK/CLI, or incompatible daemon-protocol change
moves only that item to 0.2.0.
