# v0.1.2 release refresh

> **Status:** Release-finalization plan authorized on 2026-09-04; not measured evidence.

Issue [#12](https://github.com/Ephemeral-AI-Lab/layerfs/issues/12) owns this
refresh and publication. Issue #20's immutable SDK edit campaign remains
recorded at candidate `3337728e`; it is not relabeled as a later-source run.

## Scope and order

1. Finish the independently owned daemon mount-close acknowledgement fix and
   its focused regression proof before freezing the release candidate.
2. Refresh `init_namespace`: all four registered file-count tiers, seeds
   1/2/3, followed later by one independent verifier per tier.
3. Refresh `store_footprint`: all three registered 500 MB controls, three fresh
   Stores per control, followed later by one verifier per control.
4. User clarification after the retained compatibility diagnostic: the old
   32 MiB payload campaign is obsolete for the current release decision. Do not
   rerun, optimize, or apply its temp-copy/rename timing gates to SDK edits.
   Preserve the diagnostic and its failed historical gates without relabeling
   it as a pass. Active release tables cover the five current families.
5. Collect performance before final verification. Add only thin full-family
   `collect` / `verify-all` entrypoints where existing runners otherwise force
   verification immediately; preserve original commands and raw schemas.
6. Run final native checks and exact-source CI, document evidence/source
   boundaries and limitations, build source-only archives/checksums, publish
   the annotated tag and GitHub release, then close #12. Leave #18 open.

## Evidence and acceptance

Fresh results are release-candidate observations, not a newly optimized
baseline/candidate comparison. Keep all original fixtures, operation paths,
byte units, and memory scopes. Initialization fixtures may be reused; never
reuse initialized output Stores to skip initialization. Each sample uses its
own writable Store and Workspace. No performance result is reused as a fresh
measurement, and full content verification stays outside performance timers.

Report sample count, elapsed-time median and range, throughput with its byte
basis, and distinct process/cgroup/storage metrics. Do not combine first-sample
and subsequent-sample cache cohorts. Preserve all attempts and raw output.
Any post-#20 product change is described explicitly; original edit evidence
does not automatically certify that changed source.

Namespace retains its frozen workload targets. Store footprint retains the
accepted compatible-layout limitation (historical primary durable median
662,831,104 bytes versus the original 600 MB goal); this refresh does not
authorize storage redesign, physical packs, or a new optimization campaign.
Show refreshed values and any regression honestly. Existing user-approved
edit tolerances, three named exceptions, and coarse memory-observation policy
remain as documented in the SDK edit specification.

#18 is far-future, unscheduled alternative-storage exploration, not a planned
minor-release optimization. Publication is authorized by the user only after
the required release work and checks are complete.
