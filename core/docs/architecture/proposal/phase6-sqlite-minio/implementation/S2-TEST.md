# S2 test and comparison plan

Status: Current planning checklist; no release candidate exists.
Owner-directed update, 2026-10-02, for [#296](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296).
All new S2 cases below are `NOT_RUN`. This document executes no checks, freezes
no candidate identity, and introduces no new runner. Read [S2 specification](S2-SPEC.md),
[existing SP1 test contract](sp1/test.md), [benchmark rules](../../../../../../docs/general/benchmark_rules.md)
and [report template](../../../../../../benchmark_agent_report.md).

Owner goal: pass the complete existing `sp1/test.md` contract, qualify the
simplified public Exec-to-Commit path including concurrent successor workload,
then cover all seven `fs-bench-pro` families and compare speed and storage against
a declared baseline. New case names here are prospective specification IDs,
not existing Cargo targets or CLI selectors. Publish parser-valid commands when
the owning external tests and versioned runner adapters exist.

## 1. Acceptance order and evidence custody

1. Inventory every existing `sp1/test.md` requirement and its exact receipt,
   relevant source/binary/fixture identity, proof coverage and omissions. Reuse
   qualifying unaffected evidence; close missing or changed integration proofs.
2. Qualify S2a final-state growth and atomic admission, then S2c shared engine,
   scope, lazy inherited state and lifetime. Existing C1/C2 algorithms remain.
3. Qualify the real SDK/FUSE/SP1/C5 path and S2b paged independent verification.
4. Qualify stable capture and successor activity while Commit is pending, with
   one construction producer. This is an explicit separate acceptance gate.
5. Freeze the seven-family selection and comparison matrix, then collect the
   complete declared campaign once. A reused historical proof or control is
   cited as reused, not emitted as a new sample.

At each final source checkpoint, run owning external tests once; scope Core,
adapter and harness checks from actual edits. Diagnose a red result from its
retained output/source, apply a fix, then run its covering command once. Do not
repeatedly sweep passing tests or families after unrelated edits. No CI,
preflight, replacement aggregate wrapper or new collector is authorized.

Each case records semantic, authenticity, timing-budget, storage, resource,
cleanup, custody and cache-eligibility verdicts separately. `PASS` on functional
proof does not imply a numeric speed PASS. All missing required proof remains
`INCOMPLETE` or `NOT_RUN`; missing telemetry is `UNAVAILABLE`, never zero.
The owner's requested speed/storage comparison remains open wherever an
applicable comparison lacks eligible, matched time evidence or complete storage
accounting. A functional seven-family PASS cannot silently close those numeric
gaps. Inapplicable metrics need a prospectively declared scope and reason.

## 2. Preserve every existing test.md requirement

The original contract remains normative for its fixtures, seed, independent
oracles, selection policies, routes, bounds and retained-history schedules.
This inventory prevents treating the 14 small component matrix cells as the
whole contract. Publish a requirement-to-receipt manifest with exact hashes and
source-scoped reuse justifications before declaring this stage passed.

| Existing requirement | Required scope; reuse or new proof | Current S2 disposition |
| --- | --- | --- |
| `sp1-reader-strict-v2` | Mixed FULL/PREFIX, multi-record Native, dependency/intermediate/final authentication, order/cardinality, SQL-only metadata/pool read with zero payload-provider reads | Reuse only exact qualifying component proof; changed S2 read/capture route needs affected proof |
| `sp1-writer-strict-v2` | Same-save and cross-pack eligibility, exact CAS, grouped packing, threshold `T-1 -> T -> T-1`, sealed costs/selections, retained dependencies | Component receipts alone do not prove public integration |
| Metadata pooling | Four 40-value leaves, FULL plus three deltas; ordinals; two-leaf private reuse; private/public visibility before cache answers; bounded SQL seals and no MinIO metadata | Keep every original fixture and visibility predicate |
| Logical provenance and dual-use IDs | Actual C1 file/attribute producers, both mandatory domain locations, reversed read order, omitted-domain refusal despite warmed other-domain cache, reference-use routing | Preserve exact canonical producer/oracle identities |
| `sp1-refusals-custody-strict-v2` | Stored corruption, malformed locators/chronology/role, depth/work policy, missing required bases, definite and Unknown PUT/SQL outcomes, first-locator and retained-base custody | Reuse unaffected owning scopes; extend changed capture/installation custody |
| `sp1-exec-history-strict-v2` | Three ordinary Exec processes and three retained Commits through actual SDK/FUSE/native/storage/C5; full old/new bytes and sealed content roots | Mandatory public witness; currently not supplied by component speed screen |
| Timestamp and filesystem-root oracle | Deterministic metadata only through supported ordinary operations; independent full root or explicit unsupported-field `INCOMPLETE` | Never copy candidate timestamps/root into expected state |
| Owning checks | Locked Core and independent adapter test/Clippy/fmt plus boundary checks as required by actual source changes | Publish exact commands/results and any non-run reason |
| History stride 10, 3, 1 | Original independent 157-state corpus, direct selected transitions, complete retained closure, matched encoded/physical accounting | Keep original schedules and bounds; no duplicate sampling for an unchanged relevant mechanism |

The history order remains 10 -> 3 -> 1. Stride 10 has 17 states / 561,010,345
logical bytes, stride 3 has 53 / 1,676,767,835, stride 1 has 157 /
4,936,693,030. Their registered complete/proof exceptions remain 60s/10s,
170s/20s and 170s/30s respectively. They do not enlarge ordinary S2 limits.
Stride 1 remains explicit run-only, not optimization input. Strict-split history
needs a versioned adapter in the existing runner; do not invent backend flags.
Historical all-object-SQL receipts are not strict MinIO/SQL evidence or matched
public SDK controls. Original storage FAILs, later accepted profiles and numeric
cache ineligibility all retain their original identities and verdicts.

## 3. S2a: final-state correctness and admission

Use real SQLite and backing files through ordinary public Engine/admission
interfaces, with external tests outside production source. Fixed-size handling
windows may flush/page; they must not become total-population caps. No guard is
raised merely to make a case pass.

| Prospective case | Workload and proof | Status |
| --- | --- | --- |
| `s2-create-handle-atomic-v1` | Saturate supported handle admission, attempt CREATE+issued handle; force genuine SQL quota refusal separately. Compare pre/post inode, name, handle, revision and backing inventory. Definite refusal has no side effects. | `NOT_RUN` |
| `s2-write-admission-v1` | Metadata preparation precedes backing creation. Independently force SQL capacity refusal, OS backing-create failure and OS backing-write failure. Prior accepted bytes remain exact; no unowned source. | `NOT_RUN` |
| `s2-write-unknown-v1` | Genuine uncertain SQL/backing completion through supported OS/SQL boundaries; retain source/operation ownership, refuse guessed resend/deletion and quarantine as required. | `NOT_RUN` |
| `s2-population-growth-v1` | Separate sealed fixtures at 513, 4,096 and 16,384 live nodes, including a directory with those child counts. Create/read/list and construct/publish through actual C1/C5; independently enumerate every name and byte. | `NOT_RUN` |
| `s2-create-delete-churn-v1` | 1,024 create/remove transitions with at most 32 live temporary names. Admission follows live indexed ownership, not a lifetime CREATE counter; no leaked rows/handles/backing. | `NOT_RUN` |
| `s2-dirty-growth-v1` | Change 513, 4,096 and 16,384 distinct inodes in separately declared fixtures. Commit all declared changes and retain/read previous state through actual C5. | `NOT_RUN` |
| `s2-final-span-growth-v1` | Existing file with 513, 4,096 and 16,384 disjoint surviving one-byte replacements separated by unchanged bytes. Exact ordered final edits flow through C1 `apply_edits`, no resident whole-edit vector or silent truncation. | `NOT_RUN` |
| `s2-final-span-overlap-v1` | A 10-byte base `abcdefghij`: `[2,6)=WXYZ`, `[4,8)=1234`, `[2,4)=QQ` gives `abQQ1234ij`. Then repeat 1,024 overwrites of a fixed range with presealed final bytes; reads and Commit depend on surviving state, not replay history. | `NOT_RUN` |
| `s2-size-gap-v1` | Append, truncate, regrow, write past EOF, shrink through a replacement interval and rewrite. Independent byte oracle covers zeros, size, surviving fragments and no reappearance of truncated bytes. | `NOT_RUN` |
| `s2-names-cursors-v1` | File/directory rename, replacement, unlink, open-unlinked reads and writes, paged readdir, cycle/type/nonempty refusals. Stable cookie/keyset semantics have no skip/duplicate and refusals preserve prior state. | `NOT_RUN` |
| `s2-quota-policy-v1` | No default application DB cap. Explicit quota derives actual page size/capacity and refuses truthfully. Bounded SQL journal/results/temp work and real resource observations accompany the proof. | `NOT_RUN` |

For population, dirty and final-span growth, the three derived IDs are
`<case-stem>-n513-v1`, `-n4096-v1`, and `-n16384-v1`, in that order, with
expected cardinality three per stem. Publish each refusal's distinct fault
subcase ID before execution; one fault per disposable copy. Other matrix rows
are one logical case each unless its frozen manifest explicitly enumerates
variants. Cardinality validation uses the final expanded manifest, not this
planning table or an implicit loop.

These are concrete functional fixture tiers, not timing-gate receipts. Freeze
exact topology, operation sequence, counts, oracles and commands before running.
If a retained C1 draft/format/backing gate blocks a tier, preserve that refusal;
do not shrink the workload, bypass C1 or claim S2 complete for that scope.

## 4. S2c: persistent engine, scope and backing lifetime

All scoped keys/queries cover `(Workspace, incarnation)` and the selected view.
Inherited opening is root/context setup plus lazy bounded acquisition, not full
namespace import. Indexed SQL facts remain authoritative; no custom population
pager or resident registry is added when SQLite provides the capability.

| Prospective case | Required result | Status |
| --- | --- | --- |
| `s2-engine-lifetime-v1` | Genuine database open/configure/schema counters show one initialization per daemon; 256 sequential Workspace open/close cycles create only small scoped contexts, without schema/pager recreation or lifetime-row leak. | `NOT_RUN` |
| `s2-lazy-inherited-v1` | Increasing inherited populations open without enumerating/importing every inode/name. Reads/listing lazily acquire required metadata and payload; base and local views remain exact. | `NOT_RUN` |
| `s2-scope-isolation-v1` | Two scoped contexts use overlapping names/inode/handle/cookie IDs. No lookup, span, capture, prepared result or cleanup crosses scope. This is not permission to enable concurrent multi-Workspace runtime. | `NOT_RUN` |
| `s2-incarnation-refusal-v1` | Close/reopen same Workspace name with new incarnation; stale handle/cookie/capture/install/cleanup requests refuse without touching the new context. | `NOT_RUN` |
| `s2-source-lifetime-v1` | Superseded/partially referenced sources, open-unlinked handles, capture/read pins and Unknown custody retain exact bytes. Known unreferenced sources reclaim in bounded batches; no deletion of committed history or shared payload. | `NOT_RUN` |
| `s2-short-sql-ownership-v1` | External stalled backing/provider I/O and live operations demonstrate that shared SQL ownership/statement lifetime ends before construction, codec, provider I/O and full cleanup. No lock spans an entire Exec or Commit. | `NOT_RUN` |

The optional retirement-table simplification needs no special test hook: verify
its observable reference/pin/custody behavior and indexed bounded selection.
A source row's eligibility index may replace a separate queue only when atomic
claiming prevents new-reference/deletion races and uncertain completion remains
owned. Do not claim physical reclamation merely because an SQL row disappeared.

## 5. Real public path and non-pausing Commit

Public workloads run unmodified ordinary shell/POSIX programs via
`workspace_api.exec`, real Linux FUSE, native authenticated catalog access, C1/C2,
MinIO payload-only storage, global SQL metadata and C5. No command recognizer,
SDK range-edit substitution, direct Store mutation or host preconstruction.
A command's unsuccessful exit does not imply rollback of its filesystem changes.

Add the following witness to the existing public SP1 witness, not a new service
or alternate benchmark framework. Freeze its ordinary script SHA, public calls,
barrier protocol, operations and independent bytes before candidate execution.

| Prospective case | Required observation | Status |
| --- | --- | --- |
| `s2-successor-data-v1` | Capture G containing `abWXYZghij`; while its Commit is held, successor write produces live `abWX1234ij`. Historical published G stays exact. Installation changes the base without clearing the successor; later Commit preserves successor bytes. | `NOT_RUN` |
| `s2-successor-names-size-v1` | While captured Commit is held, rename/unlink/truncate/regrow, continue reads through admitted handles and create new names. Captured graph stays exact; live successor and eventual installation keep all accepted operations. | `NOT_RUN` |
| `s2-pending-admission-v1` | Second Commit returns BUSY under one-pending-Commit policy while ordinary admitted live reads/writes progress. One construction producer; no second codec/construction lane. | `NOT_RUN` |
| `s2-successor-failures-v1` | Branch moved, cancellation at supported boundaries, definite provider/SQL failure, lost ACK and publication-known/install-failed. Capture/pins and successor state follow each typed known/Unknown outcome; no silent rebase, guessed retry or deletion. | `NOT_RUN` |

Hold Commit at a deterministic boundary using external real provider/OS I/O
coordination and causal acknowledgements, not sleeps or test-only product hooks.
One ordinary workload can already be running and coordinate its successor
syscalls with the external controller. If concurrent public Exec admission is
required to launch another command, first qualify that independent lifecycle
milestone; do not imply that a shared database alone enables it. A stalled
provider proxy must be declared in the functional test topology and cannot
supply a normal latency measurement.

Retain evidence that the successor syscall was acknowledged while captured
Commit was still pending. Verify captured bytes/metadata, live successor bytes,
retained history, pins, installation and cleanup separately. Row-version versus
persistent-view-root storage is not frozen here; a capture row above mutable
in-place rows is insufficient. Indexed visibility must not traverse an unbounded
chain of prior generations. Simultaneous multi-Workspace/multi-Exec throughput
needs its own scope, admission and proof; these tests do not claim it.

The simplified-route proof also records actual database/schema initialization,
capture-context handoffs, intermediate output collections, qualified catalog
resolution RPCs and group-authorization calls. A combined resolution must retain
every descriptor/placement/use/domain/scope predicate. A bounded group wave may
replace per-value remote authorization only after its catalog eligibility and
epoch lifetime is proved. No cached answer bypasses refusal predicates. The
prior 70-call metadata observation and proposed 52/13-call reductions are
separate count-driven diagnostic targets, not an S2 speed guarantee or frozen
new sample selection.

## 6. S2b locality and large inherited load

At 513, 4,096 and 16,384 namespace entries, apply the same shallow one-file edit
with identical span, content seed and public-call topology. Use indexed keyset
pages and an independent paged expected catalog/work queue, with complete
namespace and declared byte coverage. Seal expected state before execution;
candidate roots/output cannot generate their own oracle.

Record SQL calls/VM steps/rows/full scans/sorts; inode/name/extent/dirty pages;
construction and reused objects; bytes read/scanned; native RPCs; GET/PUT counts
and bytes; capture/bootstrap/install/retirement work. Counts must track affected
paths and necessary structural work, not full namespace size or write history.
Tree height can affect work; no invented constant-time requirement.

A 3 GB / 100,000-file inherited namespace is the explicit later target, using a
sealed independently acquired corpus with exact bytes/count/root and lazy base
opening. Freeze the decimal/binary byte total in its manifest. Qualification
requires the smaller proofs, registered large workload/runner support and fitting
budgets; currently `NOT_RUN`. It is distinct from frozen DeepSeek retained-history
states. A full local 3 GB creation/rewrite exceeds the unchanged 1 GiB private
backing policy and is not selected or silently enabled. Bulk scan, bulk rewrite
and localized edit are different cases. An arbitrary command that reads all
bytes or enumerates all files must pay that necessary work.

## 7. Seven-family final campaign and baseline prerequisite

After the functional gates, register every selected case from the seven-family
Core fs-bench-pro plan, its current registration state, exact operation surface
and selected order. Use the existing runner and definition owners. Families
include public SDK, component controls and real SDK/FUSE workflows; these must
remain separate in reports. Do not interpret the old root runner's retired
families as the current seven-family Core selection.

Owner selected Core/Phase B [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286)
as the primary baseline on 2026-10-02. Its
[seven-family checkpoint](../../../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
and [exact reuse map](../../../../issues/286/experiments/20260930-seven-family-reuse.json)
are mixed-source scoped evidence, not a homogeneous fresh-control campaign.
All historical numeric latency remains `INELIGIBLE`. The selected product-final
baseline is `a54cda244f4ad4f99b87a852c6dbe0395633c3d3`; reviewed handoff
`2135e2dc186c0bad7fdd1b7f42ca8e5bcc26cc27`, tree
`284faeb731589310316e9aaf7436f108d97c3e55`, records the relevant equivalence in
[merge acceptance](../../../../issues/286/MERGE-ACCEPTANCE-20260930.md).
The historical cohort report `6599e64a60ee99e17bb9080c2cdcba019676a432` is not
one baseline sample. The per-case baseline
identity/comparability matrix is still a preregistration prerequisite: pin each
baseline source/binary and receipt hash; each selected case's fixture/oracle/cache/topology/timer/acknowledgement;
comparison formulas, budgets and numerical gates. Different baseline surfaces
remain descriptive historical controls, not paired speedups. Show historical
raw times side by side as diagnostics. A speed ratio requires a fresh or
qualifying identity-matched baseline with eligible cache/timer/topology evidence;
this plan does not upgrade #286 historical timing. Old all-object-SQL
storage needs matched accounting before comparison with strict MinIO/SQL.
Baseline missing/incompatible means that comparison is `NOT_RUN`/`INCOMPLETE`;
never choose a favorable receipt after seeing candidate numbers.

A seven-family result contains all active selected siblings and every registered
failure, `INELIGIBLE`, missing, deferred and explicitly unregistered row. Existing
owner-deferred deepest-270 successor speed, absent selectors and run-only history
remain visible. This request does not invent a selector or erase prior deferral;
freeze any new promotion and its owning capability/contract before collection.
The final result cannot claim all seven families PASS if a selected required
member remains unsupported, unrun or failed.

| Family | Core registry owner | Selected comparison scope and retained limitations | New campaign status |
| --- | --- | --- | --- |
| 1 `init_namespace` | `families/init_namespace.py` | Four release public SDK tiers: default 100/1,000 plus explicit 10,000/100,000. The 100,000-file fixture has 500,000,000 bytes, not the separate 3 GB target. Lite proof is not full-payload proof. | `NOT_RUN` |
| 2 `history_retention` | `families/history_retention.py` | All three v4 strides: 17/53/157 states, including explicit stride 1; independent retained roots and versioned strict-split adapter required. Historical allocation profiles remain separate. | `NOT_RUN` |
| 3 `workspace_write` | `families/workspace_write.py` | Nine append/dispersed/repeated x 100/512/4,097 public SDK/FUSE cells, full old/new bytes; original 15s/25s bounds. | `NOT_RUN` |
| 4 `workspace_commit` | `families/workspace_commit.py` + `families/workspace_commit_native.py` | Five SDK + nine native functional controls, including 8,192 writes, each with its own route. 10,240 and original profiles retain registration/owner disposition. | `NOT_RUN` |
| 5 `workspace_namespace` | `families/workspace_namespace.py` | Ten native + five SDK namespace/move/deep/retained/refusal cells. 270-level correctness is selected; extreme-depth optimization remains deferred. Component controls are not SDK speed. | `NOT_RUN` |
| 6 `workspace_mutations` | `families/workspace_mutations.py` | Three native + four current SDK mixed/retained/refusal/explicit-recovery profiles. Original exit-7 dirty-discard FAIL remains distinct from recovery v2. | `NOT_RUN` |
| 7 `workspace_shell_package` | `families/workspace_shell_package.py` | Eight SDK: mixed, 4 KiB, repeated one-byte, many 128/129/257/1,025, retained 129 G1/G2; one native progress/custody control. Original failed-command/no-commit NOT_RUN stays visible. | `NOT_RUN` |

Current evidence details remain in [Family 4](../../../../issues/286/FAMILY4-CHECKPOINT-20260930.md),
[Family 5](../../../../issues/286/FAMILY5-CHECKPOINT-20260930.md),
[Family 6](../../../../issues/286/FAMILY6-CHECKPOINT-20260930.md),
[Family 7](../../../../issues/286/FAMILY7-CHECKPOINT-20260930.md) and
[Family 7 selection](../../../../issues/286/FAMILY7-SELECTION-20260930.md).
Explicit-only cases require explicit selection; a group alias alone is not a
complete campaign. Historical Family 4 native full-pin used its own 60s profile;
it cannot become ordinary SDK speed or justify extending a new ordinary case.
Preserve that scope or preregister a new fitting contract. The selected
270-component correctness workload needs compatibility review against the
strict canonical path API's 256-component capability; do not shrink it to 256
and claim the original case passed. Unsupported selected capability blocks its
completion. Owner-deferred 10,240 writes and failed-command/no-commit remain
visible with their actual status unless a new prospective scope promotes them.

The committed campaign must expand this table into exact IDs, versions, row
counts, parser selectors and per-case proof coverage from those definition
owners; this table does not itself register new selectors. Strict-split adapter
work belongs in the existing runner and family modules. Treat changed operation
or acknowledgement as a new profile instead of laundering it into an old row.

No measured campaign starts until a committed manifest pins source/tree/product,
compilation/dependency/harness/report seals, release binary/image, root ARMv8
flags, provider/database profile/schema/wire, seed, workload/oracle hashes,
ordered cases and arms, setup/cold policy, numeric gates, verifier coverage,
allowed exceptions, resources and exact reproduction commands. All new campaign
rows remain `NOT_RUN` in this planning update.

## 8. Speed, storage and resource accounting

Use one sample per selected case/arm, seed/repetition 1 where registered. No
unchanged resampling, best-of, n3 or dropping inconvenient cells. Clone closed
validated masters with independent writable byte copies for post-init cases;
fresh setup is reserved for initialization/fresh-output histories. Reuse setup,
build/image seals and identity-matched unaffected PASS proofs; never reuse
measured work, a mutated sample, live reader state or warmed selected ranges.
Hold the per-worktree lock and use only owned writable Cargo targets. No local
build overlaps a timed phase. All construction uses one producer and exports
`LAYERFS_CONSTRUCTION_WORKERS=1`; legitimate namespace Init workers are the
existing exception. New SDK Init uses locked release binaries only.

Ordinary complete performance commands stay <=15s; only prospectively listed
exceptions can use <=25s. Ordinary separate proof stays <10s, with the existing
9.5s S2/SDK bound where registered. Retained-history exceptions keep their
original explicit bounds; no extension, worker increase, workload shrink or
moved timed work turns a miss into PASS. Report first-use setup/build separately.

Per row retain raw operation and complete-command nanoseconds, nested SDK/Server/
daemon/FUSE/transport spans and counts, separate proof and cleanup. Do not add
overlapping spans or place verification/digest/reopen/oracle work in performance.
If required cache state is unknown, unequal or resident after declared
invalidation, numeric speed is `INCOMPLETE`/`INELIGIBLE` even with functional PASS.
Clone is not cold; source/recent-write cache never credits Commit or another phase.

Comparable encoded bytes:

```
MinIO file-payload encoded pack bodies
  + SQL immutable metadata/mapping/pool encoded bodies
```

Comparable retained physical allocation:

```
exclusive actual provider allocated bytes, INCLUDING payload objects and
    provider data/control overhead
  + exclusive global SQL/C5 file allocation, INCLUDING immutable metadata bodies,
    locator/use/dependency/candidate/pooling/history/publication indexes and overhead
  + distinct persistent Workspace engine allocation (reported separately as well)
```

Report MinIO object lengths and each SQL table/body's encoded bytes separately
from allocated provider/SQL file totals. Metadata body bytes already included
in SQL file allocation are not added again; object bytes already included in
provider allocation are not added again. Shared-file/table attribution must
reconcile to the physical owner total, not sum overlapping estimates. Link the
original Family 2 encoded-body and accepted-allocation operands from the
[existing contract](sp1/test.md#4-stride10-stride3-stride1-after-the-small-gates)
and [historical baseline](HISTORY-STORAGE-BASELINE.md).

For matched eligible arms, retain raw operands and compute:

```
time_delta_ns = candidate_ns - baseline_ns
time_delta_percent = 100 * time_delta_ns / baseline_ns
allocated_delta_bytes = candidate_allocated_bytes - baseline_allocated_bytes
allocated_delta_percent = 100 * allocated_delta_bytes / baseline_allocated_bytes
encoded_delta_bytes = candidate_encoded_bytes - baseline_encoded_bytes
logical_per_allocated = declared_logical_bytes / total_retained_allocated_bytes
```

A zero/absent baseline denominator yields unavailable, not a fabricated ratio.
Do not compute a product speedup from historical ineligible or unmatched times.

Measure each physical object once; distinguish logical, encoded, apparent and
allocated bytes. Retained bases, duplicate/alternate/abandoned/Unknown objects
are included by the frozen retention profile. Do not compare MinIO payload-only
bytes to historical C2+C5 total `st_blocks*512`, or automatically inherit an old
10% tolerance for the new provider topology. Publish raw deltas and formulas;
roughly 50% speed loss for roughly 5% storage recovery is not acceptable under
repository policy.

Private backing/spool/temp high-water, host RSS/CPU, daemon anonymous/file cache,
MinIO process/VM/storage and verification-only resources are separate domains.
A lifetime cgroup peak is not a phase peak. Bounded heap/pager/codec windows do
not prove combined owner memory or excuse file-size-proportional cache/spool.
Unavailable required attribution prevents its resource PASS; quotas are policy,
not observed memory. Final reports use the family-specific tables in the
repository report template and include every nonpassing/unrun disposition.

## 9. Completion record

The checkpoint must contain the requirement-to-receipt manifest, exact covering
check commands, complete selected family/case cardinality, baseline comparison
matrix, raw/report hashes, source/LOC per commit, cleanup/retention disposition
and all remaining C1/format/resource/concurrency gates. Documentation updates
alone do not establish any new test, speed, storage or concurrency PASS.
