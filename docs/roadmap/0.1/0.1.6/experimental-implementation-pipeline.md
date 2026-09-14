# v0.1.6 experimental implementation and three-gate verification pipeline

Status: execution plan created 2026-09-15; implementation and measurements NOT_STARTED.
Execution issue: [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151), containing this pipeline while the repository document awaits its documentation commit.
Requirements and benchmark contract: [#149](https://github.com/Ephemeral-AI-Lab/layerfs/issues/149).
Reviewed connection architecture: [#150](https://github.com/Ephemeral-AI-Lab/layerfs/issues/150).

## 1. One execution track

This issue owns the complete experimental implementation and its progress/receipts. #149 and #150 are inputs to the same work, not separate projects to finish sequentially. #149 defines behavior, budgets and gates; #150 defines the reviewed ownership, connection and publication refinements. This pipeline defines the dependency order and phase exits without weakening either contract.

Create one isolated implementation branch/worktree from the peeled release commit **`v0.1.5^{commit}` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`** when execution is started. The annotated tag object `5c7c9b0b01107461c3c144bd910539a6d73b12af` is not the source commit. Preserve main, the release tag and unrelated work. Creating this document/issue has not checked out that branch or started product work.

```text
Freeze contract, source base, control and measurement configuration
                              |
                              v
Local Workspace owners + compact metadata + local payload backing
                              |
                              v
Owned snapshot G: prove old view stable while live state changes
                              |
                              v
Bounded frozen transfer + independent live/control/base-read service
                              |
                              v
Existing single-worker builder + checked admission + DB transaction
                              |
                              v
Generation-safe completion + repeated Commits + isolated teardown
                              |
                              v
Focused correctness, failure and resource proofs
                              |
                              v
Gate 1: tiny-create-500-mixed-v4
                              | PASS
                              v
Gate 2: tiny-bulk-create-500-mixed-v3
                              | PASS
                              v
Gate 3: local-snapshot-create-25000-onebyte-v1
                              | PASS
                              v
Report experiment result and remaining production scope

Failure at any stage -> diagnose that stage; preserve the attempt.
No next full benchmark qualification until the current gate passes.
```

## 2. Fixed rules throughout the pipeline

- No imported-library patches, forks, vendored changes, new dependencies, dependency version/feature changes or lockfile edits. Preserve the existing Init/canonical storage algorithms and public benchmark workloads.
- Workspace data is ephemeral. Remove Workspace-only durability flushes and per-mutation host installation. Keep volatile sync compatibility and known-error handling; Commit must work without a preceding fsync.
- No quiesce, pause, whole-cache-drain acquisition, wait-for-command completion, remount or live checkpoint installation during Commit.
- Multiple Workspaces coexist in one sandbox through one shared daemon registry. Live mounts use `/workspaces/<id>/`; private shared backing uses `/snapshots/<id>/`. One Workspace's End must preserve healthy siblings.
- At most one active Commit per Workspace. One shared host canonical compute worker; no parallel file/encoding/GC pool. Necessary FUSE/control/I/O services are bounded and counted, not hidden compute workers.
- One performance sample per case/arm/source configuration. No repetitions, n3, medians, best-of runs or automatic unchanged retries. Focused correctness/fault checks are separate from performance samples.
- Preserve CAS/authentication, CDC/extents, small-file FULL/DELTA eligibility and limits, compression/packing, checked admission, Store rollback ownership and conditional database publication.
- Use the existing declared resource budgets. Do not trade a passing wall time for unsafe memory, unaccounted CPU or temporary-storage amplification.
- Writable shared-mmap exclusion remains proposed, not adopted. The ordinary-write experiment can proceed with the documented visibility limitation; do not disable a supported mode silently or claim full mmap snapshots are proved.
- Crash-safe Store publication, cloud storage, recoverable live Workspaces and million-file spilling are outside this pipeline. Do not add their frameworks now.

## 3. Implementation order

### I0 — Freeze the execution inputs

- Commit the agreed planning documents/scoped rule exceptions before implementation measurements, and record their revisions in the execution issue.
- Create the isolated branch from the exact v0.1.5 source commit; retain a sealed control build. Do not port the current host overlay wholesale.
- Freeze seed 1, fixture/harness/verifier identity, one-worker configuration, cache/setup treatment, host/container resource scope and operation deadlines. Use the existing host coordinator and Linux FUSE/workload infrastructure.
- Use compatible pristine prepared inputs, independent writable sample copies and the existing measurement lock. Record compile/setup separately; reuse unchanged builds/inputs.
- Record the finite transfer/no-progress policy and the 64 MiB aggregate accounting model before choosing buffer/node capacities.
- The baseline for each performance case is collected once when that gate is reached. Do not prepare or benchmark later cases unnecessarily.

Exit: source/configuration/limits are attributable; dependency tree is unchanged; no ambiguity about which case or route will execute. The mmap question is recorded rather than converted into a fabricated correctness pass or a new blocker on ordinary-write implementation.

### I1 — Establish the local mutable ownership boundary

Reuse the daemon's WorkspaceId registry and v0.1.5 local operation/file-piece mechanics. Implement compact bounded metadata and inline/packed local payload ownership, with per-Workspace and aggregate admission. Keep immutable-base fetches and authenticated control traffic.

Remove ordinary host RESERVE/APPEND ownership, dirty-fact mirroring, host mutation installation and sync-triggered full-prefix export from the candidate path. v0.1.5 still has host payload backing, so simply selecting its old route does not finish I1.

Exit checks: create/read/write/rename/unlink locally; verify independent bytes under equal relative paths in two Workspaces; verify resource rejection preserves valid state or fails the disposable Workspace explicitly; counters show no required host mutation installation on the hot path. Do not measure a new benchmark yet.

### I2 — Prove the snapshot data structure locally

Implement fixed-size generation/root retention with ownership of referenced metadata/pieces/payload. Copy touched shared nodes; update exclusive nodes in place. No whole-map clone, directory copy, per-file snapshot or first-post-capture full clone. Keep internal incarnation/generation/attempt identity despite stable `/snapshots/<id>/` paths.

Exit check: write A, capture G, modify/rename/delete through the live successor, read old state from G and new state live, then release G. Verify unchanged payload is shared and only unowned backing is reclaimed. This local check isolates ownership bugs before transport or SQL is involved.

### I3 — Connect bounded immutable transfer without blocking live service

Adapt frozen changed records, pieces and needed bytes into the existing first-party framing/role primitives. Retain authorization, framing/length/offset checks and a validated end of input. The host never substitutes newer live paths for missing snapshot data.

Keep live/control and immutable-base service progressing independently of bulk transfer. Remove the inherited whole-Commit lifecycle lock from SDK editing and avoid holding a control-group mutex or all admission capacity over the entire stream. Reuse existing reserved service capacity rather than building a generic scheduler.

Exit check: deliberately stall bulk transfer; complete a local write, host SDK edit/status and uncached base read before releasing it. Cancel a partial transfer and prove snapshot-reader/backing ownership is released safely. Service lanes must not become extra encoding workers.

### I4 — Reuse canonical construction and make publication exact

Feed the frozen input into the existing single-worker CAS/CDC/FULL-DELTA/compression/packing pipeline. Preserve predecessor roots and range provenance so unchanged extents and eligible deltas remain reusable.

Retain the existing Store admission/operation permit and rollback scope. Before publication, the host owns all logical dependencies and transitive physical DELTA-base dependencies; no published output depends on sandbox files, buffers or lazy callbacks. Reuse existing dependency checks rather than scanning the whole committed tree again.

Use the existing SQLite conditional publication transaction. Default to a bounded host-memory attempt/result under the live-host failure model. Prepare the outcome and record successful SQL completion infallibly before any cancellation point, await, telemetry or result send. Cancel before publication by abandoning the owned attempt; once publication starts the host finishes/resolves it independently of the caller/sandbox.

Exit checks: correct small and large encoded files, exact CAS reuse and FULL fallback; partial admission failure; failure before branch advancement; moved-head rejection; cancellation around SQL completion; known success with a lost reply; Store-only reads after sandbox destruction. Real ambiguous SQL errors remain indeterminate until resolved; never guess from autocommit or drop rollback protection because there is one Commit worker.

### I5 — Complete the reusable live lifecycle

Replace old checkpoint installation with exact generation coverage and compact canonical result/provenance handoff. A delayed/duplicate C1 result cannot erase G+1 changes or alter C2's context. Keep live mount/inode/descriptor identities. One queued request captures only when it actually starts.

End/Discard retires only the selected Workspace's executions, mount, readers, backing and attempt context. Release snapshot ownership after Commit, not all shared backing. Do not canonicalize files merely to destroy their Workspace. An uncertain/broken Workspace may fail and be recreated; its host publication result remains authoritative.

Exit checks: held-builder live operation progress; C1 excludes later B while C2 includes it; delayed/duplicate/misaddressed result rejection; another command and Commit on the same Workspace; End A while B's open handle/read/write/Commit remain usable; bounded retention after release and a later small edit.

### I6 — Qualify one complete candidate for benchmark Gate 1

Run only the focused checks affected by I1–I5, using existing tests/helpers where possible. Ensure meaningful checks cover allocation failure, hardlink/open-unlinked/rename behavior, required threshold/locality encoding behavior and cleanup. Run applicable format/lint checks before sealing the candidate. Do not run the broad world/full-matrix suite or repeatedly rerun passing checks without a relevant change.

Seal the candidate source/binary/image, actual worker count, route counters and accounting. Verify the real public FUSE path executes this implementation. A component-only snapshot benchmark or direct Store call cannot substitute for the required public cases.

Exit: one complete candidate is ready; correctness and resource assumptions have runnable focused checks; no temporary instrumentation is misrepresented as acceptance evidence.

## 4. Three sequential benchmark gates

All gates use one control performance sample and one candidate performance sample, plus the required separate correctness verification. Full readback/reopen/digests/censuses do not enter product performance timers. Record both product-call phases and complete workflow, with host/sandbox CPU, RAM, temporary backing, canonical Store growth, request counts, build/setup and cleanup.

| Gate | Exact case and fixture | Purpose and exit |
|---|---|---|
| B1 | `tiny-create-500-mixed-v4`: 500 target creates on the registered 5,000-file / 500 MiB background, seed 1 | Prove per-operation, metadata and lifecycle overhead is comparable to v0.1.5; every gate passes before B2 |
| B2 | `tiny-bulk-create-500-mixed-v3`: 5,000 new files / 500 MiB on the registered 200-file / 1 MiB initial namespace, seed 1 | Prove bounded payload transfer and existing small/large-file encoding throughput; every gate passes before B3 |
| B3 | `local-snapshot-create-25000-onebyte-v1`: the #149-defined 25,000 one-byte files, C2 edits 256 selected files, C3 restores them, then End | Prove compact metadata, snapshot retention, successive-Commit locality and cleanup; all cycle and workflow gates pass |

The three B3 Commits are distinct operations in one lifecycle sample, not statistical repetitions. Keep the exact names, byte equations, normalization, sync calls and C2/C3 schedule from #149; do not invent another 25k fixture here.

### Performance and resource limits

```text
For each measured time T0 in the control:
    candidate <= T0 + max(0.15 * T0, 3 ms)

For control total workflow CPU C0:
    candidate <= C0 + max(0.15 * C0, 1 ms)
```

Apply time gates independently to exec, complete Commit, End/required cleanup and complete public-call workflow. B3 applies the corresponding gates to each C1/C2/C3 edit/Commit phase and the complete lifecycle. Begin and visibility are reported and included in the total. Missing/incomparable required metrics are INCOMPLETE, not PASS.

Retain #149's declared resource limits: 64 MiB aggregate accounted working allocations; 8 MiB staging/transport buffers within that total; existing 8 MiB final-delta allowance; process-memory comparison ≤ control + `max(15%, 8 MiB)` using the defined conservative peak-sum metric. B1/B2 peak temporary backing must be ≤ their respective control + `max(15%, 1 MiB)`. B3 has the 32 MiB absolute temporary-backing gate.

The 32 MiB backing ceiling does not apply to B2's legitimate 500 MiB of new content. B2 must stream from local backing under the same RAM limits and count all transfer/encoding work in Commit. Do not move new payload before snapshot capture, hide a full host staging copy, or waive the Commit-phase gate because work shifted from exec.

Use freshly collected controls to calculate limits. Historical 242.660 ms create-500 and 5.732993668 s bulk-create-500 results are context only. The historical 25k debug test did not include Commit and is not a control. No result is promised before measurement.

## 5. Failure, iteration and evidence reuse

- Stop advancement at the first failed implementation exit or benchmark gate. Inspect that case's counters/fault evidence and make the smallest root-cause change.
- One relevant source change permits one necessary new candidate sample. Do not rerun an unchanged valid candidate for a better number, add samples, or delete an inconvenient result.
- A control may be reused only while harness, fixture, metrics, cache treatment, worker configuration and environment remain comparable. Preserve all failed/invalid receipts with exact identities.
- If a later fix affects an earlier passed case or proof, invalidate that applicability and collect one new sample/check for the changed path. Do not claim three-case success by mixing incompatible candidate revisions, and do not rerun unaffected passing cases automatically.
- Keep one append-only ledger on the execution issue or its linked evidence document, with stage, source identity, command, result, metric/gate, raw receipt path and next action. Do not duplicate progress campaigns across #149 and #150.
- A resource error is a target-case failure even when handled safely. No automatic budget increases, worker additions, timeout extensions, pause fallback or disabled encoding to obtain a pass.
- Unsupported mmap visibility stays explicit. A fast ordinary-write result does not silently qualify all mappings. Release admission remains false for this single-run experimental pipeline.

## 6. Completion and handoff

This execution issue is complete only when I0–I6 and B1–B3 have attributable passing evidence, required cleanup passes, and a final source-applicability table identifies which implementation each result validates. Report workflow/phase time, CPU, memory, temporary and canonical storage, non-pausing behavior, multi-Workspace isolation and remaining limitations.

No new architecture document or benchmark family is required merely to mark progress. Keep implementation concentrated in the first-party local-owner/snapshot/transport/commit adapters and reuse existing core/storage behavior. Broader consumer migration, mmap scope resolution, full release testing, durability and cloud work remain separately recorded. Passing this pipeline does not automatically merge main, close #149/#150 or release v0.1.6.

## Execution checklist

- [ ] I0: source base, documents, control, fixture/harness, budgets and metrics frozen.
- [ ] I1: local mutable ownership and backing; two Workspaces; no hot-mutation host installation.
- [ ] I2: stable owned snapshot with compact sharing and safe release.
- [ ] I3: bounded frozen transfer and live/control/base-read progress under stall.
- [ ] I4: existing encoding and complete dependency closure; atomic publication and exact host outcome.
- [ ] I5: generation-safe completion, repeated Commits and sibling-preserving teardown.
- [ ] I6: focused proofs/checks pass; complete public-path candidate sealed.
- [ ] B1: one-sample create-500 control/candidate and separate verification pass all gates.
- [ ] B2: one-sample bulk-create-500 control/candidate and separate verification pass all gates.
- [ ] B3: one-sample 25k three-Commit control/candidate and separate verification pass all gates.
- [ ] Final evidence/applicability table, cleanup and adoption recommendation recorded.
