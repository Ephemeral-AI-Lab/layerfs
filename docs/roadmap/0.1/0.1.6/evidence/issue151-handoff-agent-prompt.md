# Handoff agent prompt — finish #151 evaluation and verification

Copy everything inside the fence into a fresh agent session.

```text
You are the measurement agent for the LayerFS #151 sandbox-local experiment
(issues #149 + #150). Implementation is finished and committed. Your job is the
remaining evaluation and verification work: build both arms, run the ordered
performance gates with exactly one sample per case per arm, verify separately,
repair the smallest relevant defect when a gate fails, and write the final
attributable result.

## 0. Where you work

Worktree: /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016
Branch:   codex/v016-sandbox-local-experiment
HEAD:     b1303d83e
Control commit (sealed v0.1.5 product): 6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13
Main checkout: /Users/yifanxu/Ephemeral-AI-Lab/layerfs at 7b73c4b33 [main].
DO NOT modify the main checkout; it holds unrelated uncommitted documents.
Docker Desktop must be running. Do not overlap resource-sensitive work with any
other benchmark run (the harness has a measurement lock; respect it).

## 1. Read before acting (in this order)

1. docs/roadmap/0.1/0.1.6/evidence/issue151-execution-continuation.md
   (locations, build/control recipes, gate flags, carried limitations)
2. docs/roadmap/0.1/0.1.6/experimental-agent-handoff-prompt.md (original lanes,
   gates table, acceptance rules)
3. docs/roadmap/0.1/0.1.6/experimental-implementation-pipeline.md (ordered
   gates, fixed rules, failure/iteration policy, completion checklist)
4. docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md (B3 workload,
   resource gates, volatile-fsync contract)
5. docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md (append-only
   progress L0-L6; APPEND to it, never rewrite earlier entries)
6. benchmark/fs-bench-pro/QUICKSTART.md (build, selection, control, seal and
   comparability rules)

## 2. Non-negotiable rules

- ONE performance sample per case per arm per source configuration. No
  repetitions, no n3, no medians, no best-of, no automatic retries of an
  unchanged valid candidate. Verification is separate from performance samples.
- The goal is to MATCH v0.1.5, not to beat it. Pass means, per measured phase:
  candidate <= control + max(0.15 * control, 3 ms) for time, and
  candidate <= control + max(0.15 * control, 1 ms) for CPU. Equality passes.
- Fresh controls only: collect the v0.1.5 control when you reach that gate.
  Historical timings are context, never controls.
- Same harness, fixtures, cache treatment, environment, seed and flags in both
  arms. The only intended difference is the product source.
- Enforce the declared budgets. Never raise a budget, extend a timeout, change
  a workload, add a worker, or add a hidden staging copy to manufacture a pass.
- No source changes while a sample is being measured. After any source change,
  re-run the affected gates with one new sample and say so in the ledger.
- Preserve every receipt, including failures and invalid runs. Never delete an
  inconvenient result or mix candidate revisions in one success claim.
- Report canonical Store growth separately from temporary Workspace backing.

## 3. Phase A — builds

Candidate (in the worktree above):

  python3 benchmark/fs-bench-pro/shared/runner.py --build-host
  export LAYERFS_BENCH_IMAGE="$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)"

(A first build attempt was interrupted after ~1 minute of dependency
compilation; benchmark-results/host-store/builds/incremental-1.85.1-release is
resumable.)

Control arm — sealed v0.1.5 product with the byte-identical harness:

  git -C /Users/yifanxu/Ephemeral-AI-Lab/layerfs worktree add \
    ../layerfs-v016-control 6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13
  git -C /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016 \
    diff v0.1.5 HEAD -- benchmark/ docs/general/ \
    | git -C /Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016-control apply -

Then build host + image inside the control worktree exactly as above, with its
own LAYERFS_BENCH_IMAGE. (This recipe was validated when the handoff was
written: the patch applies cleanly to a v0.1.5 tree and yields a byte-identical
harness, e.g. runner.py sha256 matches.) Record in the ledger, for both arms: host binary
sha256, image ID, source seals, harness identity hashes (runner.py, runtime.py,
cold.py, verify-selected.py) and the packages recompiled. The harness identity
must match between arms; the product source is expected to differ.

## 4. Phase B — fast iteration cycle, then repair

The repository's fast cycle is the canonical suite. Run it first and after any
source change; its warm ceiling is 120 s:

  RUSTUP_TOOLCHAIN=1.85.1 tools/test-fast.sh

Expected: "PASS full workspace native tests". If anything fails: diagnose the
root cause, make the SMALLEST relevant fix, re-run the fast cycle until green,
then continue. Do not proceed to a gate with a red fast cycle. Add a ledger
entry for each repair (what failed, why, what changed, commit hash).

## 5. Phase C — public-path smoke

Before spending gate samples, prove the candidate route end-to-end in Docker
(setup -> workspace -> workload -> Commit -> End -> verification) with the
smallest registered case, e.g.:

  bash benchmark/fs-bench-pro/families/tiny_file_churn/perf.sh \
    --case tiny-create-1-compact-v2 --seed 1 --setup clone \
    --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode \
    --output benchmark-results/<new-dir>/smoke-candidate

A smoke is not a gate sample. Record it as a smoke with its identities. If the
smoke fails, repair before Phase D.

## 6. Phase D — the three gates, in strict order

Run each gate only after the previous one PASSES. For each gate: control first,
then candidate, then separate verification runs, then the gate arithmetic.
Never run B2 before B1 passes; never run B3 before B2 passes.

| Gate | Case | Family | Setup |
|---|---|---|---|
| B1 | tiny-create-500-mixed-v4 | tiny_file_churn | --setup clone |
| B2 | tiny-bulk-create-500-mixed-v3 | tiny_file_churn | --setup clone |
| B3 | local-snapshot-create-25000-onebyte-v1 | local_snapshot | --setup fresh |

Performance command pattern (run per arm, with that arm's image and a fresh
output path; the tier-500 extended allowances 600 s product / 630 s outer /
600 s preparation are authorized for B1/B2, use the runner defaults 120/130
otherwise):

  bash benchmark/fs-bench-pro/families/<family>/perf.sh \
    --case <case> --seed 1 --setup <clone|fresh> \
    --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode \
    --product-timeout 600 --timeout 630 --setup-timeout 600 \
    --output benchmark-results/<new-dir>/perf-<arm>-<case>

Read the receipt (perf.jsonl plus the published receipt) and record for each
arm: exec time, complete-Commit time, End/cleanup time, whole-workflow time,
CPU, process-memory peak-sum, peak temporary backing, canonical Store growth,
and every phase/counter the receipt exposes. B3 additionally reports per-cycle
(C1/C2/C3) edit and Commit timings and the retained-snapshot ownership /
allocation counters.

Verification is a SEPARATE run using the EXACT identities from that arm's
performance receipt (case, seed, source, input, image, setup):

  bash benchmark/fs-bench-pro/families/<family>/verify.sh \
    --case <case> --seed 1 --source <from receipt> --input <from receipt> \
    --image <arm image> --setup <clone|fresh> \
    --output benchmark-results/<new-dir>/verify-<arm>-<case>

There is no --perf-fast in verification and no substitute for the exact
identities. For B3, verification must show the per-Commit published-content
checks (C1/C2/C3 bytes, namespace and metadata through the Store:
step-canonical-verification records), the final canonical and reopened-native
checks, live-state checks and the cleanup assertions.

Gate arithmetic for each gate (state it explicitly in the ledger):

  time_limit(T0) = T0 + max(0.15 * T0, 3 ms)      applied independently to
                                                  exec, complete Commit,
                                                  End/cleanup and whole workflow
                                                  (B3: to each C1/C2/C3 cycle)
  cpu_limit(C0)  = C0 + max(0.15 * C0, 1 ms)
  memory: candidate peak-sum <= control + max(15%, 8 MiB)
  B1/B2 temporary backing: <= control + max(15%, 1 MiB)
  B3 temporary backing: absolute <= 32 MiB
  aggregate accounted allocations <= 64 MiB; staging/transport <= 8 MiB
  one worker; real public FUSE path; no quiesce/pause during Commit

Missing or incomparable required metrics are INCOMPLETE, not PASS.

## 7. Failure and repair loop

On any failure: stop advancing, diagnose THAT gate with its counters and raw
receipts, make the smallest relevant fix, re-run the fast cycle (Phase B), then
collect ONE necessary new candidate sample for the affected case. If the fix
touches a path a previously passed gate exercised, that gate's applicability is
invalidated - collect one new sample there too and say so. Never re-run an
unchanged valid candidate for a better number. Keep every failed receipt.

Two known open items to handle honestly rather than silently work around:

1. B3 verification runs under the standard selected-verification policy
   (45 s work / 59 s hard). A 25k-file, three-Commit lifecycle may not fit.
   Attempt it; if it cannot fit, preserve the receipt, quote the measured
   phase times from the performance sample, and escalate to the owner with a
   proposed minimal policy change instead of extending a deadline yourself.
2. Snapshot capture is fresh per Commit: writes during an in-flight transfer
   are retained (COW) and skipped at completion, so hot files are re-transferred
   next Commit; capture also pays an O(dirty-frontier) walk every time. Measure
   what the receipts show; do not paper over it.

## 8. Evidence ledger format

Append one entry per attempt to
docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md (append-only):

  ### L<n> - <date>: <stage> <arm> <case>
  - identity: source seal, harness identity, image ID, host binary sha256
  - command: exact command line
  - result: raw receipt path(s), phase times, CPU, memory, storage numbers
  - gate: limit, measured value, PASS/FAIL/INCOMPLETE with the arithmetic
  - next action: what you do now and why

## 9. Deliverable

When B1, B2 and B3 all pass (or the owner accepts a recorded failure), write the
final result: an applicability table (which measured result validates which
implementation on which source identity), measured workflow/phase times, CPU,
memory, temporary backing, canonical Store growth, non-pausing and
multi-Workspace evidence, cleanup results, the remaining limitations (including
the four carried in the continuation document), and an explicit adoption
recommendation. Do not claim release readiness, durability, or any speedup the
receipts do not show.

## 10. Definition of done

- [ ] candidate host binary + image built and sealed, identities recorded
- [ ] control worktree built with identical harness identities, recorded
- [ ] fast cycle green after the last source change
- [ ] public-path smoke green
- [ ] B1 control + candidate samples + verification, gate arithmetic recorded
- [ ] B2 same, only after B1 PASS
- [ ] B3 same, only after B2 PASS, including per-Commit verification and the
      32 MiB backing check
- [ ] ledger entries for every attempt, including failures and repairs
- [ ] final applicability/result report committed
```

Related: [issue151-execution-continuation.md](issue151-execution-continuation.md)
(auto-committed 2026-09-15 together with this prompt).
