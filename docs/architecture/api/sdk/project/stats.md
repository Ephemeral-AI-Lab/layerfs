# project.stats

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs project stats <store>/<project> [<branch>] [<commit>]`
(SDK `project.stats(project, branch?, commit?)`) returns
`{project, at:{id,kind,branch,root}, files, directories, symlinks, others,
logical, complete, reason?}`. Branch defaults to `main`, Commit to its saved head.
An explicit saved id is full and admitted by the Branch context, like saved history reads.
The pin is resolved once. Counts include no uncommitted Workspace changes.

Root directory is excluded; hard links count per namespace path; symlinks are
counted without following targets. Current canonical types are regular files,
directories and symlinks, so `others` is zero. Logical bytes are the whole
saved tree's recorded regular-file bytes, including every hard link.

The walk retains bounded directory windows, with the same 10,000-entry
per-request work budget as saved reads. Exhaustion returns partial entry
counts with `complete:false` and a reason; no silent cap or completeness claim.
`logical` still describes the whole saved pin. Errors reading objects remain
refusals, not partial success. Empty trees return zero counts and complete true.
Cache by immutable `at.root` plus this definition (v1), retaining completeness.
Retained-history size and per-project physical attribution are outside this call.
