# Handoff — fix the six `workspace_reliability` proof failures left by #152

Status: **open work item**, handed over from the #152 campaign
([final report](evidence/issue152-final-report.md), group report
[#152 comment 5677526128](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152#issuecomment-5677526128),
corrected classification
[#152 comment 5682534148](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152#issuecomment-5682534148)).

Read first: `AGENTS.md`, `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md`,
[`sandbox-host-connection-architecture.md`](sandbox-host-connection-architecture.md),
and the ledger entries `L25`–`L29` in
[`evidence/issue151-experiment-ledger.md`](evidence/issue151-experiment-ledger.md).

## 1. What is broken, in one line each

Six registered verification proofs for `workspace_reliability` fail on the current
candidate. Five fail because their **fault injection still points at the
pre-v0.1.6 host-owned code paths**; one fails because the sandbox route **genuinely
lost a recovery behaviour** the materialized route still has.

| # | case | class | one-line cause |
|---|---|---|---|
| 1 | `workspace-candidate-failure-retry-compact-v2-proof` | H | injection consumed at `lifecycle.rs:98` → `INJECT_CANDIDATE_FAILURE` (`changes.rs:333`); the sandbox route builds via `build_remote_candidate` (`changes.rs:368`), which never consults it |
| 2 | `workspace-admission-batch-failure-retry-compact-v2-proof` | H | the store fault is armed **after** admission on this route, so its gate (`committed_early_transactions == 1`, `schema.rs:930-936`) can never hold |
| 3 | `workspace-deferred-nospace-compact-v2-proof` | H | `VerificationFault::NoSpace` is consumed in the host shell's `Workspace::append` (`file_io.rs:449`); the payload write now happens in the **container** (`LocalSpool`, `live_owner.rs:979`) |
| 4 | `workspace-short-spool-write-compact-v2-proof` | H | same as #3 via `VerificationFault::ShortAppend` (`file_io.rs:468`) |
| 5 | `workspace-published-presentation-failure-smoke-v3-proof` | H | `VerificationFault::PresentationResume` is consumed only at `lifecycle.rs:639`, the materialized presentation transition |
| 6 | `workspace-final-publication-failure-retry-compact-v2-proof` | **product defect** | `remote_commit.rs:50-58` refuses *any* Commit while `pending_stage.is_some()`; the materialized route retries instead |

Control A/B is already established: **all six PASS on the reconstructed v0.1.5
control**, so none of them is a stale-in-both-arms test.

## 2. Current frozen state

```
source seal      d1bbf882067d824b6a7135fa515059fe4db59488f5bb49c39f2cfbb781ad241e
product seal     dc2b3a14f45a4eb7d07b132562b3eadaba5e87644b517aaf6189ce1008e8490d
compilation seal 2c4ec5eea9a731b9469900c2266d10d45d18ccb98c5c6e0179f00c5edfbf5290
harness identity daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044
workload source  821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8
image            layerfs-bench-infra:d1bbf882067d824b
commit           b9bca593c (then c553acbf7, docs only)
```

Rebuild after any change: `python3 benchmark/fs-bench-pro/shared/runner.py --build-host`
then `--build-image`. The runner refuses to run when the host and image compilation
seals differ, and when the source seal differs from the image label.

`benchmark-results/` is **git-ignored**, so the helper scripts and the registry dump
referenced below live only on this machine. Regenerate the registry if it is gone:

```bash
mkdir -p benchmark-results/issue152/registry
target/release/fs-benchmark-pro infra-list workspace_reliability \
  > benchmark-results/issue152/registry/workspace_reliability.jsonl
```

## 3. How to reproduce one proof

`verify-selected.py` demands exact identities, and the input identity is
`digest({family, case, seed, source, recipe})` — it **changes with the source seal**,
so recompute it after every re-seal:

```bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
CASE=workspace-deferred-nospace-compact-v2-proof
python3 - "$CASE" <<'PY'
import json, pathlib, sys
sys.path.insert(0, 'benchmark/fs-bench-pro/shared')
import runner
case = sys.argv[1]
h = json.loads(pathlib.Path('target/release/fs-benchmark-pro.identity.json').read_text())
source = h['LAYERFS_SOURCE_SEAL']
reg = pathlib.Path('benchmark-results/issue152/registry/workspace_reliability.jsonl')
row = json.loads([l for l in reg.read_text().splitlines() if case in l][0])
print(source, runner.digest({"family": "workspace_reliability", "case": case,
                             "seed": 1, "source": source, "recipe": row}))
PY
```

then (fresh `--output` every time; receipts are append-only):

```bash
LAYERFS_CONSTRUCTION_WORKERS=1 python3 benchmark/fs-bench-pro/verify-selected.py \
  --family workspace_reliability --case "$CASE" --seed 1 --setup clone \
  --image layerfs-bench-infra:d1bbf882067d824b --collection-mode \
  --source <SEAL> --input <DIGEST> --output benchmark-results/issue152/fix/<name>
```

The failure is in `verification.json` (`status`, `error`) and in the command log.
Walls are 0.5–0.9 s per proof, so iterate freely. The 27 passing proofs and the
control run live under `benchmark-results/issue152/g7/verify-*proof/` and
`/Users/yifanxu/layerfs-v016-control/benchmark-results/issue152-control3/`.

## 4. The work, case by case

### 4.1 #6 first — it is the only product bug

`workspace-final-publication-failure-retry-compact-v2-proof` arms
`VerificationStoreFault::FinalPublication`, which **does** fire: both
`verification_candidate` (`workspace.rs:286`) and the checkpoint (`workspace.rs:361`)
sit inside `commit_workspace_candidate`, and the sandbox route calls it
(`remote_commit.rs:360`). The proof therefore gets past its own assertion — the
faulted Commit returns exactly `Integrity("injected qualified Workspace transaction
failure")`. What fails is the **retry**, and the error shape says so: the propagated
message is the product's `Workspace(Storage(InvalidInput("workspace stage retained")))`,
not the proof's `"faulted Commit did not surface exact injected error"`.

* Materialized route: no `pending_stage` guard at entry.
  `commit_workspace_session_with_status` computes
  `completion_pending = pending_stage.is_some() || pending_publication.is_some()`
  (`lifecycle.rs:596-598`), **skips capture**, and calls `workspace.commit()`, which
  re-drives publication (`lifecycle.rs:36-40`) and clears the stage on success
  (`lifecycle.rs:136`). That is the "supported Commit retry" the proof and its
  comment encode.
* Sandbox route: `remote_commit.rs:48-58` returns
  `InvalidInput("workspace stage retained")` for any Commit while a stage is
  retained, **before** the `pending_completion` re-delivery path at `:61-64`. Its
  comment claims *"exactly as on the materialized route"*, which the code does not do.

**Fix direction.** Make the sandbox entry re-drive a retained stage the way the
materialized route does, instead of refusing. The fields already exist on the shared
`Workspace` (`pending_stage`, `pending_publication`, `pending_completion`,
`pending_checkpoint` — see `cow_tree.rs:57` and the uses at `remote_commit.rs:216`,
`:268`, `:374`), and `resolve_pending_completion` (`remote_commit.rs:248`) is the
existing re-drive machinery for the sibling case. Ordering matters: today the stage
guard pre-empts completion re-delivery, so whatever you do must not strand an
undelivered completion.

**Do not just delete the guard.** It is possible it was written because a retained
sandbox stage cannot always be re-driven. Establish which is true before changing
behaviour; if it cannot be re-driven, the honest outcome is a documented contract
change (retry unavailable on this route) plus an owner decision, *not* a silently
edited proof. User-visible stake: today the only recovery from a failed final
publication on the sandbox route is `EndWorkspaceMode::Discard`, which throws the
retained publication away.

**Verify with:** proof #6, then re-run #1 and #2 (they share the retry tail), then
the whole `workspace_reliability` set — see §6.

### 4.2 #1 and #2 — arm the faults where this route builds

Both are one-shot faults consumed exactly once and then receipt-checked by the
harness (`verify_fault` / `take_verification_store_fault_receipt`, `hit_count == 1`).

* **#1** — the materialized path consumes `VerificationFault::Candidate` at
  `lifecycle.rs:94-100` and then calls `inject_candidate_failure_once()`. The sandbox
  route reaches `build_remote_candidate` (`changes.rs:368`) via
  `lifecycle.rs:549 → remote_commit::commit_remote`. Add the equivalent consume plus
  the flag check on that route, **or** better: hoist the consume into the shared
  commit entry and have the shared builder honour the flag, so the two routes cannot
  drift again.

  A **build** failure on the sandbox route retains no stage
  (`.build_remote_candidate(...)?` at `remote_commit.rs:143` is a plain `?`), so the
  retry behind it behaves — this is why #1 should go green once the injection fires.

* **#2** — the gate in `verification_store_checkpoint` (`schema.rs:930-936`) needs
  `active && hit_count == 0 && fault == LaterAdmissionBatch &&
  committed_early_transactions == 1`. `active` is set by `verification_candidate`,
  which the materialized route calls at `lifecycle.rs:106` — **before** construction
  — while on the sandbox route it is only reached inside
  `commit_workspace_candidate` (`workspace.rs:286`), i.e. **after** the admission
  that `verification_early_committed()` (`objects.rs:2222`) counts. So the counter
  can never reach 1. Fix: mark the fault active before the sandbox route's candidate
  construction, mirroring `lifecycle.rs:106`.

**Threading hazard for both.** `VERIFICATION_FAULT` is a process-wide
`std::sync::Mutex` (`lifecycle.rs:1815`), which is fine, but
`VERIFICATION_STORE_FAULT` is a **`thread_local!`** (`schema.rs:882`) and
`INJECT_CANDIDATE_FAILURE` is a **`thread_local!` `Cell`** (`changes.rs:2055`).
Arming on the SDK caller's thread and firing on a commit/construction worker thread
will silently do nothing. Decide explicitly whether to move the arming to the
worker, to make the channels process-global like `VERIFICATION_FAULT`, or to pin the
work to one thread — and say which in the commit message.

### 4.3 #3 and #4 — these need a cross-process channel, not a moved call

The payload write these two proofs fault is performed by the **container's FUSE
daemon** (`live_owner.rs:979`, `self.0.spool.reserve(...)` against `LocalSpool`),
while the fault is armed from the **host** SDK process. Both fault channels are
process-local, so migrating the consume site cannot work:

* The injection sites are the host shell's append path (`file_io.rs:449` for
  `NoSpace`, `:468` via `backing.append(..., inject_short)` for `ShortAppend`).
* On v0.1.5 this worked only because the payload backing **was** host-owned, so the
  container's FUSE write was forwarded into the same host process that armed the
  fault.

So the real options are:

1. put a fault arm in the container side (env var or a control opcode next to
   `LAYERFS_EDIT_FAILURE_DIAGNOSTIC` / the `OBSERVE` family), consumed in
   `LocalSpool`/`live_owner`; or
2. re-express the two proofs so they fault a boundary that is on the host
   (publication, completion delivery) and keep the spool-budget assertion as a
   separate unit test.

Either way the **guarantees themselves are present on the new route** — `LocalSpool`
charges against the workspace spool policy and returns `NoSpace`, and `live_owner`
maps `InvalidInput("workspace spool limit" | "workspace live allocation")` →
`PortError::NoSpace` (`live_owner.rs:158`) — so this is instrumentation work with a
real design choice in it, not a product repair. Record the choice on #152.

### 4.4 #5 — one consume site, same process

`VerificationFault::PresentationResume` is consumed at `lifecycle.rs:635-643` after
a materialized Commit succeeds, and drives `projection::inject_resume_failure_once()`
→ `INJECT_RESUME_FAILURE` (`projection.rs:520`), read by
`projection::resume` — whose comment already says the sandbox-owned route *has no
freeze/resume* and that this function "only owns the recorded failure state".
The sandbox route computes its own presentation state
(`presentation_failed: !completion_delivered || observations.is_err()`,
`remote_commit.rs:242`), so the equivalent consume site belongs alongside that.
Same `thread_local!` hazard as §4.2.

## 5. Rules that bind this work

* **H fixes never touch the product.** #1–#5 are H: fix them in `benchmark/` or in
  the `#[cfg(feature = "test-instrumentation")]` surface, keep the product seal
  unchanged, and say so. #6 is the exception and needs a product commit plus a
  regression test that fails without it.
* **Re-sealing.** Any `crates/` change moves the product seal; any `benchmark/`
  change moves the source and compilation seals. After a re-seal, recompute the
  input digest (§3) and re-run the affected proofs only. If product behaviour
  changes, re-run the *impact set by call path* — for #6 that is the three
  `*-failure-retry` proofs plus the rest of `workspace_reliability`, and it
  invalidates any perf row whose call path changed.
* **`tools/preflight.sh` before every push.** There is no CI (ledger `L21`); a push
  may never claim "CI green". Run it for every commit here.
* **One sample, no warm-cache credit, no worker-count changes to pass a gate.**
  `LAYERFS_CONSTRUCTION_WORKERS=1` stays exported in every run.
* **Receipts are append-only.** Use a fresh `--output`; never overwrite a failing
  receipt. Keep the six current failures on disk as the "before" evidence.
* **Do not relax a proof to make it pass.** If #6 turns out to be an intentional
  contract change, that is an owner decision recorded on #152, not an edited
  assertion.

## 6. Definition of done

1. Each of the six proofs runs and **passes** on the candidate, or carries a written
   RCA plus an owner decision on #152 explaining why it cannot.
2. `benchmark/fs-bench-pro/verify-selected.py --family workspace_reliability` for all
   27 verification-supported proofs: `PASS`, fresh receipts under
   `benchmark-results/issue152/fix/`.
3. `tools/preflight.sh` passes for every commit; the tree is clean for sealed runs.
4. A group-style report posted on #152 with the new identities, the per-case result,
   the control A/B where it still applies, and the updated tally — including
   `workspace-sustained-600s-compact-v2-proof`, which stays `NOT_RUN_OPTIONAL`.
5. Ledger entry appended (next free number after `L29`).
6. If #6 landed as a product fix: the impact set re-run and stated, and the
   `docs/roadmap/0.1/0.1.6/README.md` / final-report limitation about recovery after
   a failed publication corrected to match reality.

## 7. Other open items from #152 — **not** part of this note's scope

Listed so nothing is a surprise; each is already terminal and documented in the
[final report §7](evidence/issue152-final-report.md).

| item | disposition | note |
|---|---|---|
| `historical_access` 11 perf + 11 proofs | `NOT_RUN` | sealed v2 Store (`store_sha256 f323de0e…`) is absent; the documented path was removed by the `L17` cleanup. Re-runnable if the Store reappears |
| kernel-dirty shared mmap not captured by a Commit | open limitation | declared by the frozen spec; both repairs are out of bounds (`§9.1` is "PROPOSED, not adopted") |
| ~1.03× payload-proportional file cache on the rewrite route | named guardrail FAIL | v0.1.5 parity, so not a regression |
| host FUSE write-spool metric dead on the sandbox route | limitation | `spool_write_bytes` reads 0; needs a new cross-boundary counter |
| 6 single-worker time regressions in the dedup block | FAIL (diagnosed) | `LAYERFS_CONSTRUCTION_WORKERS=1` removes the released 4-way small-content parallelism; proven by a labelled diagnostic |
| 5 `edit_length_changing_capped` v1 cells | `NOT_RUN_OPTIONAL` | superseded duplicates, not admitted to host execution |
| `repository_history` 3 profiles | `NOT_RUN_OPTIONAL` | optional |
