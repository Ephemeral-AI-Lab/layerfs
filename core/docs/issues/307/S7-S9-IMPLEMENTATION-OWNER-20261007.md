# S7/S9 implementation-owner continuation

> Status: Current planning checklist; no release candidate exists.

The human requested a handoff after E04 closure. The
[E04 stopping record](HANDOFF-E04-CLOSED-S7-S9-20261007.md) closes that
functional checkpoint on both profiles. A later human-dispatched assignment
resumed at `490c3ab3a71823268e476e6affdcfbf0c6f9fef0`: checkpoint 4 below is
now implemented ([report](S7-COMPLETION-OWNERSHIP-20261007.md)); later rows
remain tentative until each receives its own deepest-file plan.

The subsequent [owner direction](checks/e04-disposal-20261007/63-owner-disposable-development.json)
selects Disposable as the sole active development verification profile. Before
any next test/proof/diagnostic/performance selection, make mixed-profile external
tests/harnesses select Disposable and explicitly defer Durable-only execution.
Do not add product test branches or remove Durable support. Outstanding Durable
execution is NOT_RUN — deferred by owner for Disposable-only development; retain
the earlier both-profile E04 source identities/results and do not rerun them.

Human-dispatched assignment starts in the primary checkout at
`a08bbe39deb7d892a7a5a69b0daa001ab7e53774`, tree
`c2b1a7c0662120803525dd43f518607441b827b0`, core/crates tree
`d97312c619f0726ff863e05b9dbe450759cce0e1`. Existing pending work is retained.
The [reconciliation](S7-S9-EXIT-RECONCILIATION-20261007.md) remains the priority
ledger; [primary validation](../303/07-implementation-validation.md), owner
instructions and [performance acceptance](PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md)
govern. No other chat or goal is resumed or messaged.

## Checkpoint 1: original E04 and resource custody

Reuse the existing initialized Owner, Workspace mutation/provider ports, SDK
Runtime/Sessions/Supervisor, native Bridge, public Project Init/Store/Save/fork,
closed 16 MiB base and 4,096,000-byte replacement inputs, and current retained
receipt validator. No new importer, database, product algorithm or generic
measurement runner is planned. The diagnostic remains uncontrolled-cache,
zero E1 samples, qualification NOT_EVALUATED and whole E2 INCOMPLETE.

| Responsibility | Exact existing or pending files | Named exit and intended change |
| --- | --- | --- |
| Original attachment custody | `core/crates/layerfs-daemon/src/upstream/owner.rs`, `types.rs`, `mod.rs`; external `tests/upstream.rs` | E2: preserve pending original Open Completion and Binding/Policy Messages; reuse four host/four Linux passes if source remains identical |
| External bounded resource output | `core/benchmark/fs-bench-pro/shared/evidence_resources.py`, `tests/test_evidence_resources.py` | E3 prerequisite: verify final raw FileIO/prefix/close correction; reproduce simultaneous input read/close failure before a minimal first-cause correction |
| External retained E04 validator | `core/benchmark/fs-bench-pro/shared/evidence_jobs.py`, `tests/test_evidence_jobs.py` | E2: reconcile actual Open input route (absent until Open succeeds); bind actual Docker argv/CID/inspect/mount evidence; add negative coverage before correction |
| External consumer/host vehicles | `core/crates/layerfs-daemon/examples/e2_writes.rs`, `e2_writes_host.rs`, and `examples/e2_writes/{driver,fixture,records,ports,streams,oracle,outcomes,host,host_fence}.rs` | E2: source-accurate host/Linux builds and static checks before one original diagnostic; independent full-byte/EOF/membership/old-root/binding oracle |
| Source/API notes | `core/docs/architecture/47-daemon-upstream.md`, `57-external-resource-observations.md`, `58-e04-original-write-receipts.md`, `README.md` | Describe actual retained custody and observation scope, preserve historical check identities and failures |
| Evidence | `core/docs/issues/307/checks/e04-writes-20261007/` | Append only: reconciliation/source hashes, failing regressions, corrected scoped checks, build/binary/image/input identities, first execution and retained validation |

Tentative new files are only fresh receipts and a checkpoint-specific ignored
preparation/launch helper under `core/target/cluster2-307/`, if the existing
preparation mechanisms need E04-specific composition. The helper orchestrates
the named compiled examples; it supplies no product mutation or substitute
validation algorithm. Enumerate its actual inputs before execution.

All builds/tests/measurements are serialized by the integration owner. Owned
review agents may author non-overlapping tests/helpers; they run no checks.
Every test has an explicit wall stop at most 120 seconds, syntax/no-run build
first, fresh receipts and one ordinary construction producer. Native Init keeps
its independently declared four-worker profile. Earlier failed build15 and
Clippy20, the 80-case validator and 13-case resource receipts remain immutable.

## Following independent work

### Checkpoint 2: backing capability and original guest artifact

Parent is local `7409c2922b98aefb0adc589234015feeac592102`. E04 receipts
68/69/75 prove the selected host share's additive range-allocation failure.
Do not execute that route again. Reuse the existing exact-range Allocation,
one Overlay startup owner, safe locked nix filesystem observation, real native
guest storage and existing E04 fixed workload/oracle. No fallback or new database.

| Owner / exact file | Change and named exit |
| --- | --- |
| Overlay `src/database/startup.rs` | E2/E3: unconditionally reopen read-only/O_NOFOLLOW, verify same regular-file identity/link count, record each open/metadata/fstatfs attempt and observed type; refuse evidenced magic 0x6a656a63 before Allocation/admit/SQLite |
| Overlay `src/contract/error.rs` | E2: typed UnsupportedFilesystem carrying observed Linux magic; original probe error remains original Io |
| Overlay `tests/startup_cost.rs` | E2: actual independently observed descriptor type on success/schema failure, no invented observation before create failure or on macOS |
| Tentative new Overlay `tests/allocation_filesystem.rs` | E2: explicitly selected owned host-share refusal, create/reopen/probe once, two descriptor identity reads, zero SQL/allocation/SQLite-open and retained empty artifact |
| Daemon `examples/e2_writes/{driver,streams,outcomes}.rs` | External E2 harness: versioned startup capability observations while retaining exact original-operation behavior |
| Existing harness `shared/evidence_jobs.py`, `tests/test_evidence_jobs.py`; tentative focused `shared/evidence_backing.py`, `tests/test_evidence_backing.py` | External E2/E3 evidence: preserve v1 and add independently bound guest backing/mapping v2, original guest stat versus exported byte copy |
| Ignored `core/target/cluster2-307/run_e04_owner_diagnostic.py`; tentative narrow `e04_native_backing_observation.py` | Declare a new owned native guest volume before start, retain actual filesystem/container/mount custody, no `/work` assumption for database argument |
| Architecture 35/58, S6 reservation addendum, current reconciliation/audits | Root-owned docs: narrow filesystem refusal and exact proof/qualification scope; no universal qualification of other filesystems |

The capability worker owns only Overlay's two source files and two external test
files. Separate workers own the three named E04 Rust vehicle files plus retained
validator/tests, and the two ignored launch/observation files, respectively.
Root owns specification, documents, integration review and all execution.
No allocation-algorithm change, lifecycle cap increase, new dependency, unsafe
dependency patch, SQL schema change or S10 pipeline work is part of this package.
The new mapping/startup evidence contract is committed before its harness work.

### Checkpoint 3: explicit original disposal and logical bind witnesses

Reuse Supervisor's existing explicit fence/application-control pattern, original
Binding Message/Delivery, Upstream fence/join, finite raw control I/O and the
same independent E04 oracle. The prospective
[disposal v3 contract](../../../../docs/roadmap/0.1/0.1.7/cluster-two-e04-disposal-v3.md)
is committed before authoring this correction. No product/Bridge error is relaxed.

| Owner / exact files | Change and named exit |
| --- | --- |
| Host worker: existing Daemon `examples/e2_writes/{host,host_fence}.rs`; tentative shared `examples/e2_writes/control.rs` and external `tests/e04_disposal_control.rs` | External E2/R1 prerequisite: exact received/delivered final Binding, one explicit host fence, real joins/release before acknowledgment; bounded original control publication/read |
| Consumer/validator worker: `examples/e2_writes/{driver,oracle,streams,outcomes,fixture}.rs`, `examples/{e2_writes,e2_writes_host}.rs`; harness `shared/{evidence_jobs,evidence_backing}.py`, `tests/{test_evidence_jobs,test_evidence_backing}.py`; tentative focused `shared/evidence_disposal.py`, `tests/test_evidence_disposal.py` | External E2: retain original final Message identity, coordinate after actual Close/Gone, require own later native fence; new versioned fields/argv and logical input/mount proof, preserve v1/v2 |
| Vehicle worker: ignored `core/target/cluster2-307/{run_e04_owner_diagnostic,e04_native_backing_observation}.py` | External E2/E3: pre-host control seal, explicit actual bind/input/probe witnesses, preserve verified inspect state beside later evidence refusal |
| Root: specification, architecture58, reports/reconciliation/audits/check receipts | Source/build/test/fixture/profile/custody identities and exact original success/failure scope |

Workers coordinate the shared control interface before edits and own disjoint
files. Tests remain outside src; tentative new control tests may be placed under
Daemon tests only with a real public/external vehicle scope. No new product API,
additional constructor, database, allocation window or S10 pipeline is proposed.
After this E04 correction, the separate per-job family/capacity ownership change
must receive its own deepest-file plan and exact unchanged-capacity proof.

### Checkpoint 4: original SQL families and credited completion ownership

Status: **implemented** after `490c3ab3a`; see the
[checkpoint report](S7-COMPLETION-OWNERSHIP-20261007.md) for layouts, charge
arithmetic, proofs, retained failures and exact LOC. The earlier tentative
table is replaced by the plan that was refined before editing and then built.

Reused unchanged: Overlay `DatabaseWork`/`StatementKind`/`StatementWork`
snapshots and every SQL statement; the fair lane rotation and class ordering
rules; `Command`, `Response`, `OwnerError::Unattempted`, the declared reply
sizes and the 512-byte allowance; the shared owner wake for the owner thread.
Caps stay 8 MiB, 64 KiB, 16 namespaces, two lifecycle slots.

| Exact file | Change | Requirement |
| --- | --- | --- |
| New Daemon `src/service/job_sql.rs` | `JobSql`: exact per-family rows, `family`/`total`/`expanded`, overlap-free accumulation; the family counts used for charging | E2 |
| Daemon `src/service/observations.rs` | `JobWork.sql` becomes `JobSql`; the receipt is no longer `Copy` | E2 |
| New Daemon `src/service/completion.rs` | Typed completion cell, single publisher, one-shot `Pending`/`Completion`, thread-park wake, the stage-maximum `charge` | E2/E4 |
| Daemon `src/overlay/queue.rs` | Boxed jobs in pointer queues over a lane table of fixed startup capacity; `scheduler_bytes`, `queued`/`peak_queued`, receipt-overrun counters; one `refuse` path for unattempted outcomes | E2/E4 |
| Daemon `src/overlay/owner.rs` | One connection delta per turn for both receipt and aggregate, published before visibility; uniform failure counting; admission over the lane table and scheduler bytes; startup validates the real lifecycle charge | E2/E4 |
| Daemon `src/overlay/credits.rs`, `commands.rs` | Credit carried by the cell with honest overrun charging; `charge` delegates to the stage formula | E2 |
| Daemon `src/{lib,service/mod}.rs` | Declarations and reexports only | — |
| External Daemon `tests/completion_ownership.rs`, `tests/completion_storage.rs` (new); `tests/{job_cost,indexed_scratch,edit_backing,upstream}.rs` | Family reconciliation on every path, 32-slot saturation, one-shot/wake/stop, counting-allocator storage evidence; existing bodies read `sql.total()` | External E2/E4 proof |
| External `examples/e2_writes/records.rs` | Serializes `sql.total()` into the unchanged historical schema | Compatibility only |
| Architecture 21 and 36, this plan, the audits and reconciliation | Implemented source scope and limits | Docs |

Not built from the tentative list: no boxing of large `Command`/`Response`
variants was needed once stages stopped coexisting, and no change to the E01/
E04 serializer schema or the Python validator was made. That successor is the
next operation-accounting change below.

### Subsequent priorities

| Requirement | Next concrete work | Dependency retained |
| --- | --- | --- |
| E1 | Complete actual route/source/build/input/observer registration, numerical authorities/control formulas, schedules and feasible Q05 budget | No candidate sampling while required identities/metrics are unavailable |
| E2 | Done: per-original-job families and receipt/queue/held-result charging. Next: a family-carrying job-receipt successor schema in the external E01/E04 serializers and validator; provider demand facts/IDs and whole-operation copies at the Workspace/SDK port boundary; statement template/bind/EXPLAIN correlation; queue-only bytes; exact eligible debt | Native request/reply/open/lookup accounting belongs to S8 |
| E3 | Integrate and calibrate actual phase baseline/interior/final observations, ownership, ordered inventories, gaps, clocks and observer overhead | Missing physical/kernel domains remain explicit, never guessed zero |
| E4 | Register finite class/Workspace arrivals, runnable progress, pressure/headroom and live/final-idle automatic debt drain | Sequential E04 writes are not this proof |
| R1 | Finish actual application supervision and complete owner accounting | Reuse independent socket workers and existing fair provider units |
| R2 | Owning Content root topology qualification and shared provenance, with dangling serial/alias/cycle/reachability/count/context negatives | Shared K2 prerequisite only; no hidden repeated-bind walk |
| R3 | Actual connection/consumer/host restart boundaries, original surviving results and terminal unknown | No crash resolver, guessed Finish/abort/replay/deletion |
| R4 | Owning remote Save/FinalizedConsumer plus separate same-Save reader and two interleaved Saves; actual API-core/Sandbox boundaries | No manifest-only activation or legacy server |
| Q1 | Real source-removed complete roots on both profiles, full native scale/large dense/sparse/resource proof | Small component fixtures do not close Q1 |

Before each following checkpoint, replace this high-level next-work row with
its deepest-file plan and actual reuse/evidence scope. S8 supplies S7's exact
native residual; pinned fuser public INTERRUPT is a dependency capability issue,
not authority to patch beyond signed timestamps. Captured namespace normalization
and full capture→Save→Stage→history→install remain S10 unless a named S9
prerequisite requires the shared correction. S11–S13 and reference retirement
are outside this assignment.

Protected untracked resume/speed notes, unrelated processes/worktrees, four
identified running containers and all historical evidence remain untouched.
No remote push, release or deployment is authorized. Each local commit needs
exact parent/staged/committed production LOC with the unchanged counter and
separate core/reference/active/excluded subtotals.


## Independent E3 admission correction

Root owns only `core/benchmark/fs-bench-pro/shared/evidence_gates.py` and
`tests/test_cluster_two_registration.py` for this focused change. Reuse the
existing phase/sample arithmetic and reject a finite sampled inventory labeled
continuous. Reproduce the missing refusal before correction; no new observer
protocol or metric value is invented. Architecture57/reconciliation and fresh
`checks/e3-sampled-peak-20261007/` receipts retain E1/E3 scope. This closes an
admission loophole, not phase calibration, continuous coverage or an E1 sample.
All broader resource integration remains a separate implementation requirement.


## Owner-directed remaining package organization

After E04 functional closure, the existing E1–E4 ledger is executed as three
focused packages: operation accounting (families, provider demand/copies,
queue/held allocations, waits and eligible debt); resources/cache (phase memory,
I/O/backing/cache and actual calibration, only permitted bounds); service
acceptance (finite arrivals, numerical criteria, progress, pressure and automatic
live/idle drain). Checkpoint4 is the first operation-accounting change. Reuse
run-level identities, compact referenced events and fixed aggregates; detailed
per-job tracing is selected only where the named accounting requirement needs
it. No generic profiling platform or unchanged E04 resampling is authorized.
