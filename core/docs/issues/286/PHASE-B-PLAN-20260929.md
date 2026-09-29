# #286 Phase B: ordered benchmark evaluation and optimization

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owner-directed next-agent assignment, 2026-09-29. Parent [#284](https://github.com/Ephemeral-AI-Lab/layerfs/issues/284),
> sub-issue [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286), draft integration [PR #285](https://github.com/Ephemeral-AI-Lab/layerfs/pull/285).
> Starting product/test source: `671f4a46f8f354cf764d3d1555529b73a5c939e0`.
> Phase A documentation source: `4a4447db1abcbfccea0bcf1c79b9cc1d80ac2f04`.
> This publication collects no benchmark sample; every Phase B gate remains NOT_RUN.

Phase A selectively integrated active backing into the component-only namespace.
Its [implementation report](../284/PHASE-A-IMPLEMENTATION-REPORT.md) establishes
the small correctness cohort and owning checks, with scaled benchmarks unrun.
Phase B organizes, evaluates and fixes that integrated product. This plan
supersedes the earlier Phase B hold and execution order, while retaining the
source-pinned [architecture and integration plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/IMPLEMENTATION-MERGE-PLAN-20260929.md),
[family layout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BENCHMARKS.md)
and [baseline inventory](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BASELINE-20260929.md).

## 1. Mandatory family order

| Order | Family | Required evaluation |
| ---: | --- | --- |
| 1 | `init_namespace` | Existing public SDK Init, release driver/verifier, default 100 then 1000 files; explicit 10000/100000 tiers retain separate selection/status. |
| 2 | `history_retention` | Stride10, stride3, stride1: 17/53/157 retained states. Update and enforce the hard storage gate and independent history proof. |
| 3 | `workspace_write` | All nine public SDK/POSIX-FUSE cells: append/dispersed/repeated x100/512/4097. Correctness and the speed gate must pass. |
| 4 | `workspace_commit` | Clean/one-edit retained-state controls; live G1/G2, lowering, headroom, old pins, known/unknown failures and checked cleanup. |
| 5 | `workspace_namespace` | Component/deep access, inherited and resident descendants, rename/replacement/move-back/listing and unrelated-resident scaling. |
| 6 | `workspace_mutations` | Mixed create/write/resize/metadata/link/move/remove/symlink operations, live selections and failure custody. |
| 7 | `workspace_shell_package` | Existing refresh/package selections, then prospectively registered many-file workloads. |

**The first three are the first wave, not the endpoint. Continue families 4-7
after the first three pass.** Register the complete selection before collecting
its first official sample. Keep skipped, unavailable and explicit tiers visible
as NOT_RUN with reasons. Finite cases do not complete #256 or the broader #248
streaming/more-than65535/progress scope.

## 2. Canonical folder ownership

```text
core/benchmark/fs-bench-pro/
  runner.py                    sole dispatch and retained-evidence commands
  families/
    init_namespace.py          existing sole Init registry/runner
    history_retention.py       thin adapter to the existing Rust history owner
    workspace_write.py         canonical 3x3 and separate write selections
    workspace_commit.py        canonical Commit selections
    workspace_namespace.py     canonical namespace selections
    workspace_mutations.py     canonical mixed-mutation selections
    workspace_shell_package.py canonical package selections
  shared/                      reuse current seals/lifecycle/receipts/cache tools
  writers/                     ordinary POSIX workload programs
  tests/                       focused harness/registry checks
core/benchmark/fs-bench-pro-storage-content/
  src/{families,workload,ops}/history.rs  existing history implementation owner
core/docs/issues/286/
  PHASE-B-PLAN-20260929.md
  HANDOFF-PHASE-B-20260929.md
  EXPERIMENT-LOG.md             append-only index of actual rounds
  experiments/<round-id>.md    committed observations and receipt manifest
```

Add a family module only when implementing its cases. Reuse the existing Rust
history backend and SDK examples; do not duplicate measured algorithms or add
a second Init runner. The donor Workspace harness needs selective adaptation
to the integrated APIs and current observations. One canonical registry owns
each case. Keep historical entry points and receipts intact.

Python modules use `snake_case`, CLI families use `kebab-case`, new case IDs
use `<family>-<scenario>-<dimension>-<value>-v<version>`. Preserve original
`history-stride10`, `history-stride3`, `history-stride1`, `issue273-*` and
`issue248-separated-4097-v1` identities. A changed metric or operation gets a
prospective version; a folder move does not rewrite old evidence.

## 3. First-wave gates and baseline roles

### Init

Use `runner.py` / `families/init_namespace.py`, route `host-direct-sdk-v2`,
fixture `core-sdk-init-fixture-v2`, seed1 and locked release binaries under
`target/release/examples/`. The driver calls public `Client::init_project` once.
No FUSE or component-only substitute supplies an Init row.

> **Entrypoint-name correction, 2026-09-30:** The dated line above names a
> nonexistent `Client` method. The actual source-pinned driver calls
> `ProjectApi::new(&server).init(...)` once, after creating the host Server and
> immediately inside the recorded SDK-call timer. Historical receipts keep
> their original labels; future runner metadata uses `ProjectApi::init`.

The current [Init contract](../../../benchmark/fs-bench-pro/AGENTS.md) declares
15s complete command and 9.5s separate lite verification. Its verifier checks
the full namespace/directory metadata and all bytes/metadata of its deterministic
file sample; it is not a full-content oracle for every file. The existing
`source-cache-uncontrolled-v1` profile has `performance_gate=NOT_FROZEN` and
numeric admission INELIGIBLE. Freeze any new numeric metric/threshold and
enforceable profile prospectively before claiming it. A 15s command-budget PASS
does not invent a raw SDK latency target; the historical 100000-file 2.7s cold
target is distinct. Retain raw results and the exact eligibility verdict.

For the family1 checkpoint, use the existing [functional completion rule](../../../docs/benchmark/fs-bench-pro/issue-231/FUNCTIONAL-COMPLETION-20260924.md):
correctness, declared command/verifier budgets and cleanup must pass. Raw SDK
time remains diagnostic and numeric eligibility remains explicit. A new numeric
Init claim needs a new prospective contract; do not invent that as a prerequisite
for continuing to history under the existing functional profile.

### History: update the actual storage gate

Keep the fixed corpus/policy and differences between consecutive selected
states from the [history specification](../../../../docs/roadmap/0.1/0.1.7/retained-history-storage.md).
The existing driver reports a C2 Store footprint but does not enforce its
named storage gate and does not persist a C5 retained-history catalog.
Correct both omissions before claiming retained-product storage.

| Original schedule | States | Cumulative logical bytes | Strict allocated-byte ceiling |
| --- | ---: | ---: | ---: |
| `history-stride10` | 17 | 561010345 | **<49344512 B** |
| `history-stride3` | 53 | 1676767835 | **<64024576 B** |
| `history-stride1` | 157 | 4936693030 | **<83947520 B** |

The updated Phase B hard metric is:

```text
total_retained_allocated_bytes
  = allocated(C2 Store) + allocated(C5 History)
  + allocated(any distinct required persistent index file)
allocated(file) = st_blocks * 512
PASS requires total < the schedule's ceiling; equality is FAIL.
```

Embedded indexes already belong to their database allocation; count each owner
once. Persist real retained commits/roots through the public C5 owner. An empty
History file or an ephemeral harness root map cannot qualify the compound claim.
Measure actual closed at-run files with exclusive allocation attribution;
reflinks, apparent size and archived-copy blocks cannot decide the gate.
Report C2, C5, distinct indexes, total, apparent bytes, logical SQLite bytes,
pack bodies, freelist, retained states and dedup separately.

Before collecting the compound profile, freeze deterministic catalog binding/
incarnation, stack, branch, scope and policy; genesis treatment, per-state
Commit/Layer publication and retention; and expected persisted state/root/row
counts. Reopen History and query every selected retained root through public
C5 APIs. These are declared workload choices, not candidate-dependent size
adjustments; do not create history rows through raw SQL.

Preserve original C2-only `g1.o6-below-v016` semantics and receipts. Register
the compound operation/profile/receipt and gate prospectively; proposed IDs
are `history-retention-stride-10-total-storage-v1` (and stride3/stride1 equivalents),
profile `c1-c2-c5-retained-history-v1`, gate
`g1.o6-total-retained-below-v016-v1`. Use the strict ceilings above for the
new primary gate; do not silently relabel a historical C2-only row. Implement
fail-closed threshold enforcement in registry, evaluator, verifier and report.
Missing/unreadable measurement is INCOMPLETE, shared/reflink attribution is
INELIGIBLE, and a valid exclusive total >=ceiling is FAIL. None passes the gate.
This remains a component history route, distinct from SDK/FUSE Commit timing.

The current backend lacks a complete independent expected-root ledger. Identify
and freeze its source/pin-generation method before qualification, using an
approved sealed reference or independent reference implementation rather than
the candidate's own trace/replay. Implement every-state root comparison,
full state-tree comparison against
`oracles/<sha>.json`, deterministic 10% byte checks plus endpoints and all
canonical-count pins. Deduplicate byte verification by distinct object.
Stride3 pins are 589423458 canonical bytes /73476 objects; stride1 pins are
871588115 /104705. Stride10 has first-run pins, not zero placeholders. A
candidate's own trace or replay is not an independent expected result.
Resolve any canonical-profile mismatch explicitly without rewriting pins.
Check source-aware schema identity: current C2 `user_version=10`, six
application tables; C5 `user_version=1`, seven. SQLite `schema_version` is
not the product version. Retain pack/accounting, integrity and no-sidecar
checks, with one applicable database per owner.

History retains its separate InProcess/no-prepared-Store policy: construct and
save every selected state in the measured operation. Corpus acquisition can
be reused outside timers; a preconstructed Store or history cannot. Its
verifier ceilings are 10/20/30s for stride10/3/1. Declare per-tier complete-command
ceilings from the recorded stride10 baseline before optimization, as the
history contract requires; do not transfer generic 15/25s or a 60s verifier
default. Time is the existing diagnostic tripwire, not a new history speed
gate. Stride1 is run-only: investigate scaling at stride10/3, with no product
policy tuned from a stride1 number.

### Workspace write: all nine speed cells

Preserve the frozen [checkpoint-5 execution specification](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md):
10MiB/10485760-byte all-A master, one-byte writes `B+(i%24)`, dispersed
offset `(104729+i*2654435761)%10485760`, repeated offset5242880, ordinary
O_APPEND for append; one writer and one canonical construction worker.

**Every 100/512 cell must meet <=15s, and every 4097 cell <=25s.** The speed
metric is external driver launch-to-exit, covering lifecycle, Mount, arbitrary
Exec, Commit, Status and checked unmount/deletion. It is not Exec alone,
Commit alone, a sum of selected intervals or a nine-cell average. Retain raw
Exec/Commit/cleanup observations and the separate 9s independent verifier.
Require exact old/new bytes and successful known Commit for every cell.
An eligible performance claim also requires its declared symmetric cache
method. Under-budget INELIGIBLE rows retain that label; no invented 2x target.

The original #248 separated4097/8194-byte fixture stays separate. Native8192
at default8MiB is a functional/count selection; public SDK8192 needs separate
prospective registration. Observe the actual10240 outcome at the new source:
prove success or the exact known-result/local-C5 refusal custody; do not
manufacture an old capacity boundary. Required full lowering/headroom and
reordered Base proofs remain distinct selections in write/Commit ownership.

Commit/mutation qualification also requires a separate deterministic live SDK
stopping/refusal proof. Deadline, lost-result and checked-release proofs do
not substitute for stopping. Recheck impacted known-C1/local-C5 held leases,
fixed response Budget, metadata/token authority and cleanup at the final
product source; reuse unaffected exact-identity proofs explicitly.

## 4. Baseline selection and resource scope

Use whole source/profile cohorts from the frozen baseline inventory, never
the fastest run from different experiments. The old nine matrix rows remain
functional PASS/numeric INELIGIBLE; the earlier algorithm control
`48b51e874a41b3e1e6c6661e145316df8b408f07` remains NOT_RUN until actually run.
Declare any matched control before both arms; a new candidate artifact/harness
requires matching identities, limits, boundary and cache method. No speedup
denominator follows from an unrelated Init/history surface.

Additional host memory qualification stays deferred to [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283).
Do not turn it into a repeated unit/probe campaign. Available phase-local
observations may be reported; lifetime peaks retain their label. Actual
Budget, private quota, physical blocks, precharge, pins, transfer, refund and
failure custody remain correctness requirements. Missing host/cache proof
keeps numeric eligibility explicit and does not fabricate a memory leak or
block independent functional/harness engineering.

## 5. Two iteration loops and delayed regression

```text
family n / frozen committed code+method
    -> one sample per selected case/arm + required separate verifier
    -> append round facts, failures, cause evidence and next action
    -> commit report / publish commit-linked #286 performance comment
    -> FAIL? fix shared cause, commit meaningful new identity, repeat n
    -> correctness AND required speed/storage gates PASS?
         freeze checkpoint -> check families 1..n-1 once
         earlier regression? fix cause, re-establish affected n gates,
                             then affected earlier checks
         checkpoint PASS -> n+1; continue through family7
```

Every invocation, including setup failures and partial/verifier/cleanup misses,
gets a fresh output path and an [experiment record](EXPERIMENT-LOG.md).
Record exact stats and verdicts for correctness, hard metric, numeric
eligibility and cleanup separately. Missing counters are unavailable, never
zero. The Phase A port excluded donor Host timing/status aggregation; adapt
the observer explicitly rather than claiming absent counters.

Commit a patch before its official sample, then commit its result report.
Every round gets an actual report/fix commit and a #286 comment with measured
source, commits, family/case stats, gates, issues and next fix. Batch a declared
family selection in one round report, retaining a row for every invocation;
do not add empty product commits or overwrite a prior attempt.

**Do not rerun successful unit tests whose behavior/inputs are unchanged.**
After a patch, use only the smallest affected check. **Do not rerun families
1..n-1 until n passes correctness and its required speed/storage gate.**
Reuse identity-matched PASS receipts where allowed, recording the omission;
an unchanged arm cannot be resampled merely to populate a checkpoint. A new
relevant source/method needs a prospectively declared regression receipt.
Required owning tests/examples/fmt/Clippy/boundary checks run once at final
changed source; no CI or retired preflight substitute.

Keep commands under3min and each family within its own frozen limits. Reuse
sealed builds/images and validated input copies outside timers; `--setup clone`
applies to prepared post-init cases, not Init or InProcess history. No warm
credit, best-of, unchanged-arm replay, extra workers, workload/oracle change
or enlarged quota/deadline. Preserve all original FAIL/partial evidence.

## 6. Completion and handoff

Use the [next-agent prompt](HANDOFF-PHASE-B-20260929.md). Publish a complete
family/gate/status matrix, committed round index, raw receipt hashes and
reproduction commands. `core/target/` evidence is local-only; publish compact
receipts/manifests and source-pinned reports, never a fictitious GitHub raw URL.

If a necessary contract or host decision prevents a mandatory gate, name the
exact missing decision with source evidence and next executable step; work
independent parts meanwhile. Do not mark that family or Phase B complete.
Final recommendation must cover all seven families, remaining issues and
explicit owner dispositions. Keep PR#285 draft/unmerged and issues open until
separate owner action; this assignment does not authorize a merge or release.
