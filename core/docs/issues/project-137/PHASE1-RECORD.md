# Connect CLI phase 1 record

> **Status:** Archived; retained for historical evidence only.
> Verification date: 2026-11-09. This is not a release qualification.

Phase 1 implements and verifies LayerFS work for layerfs-project #138–#144.
Delivery is [LayerFS PR #631](https://github.com/Ephemeral-AI-Lab/layerfs/pull/631)
into `layerfs_v2_0_0`, from base
`ce0e00bef2c958686038544afbf7da4e23dce8af`. The PR records the merge and
issue-closure receipts. Parent [#137](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/137)
remains open for Connect phase 2 (#145–#153); this change does not implement it.

## Implemented outcome

| Issue | Result | Implementation commit |
| --- | --- | --- |
| #139 U1 | `workspace exec --events`: ordered progress/output NDJSON, one final result/refusal, distinct 0/125/2 exits | `5a6f74b5ce` |
| #140 U2 | A cancel before registration leaves a 30-second Workspace/id mark, consumed once at start | `ee7d5f48f5`; signal documentation `6ff76226a2` |
| #141 U3 | Build-time commit/dirty/features JSON and complete table/CLI capability discovery | `dd4458a615`, `4fab346581` |
| #142 U4 | Store-wide existing counters, native/worker/PostgreSQL routes, no content/history scan | `d6fbbd723b` |
| #143 U8 | Counts of one Branch-admitted immutable tree, with explicit partial-count budget exhaustion | `d38a8a2634` |
| #144 U5/U6 | Layer endpoints through the existing scalable Commit comparison; authenticated stateless revision/comparison continuation | `81dbe05b64` |
| #138 | Final release contract suite, native/Docker/PostgreSQL/MinIO proofs and retained negative probes | `7f0ef57409` and the receipt commit |

The API table has 66 calls. No JavaScript or Python names were added by hand.
No dependencies were added and no tests were deleted. The unrelated machine
feature test's missing `send` helper was replaced with its existing typed frame
writer so the selected API targets compile.

## Verification

See [the phase receipt](receipts/PHASE1-20261109.md) for every #138 checklist
item, binary identity, commands, issue evidence, limitations and retained failures.
The final PR-head handoff build is pinned outside the build cache; its resolved
identity is posted on PR #631. Documentation-only commits after the verified
product/test pin do not change implementation or test bytes.

Observed constraints remain explicit: default structured exec still omits
spooled output above 1 MiB; `--events` supplies it. Delivery follows completion,
with no live interleaving promise. At-spawn cancellation reports 128 + the actual
signal (native proof 137, Docker proof 143). PostgreSQL project removal is
Unsupported. Entry counts may be partial; logical bytes still describe the
whole pin. Continuation does not relax per-request Capacity limits.

The distinct live-CLI failure injection at every move phase was NOT_RUN.
The existing control harness verifies later install/record/retire custody and
recovery, while live CLI termination was observed at the first check event.
Durable execution was NOT_RUN — disabled by owner until explicit reauthorization.

## Production source comparison

Counter: `tools/production_loc.py` at base `ce0e00bef2`, blob
`35e4a6d601fa88002f06b4589f1d53c4f9cabeac`. Count nonblank/non-comment product
code on exact parent/staged trees; include the shipped CLI build identity
(`build.rs`, 58 final production lines), which the standard path scope omits.
The final independent standard scan is 155,866; including that build input is
155,924. Final subtotals: core 39,749; engine 73,079; control 38,072;
API 5,024 including the build input. Reference absent (retired R9c; last counted
65,417 at tree `498dd1917812`). These are source sizes, not performance claims.

| Commit | Production LOC before → after | Signed delta |
| --- | --- | --- |
| `dd4458a615` | 155214 → 155392 | +178 |
| `5a6f74b5ce` | 155392 → 155450 | +58 |
| `ee7d5f48f5` | 155450 → 155480 | +30 |
| `d6fbbd723b` | 155480 → 155555 | +75 |
| `d38a8a2634` | 155555 → 155682 | +127 |
| `81dbe05b64` | 155682 → 155908 | +226 |
| `4fab346581` | 155908 → 155924 | +16 |
| `7f0ef57409` | 155924 → 155924 | +0 |
| `6ff76226a2` | 155924 → 155924 | +0 |
| Final receipt/docs commit | 155924 → 155924 | +0 |

All resource ownership is private to the registered `phase1-137` worktree.
Binaries, bundle, logs and both failed and passing receipts remain pinned in
its registration state. The handoff exports only its completed build caches
for cleanup; no other worktree is switched, migrated or released.
