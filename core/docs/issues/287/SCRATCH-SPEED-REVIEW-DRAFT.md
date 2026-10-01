# Scratch construction and eight-module speed evidence review

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Read-only source/evidence audit at published
> `79639cc6f58ae78eeb1fb638996b1d6948767215`, 2026-10-01.
> Effective-graph implementation is still the current checkpoint. Finish and
> publish that checkpoint, then pause further implementation as directed by the
> owner. The accompanying checkpoint records the final source seal; its published
> commit link belongs in the external issue update. No connection reuse, small-path product change, benchmark run,
> build, timing, new runner, receipt rewrite or issue update was performed here.

## Evidence inventory and scope

[runner.py](../../../benchmark/fs-bench-pro/runner.py) imports eight family
modules. The [seven-family checkpoint](../286/SEVEN-FAMILY-CHECKPOINT-20260930.md)
numbers seven report groups because `workspace_commit_native` is Family4's
component lane. There is no separately registered Family8 performance module in
this inventory. This review covers all eight modules without inventing an eighth
numbered campaign or restarting the completed PhaseB assignment.

| Existing module | Report group and operation | Retained evidence reviewed |
| --- | --- | --- |
| `init_namespace` | F1, public SDK Project Init | [r045](../286/experiments/20260930-init-regression-r045-receipts.json), with explicit larger tiers still described by [r041](../286/experiments/20260930-init-100000-v4-r041.md)/[r042](../286/experiments/20260930-init-10000-v4-r042.md) |
| `history_retention` | F2, InProcess C1/C2/C5 history component | [r046](../286/experiments/20260930-history-regression-r046-receipts.json)/[r047](../286/experiments/20260930-history-stride1-regression-r047-receipts.json) |
| `workspace_write` | F3, SDK/host Server plus Linux FUSE write matrix | [r063](../286/experiments/20260930-workspace-write-reconciliation-r063-receipts.json) and explicit [r044 cache audit](../286/experiments/20260930-workspace-write-r044-cache-audit.md) reuse scope |
| `workspace_commit` | F4 public SDK retained Commit lane | [r062](../286/experiments/20260930-workspace-commit-fast-checkpoint-r062-receipts.json), [Family4](../286/FAMILY4-CHECKPOINT-20260930.md) |
| `workspace_commit_native` | F4 native component controls; SDK latency is N/A | [r061](../286/experiments/20260930-workspace-commit-native-reconciliation-r061-receipts.json), [Family4](../286/FAMILY4-CHECKPOINT-20260930.md) |
| `workspace_namespace` | F5 native controls and public SDK namespace operations | [r066](../286/experiments/20260930-workspace-namespace-sdk-r066-receipts.json), [Family5](../286/FAMILY5-CHECKPOINT-20260930.md) |
| `workspace_mutations` | F6 native and SDK mixed mutations/recovery | [r070](../286/experiments/20260930-workspace-mutations-sdk-recovery-r070-receipts.json), [Family6](../286/FAMILY6-CHECKPOINT-20260930.md) |
| `workspace_shell_package` | F7 package/many-file SDK cases and native custody control | [r078](../286/experiments/20260930-workspace-shell-package-sdk-checkpoint-r078-receipts.json)/[r079](../286/experiments/20260930-workspace-shell-package-native-checkpoint-r079-receipts.json), [Family7](../286/FAMILY7-CHECKPOINT-20260930.md) |

The original raw roots remain under
`/Users/yifanxu/.codex/worktrees/issue286-phase-b/layerfs/benchmark-results/fs-bench-pro/`.
No files there were modified. Ten retained manifest file SHA256 values were
read and matched their committed compact indices: r045, r046, r047, r063, r062,
r061, r066, r070, r078 and r079. The two r045 raw receipt SHA256 values also
matched the compact index. This was not a fresh verifier or a full rehash of
every manifest member. The compact indices retain the exact commands, source,
artifact, image, fixture, proof and manifest identities.

All referenced seven-family numeric speed scopes remain **INELIGIBLE/open**.
Functional/command-budget success and storage tolerance do not turn those rows
into numeric latency admission. Native component command walls include in-child
oracles and cannot be compared to SDK Commit calls. The older source/receipts
are historical evidence, not measurements of issue287 source. In particular,
this review preserves 10240 OWNER-DEFERRED, the original dirty-discard FAIL,
explicit recovery profiles, failed attempts, cache gaps and distinct reused
sources. The entire Family2 group was read, not rerun.

## Historical observations relevant to fixed and scaling cost

Times below are the single raw observations already retained. They are neither
new samples nor a speedup comparison. Metric boundaries remain distinct; none
is presented as a median or a complete Server/SQLite attribution.

| Module / exact case | Exact measured source | Raw interval already recorded | Implication and limit |
| --- | --- | --- | --- |
| Init / `namespace-100-compact-v3` | `530dfa5363c7e0bd38d6ab6daf94a30da65302b5` | SDK call34,812,375ns; command56,742,834ns | Init is a separate route; adding Stage scratch does not explain this interval. Cache INELIGIBLE. |
| History / `history-retention-stride-1-total-storage-v4` | `b2ea44f0f0f3e0112f8b862ce71f2087e018c7a7` | complete driver136,703,421,625ns; verifier21,002,929,500ns | Extended component contract, not an ordinary15s Workspace speed row. Internal operation span is clipped/unavailable for a whole-operation claim. |
| Write / `workspace-write-dispersed-writes-4097-v1` | `2b1302454ed78f19ba82814d8005276f2f33f2d4` | command9,768,942,167ns; Exec8,286,253,042ns; Commit558,908,125ns | Much of command is Exec/lifecycle. The two-line historical reconciliation fix and eight reused cells retain their original scope. |
| SDK Commit / `workspace-commit-one-edit-retained-writes-4097-v3` | `2b1302454ed78f19ba82814d8005276f2f33f2d4` | Commit16,508,375ns; complete command9,691,391,667ns | Small nonempty Commit is sensitive to added constant work; no new scratch overhead was measured here. |
| Native Commit / `workspace-commit-headroom-quota-4mib-native-v2` | `2b1302454ed78f19ba82814d8005276f2f33f2d4` | component command170,988,208ns; reported Commit1,401,625ns | Distinct native functional/custody route; not an SDK latency comparator. |
| Namespace / `workspace-namespace-components-270-sdk-v1` | `d4a4252caa54a0dbb7454971d4ae8f169bd11c6d` | Commit767,457,958ns; Exec1,157,090,208ns; command2,805,615,834ns | Existing report identifies deep cycle/ReserveInodes work; fixed scratch cannot be inferred as its historical cause. |
| Mutations / `workspace-mutations-shell-exit7-retained-explicit-commit-sdk-v2` | `2f49da75009745d42332f72263418162d3db20b6` | final Commit16,663,500ns; complete recovery command5,244,737,792ns | Recovery includes prelude, retained pin and cleanup; final phase alone is not workflow wall. |
| Package / `workspace-shell-package-many-1025-sdk-v2` | `8e92c96a30cbb9e731b63ebfdd772b48e3ef50a4` | Commit9,629,237,416ns; Exec8,315,172,083ns; command18,795,341,167ns | Scaling work is substantial; full Server timer tree is clipped. No exclusive scratch/SQLite breakdown is available. |

The clean retained SDK control in r062 records Commit5,356,833ns and a verifier
with `advanced=false`. It must not be assumed to take the nonempty Stage path.
The F7 native [r079](../286/experiments/20260930-workspace-shell-package-native-checkpoint-r079.md)
at source`6599e64a60ee99e17bb9080c2cdcba019676a432` separately proves observer
refusal/custody/refund in383,237,667ns complete component command; that is safety
evidence, not a ninth speed family or an SDK speed arm.

The known Family7 completion-credit correction and deep270 cause observations
are preserved in their original reports. No historical timing isolates the
new metadata-scratch setup. A new profile cannot promote these intervals into
an exact-candidate speed result or substitute lifetime/heap-only memory for
the missing phase/cache observations.

## Source-pinned scratch work and its timing gap

At the research pin`7edddbdb8e8512627aed0ed42533ef099d802384`, Server
`service/save/catalog.rs::stage` moved from validated head/base/scope/deadline
directly to Store `begin_save`, prepared filesystem construction and Save
finish. It had no Construction scratch owner. The published79639 source adds
[Construction::begin](../../../crates/layerfs-server/src/service/construction.rs)
before Save/body, and [Stage cleanup](../../../crates/layerfs-server/src/service/save/catalog.rs)
releases known scratch before finishing Save. Therefore old PhaseB receipts
cannot measure or explain the newly added constant scratch cost.

The current call graph contains these real owners/actions:

1. First use of a configured Store lazily prepares one ScratchAuthority: checked
   base descriptor/identity, private directory nonce/name and fixed owner slots.
   The shared private-directory authority is already reused. It is incorrect
   to count full directory initialization as new work on every later Stage.
2. Each nonempty prepared Stage admits a new exact operation/source, creates a
   fresh O_EXCL private file, reserves the captured default16MiB or configured class, and observes its identity,
   apparent size and physical allocation. The source token is prepared once
   and moved into the actual input spool.
3. [Private profile initialization](../../../crates/layerfs-storage/src/construction_state/profile.rs)
   opens the actual connection, applies/readbacks fixed PRAGMAs and limits,
   begins one schema transaction, creates private profile4 owner/site/graph/root tables, the two
   site indexes and two partial graph indexes, inserts owner rows, commits, sets max pages and reads back the
   exact profile. It is not a per-row schema operation.
4. Site insert/closure/fact/seal/retirement and root indexed state issue their
   actual bounded transactions, lookups, hash walks and native observations.
   The [selected graph contract](R1D-EFFECTIVE-GRAPH-FREEZE.md) adds one
   effective adjacency build, indexed SCC state and a sequential graph retirement
   phase. The accompanying checkpoint distinguishes count/provider proofs from
   unrun exact-source speed qualification.
5. [Resource close](../../../crates/layerfs-storage/src/construction_state/session.rs)
   requires known autocommit, checked connection close, descriptor/path identity,
   one native close/unlink/removal confirmation and exact credit refund.
   Unknown/release failure retains the file/engine/capsule and credit.

Empty populations still use owner/header/seal/phase transactions on the current
native route. That is a plausible source of constant latency. This document
provides no elapsed-time or syscall-cost number for it. Existing ordinary
request, source, Save, catalog and cleanup work must remain in the metric that
owns it; no initialization may be silently moved into benchmark setup.

Init uses the separate import/C1 build route, and the history module uses its
existing component backend. They must be inspected/qualified on their own
actual paths; a Stage-specific scratch hypothesis does not explain all eight
modules. Common C1 changes can still affect them, so this is not a claim that
their historical source proofs qualify the new source unchanged.

## Current graph transaction count review

Static source review of the selected configured48 native-format fixture finds
131072 nodes and65536 edges, constructed once. It expands each node once via
the partial index. Its star plus isolated shape has65536 DFS roots and65536
tree descents/returns, producing393216 CAS calls and131072 singleton pop calls:
524288 separate solver mutation/pop transactions. The solver requests196608
edge pages,262144 live node gets and1024 projection pages. Adjacency and proof
each walk1024 node waves and512 edge waves; retirement uses1536 combined windows.
These are source-derived counts for this exact raw format fixture, not measured
latencies or an ordinary supported C1 request profile.

Native verification uses fixed metadata/empty-index queries per selected call.
All graph choices are primary/partial-index seeks; no population-quadratic loop
was found in this review. Singleton pop still reserves a128-record result window,
and each pending attempt initializes six fixed128-slot arrays. This is bounded
constant-per-call allocation and transaction work, which can still be expensive.
Inline Graph/GraphAttempt options also enlarge old-profile active objects even
with graph=None. Encoded record widths do not account for these layouts.

A configurable scratch setting establishes capacity. This source review does
not establish satisfactory per-record transaction latency, a speedup, physical
global containment or release admission. Any future solver batching/allocation
change would preserve closed CAS, exact ownership and Unknown custody and require
its own authorized checkpoint; it is not selected during the requested pause.

## Prospective known-clean reuse constraints

No reuse is implemented or selected for the current effective-graph checkpoint.
The current contract requires per-Stage native release; reuse would be an
explicit future contract/source change with new owning proofs.

A bounded per-Store idle DB/connection pool could retain known-clean schema,
file identity and engine state. It must not become a per-Workspace or lifetime
owner registry. Retain the current fixed owner count, worker count,512KiB cache,
page policy and exact configured native budget; idle pooled disk/cache remains
charged and visible. Never describe return-to-pool as physical refund.

Eligibility requires known successful completion, no open transaction/BLOB
handles/cursors/borrowed state, no failure/quarantine/release attempt, and exact
empty node/edge/site/root tables and projections. Existing root phase completion
does not delete its rows, so "clean" cannot be inferred from successful Commit
alone. Checked bounded retirement/reset is required; no uncertain table may be
queried to guess whether a failed operation committed.

Every checkout burns a fresh operation identity and uses the newly prepared
SourceId. Before body effects, rebind the continuously owned descriptor/header
and exact selected budget/profile in one acknowledged transaction. Reusing a
known live file is not permission to reopen/adopt a leftover path. Old leases,
tokens, selected roots/locations and epoch authorities must be ended so they
cannot select the next borrower. Different budget/provider/schema/Store-authority
classes cannot be adopted into one another.

Unknown, incomplete reset, observed allocation mismatch, identity mismatch,
failed close or cleanup never returns idle. Such an owner stays charged and
retained; no automatic retry, repair, rollback-on-guess or cold-fresh fallback
turns it into success. Refuse bounded capacity when the fixed pool is occupied.
Explicit known pool shutdown drains/removes each owned resource once; no SQLite
global shutdown or modification of foreign connections is involved.

Reuse may remove real repeated product work, but it also changes resource
lifetimes and cache state. A prospective performance profile must include first
use/initialization and idle resource ownership, distinguish repeated operations
within one actual product lifetime, and apply the declared cache contract. A
warm schema, retained source page, clone or prior sample cannot be silently
credited to an ordinary cold/measured phase. The complete command still owns
its lifecycle and cleanup. Existing cache-INELIGIBLE receipts remain so.

## Prospective small-path boundary

A source-selected empty-topology plan is a narrower candidate than general
small-graph routing. Exact declared `directories=0` and `names=0` creates no
directory-binding cycle seeds, sites or directory-root rows. All actual decoder
counts/global EOF, existing-kind/content-root identity restrictions, source
binding, root/allocator checks, ordinary canonical file/metadata work, Save/C5
custody and deadlines still apply. A lying body must fail explicitly.

An explicit first-party zero-record state capability could avoid a disk owner
only if its exact live operation/source/Store context and seals are genuine,
and every impossible append/page/mutation fails. It cannot supply guessed
native identity, simulated physical accounting or fake successful cleanup.
Keep the actual Store/Save/native engine requirements: selecting a shape cannot
bypass an unavailable required provider guard. Native budget zero and any
logical control admission must be reported honestly if such a capability is
later adopted.

Do not use workload names, command recognition, suffix rules or a caught
Capacity/Unsupported error to choose it. A small changed-name count does not
bound descendants in uncertified v1, so general `B<=128` is insufficient for a
small graph route. Regular-file-only topology cannot be inferred from unknown
row kinds before receiving/checking the ordinary source. The empty D/B shape is
only a proposal; current transport/controller call sites must prove that the
intended ordinary one-file operation actually emits it. No historical clean
no-op Commit may stand in for a nonempty Stage proof.

## Evidence needed for a later decision

The owner directed pausing implementation after the current checkpoint. This
draft does not authorize implementing the proposals during that pause.
Root may finalize the review from the actual checkpoint source and the existing
eight-module evidence. Any later authorized change needs its own concrete
contract, tests, source pin and exact per-commit LOC; no schema/pool/small-path
code belongs in this checkpoint.

For a future performance decision, prospectively select the existing applicable
small nonempty Commit/namespace and many-file case with exact source, route,
cache, bounds and independent old/new bytes. Keep initialization, Stage, Save,
catalog, known cleanup and expected-retained outcomes distinct. Required count
diagnostics would establish actual file opens/preallocation/schema/transaction/
lookup/close/unlink counts, source work, expansions and SCC steps at that frozen
source; none were run here. A byte oracle remains separate from allocation,
liveness and speed. No new runner/family, routine Family2 rerun or benchmark
campaign is selected. #288 qualification remains delegated unless separately
authorized by the owner.

Pending review gates: actual checkpoint pin; exact source-to-module routing;
count attribution for constant cost; native/engine/OS resource observation;
generic empty-shape proof; known-clean/Unknown lifecycle proofs for any reuse;
and exact-candidate speed/cache qualification. No strong performance, release,
full R1/R2-R7 or benchmark admission is claimed.
