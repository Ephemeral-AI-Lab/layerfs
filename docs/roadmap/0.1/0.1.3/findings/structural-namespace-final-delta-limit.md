# Structural namespace Commit exceeds the baseline final-delta limit

Status: **reproduced product resource/capacity finding; product FAIL**. Phase 1
performance collection has reached the required ordinary workload and observed
a failed Commit. This is not a harness failure, passing publication, or a
release admission. Independent verification remains a separate required stage.

## Frozen scope and source

The [tiny-file family](../tiny-file-churn.md) fixes a **500-shard, 100,000-file,
524,288,000-byte background** for create/stat/unlink curves. Target parents are
prepared; the selected operation changes only 1/10/100/500 targets. At tier 1,
the canonical size cycle creates an empty file. Shrinking this background,
raising the product resource policy, changing the workload, or silently
substituting SDK edits would change the question rather than explain this result.

The observed source is
[`4c207c70f3282c316d5ab18d832504085835eda3`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/4c207c70f3282c316d5ab18d832504085835eda3),
contract `837da2f6b6167b225958bb421572e23a38b94e50`, instrumented product seal
`810655a13d8621b2e04efeda5747e54929e4d4717e8d5d82dcddcf75f905b727`, and runtime
image `sha256:781f4513dcba84f51bb5b7fda4704e7e5dfe52c8aabf777b310778afba41935f`.
The default limit and structural fallback were already present at the untouched
[baseline `1e81e9b8`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/1e81e9b8cf871324341c221a51b0a0239c580da9/crates/layerfs-workspace/src/limits.rs).
Passive observations did not introduce or lower this gate.

## Mechanism

1. [`ResourcePolicy::default`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/4c207c70f3282c316d5ab18d832504085835eda3/crates/layerfs-workspace/src/limits.rs#L9)
   sets `max_final_delta_memory_bytes = 8 * 1024 * 1024 = 8,388,608`.
   Public Workspace creation uses that default; the harness does not override it.
2. [`build_localized_candidate`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/4c207c70f3282c316d5ab18d832504085835eda3/crates/layerfs-workspace/src/changes.rs#L353)
   returns `None` whenever a loaded directory has binding changes. Creating or
   unlinking one target necessarily produces such a change.
3. [`build_candidate`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/4c207c70f3282c316d5ab18d832504085835eda3/crates/layerfs-workspace/src/changes.rs#L35)
   then builds the complete base manifest before the final manifest.
   [`base_manifest`](https://github.com/Ephemeral-AI-Lab/layerfs/blob/4c207c70f3282c316d5ab18d832504085835eda3/crates/layerfs-workspace/src/changes.rs#L631)
   charges every base path as `512 + 4 * UTF8_path_length`, checking the budget
   after each insertion. This charge describes the planner's accounting model,
   not observed process RSS or the byte length of the user edit.
4. The 100,000 regular files alone require at least 51,200,000 charge bytes,
   even before their names and directory entries. The complete base therefore
   cannot fit the unchanged 8 MiB budget. No payload read/CDC work or final-view
   generation is required to establish this incompatibility.

For this frozen layout, root listing returns five directories and the stack
visits `wide/` first. Each wide file has an 18-byte relative pathname and costs
584 charge bytes. Root entries cost 2,656 bytes. The 14,360th wide file crosses
the gate: `2656 + 14360 * 584 = 8,388,896 > 8,388,608`. Listing counts whole
128-entry pages before processing individual entries, explaining the observed
`namespace_base_paths_visited = 5 + 113 * 128 = 14,469`. This is accounting
arithmetic derived from frozen source, not an additional benchmark run.

## Original evidence and reproduction

The first retained attempt is
[`tiny-create-1`, seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-3a3f77d6ca0c/raw.jsonl).
Its [acquisition](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-3a3f77d6ca0c/preparation/acquisition.json)
confirms 100,000 initial regular files and 524,288,000 logical bytes. The actual
workload and public Exec completed successfully: one target/file creation,
zero payload bytes written, one root-directory sync. Inner workload wall was
5,173,000 ns; the failed public Commit service wall was 5,019,114,833 ns.
The latter is **failed-operation time**, not successful Commit latency.

The original [stderr](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-3a3f77d6ca0c/stderr.txt)
contains `Workspace(Storage(InvalidInput("workspace final-delta limit")))`.
The failed Commit diagnostic reports 14,469 base paths visited, zero final
paths visited, two localized candidate probes, zero CDC bytes, and observed
physical spool current/peak `Some(0)` with no observation errors. Store logical
and allocated size remained 672,595,968 bytes across the failure. This does not
by itself prove unchanged Branch/head/root; the later independent verifier must
still establish the appropriate failed-publication state.

The following distinct prescribed seed attempts reproduce the same signature
on the same product/image/source. These were normal required collection slots,
not favorable rerolls. Each exact-signature classification links another seed
of the same case and retains the original failed outcome.

| Case | Independent retained attempts | Product result |
| --- | --- | --- |
| `tiny-bulk-delete-100` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s1-performance-ba27c14fa2bf/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-100-s2-performance-4505aea3ab4f/outcome.json) | FAIL: final-delta limit |
| `tiny-create-1` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s1-performance-3a3f77d6ca0c/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s2-performance-3ad81dad4042/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-1-s3-performance-3d156aa6829b/outcome.json) | FAIL: final-delta limit |
| `tiny-create-10` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s1-performance-40288364827f/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s2-performance-a4c4d458bfc5/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-10-s3-performance-962fec47bc17/outcome.json) | FAIL: final-delta limit |
| `tiny-create-100` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s1-performance-1d79a2cb81c7/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s2-performance-d9ccdfae1ce7/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-100-s3-performance-c5bea0e4f02e/outcome.json) | FAIL: final-delta limit |
| `tiny-create-500` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s1-performance-e1245cd1d674/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s2-performance-f9efc1ed8c63/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-create-500-s3-performance-31bf67cc4035/outcome.json) | FAIL: final-delta limit |
| `tiny-unlink-1` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s1-performance-d4842bcaeaf8/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s2-performance-9c091dae1c72/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-1-s3-performance-76f85e2a5b82/outcome.json) | FAIL: final-delta limit |
| `tiny-unlink-10` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s1-performance-46d111390331/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s2-performance-149e961df39d/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-10-s3-performance-bd48132d46c0/outcome.json) | FAIL: final-delta limit |
| `tiny-unlink-100` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s1-performance-08dea611a747/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s2-performance-d19dbe985993/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-100-s3-performance-e0db15041bc4/outcome.json) | FAIL: final-delta limit |
| `tiny-unlink-500` | [seed 1](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s1-performance-2003a900c0b8/outcome.json); [seed 2](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s2-performance-62587635ae1a/outcome.json); [seed 3](../../../../../benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-unlink-500-s3-performance-7e072bd04e8b/outcome.json) | FAIL: final-delta limit |

Classifications are in the campaign's
[classifications.json](../../../../../benchmark-results/fs-bench-pro/phase1-v013/classifications.json).
The completed first tiny performance campaign contains **27 failed Commit outcomes** with this exact signature.
The table also includes newly completed bulk-delete rows with this exact
signature. Those use their own N-shard target plus one witness shard, not the
small-operation 100,000-file background; they hit the same whole-base planning
gate. Their acquisition receipts preserve the actual distinct fixture sizes.
Other failures, including an ordinary root `fsyncdir` error, are not assigned
this classification. Classification is not an exemption from reached-phase
route, purity, observation, source, resource or cleanup validation.

## Recovery evidence and limits

The first attempt retains `Ok(WorkspaceEndResult { ..., discarded: true })`,
an explicit successful `workspace.end` operation receipt, and an after-Discard
owned spool/capture/rebase census of zero files, zero logical bytes and zero
allocated bytes. The owned container was stopped after Client destruction,
was not OOM-killed, and explicit supervisor removal passed. The independent
mutable Store remains under `scratch/` intentionally for investigation.

These observations support successful **failure recovery and owned spool
cleanup**. They do not turn the failed Commit into success. The failure branch
returns before the success-path active Workspace/execution zero checks and
`after-client-drop-cleanup` census; therefore do not claim that this row has
those absent receipts. Final verification and the integrated failure proofs
must supply publication atomicity, exact prior contents and any additional
lease/mount cleanup evidence required by the terminal gate.

## Phase 2 dependency

Group this with structural-change planning that expands untouched namespace
state: create/unlink, directory construction, populated subtree relocation,
and fixed moves are candidate affected routes. Dense or owner SDK writes have
a different eligibility path and must be interpreted from their own evidence;
read-only `UpToDate` controls do not exercise structural publication.

Phase 2 should investigate a bounded changed-frontier namespace planner or an
explicit supported-capacity contract, with the unchanged scenarios and retained
baseline failures as its comparison. Merely increasing the policy would not
establish locality or remove whole-tree work. No product optimization, limit
change, fixture reduction, new benchmark family, or release claim is performed
by this finding. Continue all remaining Phase 1 performance and separate proof
slots with product failures remaining FAIL.
