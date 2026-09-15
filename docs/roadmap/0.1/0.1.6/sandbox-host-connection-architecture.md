# v0.1.6: simplified sandbox/host connection architecture

Status: reviewed design, 2026-09-15; implementation and measurements NOT_STARTED.
Tracking: [#150](https://github.com/Ephemeral-AI-Lab/layerfs/issues/150), which includes both documents while their repository copies await a documentation commit.
Parent experiment: [#149](https://github.com/Ephemeral-AI-Lab/layerfs/issues/149).
Execution pipeline: [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151) tracks implementation of #149 and #150 together and the three benchmark gates; follow its [stage order](experimental-implementation-pipeline.md).
Review evidence: [three-subagent review record](sandbox-host-connection-review.md).

This document is the connection/ownership refinement of #149. It makes the before/after architecture, removed machinery, remaining communication, transaction boundary and minimum implementation work explicit. Its host-memory outcome and cancellation rules refine the earlier broader receipt language in #149; it does not change that experiment's performance gates or authorize a release.

## 1. Decision

**One sandbox daemon manages multiple live Workspaces, each keyed by WorkspaceId and owning its own mutable state. The host owns canonical construction and accepted committed history.**

Ordinary hot filesystem mutations complete locally. Mutable Workspace metadata and replacement bytes are transferred to the host as immutable snapshot input at Commit, rather than installed on the host after every operation.

“Only communicate at Commit” is shorthand that must not become a false contract. Begin/End, host-initiated commands/SDK edits/status, immutable-base cache misses, capture control and publication completion still communicate. What disappears is the mandatory host installation/acknowledgment of each ordinary FUSE mutation and the continuously synchronized host mutable mirror.

Start implementation from `v0.1.5^{commit}` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`. The release supplies reusable local mutation and file-piece code, but its replacement backing still uses host RESERVE/APPEND calls. Restoring that route alone is not the new design.

Owner rules remain: disposable Workspace; no Workspace durability flush; no pause/quiesce; same live Workspace across commands and Commits; one Commit compute worker; no imported-library/dependency patches; one performance sample per case/arm; create-500, then bulk-create-500, then 25,000-file gates against v0.1.5; preserve CAS, CDC, FULL/DELTA, compression, packing and atomic database publication.

## 2. Before: current host-authority v0.1.6

```text
        SANDBOX                                  HOST
+-------------------------+          +--------------------------------+
| Commands / applications |          | Mutable Workspace authority    |
|            |            |          |                                |
|            v            |          | - inode / namespace state      |
| FUSE adapter            | -------> | - dirty and range indexes      |
|                         | per-op   | - payload arenas and catalogs  |
| create / write / chmod  | RPC      | - mutation replay / ownership  |
| lookup / getattr / ...  | <------- |            |                   |
|                         | result   |            v                   |
+-------------------------+          | Host snapshot                  |
                                     |            |                   |
                                     | Maintenance + canonical build  |
                                     |            |                   |
                                     | SQLite publication transaction |
                                     |            |                   |
                                     | More maintenance / teardown    |
                                     +--------------------------------+

Ordinary mutation:
  syscall -> host installation -> reply -> next syscall

State responsibilities:
  sandbox kernel/adapter state + host live overlay + host canonical Store
```

This route sends thousands of host exchanges for the serial tiny-500 workload. Its per-operation work includes sparse copy-on-write disk indexes, payload ownership updates and later reclamation. Batching individual maintenance steps does not remove that work.

The v0.1.5 starting point differs: mutable operation metadata is local, but payload reservations/appends and dirty-fact export still involve host state. The replacement must remove those dependencies too.

## 3. After: local mutable owner, immutable Commit input

```text
        SANDBOX                                  HOST
+-------------------------------+    +--------------------------------+
| Commands -> FUSE/local owner  |    | Coordinator / control          |
| SDK requests enter same owner|<--->| Begin, SDK, status, End        |
|              |                |    +--------------------------------+
|              v                |
| Compact local metadata        |    +--------------------------------+
| File pieces + inline bytes    |<--->| Immutable base/CAS reader      |
| Packed local payload backing  |miss| Authorized root/range context  |
|              |                |    +--------------------------------+
|              v                |
|       Capture generation G    |
|              |                |
|       +------+-------+        |    +--------------------------------+
|       |              |        |    | One host Commit compute worker |
|       v              v        |    |                                |
| Frozen G       Live successor |    | Existing canonical builder     |
|       |        keeps changing |    | CAS / CDC / DELTA / compression|
|       |                       |    |              |                 |
|       +----------------------------> Checked object admission       |
|       bounded frozen input    |    |              |                 |
|       and needed bytes        |    | Complete Store-owned candidate |
|                               |    |              |                 |
| Published coverage and <-----------| Atomic DB publication          |
| canonical result references   |    | + bounded host-memory outcome  |
| preserve later live edits     |    +--------------------------------+
+-------------------------------+

Local write:
  validate/reserve -> update owned state -> return

Commit:
  retain G -> transfer frozen changes -> encode/admit -> publish

After Commit:
  same mount, same live inode identities, later commands and Commits
```

The host can own bounded candidate/staging data during Commit. This is not a competing live Workspace. Its data is tied to a frozen generation and cannot change when the sandbox modifies its successor.

An individual broken Workspace is disposable; disposing it must not stop the shared daemon or retire healthy sibling Workspaces. Successful publication leaves that Workspace usable. Host transaction-outcome state survives Workspace disposal. An actual shared-daemon or sandbox failure can affect all of its Workspaces; logical Workspace isolation does not claim separate process failure domains.

### 3.1 Multiple Workspaces in one sandbox: identities and paths

The default visible mount is **`/workspaces/<workspace-id>/`** and the private backing/snapshot area is **`/snapshots/<workspace-id>/`**. The owner selected these two roots with at most one concurrent Commit per Workspace. Existing explicitly supplied placement paths remain usable if unique and validated; the logical identity is WorkspaceId, not the path or branch name.

```text
ONE SANDBOX

/workspaces/
    <workspace-A>/                  FUSE mount A, application-visible
        src/
        package.json
    <workspace-B>/                  FUSE mount B, application-visible
        src/
        package.json

/snapshots/
    <workspace-A>/                  private shared backing / slot A
        payload-0001.bin
    <workspace-B>/                  private shared backing / slot B
        payload-0001.bin

One daemon's bounded registry:
    WorkspaceId A -> live root A, snapshot A/G, mount A, backing A
    WorkspaceId B -> live root B, snapshot B/G, mount B, backing B
```

This is the selected layout for the new design, not a claim that these directories have been created. `/snapshots` is daemon-private backing, not a second FUSE mount or a materialized copy of application files. It stays outside every Workspace mount, with private permissions and a verified disk-backed sandbox filesystem when spilling larger payloads. A tmpfs mount would charge those payloads to RAM. Snapshot roots/ownership descriptors remain in daemon memory and share the Workspace's backing; tiny inline-only state may need no payload file at all.

One active Commit permits one active snapshot slot per Workspace, so no generation subdirectory or historical snapshot directory is needed. Internally retain Workspace/session incarnation, generation and attempt identity even though the path is stable. The slot is reused only after the prior attempt is resolved and its transfer readers have finished or been cancelled; a late message must never read or acknowledge the next snapshot through the same path.

`/snapshots/<id>/` may also contain payload bytes still used by the live successor. Completing Commit releases the captured root and reclaims only unowned ranges; it must not recursively delete this directory while live pieces or other admitted readers still reference it. Reuse the backing across later Commits. Remove it at End/Discard after ownership is retired. This deliberate shared-backing lifetime avoids a third backing tree and avoids copying live bytes into a separate snapshot tree on every Commit.

Each owner has independent inode/namespace/change state, local generation, an optional active Commit snapshot, payload ownership, branch/base association and lifecycle. A command names its Workspace and starts in the corresponding mount. Snapshot/stream/result identity includes the daemon/session incarnation, WorkspaceId, generation and attempt where applicable; equal generation numbers in A and B are unrelated. A late A result cannot alter B or a replacement Workspace. Validate ownership of an existing private directory before reuse; a directory name alone is not an incarnation token.

End/Discard A retires only A's mount, owned executions, snapshot/readers, backing and attempt context. B continues running and committing. Keep existing branch lease/conditional-head rules; multiple Workspaces do not imply different branches or authorize an unchecked same-branch overwrite.

Reuse the existing daemon's WorkspaceId-keyed mount registry and per-Workspace execution/cleanup routing rather than creating a daemon or sandbox for each Workspace. Share endpoint/control infrastructure and authorized immutable caches where useful, while keeping mutable ownership separate. The initial experiment still uses one shared host Commit compute worker, processing one construction at a time; queued requests capture only when scheduled, and all Workspaces' admitted ordinary operations continue. This clarification does not add a worker per Workspace.

Enforce both per-Workspace accounting and aggregate daemon/host admission. The experiment's 64 MiB accounted working-allocation limit is an aggregate across its active Workspace owners and attempts, not a fresh unbounded 64 MiB grant per registry entry. Shared caches count once when actually shared; real copies count separately. Bound the registry and queued requests using existing admission limits.

Add one focused two-Workspace correctness proof, not a new performance campaign: create the same relative filename with different bytes in A and B; capture A while both receive commands; verify A's Commit excludes B's state; deliver an incorrectly addressed/late result and reject it; End A while B's open handle/read/write/Commit still work; verify A cleanup leaves B's files/backing intact. This proof does not add statistical samples to tiny-500 or 25k.

Existing foundation: [WorkspaceId-keyed daemon registry](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-daemon/src/main.rs#L77-L82), [duplicate ID/root rejection and mount registration](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-daemon/src/main.rs#L1328-L1377), and [per-Workspace retirement](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-daemon/src/main.rs#L1664-L1704).

## 4. What is removed, what is reused

| Remove from the new experimental route | Why it can go | Reuse/retain |
|---|---|---|
| Per-FUSE-mutation host installation and acknowledgment | One local owner already orders installed state | Local validation, operation atomicity, immutable-base reads |
| Host RESERVE/APPEND backing for ordinary writes | Replacement bytes belong to the sandbox until Commit | Local packed backing and retained buffer ownership |
| Host dirty-facts/inode mirror | Commit reads the frozen generation directly | Bounded immutable candidate input during Commit |
| Sparse on-disk overlay metadata/range/refcount/location catalogs | Initial target uses compact bounded resident state | Bounded shared nodes, existing PieceTree and a small accounting ledger |
| Workspace fsync durability and sync-triggered dirty-prefix export | Uncommitted Workspace recovery is outside scope | Application-facing volatile completion/known-error handling |
| Freeze/resume, whole-cache-drain acquisition and live checkpoint installation | Snapshot ownership separates G from later live state | Brief local metadata ordering and generation-aware coverage |
| Whole-Commit lifecycle locking of SDK/control work | SDK edits must complete while Commit runs | Owner-local ordering for FUSE and SDK operations |
| One serialized control transaction carrying the entire snapshot stream | Bulk transfer must not prevent live service progress | Existing endpoint authentication and small fixed service/data lanes |
| Persistent Workspace recovery, uncertain mutation replay and reconnect reconstruction | Broken/uncertain Workspace may fail and be recreated | Safe disposal; no automatic replay of uncertain append/rename |
| Persistent publication receipt table/ancestry recovery by default | Host restart is outside the current failure model | One bounded host-memory attempt/outcome, with a proved completion handoff |
| Parallel encoding, file-build or GC worker pools | Experiment is single-worker | One canonical worker; disclosed necessary I/O/control service threads |
| Canonicalizing live files solely for End | No next Commit will use the ending Workspace | Resolve host outcome, retire readers/mount, release resources |
| Unbounded raw-generation/operation history | Only actual current/snapshot/read owners need data | Compact latest-published provenance for incremental C2 |
| A second CAS/CDC/DELTA/compression/Init implementation | Existing canonical algorithms are the retained asset | Existing authenticated construction, packing and publication |

“Remove” means omit/replace in the new branch's execution path, not erase historical source or indiscriminately delete shared helpers with other callers. Do not port the current host overlay and then optimize it again.

Writable shared-mmap removal is still **PROPOSED, not adopted**. If that scope change is selected, omit the unsupported writable-mapping retrieval/drain machinery too, while preserving tested read-only/private mapping behavior and cache invalidation. Connection simplification alone does not solve kernel-dirty snapshot visibility.

## 5. Why transferring mutable changes at Commit is efficient

1. **Remove latency from the serial syscall chain.** Local create/write/chmod operations no longer wait for host installation. A bounded transfer at Commit amortizes framing and cross-boundary waits across many records.
2. **Transfer final effects.** Repeated overwrites before G are coalesced. A file created and removed before G need not become committed content. Append/range data that still contributes to the final state remains necessary; do not claim every workload collapses to one tiny buffer.
3. **Keep one live representation.** Avoid a sandbox mutation view plus a continuously maintained host dirty mirror and its disk ownership databases. Only captured state crosses into construction.
4. **Canonicalize at the useful boundary.** Preserve CAS deduplication, small-file FULL/DELTA selection and large-file CDC/extent reuse, without requiring canonical work per write.
5. **Contain disposable failures.** Before a complete candidate exists, a broken sandbox can be discarded without exposing a partial branch snapshot. After publication, committed data belongs to the Store.
6. **Reduce synchronization and cleanup.** No Workspace durability flush, mutation replay database, or mandatory per-file canonicalization at End. Actual snapshot/reader retention still consumes resources and must be reclaimed safely.

These are reasons to expect an improvement, not a measured result. Transfer, encoding, canonical result handoff, SQLite work and cleanup still count in complete Commit/workflow timing. A constant-size snapshot handle does not make the entire Commit constant-time. Lazy base reads preserve cheap startup; preloading the entire base just to eliminate every non-Commit data request can lose that benefit.

## 6. The minimum connection contract

These are logical operations over reused first-party transport primitives, not a generic RPC framework or a requirement for a separate process per row.

| Operation | Direction | Required behavior |
|---|---|---|
| Begin/bind | Host -> sandbox | Workspace incarnation/session, authorized immutable base context, limits and protocol capability; return the mounted owner identity |
| Command/SDK/status | Host <-> sandbox | Existing control surface, bounded requests, mutable SDK operations installed by the same local owner; query status rather than mirror it |
| Immutable fetch | Sandbox <-> host | Bounded namespace/content/range lookup against retained authorized canonical context; preserve authentication and base ownership |
| Capture G | Host -> sandbox | Return a generation-bound snapshot token under short local ordering; no dirty scan, payload flush or remote I/O under the capture lock |
| Read frozen input | Host <-> sandbox | Bounded records, pieces and payload batches tied to the exact token; validate lengths/offsets/identity/end-of-stream; no live-path fallback |
| Publication/canonical result | Host -> sandbox | Exact attempt outcome, published root/head, covered generation and required bounded canonical correspondence or its retained reader |
| Cancel/release | Host <-> sandbox | Stop new snapshot reads, finish/cancel admitted readers, release their ownership; host independently settles publication if already started |
| End/Discard | Host <-> sandbox | Explicit lifetime termination, outcome settlement and cleanup; no live canonicalization solely to prepare for future reuse |

Snapshot tokens and attempts identify a Workspace incarnation so a late message cannot mutate a newly created Workspace reusing a path. Canonical ObjectIds remain independent of runtime tokens, pointers, host paths and physical pack identifiers. A base content hash proves content identity, not permission to access an unrelated project's object; retain the existing bound capability/context checks.

Use the existing authenticated endpoint/role mechanism with a small fixed separation of live/control service, immutable reads and snapshot bulk traffic. Reserve live/control request and buffer capacity. Do not hold the entire control-slot mutex, a live metadata lock, or all backing permits while awaiting a bulk stream/network reply. Prefer reusing existing bounded service reservations to a new scheduler/multiplexer framework.

```text
Authenticated endpoint / fixed roles
  |
  +-- live/control -------- SDK edits, status, capture, cancel, results
  +-- immutable reads ---- cold base metadata/content fetches
  +-- snapshot bulk ------ bounded input for G

If snapshot bulk is deliberately stalled:
  local write       completes within admitted capacity
  SDK edit/status   completes on live/control service
  cold base read    completes on immutable-read service

Exactly one canonical construction/encoding worker.
Service separation does not authorize additional compute workers.
```

A finite transfer/no-progress/cancellation policy must be recorded before running the prototype. Reuse existing operation deadlines where suitable. One snapshot bounds count, not retained bytes or lifetime. No unbounded wait may retain generations forever, and no post-timeout publication may be guessed from a closed socket.

## 7. Atomic publication with a disposable sandbox

The host owns one bounded attempt record containing Workspace incarnation, captured generation, expected branch head/base/root, candidate identity and phase/outcome. Caller cancellation or sandbox death does not destroy an executing publication worker or that record.

Before publication, the host must own a complete candidate with **logical object dependencies and transitive physical DELTA-base dependencies** independently of sandbox buffers/files/read callbacks. Root-row existence alone is insufficient. Reuse existing checked admission and dependency ownership; do not add a full committed-tree reread before every Commit.

Keep v0.1.5's Store admission/operation permit and rollback ownership. Its rollback can remove objects/packs newer than an admitted baseline; one Commit worker does not mean other Store operations cannot exist. Dropping that permit could delete another operation's data. Existing Store serialization does not require freezing local sandbox commands.

Publication reuses the SQLite transaction: check expected branch/base/root and stage, insert/verify Commit identity, conditionally advance the head, remove the matching stage, COMMIT. Prepare the success result before SQL completion. After `transaction.commit()` returns success, assign the host-memory outcome infallibly **before an await, cancellation point, telemetry, sandbox acknowledgment or API result send**. Prove this handoff; do not merely assume task cancellation leaves a receipt.

```text
Capturing / transferring / building
  |  cancellation -> abort owned attempt and release readers/backing
  v
Complete checked candidate
  |  cancellation before publication -> abandon owned stage
  v
Publication has started
  |  caller cancellation -> host still finishes/resolves exact transaction
  v
Known SQL success -> record host-memory outcome -> send result/coverage
  |  sandbox lost -> committed history still exists
  v
Settle/release the attempt under the host's bounded lifetime policy
```

A known successful SQL COMMIT followed by a lost reply is different from SQL COMMIT itself returning an ambiguous error. Normalize/inspect transaction state while retaining Store ownership. Autocommit being restored does not distinguish COMMIT from ROLLBACK. If the outcome cannot be established, report **indeterminate** and contain further destructive Store writes until resolved. Never guess “not published,” delete its stage or recapture newer state as a retry.

Default to the bounded host-memory outcome under the live-host failure model. Do not port the persistent receipt table, ancestry search or receipt-deletion transaction merely for future crash recovery. A small transactional marker becomes justified only if the actual implementation cannot close the handoff/ambiguity gap; document that demonstrated requirement before adding it.

Scope of exact result handling: host Store-to-coordinator completion and host-to-sandbox coverage delivery. The existing public SDK Commit call has no caller-supplied request ID; do not claim generic exactly-once behavior for arbitrary external API retries. A lost public response needs explicit reconciliation/status handling, not an automatic new capture pretending to be the old call.

The current failure model keeps the host DB process and OS alive. Preserve SQLite rollback machinery; `journal_mode=MEMORY` and `synchronous=OFF` do not promise host-process-crash or power-loss integrity. Never replace journaling with OFF to make the experiment faster. Durable database publication is later work under #69.

## 8. C1 completion while G+1 stays live

A published root alone is not necessarily enough for the next efficient Commit. Return or expose bounded canonical result/provenance data for G. This is reverse data flow at Commit time, not continuous host mirroring.

Completion identifies Workspace incarnation, attempt, captured generation, published root/head and covered sequence. Apply it idempotently against captured identities/revisions. Clear a dirty key only if its current version still matches the captured version. Preserve newer payload, namespace changes and inode identities. Duplicate or late C1 completion must not overwrite C2's context.

Do not call v0.1.5 `finish_checkpoint()` on the live successor: it clears generation/path/dirty state for the old frozen model. Retain the useful semantics of canonical correspondence in a compact form, without restoring the disk maintenance engine. If applying coverage becomes uncertain, the disposable Workspace may fail; the host still retains C1's true outcome.

## 9. Memory, CPU and evidence

Use #149's budgets and one-sample gates without changing thresholds. The 64 MiB working-allocation and 32 MiB 25k temporary-backing limits are initial engineering targets, not established capacity. Before coding the representation, account for bounded node capacities, retained COW paths, packed payload/chunk slack, base caches, provenance, transfer buffers, request/result slots and canonical scratch. Count actual host/sandbox copies separately; count shared ownership once only when the physical allocation is shared.

Report temporary staged/failed-attempt growth inside SQLite separately from accepted committed content, so excluding the final Store from the temporary-file gate does not hide construction growth. Base-cache budgets remain explicit even if the database has its own cache. No unbounded inode map becomes safe merely by attaching an Arc.

One Commit worker constrains canonical computation; FUSE, transport, SDK service and the workload may still consume CPU concurrently. Report all relevant host/sandbox CPU and memory with matched boundaries. No extra file/encoding/GC pool or unmeasured background cleanup.

No benchmark was run for this review. The owner-selected sequence is one control/candidate sample for `tiny-create-500-mixed-v4`, then `tiny-bulk-create-500-mixed-v3`, then one three-Commit 25k lifecycle sample per arm. Each case must pass before the next full qualification. Bulk-create-500 writes 5,000 files / 500 MiB and must use bounded buffers/local backing; the 32 MiB absolute backing limit belongs only to 25k. Keep the complete-Commit gate even though bulk transfer moves into that phase. Separate focused correctness checks are not statistical repeat samples.

## 10. Minimal implementation/proof sequence

1. Freeze this connection refinement with #149; branch the implementation from the exact v0.1.5 commit. Keep dependency bytes/versions unchanged.
2. Localize payload backing and compact metadata; remove ordinary host reservations, mutable facts and host installation from that path. Preserve immutable fetch/control behavior.
3. Connect fixed-size capture and bounded frozen readers to the existing single-worker builder; preserve independent service progress and resource reservations.
4. Replace live checkpoint installation with generation-safe coverage/provenance. Add the bounded host-memory attempt/outcome and explicit publication cancellation boundary.
5. Run focused proofs: stalled builder; stalled bulk transfer with local write, SDK/status and cold read; delayed/duplicate C1 completion; partial-transfer failure; rollback with a competing Store operation; cancellation on either side of publication; known SQL success with lost reply; Store-only reads after sandbox destruction, including DELTA and reused extents.
6. Execute #149's create-500, bulk-create-500, then 25k gates with one performance sample per case/arm. Report misses and remaining mapping/platform limits; no implicit main replacement or release.

## 11. Durability and cloud extension: preserve boundaries, defer machinery

Later durable Commit strengthens the storage/publication acknowledgment, without requiring every live write to become durable. Preserving every acknowledged uncommitted write across sandbox loss would require additional survivable logging/replication and is a separate product choice.

For the [#82 hybrid proposal](https://github.com/Ephemeral-AI-Lab/layerfs/issues/82), immutable packs and all logical/physical dependencies must be available under the selected remote persistence policy before transactional metadata publishes their locators/root/head. Upload and metadata publication are not one transaction. Failed publication may leave reclaimable orphan packs; published roots must never depend on missing packs/bases. Metadata must satisfy the same failure-domain guarantee as content.

Keep canonical identities location-independent and preserve explicit versioned, bounded input/output contracts. Do not build cloud adapters, persistent live journals, failover, replicas, remote GC or a backend plugin hierarchy in v0.1.6. [#69](https://github.com/Ephemeral-AI-Lab/layerfs/issues/69) owns durability; [#52](https://github.com/Ephemeral-AI-Lab/layerfs/issues/52) owns the broader cloud deployment.

## 12. Acceptance and open decisions

- [ ] Local payload and metadata ownership; no required host installation on ordinary hot FUSE mutations.
- [ ] Multiple WorkspaceId-keyed mounts/owners in one sandbox; independent state and teardown; aggregate admission and two-Workspace isolation proof.
- [ ] No live host mirror, freeze/quiesce, whole-Commit SDK lock or bulk-control starvation.
- [ ] Bounded immutable G input and generation-safe completion; later commands and Commits remain usable.
- [ ] Existing canonical encoding, dependency closure, admission/rollback protection and conditional publication preserved.
- [ ] Host-memory outcome handoff/cancellation proof; actual ambiguous SQL errors contained; no unproved public API exactly-once claim.
- [ ] Single compute worker, complete resource accounting and inherited single-sample performance gates satisfied.
- [ ] Writable shared-mmap support decision remains explicit; no silent scope reduction or false visibility proof.
- [ ] Future durability/cloud boundaries preserved without implementing those systems now.

Review verdict: **proceed with the scoped experiment after these connection contracts are implemented and proved; performance, resource feasibility and full mapping support remain unverified.**
