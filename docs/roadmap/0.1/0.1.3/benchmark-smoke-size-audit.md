# Remaining low-tier smoke-size risks after compaction

Date: 2026-09-05. Read-only review of the active #45 task and its uncommitted
compact-fixture work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`.
HEAD remains `810bb3a58`; the findings below concern working-tree files, not that
commit alone. No build, fixture generation, benchmark or Docker sample was run.
The active infrastructure task was not modified or messaged by this review.

## What the current compaction fixes

The latest user-authorized scope caps low-tier initial/final state at 50 MiB and
1,000 regular paths, gives changed fixtures new IDs, and leaves inherently large
controls outside automatic smoke selection. Code has changed ordinary tier 1/10
to compact-v2, Workspace reuse to 1/10 MiB bases, namespace to 5/20 decimal MB,
and added compact Store-footprint controls. The task reports a compact directory
construction verification PASS at 8.71 seconds; this review did not rerun it.

No further 500 MiB / 100,000-file background was found in the inspected compact
ordinary tier-1/tier-10 fixture dispatch. Remaining concerns are mostly depth,
trajectory work, proof setup, and how smoke eligibility is determined.

## Remaining cases that need attention

| Case/group | Current source facts | Recommendation |
| --- | --- | --- |
| All `workspace_reliability` selections | Every proof is labeled tier 1. Shared fixture has 1,000 32 KiB sentinels, a balance file and a hard-link alias: 1,002 regular paths, with approximately 32 MiB logical fixture size. | Use a genuinely small versioned fixture for selected simple smoke proofs; retain minimum state needed to trigger each fault. These are already excluded from automatic performance smoke, but explicit verification setup remains larger than necessary for many checks. |
| `workspace-exec-500-proof` | Tier 1 label, but executes 500 processes before final Commit. | Not an initial smoke selection. Use an existing single-execution lifecycle proof or an explicitly new short proof; do not pass the old 500-Exec contract using fewer iterations. |
| `workspace-sustained-600s-proof` | Tier 1 label, intrinsically requires 600 seconds of active work. | Keep outside bounded smoke verification. It is already marked unsupported by the selected <59-second verifier; preserve that disposition. |
| `dedup-history-unrelated-10` | Initial 1 MiB / 200 files, but ten whole-tree rewrites and ten Commits: 2,000 file rewrites and 2,000 file fsyncs, plus directory synchronization. | Tier 1 is the first smoke. Tier 10 is a second-stage repeated-lifecycle check, not a cheap setup-equivalent test. Other history kinds also retain ten Commits but have smaller per-cycle edits. |
| Compact ordinary shard-based cases | Reduced file population still includes ordinal 199; `shard_path` retains a 128-directory spine even at tier 1/10. History's existing common shards retain the same depth. | Consider a 4–8-level spine in a separately identified smoke/compact layout. Keep 128-level traversal as explicit depth-stress coverage. Data/file caps do not bound path-resolution or directory-operation work. |
| `git-tool-1/10-compact-v2` | Background is now 1/4 MiB with about 50/200 background files, plus selected targets. Preparation also builds `.git`, imports a Store and prepares an independent reference; real workflow retains six Git commands. | Accept as a Git integration smoke after simpler FUSE setup works, but inspect total preparation/verification wall. No evidence from this review proves the compact Git case is too slow; do not shrink it further solely on suspicion. |

References in the integration checkout:

- `families/workspace_reliability/mod.rs:36` assigns tier 1; `:56` builds the
  shared fixture. `src/workspace_reliability.rs:739` performs 500 Execs.
- `families/dedup_branch_history/mod.rs:20` builds the 1 MiB/200-file background;
  `workspace_registry.rs:88` uses the tier as history step count;
  `dedup_workloads.rs:351` performs per-file writes, metadata and fsync.
- `ordinary_workloads.rs:20` retains the deep-file ordinal; `:75` builds paths
  with the unchanged 128-directory spine. `workspace_common.rs:425` does the
  same for non-compacted history shards.
- `ordinary_workloads.rs:1161` includes `.git` in the actual prepared-repository
  bound; `src/infra.rs:256` prepares the independent Git reference. The registry
  descriptor itself counts the recipe fixture before those extra artifacts.

## Smoke selector gaps

### 1. Initial fixture size is not the full smoke workload

`src/infra.rs:24` currently computes:

```text
smoke_supported = fixture_bytes <= 50,000,000
                  && fixture_files <= 1,000
                  && !proof
```

It does not constrain history depth, process count, generated/final state or path
depth. As a result, the source logic marks all of these size-eligible:

- `dedup-history-unrelated-500`: starts from 1 MiB/200 files, then 500 cycles.
- `payload-create-500m`: starts empty, then writes a 500 MiB file.
- `tiny-bulk-create-500`: starts with a small witness, then creates 100,000 files.

The current sort usually chooses the smaller tier, so this is an overly broad
eligibility definition, not an assertion that default smoke already ran these
large cases. Eligibility should independently reject them.

Use cheap selected-case metadata for initial and maximum/generated state,
declared steps/process count and intended smoke tier. Keep a small explicit
family smoke default. Do not infer every family's meaning from a numeric tier:
namespace uses file count, history Commit count, reliability a placeholder 1.

The code uses 50,000,000 bytes while the amendment and ordinary bound use
50 MiB (52,428,800 bytes). That is a stricter selector, not an oversize leak,
but use one stated unit to avoid excluding otherwise valid compact cases.

### 2. Selecting smoke still builds descriptors for the large cases

`shared/runner.py:129` runs `infra-list <family>` before selecting a smoke ID.
`src/infra.rs:91` calls the fixture generator for every selected-family case,
including unchanged tier-100/500 cases, to count entries. The host filters and
sorts those rows only afterward (`runner.py:137`).

This does not write full file contents, but it can allocate and traverse large
100,000-file descriptor trees under the listing container's 256 MiB/one-CPU
limits merely to choose a small case. No OOM or latency was reproduced here;
the unnecessary large-descriptor work is confirmed in code.

Select from cheap registry metadata, then construct/validate only the chosen
fixture. A per-family small default is simpler than a full enumeration/census
before every smoke run. Preserve the complete listing command for explicit use.

## Low tiers that now look appropriately sized

| Family | Current low-tier shape |
| --- | --- |
| Three SDK edit families | One 1/10 MiB file, with bounded small replacement operations |
| Payload creation/read | One 1/10 MiB payload or empty start followed by that payload |
| Cross-file CAS | Shared 1 MiB/one-file anchor; 10 MiB/ten-file profiles |
| CDC locality | Roughly 2/11 MiB over 2/11 files, including reference and variant size differences |
| Workspace reuse | 1/10 MiB base; 2/20 MiB final state, 2/20 files |
| Namespace | 5 MB/100 files, then 20 MB/1,000 files; use 100-file case first |
| Store footprint compact controls | 5 MB/100 files or 10 MB/10 files |

Capped-result edit cases remain approximately 500 MiB and are already correctly
large-only. The archival five-case payload entrypoint is opt-in, not a smoke path.

## Suggested priority

1. Make smoke eligibility describe the complete selected workload, and avoid
   generating large-case descriptors just to select smoke.
2. Keep history depth 1 as default; exclude high process-count and duration proofs.
3. Introduce smaller per-proof reliability fixtures where semantics permit them.
4. Consider compact depth separately from compact byte/file count.
5. Measure compact Git preparation and selected verification before deciding
   whether another fixture reduction is needed.

Keep all changed fixture identities explicit. Compact smoke results do not prove
historical large-background scaling, and a compact tier-10 to original tier-100
transition changes more than one workload dimension. Record that profile boundary
instead of pooling the rows as an unchanged proportional curve.
