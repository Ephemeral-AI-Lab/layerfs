# #152 reliability fix: the six `workspace_reliability` proof failures

Status: **complete**. Work item handed over by the [#152 final report](issue152-final-report.md)
§7.2; specification [`issue152-reliability-fix-handoff.md`](../issue152-reliability-fix-handoff.md).
Product commit `ac729dfeb`. Ledger entry
[`L30`](issue151-experiment-ledger.md).

## 1. Result

**27/27 verification-supported `workspace_reliability` proofs PASS** on one
frozen candidate, including the six that failed at L28. One sample per case,
fresh append-only receipts, nothing reused.

| identity | value |
|---|---|
| source seal | `8308cd8e628a97cd8b7d17d184a8f69ff5f212d21b0646d84913a6df5d444e9a` |
| product seal | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| compilation seal | `bff3ff080d64671f9bf9ef7e73450afd4dbaaecbce86c91a5c6ae285d5b63743` |
| dependency seal | `a1cf72ac4b77536d2d3b44c09872457a246673ca2eacf0997b11b293900709eb` |
| harness identity | `daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044` (unchanged) |
| workload source | `821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8` (unchanged) |
| image | `layerfs-bench-infra:8308cd8e628a97cd` (`sha256:…`, resolved by the runner) |
| commit / tree | `ac729dfeb`, clean (`LAYERFS_SOURCE_DIRTY=false`) |

Every receipt pins `seed 1`, `--setup clone`
(`clone_method: closed-quiescent-byte-copy`, `master_unchanged: true`),
`reused_proof_identities: []`, cleanup `PASS`. Complete commands 0.62–4.70 s
(largest `workspace-exec-500-compact-v2-proof`), end-to-end walls 1.69–6.02 s,
against a 15 s complete-command rule and a 59 s hard budget.
`workspace-sustained-600s-compact-v2-proof` stays **NOT_RUN_OPTIONAL**
(`verification_supported: false`).

## 2. Per-case outcome

| # | case | before (L28, `d1bbf882…`) | after (`8308cd8e…`) |
|---|---|---|---|
| 1 | `workspace-candidate-failure-retry-compact-v2` | FAIL `faulted Commit did not surface exact injected error` | PASS — `Integrity("injected Workspace candidate failure")`, `Candidate hit_count: 1`, retry publishes |
| 2 | `workspace-admission-batch-failure-retry-compact-v2` | FAIL `faulted Commit did not surface exact injected error` | PASS — `LaterAdmissionBatch hit_count: 1, committed_early_transactions: 1`, retry publishes |
| 3 | `workspace-deferred-nospace-compact-v2` | FAIL `faulted write did not produce its exact errno proof` | PASS — ENOSPC at the write boundary, `NoSpace hit_count: 1` |
| 4 | `workspace-short-spool-write-compact-v2` | FAIL `faulted write did not produce its exact errno proof` | PASS — EIO at the write boundary, `ShortAppend hit_count: 1` |
| 5 | `workspace-published-presentation-failure-smoke-v3` | FAIL `Workspace fault never reached exactly once` | PASS — `Created` + `presentation_failed: true`, `PresentationResume hit_count: 1`, recover completes |
| 6 | `workspace-final-publication-failure-retry-compact-v2` | FAIL `Workspace(Storage(InvalidInput("workspace stage retained")))` | PASS — injected `Integrity(…)` on the first Commit, `FinalPublication hit_count: 1`, retry publishes and the commit count advances by one |

Control A/B unchanged and still applies: all six PASS on the reconstructed
v0.1.5 control (product `276c5970aabf`, source `40bb391e1efc`, image
`layerfs-bench-infra:40bb391e1efccde7`,
`/Users/yifanxu/layerfs-v016-control/benchmark-results/issue152-control3/`). None
of the six is a stale-in-both-arms test.

## 3. What each fix is

### 3.1 #6 — product defect: a failed final publication was unrecoverable

`commit_remote` refused **any** Commit while a stage was retained
(`InvalidInput("workspace stage retained")`, ahead of the pending-completion
re-delivery path), while the materialized route re-drives — so the only recovery
from a failed final publication on the sandbox route was
`EndWorkspaceMode::Discard`, which throws the retained publication away.

The guard could not simply be deleted. The sandbox resolves **one attempt at a
time** (`CAPTURE` returns `Busy` while a slot is retained), and
`LiveWorkspace::capture_frontier` *moves* the live dirty set into the attempt, so
abandoning the attempt and capturing again would publish an **empty** candidate:
the retry would report `UpToDate`, delete the retained stage, and silently drop
the data the retry exists to publish. That is why the fix retains the attempt
instead of recapturing:

- `commit_remote` samples the retained attempt at entry and, when one exists,
  re-drives it instead of capturing;
- the capture summary is retained on the workspace (`Workspace::pending_attempt`)
  before the first step that can fail, so a failure after capture is retryable;
- a retry re-pulls and re-publishes that **exact** frozen generation — never
  newer state (the same rule the sibling `pending_completion` path already
  follows);
- the attempt is cleared exactly when it is completed or cancelled
  (`settle_attempt`); `Workspace::discard` clears it and `end_clean` treats it as
  unfinished publication state;
- `"workspace stage retained"` now reports only the genuinely unrecoverable case:
  a retained stage whose attempt was already cancelled (the Busy/HeadMoved
  result path, unchanged).

Regression test
`failed_publication_is_re_driven_by_the_supported_commit_retry`
(`crates/layerfs-workspace/src/remote_commit.rs`) drives a real local live owner
and the real `VerificationStoreFault::FinalPublication` Store fault, asserts the
exact injected error, the retained stage **and** attempt, then asserts the retry
publishes and the retained generation's bytes read back from the published root.
Verified to fail without the fix with the recorded
`Storage(InvalidInput("workspace stage retained"))`.

### 3.2 #1, #2, #5 — consume sites moved onto the route that does the work

- **#1** `VerificationFault::Candidate` is consumed and
  `inject_candidate_failure_once()` is set before `build_remote_candidate`
  (mirroring `lifecycle.rs:94-100`), and the shared builder now honours the
  one-shot flag exactly as `build_candidate` does — so the two routes cannot
  drift apart again.
- **#2** the Store admission-fault activation (`verification_candidate`, mirroring
  `lifecycle.rs:106`) now runs before the sandbox route's construction, which is
  what makes the fault's own gate
  (`active && committed_early_transactions == 1`) reachable. The receipt proves
  it: `committed_early_transactions: 1`.
- **#5** the `PresentationResume` consume site and the recorded
  `workspace.presentation_failed` state now exist on the sandbox route, where
  presentation is computed from completion delivery and read-metric observation.
  A *retained completion* is deliberately not recorded as a presentation failure:
  the design re-delivers it on the next Commit or End, and recording it would make
  the workspace unrecoverable.

**Threading decision** (asked for explicitly): no fault channel was made
process-global and no worker count changed. Each consume site was moved onto the
thread that performs the work the fault targets, which is the same thread that
already arms it — the SDK caller thread for the Commit path and the live owner
for the append path. `LAYERFS_CONSTRUCTION_WORKERS=1` stayed exported in every
run.

### 3.3 #3, #4 — the payload append is in the container

Both fault channels are process-local and the append now happens in the sandbox
owner (`LocalSpool` in `LiveOwner::write_owned`), so moving the consume site
could not work: on v0.1.5 it worked only because the payload backing was
host-owned. The guarantees were already present; only the arm had to move.

Chosen design (option 1 of the handoff): a **one-shot arm on the existing
authenticated control lane**, `wire::VERIFICATION_FAULT` /
`wire::VERIFICATION_FAULT_RECEIPT` (opcodes 52/53), consumed in `write_owned`:

- `NoSpace` lowers the workspace policy to the current charge **for one append**,
  so the real `ResourcePolicy::check` rejection and its public `ENOSPC` come from
  product code — the same injection shape the pre-v0.1.6 host shell used in
  `file_io.rs`;
- `ShortAppend` completes half the reserved range and fails the append with
  `EIO` before any piece is applied;
- both are one-shot and receipt-checked from the host
  (`RemoteVerificationFaultReceipt { fault, hit_count: 1 }`), so the proofs keep
  their exactly-once evidence rather than asserting a bare errno.

The whole surface — frames, owner state, injection points, host and SDK entry
points — is behind the new `layerfs-fuse/test-instrumentation` feature,
propagated by `layerfs-workspace/test-instrumentation` and
`layerfs-daemon/test-instrumentation`. `Dockerfile.layerfs` is the only build that
enables it, and it enables it on **both** container-side binaries: the benchmark
mount path hosts the live owner inside `layerfs-daemon`, not in the standalone
`layerfs-fuse` helper. A released daemon therefore carries neither the frames nor
the injection points, and the released behaviour of every code path above is
unchanged.

## 4. Impact set, and what was *not* re-collected

- **Re-run in full** (fresh receipts): all 27 verification-supported
  `workspace_reliability` proofs — the handoff's impact set for #6 (the three
  `*-failure-retry` proofs plus the rest of the family).
- **Not re-collected**: performance rows. The code paths they time are unchanged
  in kind, but two additions do land in them and are stated rather than hidden:
  the sandbox Commit entry now takes one additional uncontended workspace-lock
  acquisition and stores one `Option<CaptureSummary>`; the benchmark image's FUSE
  write path takes one relaxed atomic load per write (that load does not exist in
  a release build at all, because the feature is off). Both are sub-microsecond
  against Commit totals of 10 ms–2 s, and no earlier perf row is re-labelled: the
  L25–L29 rows keep their recorded identities and are not comparable to the new
  product seal.

## 5. Non-passing lines, stated plainly

1. **A failed append now leaves its reserved range packed in the sandbox spool
   segment.** The newly meaningful cross-boundary counter reads
   `physical_spool_allocated_bytes` 4096 → 8192 after the injected failure in both
   #3 and #4, where the v0.1.5 control read 4096 → 4096 (its short-append run
   raised only the *peak*, 4096 → 8192, and truncated back). The dead range is
   bounded by segment capacity (1 MiB) and retired with the segment. Not repaired:
   reclaiming it changes product spool policy, which this instrumentation work
   item does not own. Recorded for a future decision.
2. **The two proofs' `spool_segment_bytes` conjunct is vacuous on this route** —
   `verification_workspace_state` reports 0 for every remote workspace, the
   already-declared dead host write-spool metric (final-report §7.4). The
   assertions that carry meaning all hold: exact errno at the public boundary,
   `write_acknowledged_bytes=0`, unchanged published snapshot, intact
   re-verification of the old root, clean Discard and reopen.
3. **`changes::tests::tiered_spill_partial_writes_and_final_merge_are_retryable_and_clean`
   is a pre-existing parallel-run flake.** It counts process-wide file
   descriptors, so a concurrent test that opens any file can fail it. Confirmed on
   the pristine tree (entire change set stashed, same failure; `--test-threads=1`
   passes 73/73). `tools/preflight.sh` passed on the final tree.
4. **Product seal moved.** `dc2b3a14…` → `970964e9…`. The handoff asked that
   #1–#5 keep the product seal unchanged; that is not achievable, because the
   seal hashes every file under `crates/`, and #1/#2/#5's consume sites are
   necessarily in `crates/`. What *is* true and checked: no released code path
   changes behaviour for #1/#2/#5, and #3/#4's surface is compiled out unless the
   benchmark-only feature is enabled. #6 is an intentional behaviour change.

## 6. Reproduction

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
python3 benchmark/fs-bench-pro/shared/runner.py --build-image
# per case; the input identity is digest({family, case, seed, source, recipe})
python3 benchmark-results/issue152/fix/digest.py workspace_reliability <case>
LAYERFS_CONSTRUCTION_WORKERS=1 python3 benchmark/fs-bench-pro/verify-selected.py \
  --family workspace_reliability --case <case> --seed 1 --setup clone \
  --image layerfs-bench-infra:8308cd8e628a97cd --collection-mode \
  $(python3 benchmark-results/issue152/fix/digest.py workspace_reliability <case>) \
  --output benchmark-results/issue152/fix/<case>
```

`benchmark-results/issue152/fix/run.sh` and `run-all.sh` wrap exactly that;
`provisional-329a33bc/`, `provisional-4f82c19a/` and `provisional-b4c4afee/` hold
the intermediate attempts (including the two receipts that exposed the
receipt-after-consumption bug), kept because receipts are append-only.

## 7. Group report

Posted on #152: [#152 comment 5683593603](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152#issuecomment-5683593603).

## 8. Still open (not this work item's scope)

`workspace-sustained-600s-compact-v2-proof` (`NOT_RUN_OPTIONAL`), the
final-report §7 items (kernel-dirty shared mmap, the ~1.03× rewrite-route file
cache, the dead host write-spool metric — now partially superseded by the
container-side counter observed in §5.1 — the six diagnosed single-worker time
regressions, the `edit_length_changing_capped` v1 cells, `repository_history`,
and `historical_access`'s absent sealed v2 Store).
