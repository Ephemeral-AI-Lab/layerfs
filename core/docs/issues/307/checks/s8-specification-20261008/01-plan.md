# S8 specification task: plan and record

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner-dispatched assignment 2026-10-08:
[HANDOFF-S8-SPECIFICATION-20261008.md](../../HANDOFF-S8-SPECIFICATION-20261008.md),
executed as written. Parent `32d969776`; product source pin `f0797c646`.
Tracked tree clean at start; the three protected untracked notes matched their
recorded hashes and were not touched.

- Read-only investigation of active core source, the #303 contracts, current
  #307 reports, the retained issue snapshots and the experiment branch at
  `1451b68a720bbe2175a103dd9b35693ad05e2be1` through `git show` only.
- Three authorized read-only review subagents: kernel/FUSE and native
  ownership; cache, concurrency and lifecycle; historical evidence and proof.
  Their reports are retained verbatim as `02-`, `03-` and `04-` here. Every
  finding has a disposition in [05-finding-ledger.md](05-finding-ledger.md).
- Five documents written by the task owner:
  [specification](../../S8-SPECIFICATION-20261008.md),
  [implementation plan](../../S8-IMPLEMENTATION-PLAN-20261008.md),
  [mechanism and evidence ledger](../../S8-MECHANISM-EVIDENCE-20261008.md),
  [proof plan](../../S8-PROOF-PLAN-20261008.md) and
  [implementation handoff](../../HANDOFF-S8-IMPLEMENTATION-20261008.md).
- Documentation checks are in `06-document-checks.json`; the exact
  first-parent and staged production LOC comparison is in
  `07-production-loc.json`.
- Not done, by scope: no S8 product implementation, no build, test, native
  probe, mount or measurement, no checkout of another branch, no new worktree,
  no remote issue edit, no push. The implementation handoff is prepared and not
  dispatched. Rust and runtime checks are inapplicable to a documentation-only
  change.
