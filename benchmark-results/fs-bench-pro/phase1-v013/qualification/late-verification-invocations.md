# Late verification invocations — read-only preflight, 2026-09-04

No command below was executed during this preflight. Current live GitHub issues
#31 and #34 remain OPEN and include the failure-repair amendment: required
functional, integrity, resource, deadline and cleanup failures must be repaired
before terminal pass. The canonical registry declares 130 new timed cases,
five inherited cases, one CDC proof and 28 reliability subcases.

Finish prescribed performance first. The full verification inventory is
**390 new case/seed variants + 5 inherited fixed-input verifiers + 1 aggregate
CDC proof + 28 reliability proofs = 424 slots**. Two existing ordinary proofs
are to be reused through explicit source mapping, leaving **422 new executions**
at this snapshot. Any later qualified proof reduces only its exact matching slot.

## Keep these two completed ordinary proofs

| Excluded late slot | Existing sealed attempt | Producing revision |
| --- | --- | --- |
| `payload-create-1m`, seed 1, verify | `attempts/payload-create-1m-s1-verify-09ef8212a24f` | `fbf32e84662d00993c033515e113437965395494` |
| `tiny-bulk-delete-500`, seed 1, verify | `attempts/tiny-bulk-delete-500-s1-verify-4ed93a7acfd4` | `fbf32e84662d00993c033515e113437965395494` |

Their concrete early-verification reasons are retained in
`early-verification-reasons.jsonl`. Both original outcomes report product pass;
final report validation and compatibility remain mandatory. Preserve the
`evidence-builds.json` selectors `slot:payload-create-1m:1:verify` and
`slot:tiny-bulk-delete-500:1:verify`, their build-manifest bindings, and the
required performance/verification source bridges when choosing the final build.
Do not rewrite their original source identity or relabel them as newly executed.

The runner's slot key includes harness, product, image and environment identity.
A new verifier build will therefore **not automatically skip these old-source
proofs**. Running `--all` for payload/tiny with the new assets would recollect
both. The explicit exclusions below prevent that. Do not use
`--invalidate-reason` for these valid reused proofs.

## Complete remaining selection

The coordinator must set `LAYERFS_V013_ASSETS` to the qualified sealed build that
contains the final verifier/report fixes. Do not use an unsealed binary, nor an
older build missing genesis/canonical-union evidence. `source_validation` also
requires the live runner/custody/runtime scripts to match that sealed revision.
The shell variable below selects that real build; it is deliberately not bound
to an invented future revision.

```bash
set -euo pipefail
: "${LAYERFS_V013_ASSETS:?Select the qualified final verifier assets directory}"
phase1_repo=/Users/yifanxu/Ephemeral-AI-Lab/layerfs
phase1_campaign="$phase1_repo/benchmark-results/fs-bench-pro/phase1-v013"
phase1_verify() {
  python3 "$phase1_repo/benchmark/fs-bench-pro/workspace-runner.py" \
    --mode verify --source-arm corrected \
    --assets "$LAYERFS_V013_ASSETS" --output "$phase1_campaign" "$@"
}

# 23 payload proofs: omit only the existing create-1m/seed 1 proof.
for phase1_case in payload-create-1m payload-create-10m payload-create-100m payload-create-500m \
                   payload-random-read-1 payload-random-read-10 payload-random-read-100 payload-random-read-500; do
  for phase1_seed in 1 2 3; do
    if [[ "$phase1_case" == payload-create-1m && "$phase1_seed" == 1 ]]; then continue; fi
    phase1_verify --family payload_create_read --case "$phase1_case" --seed "$phase1_seed"
  done
done

# 59 tiny proofs: omit only the existing bulk-delete-500/seed 1 proof.
for phase1_profile in tiny-create tiny-stat tiny-unlink tiny-bulk-create tiny-bulk-delete; do
  for phase1_tier in 1 10 100 500; do
    for phase1_seed in 1 2 3; do
      phase1_case="$phase1_profile-$phase1_tier"
      if [[ "$phase1_case" == tiny-bulk-delete-500 && "$phase1_seed" == 1 ]]; then continue; fi
      phase1_verify --family tiny_file_churn --case "$phase1_case" --seed "$phase1_seed"
    done
  done
done

# 307 slots: 306 ordinary seeded variants plus the one aggregate CDC proof.
for phase1_family in directory_construction_traversal git_tool_workflow namespace_mutation \
                     workspace_change_locality mixed_load_bearing dedup_cross_file \
                     dedup_cdc_locality dedup_workspace_reuse; do
  phase1_verify --family "$phase1_family" --all
done
phase1_verify --family dedup_branch_history --all --extended

# Five fixed-input inherited verifiers; --all selects repetition 1 only in verify mode.
phase1_verify --family edit_length_changing_capped --all

# Exactly 28 proof subcases, each once with aggregate seed 1; six require extended admission.
phase1_verify --family workspace_reliability --all --extended
```

The commands reuse the existing selected-input preparation/cache machinery and
fresh sample isolation. They do not prepare unused families. They run serially;
the existing campaign measurement lock remains authoritative. Stop on an actual
failed invocation, investigate and repair it, then recollect only invalidated
slots. Keep every failed attempt sealed. Successful same-identity slots are
reused automatically on a resumed selection; failed slots require an explicit
invalidation reason after their cause is fixed.

## CDC boundary proof is already included above

`--family dedup_cdc_locality --all --mode verify` schedules its 20 timed cases
at seeds 1/2/3 **and** `dedup-cdc-boundaries-proof` once at aggregate seed 1.
Do not separately invoke the boundary proof after that family selection.
If scheduling it separately instead of the family-wide command, its exact call is:

```bash
phase1_verify --family dedup_cdc_locality --case dedup-cdc-boundaries-proof --seed 1
```

The aggregate selector internally includes all three seed cohorts: lengths
0,1,8191,8192,16384,32768,32769; an exact pair per length and a midpoint
one-byte mutation for every nonempty length. This is **60 regular files** in
one proof, not three proof executions. The host requires verify mode and seed 1,
performs public import, canonical extent/oracle checks, and a fresh FUSE reopen.
Preparation and proof deadline are each 600 seconds. Performance-mode selection
filters proof-only members; seeds2/3 or inherited repetition selectors are
rejected for the standalone proof.

## Exact reliability inventory and deadlines

All IDs below are executed once by the final `workspace_reliability --all
--extended` command. Individually, use
`phase1_verify --family workspace_reliability --case ID --seed 1`, adding
`--extended` for the six marked rows. Never multiply fault recipes by three
seeds or by 1/10/100/500. The common preparation limit is 600 seconds and cleanup
limit is 60 seconds. The registry order is:

| Proof ID | Lane | Enclosing proof deadline |
| --- | --- | --- |
| `workspace-invalid-sdk-edit-proof` | Short | 600 s |
| `workspace-invalid-namespace-proof` | Short | 600 s |
| `workspace-lease-lifecycle-proof` | Short | 600 s |
| `workspace-open-writer-busy-proof` | Short | 600 s |
| `workspace-live-execution-busy-proof` | Short | 600 s |
| `workspace-candidate-failure-retry-proof` | Short | 600 s |
| `workspace-admission-batch-failure-retry-proof` | Extended | 3600 s |
| `workspace-final-publication-failure-retry-proof` | Extended | 3600 s |
| `workspace-published-presentation-failure-proof` | Short | 600 s |
| `workspace-dirty-end-discard-proof` | Short | 600 s |
| `workspace-dirty-net-zero-proof` | Short | 600 s |
| `workspace-short-spool-write-proof` | Short | 600 s |
| `workspace-deferred-nospace-proof` | Short | 600 s |
| `workspace-workload-cancel-proof` | Short | 600 s |
| `workspace-dirty-runtime-disconnect-proof` | Short | 600 s |
| `workspace-corrupt-descendant-proof` | Extended | 3600 s |
| `workspace-missing-descendant-proof` | Extended | 3600 s |
| `workspace-parallel-read-write-proof` | Short | 600 s |
| `workspace-shared-path-contention-proof` | Short | 600 s |
| `workspace-hardlink-alias-proof` | Short | 600 s |
| `workspace-symlink-semantics-proof` | Short | 600 s |
| `workspace-open-rename-unlink-proof` | Short | 600 s |
| `workspace-metadata-chmod-proof` | Short | 600 s |
| `workspace-metadata-mtime-proof` | Short | 600 s |
| `workspace-metadata-xattr-proof` | Short | 600 s |
| `workspace-exec-500-proof` | Extended | 3600 s |
| `workspace-repeat-publication-proof` | Short | 600 s |
| `workspace-sustained-600s-proof` | Extended | 1500 s outer;900 s active +600 s final |

There are **22 short and six extended** rows. `--all` without `--extended`
fails before preparation because the selected family contains required extended
members. `--seed 2`, `--seed 3`, and `--repetition` are rejected for proofs.

`workspace-exec-500-proof` runs exactly 500 sequential workload public Exec calls
in the same Workspace, checks each completed receipt/output and zero active
executions, then publishes and performs full-tree/reopen verification. Its
workload output must be the exact ordinal followed by newline for 0..499.
Additional verifier Exec calls are oracle operations, not extra members of the
500-call workload cohort. Per-call monitor draining prevents512-entry ring rollover
from erasing earlier receipts.

`workspace-sustained-600s-proof` uses actual filesystem activity by two workers:
read, write/sync/close, rename, peer read, scratch create/remove and a bounded
cycle-result update. It stops only after at least 600 monotonic seconds and a
completed cycle, not after an idle sleep or 500 quick iterations. Each completed
cycle must satisfy the 30-second progress gate; final output must contain a
positive `completed_cycles` and `active_elapsed_ns >= 600000000000`. The host
arms 900-second active and 600-second final-verification guards; the runner also
enforces a 1500-second enclosing proof deadline. Final verification includes
Commit, exact tree, cleanup, orderly reconnect and final resource observations.
Do not increase these limits after a failure.

The reliable completion condition is `proof-complete` after all required
observations and cleanup, not merely process exit 0. Unconsumed fault arms,
missing observations, leftover execution/workspace state, incorrect expected
errors, incomplete activity, or retained runtime files must remain failures.
The existing 64MiB retained-output cap and memory/Store/spool gates apply to all
proofs. No runtime sources, builds, tests, preparation or benchmarks were changed
or executed in this preflight.

## Specific preflight risk for the real endurance run

The current `sustained` helper rewrites `work/cycle-result` with O_TRUNC and a
64-byte write every completed cycle. `Workspace::truncate` and `write_inner`
both advance the per-file edit counter; `next_edit` rejects above 4096. The
first cycle contributes one edit and each later cycle contributes two, so the
write during cycle 2049 reaches the limit if that cycle occurs before the
600-second duration is met. No measured cycle rate or actual failure is claimed
here. This source-derived boundary is a concrete investigation target, not a
reason to reduce duration, slow the workload with artificial sleeps, raise the
limit, or report an incomplete run as passing. No runtime change was made.
