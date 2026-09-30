# C1 cycle validation checkpoint — 2026-09-30

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

Baseline: published `7edddbdb8e8512627aed0ed42533ef099d802384`.
Owned worktree `/Users/yifanxu/.codex/worktrees/phase5-deferred-opt/layerfs`,
branch `codex/phase5-deferred-opt`; primary checkout remains untouched.
This checkpoint implements only the first optimization subset of #276.

## Current status audit

The original #276 body describes historical branch-only sources. Phase B is
merged (PR #285; see the [ticket audit](../286/TICKET-AUDIT-20260930.md)).
The [seven-family checkpoint](../286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
and [reuse map](../286/experiments/20260930-seven-family-reuse.json) cover selected
SDK leases, G1/G2 bytes, failure custody, quota/refund and functional checks.
The old missing-SDK-lease and unmerged-integration statements are stale at this
baseline. Family 4's current C1 checks supersede the old ordering test-red status;
the historical red is retained and is not relabelled as pre-existing.
Finite proofs do not establish arbitrary scale or bounded frontier RAM.

Still current: unqualified numeric cache/phase-memory comparison (#283), C1
deep270 cost, hot normalization necessity, separate dirty-discard/close policy,
and the owner-deferred 10240 selection. Core owns whole-frontier/C5 streaming,
#248/#256 and later #249/#219. No retired #277–#282 replacement is created.

## Cause and prospective hypothesis

[Family 5 r066](../286/experiments/20260930-workspace-namespace-sdk-r066.md)
records deep270 complete command 2,805,615,834 ns, Exec 1,157,090,208 ns,
Commit 767,457,958 ns. The retained
[#276 cause comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828)
records validation 483,269,583 ns, covering all C1 validation, and 271 inode
reservation calls. The cycle share was not separately timed. The 36,585 suffix
visits were source-derived, not an observed counter. Reservation batching is a
different mechanism and remains unchanged.

Freeze one hypothesis: final effective bindings are immutable during validation;
an operation-scoped iterative DFS with active/completed directory states can
prove overlapping subtrees once. An edge to an active directory rejects a cycle;
a completed subtree may be reused only after all of its effective edges passed.
Every introduced cycle includes a changed binding; seed every rebound directory.
Keep parent-alias, allocation, root, build-reachability and count validation.
No owner index, canonical grammar/profile, schema or public interface change.

## Diagnostic selection and gates

External `filesystem_cycle_work::count_diagnostic`: ordered depth16 then depth270,
fresh chain then inherited/restated chain, one invocation per arm. Both have a
regular-file terminal binding; fixed serials, names, values and default resources.
The external RowSource counts `directory_for`/`value_for` calls; existing public
ValidationWork reports inode demands/pages, directory pages and site totals.
This isolates public C1 validation, not a Workspace or storage benchmark.
In-memory authenticated fixture setup is outside counting; cache is explicit
resident/in-memory, **numeric latency INELIGIBLE**, admission false. No elapsed
speedup is claimed. Counts must be reproducible across the same harness in both
arms, and all four inputs must succeed. Candidate total directory-row lookups:
fresh <=depth; inherited <=4*depth+2, demonstrating removal of repeated suffix
work without claiming path-local inherited move validation. Alias still scans
the base. Same `ordering_bytes/1024` cumulative work refusal remains.

Build locked release with worktree-local target/root ARMv8 flags, one construction
worker. Retain fresh baseline/candidate output, command wall, hashes and observed
cross-worktree interference. Diagnostic command bound15s, build preference30s;
first-use builds reported separately. No cache priming, retries, new workers,
timeout/budget lift, fixture route or other-owner build target.

## Final correctness and ownership

After implementation freeze: external deep count bound tests; independent
final-parent graph oracle with mixed stored/new directories; exact root equality
against independently assembled final rows, old-root readability, cycle/alias
rejection and no publication on refusal; covering existing topology/hardlink/
failure/bounds tests and required final Core checks once. Existing selected
Workspace pin/custody/refund proofs are reused with their scopes and identities;
no changed Workspace/allocator/Server/C2 files and no new claim for their paths.
Independent public SDK deep270 proof is selected only with existing runner,
fresh output and separate <=9s verifier; cache eligibility stays INELIGIBLE.
No unchanged performance resampling or routine Family2 campaign.

Update filesystem/limits/counter architecture with the algorithm and unchanged
allowance. Every commit uses exact parent/staged/committed archives and the same
production counter, starting reference65417/Core70279/combined135696.
Publish a source-bound report, draft PR and progress in #276; no merge/closure.
