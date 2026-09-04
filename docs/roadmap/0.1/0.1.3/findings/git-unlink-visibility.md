# Git exposes stale lookup after an acknowledged unlink

Status: **original product correctness FAIL; underlying defect reproduced and
repaired; corrected selected Git execution PASS**. Full Git-family coverage and
independent verification remain pending. The [completion amendment](../failure-repair-amendment.md)
requires this repair in Phase 1; optional performance/storage optimization stays
in Phase 2. The original failure is retained unchanged.

## Observed failure and source

The frozen [Git-tool workload](../git-tool-workflow.md) mixes modifications,
additions and deletions, then runs six ordinary Git commands through public
Workspace Exec over FUSE. It does not replace Git operations with SDK edits.

The original
[`git-tool-10` seed-1 performance attempt `5abd0cdea1ba`](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-5abd0cdea1ba/outcome.json)
used source `b8c2ad4bf4fa0415fd49d57abea15729b33a4284`, product seal
`e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`, and image
`sha256:d7cfd5b1b29a61e724d05f2e80f368b8aa5ba08133b0c516bd5c40b6cfdd8d3b`.
All ten target mutations were acknowledged, including three unlinks. The first
`git status` succeeded; the second command, `git diff --no-ext-diff --binary --`,
failed before Commit:

```text
fatal: stat 'tracked/delete-318.dat': No such file or directory
```

[Decoded workload stderr](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-visibility/decoded-workload-stderr.txt)
concatenates the exact recorded `OutputChunk.bytes` arrays in sequence. The
[original stderr](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-5abd0cdea1ba/stderr.txt)
remains intact, with SHA-256
`fd8c01bfcc93d155f21e93d7a9c8006ba57d5b8a57a205e238eaee3ff6609ecd`.
The original product exit was 1, with no timeout or container OOM; supervisor
cleanup passed and the mutable sample was retained for investigation.

This is one observed failing Git sample. The
[seed-2 attempt `e39620441956`](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s2-performance-e39620441956/outcome.json)
was interrupted during preparation and records `unexecuted` / `not-run`.
It is not a second reproduced product failure.

## Root cause and narrow repair

In the original
[`ProxyClient::unlink`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/b8c2ad4bf4fa0415fd49d57abea15729b33a4284/crates/layerfs-fuse/src/proxy_client.rs#L587),
a regular-file unlink was queued in `pending_unlinks` and acknowledged before
host execution. If the parent had a complete directory cache, removing its
cached entry made later lookups correctly return NotFound. With no cached
parent, however, `cached_lookup` had no authoritative negative entry and a new
lookup reached the host. `exchange_at` flushed pending writes, but not queued
unlinks. The host could therefore return the deleted name's old binding until
an unrelated barrier completed the deletion. The acknowledged filesystem state
was inconsistent across subsequent operations.

The socket-level regression
`proxy_client::tests::acknowledged_unlink_is_absent_with_and_without_parent_cache`
uses the public proxy-port lookup/unlink methods against a small protocol host.
Its [pre-fix result](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-visibility/reproduced/output.log)
fails deterministically: after acknowledged unlink on an uncached parent,
lookup returns the old `Attr` instead of `Err(NotFound)`. The separately retained
[initial test-compilation error](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-visibility/before/output.log)
concerns formatting a non-Debug request type; it is not product-failure evidence.

The repair in
[`0763fac6`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/0763fac6a9ff59892e985d3612d1bf710543fc86)
changes only the runtime `ProxyClient::unlink` implementation: reuse its existing
cache lock to observe whether the parent is cached; queue the regular-file
unlink as before; for an uncached parent, finish the existing barrier before
acknowledging success. Complete cached parents retain deferred batching and
immediate negative lookup. The regression's
[corrected result](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-visibility/corrected/output.log)
passes both branches and verifies that the cached-parent branch still retains
one queued unlink. Its [source receipt](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-visibility/corrected/identity.json)
binds the exact test command and source hash.

No lookup/read method, cache representation, memory layout, protocol, resource
limit, fixture, workload operation, or oracle was changed. No benchmark-specific
bypass was added.

## Corrected live recovery

The corrected
[`git-tool-10` seed-1 attempt `cd922cae2006`](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-cd922cae2006/outcome.json)
used sealed source `3422433020a678a77f88e8a110492ca293c05e30`, containing the unlink
repair, with product seal
`4637a27f57351decbee4f800ba97f63d743fb03c7c5b91bad56550eadb310170` and image
`sha256:9203d33a1217f45905e74c315915be77d34d471ec3df6110a961f2d6cd4ef4c1`.
It completed all ten target operations, all six Git commands, and one Created
Commit. Product exit was 0. The
[selected live validation](../../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/git-unlink-live-validation.json)
returned `issues=[]` and `violations=[]`.

| Required observation | Corrected selected result |
| --- | --- |
| Cgroup sample count | 2082 |
| Required dispatch observation window | 23212250834 ns |
| Last cgroup observation | 23228394510 ns |
| Maximum observed sampling gap | 13957291 ns |
| Observed `memory.peak` | 136560640 bytes |
| Observed maximum PIDs | 31 |
| Swap / OOM / OOM-kill | All zero |
| Physical spool mutation-boundary peak | 1421312 bytes |
| Physical spool observation errors | Zero; 120 observations |
| Final owned spool files / logical bytes / allocated bytes | All zero |
| Supervisor and mutable-sample cleanup | Both PASS |

These are causally sampled cgroup observations and mutation-boundary spool
measurements, not exact continuous per-phase maxima. The
[raw receipts](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/git-tool-10-s1-performance-cd922cae2006/raw.jsonl)
retain zero benchmark verifier/reopen/injection counts for performance and zero
owned spool state after End and Client cleanup. This selected recovery confirms
the reported real Git failure is repaired. It does not replace independent
Git semantic/custody/full-tree verification or the remaining prescribed samples.
No performance-gain claim is made by comparing this complete execution with the
shorter, failed original command.

## Invalidation and retained coverage

The repair has an explicit operation dependency. Previously collected evidence
that invoked the changed unlink method must not silently acquire a new source
identity:

- Recollect all 24 tiny-delete performance slots: `tiny-unlink-{1,10,100,500}`
  and `tiny-bulk-delete-{1,10,100,500}`, each with three seeds. Bulk deletion
  enumerates parent directories first, but its full cache lifetime is not used
  as an unproved retention exemption.
- Recollect the three previously passing `git-tool-1` seed slots. Each recorded
  five kernel unlink callbacks from the complete Git workflow.
- Recollect the prior `tiny-bulk-delete-500` seed-1 independent proof.
- Retain the other 96 completed performance slots only through the explicit
  source-compatibility decision: actual unlink callbacks must be zero; the
  workload, fixtures, independent expected definitions, contracts and product
  source outside the reviewed unlink implementation and test addition must
  match. A filename-level or whole-product-seal waiver is insufficient.

The [build-selection ledger](../../../../../benchmark-results/fs-bench-pro/phase1-v013/evidence-builds.json)
and [append-only invalidation ledger](../../../../../benchmark-results/fs-bench-pro/phase1-v013/invalidations.jsonl)
record the coordinator's actual selections. Original producing identities,
failures, invalid observations and interrupted attempts remain available.
Recollection and the remaining full Git proof/family coverage are pending
obligations; this finding alone cannot establish `PHASE1_TERMINAL_PASS`.

## Historical classification without manufacturing another failure

The accurate historical classification is: **observed product correctness
failure, root cause reproduced by a focused protocol regression, corrected
selected live execution passed**. The unit regression is not another
same-image failed Git benchmark, and the interrupted preparation is not a
reproduction.

The report's `validate_classification` rule for a selected failed outcome
requires a separate sealed failed sample. It must remain strict. The old Git
attempt should instead remain an unselected historical FAIL with its explicit
invalidation/supersession context and this causal evidence. The existing
historical-attempt scan retains such failures without admitting them as current
passing evidence. Select the corrected outcome on its own merits; its missing
independent verification and remaining family coverage still block admission.
There is no need to rerun known-broken work merely to manufacture a second
failure, invent `reproduction_evidence`, relax the selected-failure gate, or
relabel the old result as passing.
