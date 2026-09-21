# Bridge and multi-writer load optimization specification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Issue: [#192](https://github.com/Ephemeral-AI-Lab/layerfs/issues/192), under
[#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181). Performance driver
and campaign: [#193](https://github.com/Ephemeral-AI-Lab/layerfs/issues/193).
Owner decisions consolidated on 2026-09-20. This is the optimization and
correctness revision to the original [implementation packet](README.md), not a
claim that the current candidate implements or qualifies it. Read the
[handoff](optimization-handoff.md) for exact source custody and execution order.

## 1. Authority and agreed requirements

This revision supersedes the earlier global A=1 recommendation and the proposed
universal socket-buffer admission ceiling. It preserves the original operation,
authorization, bounded-input, outcome and evidence rules except where this
revision explicitly requires a concurrency/profile correction.

| Item | Owner direction / status |
| --- | --- |
| Multiple writers | Required: overlapping writer operations against the same logical Store, not merely multiple connections or different Stores. |
| Duplicate work/bytes | Accepted: identical concurrent submissions may waste encoding and physical chunk/pack space; prefer simple ownership over cross-writer dedup coordination. Frequency is an expectation, not measured. |
| Resource contract | Hard application-owned byte/count limits; OS socket/kernel memory, stacks and RSS are separately accounted observations, not a fabricated universal physical ceiling. |
| Large load | Bounded streaming and aggregate admission must support an explicitly qualified large-input/concurrent-writer profile. The prototype's 64 MiB / 10 s limits do not define the final workload. |
| Durability and resume | Crash-durable acknowledgements and resumable uploads are not required. No WAL, sync calls, retry spool or recovery framework is introduced. |
| Simplicity | Reuse ordinary TCP and established framing/ownership patterns; keep the existing Noise security selection and three-crate boundary. |
| Tests | Keep focused checks short, reuse preparation/compiled artifacts, retain assertions and failures; do not inflate a failing test's timeout. |
| Design before implementation | Freeze the concrete items in section 8 before their dependent changes; this document is not a completed schema or numerical qualification. |

The multi-writer tradeoff is already recorded in
[concurrency design section 7](../../02-init-commit-and-concurrency.md#7-what-concurrency-costs)
and its decision summary. Its section 8 identifies correctness prerequisites;
its older sketches are not current implementation evidence or permission for
automatic retry/rebase. One construction producer **per operation** is compatible
with multiple concurrent operations. Preserve the namespace-init exception and
existing benchmark worker rules; do not add internal construction lanes.

## 2. Recommended architecture

```text
 LINUX DAEMONS                       NATIVE HOST SERVICE
 +--------------------+             +---------------------------+
 | A: bounded I/O     |<===========>| bridge session A          |
 | bridge client     | Noise/TCP   | framing / auth / deadlines|
 +--------------------+             +-------------+-------------+
                                                  |
                                                  v
                                        +----------------------+
                                        | Save A owner         |
                                        | C1 + private C2 state|
                                        | buffers / packs      |
                                        +----------+-----------+
                                                   |
 +--------------------+             +--------------|------------+
 | B: bounded I/O     |<===========>| bridge session B          |
 | bridge client     | Noise/TCP   +--------------+-------------+
 +--------------------+                            |
                                                  v
                                        +----------------------+
                                        | Save B owner         |
                                        | C1 + private C2 state|
                                        | buffers / packs      |
                                        +----------+-----------+
                                                   |
                           A and B execute concurrently
                                      |            |
                                      v            v
                              +---------------------------+
                              | One logical SQLite Store  |
                              | save-owned physical data  |
                              | per-save publication      |
                              +---------------------------+

 Shared service policy: authorize and reserve aggregate capacity.
 This is an admission check, not one worker executing all saves.
 Results return through the owning session. Diagnostics are independent.
```

| Boundary | Owns | Does not own |
| --- | --- | --- |
| `bridge::contract` | Five operation schemas, results/outcomes, logical limits, opaque verified-caller vocabulary | Sockets, Docker, SQL, native Store paths, telemetry workers |
| Native bridge adapter | TCP/Noise, versioned frames, input finality, bounded delivery and session failure | Dedup coordination, Store authorization policy, C2 saves |
| Service | Caller-to-Store/op grants, aggregate multi-writer admission, Store lifetime, common authorized handler | Container deployment and a second direct-call algorithm |
| Daemon | Endpoint/key configuration, bounded stdin/stdout, client/input cancellation | SQL, object packing, output/payload spool |
| C1/C2 | Construction, physical admission, writer-private ownership, persistence, visibility and cleanup | Network protocol or daemon lifecycle |
| Existing telemetry crate | Shared process observations, operation recorder, bounded independent output/retention | Product success, admission or mutation-outcome decisions |

Keep `ReadFile`, `Inspect`, `ConstructFile`, `EditFile` and
`UpdatePreparedFilesystem`. Keep the frozen supported variants until an explicit
versioned change is required. N file saves plus a prepared-tree update are still
N+1 operations, not an atomic composite or logical Commit.

### 2.1 Native carrier

Use ordinary `TcpStream::connect_timeout` and `TcpListener`, explicit accepted
socket mode, `TCP_NODELAY` and bounded I/O. Retain existing Noise authentication
and encryption. A plaintext bearer capability from v0.1.6 is not an equivalent
security profile. Do not add a relay process, carrier registry or async runtime
merely to work around an incorrect socket-memory assumption.

Use the v0.1.6 tag `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` as reference:
[connector](../../../../../../crates/layerfs-daemon/src/lib.rs),
[live transport](../../../../../../crates/layerfs-fuse/src/live_transport.rs), and
[container route](../../../../../../crates/layerfs-workspace/src/container.rs).
Borrow exclusive exchange ownership and length validation. Do not import its FUSE
roles, wait-before-accept scheduler, cleared timeouts, retries or legacy binaries.

Prefer bounded blocking session workers initially. This is a proposed execution
choice, not a claim that arbitrary connection counts scale. Keep concurrent
upload/response handling so a client can receive refusal while stdin is stalled.
It is I/O concurrency, not another C1 construction producer.

## 3. Independent writers and accepted duplication

```text
 Writer A                            Writer B
 --------                            --------
 prepare content X                   prepare identical content X
 encode private representation       encode private representation
 write A-owned packs                 write B-owned packs
          |                                     |
          +------------------+------------------+
                             |
                canonical identity still means X

 Accepted cost: redundant encoding and physical bytes.
 Required: exact identity, valid dependencies, isolated cleanup.
 No global chunk reservation/dedup coordinator just to avoid that cost.
```

Each admitted save owns bounded construction/codec state, open pack placement,
its pending records and cleanup identity. Allocation of pack IDs and metadata
ordinals must be safe across writers; do not assume the old `MAX(id)+1` and
contiguous-cursor model is safe concurrently. Shared optimization caches must not
turn an unpublished foreign object into an eligible dependency.

Preparation and network waits occur outside database write ownership. Execute
bounded database transactions, not a write transaction spanning an entire upload.
SQLite may serialize those transactions; this does not serialize complete save
lifetimes. Specify transaction access among already admitted writers separately
from Q=0 operation admission. No SQL busy-retry loop, unbounded commit queue or
silent busy-timeout/profile change is authorized by this distinction.

### 3.1 Safe reuse and the unpublished-winner race

```text
 UNSAFE                               REQUIRED
 A privately writes X                 B reuses only:
 B reuses A's pending X                  published eligible content, or
 B publishes parent -> X                 content owned by B's save
 A aborts and removes X
 B's successful root is broken.        Otherwise B may prepare its own X.
```

A uniqueness conflict alone does not establish a reusable winner. Validate
identity/canonical bytes and publication eligibility under the existing collision
contract. Do not replace corruption or unknown I/O outcomes with fallback reuse.
Expected object-admission conflict resolution is not replaying a whole mutation.

### 3.2 Per-save publication and cleanup

```text
 A: PRIVATE -- writes batches -- still preparing ----------------+
                                                               |
 B: PRIVATE -- writes batches -- finish -- PUBLISHED             |
                                            |                  |
 readers can use B's complete graph <--------+                  |
                                                               v
 A later finishes: publish A's complete graph.
 A instead fails: clean up A-owned data only; B remains readable.
```

A save becomes visible atomically only after complete validated input, finished
physical writes and satisfied dependencies. Ordinary readers use published data;
same-save construction may additionally use its own pending data. A global pack-ID
prefix or independent per-pack flags cannot substitute for this invariant.

Cleanup is by save ownership, not all pack IDs above one starting baseline. An
unknown outcome must not trigger deletion or presumed rollback. Ordinary reopen
must reconstruct correct visibility, without a crash-durability claim.

### 3.3 Catalog candidate: freeze the exact design before coding

The recommended candidate is save-owned locator entries, conceptually keyed by
`(ObjectId, SaveId)`, plus per-save publication state and publication-aware lookup.
Bounded batches can store private data without holding the entire input in memory.
It permits duplicate locator metadata as well as the already accepted duplicate
physical bytes. The exact schema, additional metadata tradeoff, collision
validation, bounded lookup, ordinals and cache rules are **not yet frozen**.

An alternative retains one unique public locator per ObjectId and reconciles
private candidates into it. Moving every private row into a public catalog during
finish makes that transaction proportional to save size. Do not call it a short
constant-cost publication or violate transaction row/byte limits.

A publication-state flip alone is also insufficient proof: specify how two
concurrent conflicting candidates are validated, how pending/aborted candidates
are excluded, how lookups remain bounded, and how all same-save dependencies
become eligible together. Choose a concrete algorithm satisfying those invariants
before schema changes. No new GC, provider registry or distributed coordinator.

## 4. Transport load and deadlines

```text
 LARGE INPUT -> bounded daemon window -> encrypted frames
                         -> bounded service window -> C1/C2 batches -> Store

 Storage slows -> service reads less -> TCP backpressure
               -> daemon reads less -> source stops advancing.

 File size grows total work and stored bytes, not transport working memory.
```

Frames may remain small; TCP can have multiple frames in flight. Do not add an
application ACK per frame. Keep one operation per session, with multiple active
writer sessions. Request/response IDs are session-scoped, and trusted identity is
checked independently of a caller-supplied Store ID.

Replace the prototype's hardcoded workload envelope through a declared profile,
not by quietly widening limits to pass a test. Freeze maximum logical input/read
range, metadata/edit replay, valid frame sizes/count policy, progress timeout,
overall operation budget, session count C, active-operation/writer count W > 1,
and aggregate memory. Keep checked arithmetic before allocation and validation of
actual final byte counts. Derive a consistent frame-count guard from the allowed
file/frame policy; do not leave the fixed 8,192-frame guard as an accidental
large-file ceiling or permit tiny-frame abuse without a bounded rule.

Use one absolute deadline per process-local operation and pass it through local
layers. Specify the remote remaining-duration/local-clock mapping; never compare
`Instant` across machines. Define stalled-I/O and overall budgets independently.
A productive large transfer must not inherit the unit-test wall budget. Native
C2 calls are not universally preemptible; deadline expiry cannot imply rollback
of a completed finish.

No measured max file size, MB/s, operations/s, writer count or minimum link rate is
established by this specification. The final large-load profile must fit declared
resource bounds. More writers improve throughput only until a shared resource is
saturated; one SQLite Store is not an unlimited scale-out backend.

## 5. Resource and failure ownership

```text
 Application accounting plan:
   shared state
   + C * bounded session state
   + W * bounded save working state
   + bounded telemetry

 Separate OS domains:
   socket/kernel state | backlog | stack mappings | RSS | cgroup observations
```

This is an accounting plan, not a measured byte ceiling. Include plaintext and
ciphertext overlap, metadata decode/conversion, Vec/String capacity, native codec
and database working state, current writes, and closing ownership. Bound each real
owner and their simultaneous multiplicity. An option readback is not a persistent
physical kernel-memory cap. Mark unavailable observations unavailable.

At operation capacity, refuse immediately (Q=0). Within admitted operations use
backpressure. A slow writer can occupy its own reserved capacity; it must not gain
extra uncharged memory or a global whole-save lock. No FIFO or starvation-freedom
claim follows; any per-principal session quota must be stated and tested.

A concrete session owner retains its socket shutdown capability, worker and
admission ownership. Stop admission and interrupt cooperative socket/pipe I/O on
shutdown. Count closing owners until completion or explicit process exit. Dropping
a `JoinHandle` detaches work; it is not cleanup evidence. Preserve unresolved-save
outcomes when bounded shutdown ends during non-preemptible core work.

Only supported cooperative bounded I/O adapters carry deadline/cancellation
promises; arbitrary synchronous `Read`/`Write` implementations do not. Keep
telemetry failure independent from product results and required cleanup.

## 6. Portability and security

```text
 direct caller -----------------------+
 native TCP endpoint -----------------+--> verified caller + logical request
 future concrete carrier -------------+                 |
                                                       v
                                            same authorized handler
                                                       |
                                            C1 / C2 provider boundary
```

Keep opaque verified-caller vocabulary outside the native socket module. Only
trusted authentication/direct-entry code constructs it; arbitrary supplied public
bytes cannot manufacture verified authority. Preserve direct-call authentication
and authorization rather than giving it a bypass.

The operation contract contains logical IDs and bounded data, not socket handles,
Docker IDs, database connections, host paths or SQLite errors. Native socket/pipe,
thread and resource collection details stay in native modules. Use safe existing
libraries; no third-party patch or extra crypto implementation.

A future HTTP/WebSocket carrier replaces delivery/authentication integration,
not C1/C2 semantics. A managed SQLite provider must separately qualify transaction,
visibility, allocation, error and resource behavior; it is not a drop-in promise.
Do not implement empty adapters or speculative provider registries now.

## 7. Acceptance additions

Retain the original [V01-V17, T01-T16 and ENV01-ENV06 matrix](03-verification.md).
Append superseding dispositions where old A=1/socket-ceiling evidence does not
prove this revision. Required added cases start **NOT_RUN** on this design:

| ID | Required proof |
| --- | --- |
| O01 | At least two successful overlapping writer operations in the same Store through real separate Docker daemons/native service; exact roots and readback. |
| O02 | Identical concurrent content: correct canonical bytes and reuse/duplicate accounting; zero physical duplication is not a pass criterion. |
| O03 | B completes before A; B readable, A's incomplete dependencies invisible. |
| O04 | A aborts/disconnects while B succeeds; A cleanup cannot delete B or expose an unfinished foreign dependency. |
| O05 | Capacity W/C reached and one beyond; prompt refusal, closing accounting and reclamation before readmission; response routing with equal numeric IDs in different sessions. |
| O06 | Qualified large input/read range and frame boundaries, with bounded application working sets as total size grows; bounded edit replay separately. |
| O07 | Slow input, slow result consumer and blocked diagnostics; upstream pressure and local deadlines, no hidden accumulation or fabricated success. |
| O08 | Socket/authentication lifecycle on actual macOS/Linux, wrong peer/failed connect, no universal buffer-size admission assumption. |
| O09 | Finish/result-loss and bounded shutdown preserve known/unknown outcomes, no replay or guessed deletion; ordinary reopen reads published roots. |
| O10 | Aggregate bridge/C1/C2/telemetry memory with concurrent writers, plus separately scoped OS observations; no overlapping metric sums or false exact peaks. |
| O11 | Chosen schema/catalog validates collisions, same-save dependencies, allocation and metadata ordinals; bounded final transaction and lookup; older Store compatibility explicit. |

Use short, barrier-controlled functional cases, streaming generation and incremental
validation. Build fixtures once where safe and keep each sample's mutable Store
independent. Retain every failed attempt and exact source/binary/profile identity.
Do not shrink a frozen performance workload or turn warm preparation into measured
product work. Large-input correctness and bounded-memory qualification (O06) remain required
for #192. Comparative throughput/latency campaigns belong to #193; they do not
absorb an unfinished product acceptance gate. If a required size cannot fit its
declared verification budget and no qualifying identity-matched proof can be
reused, record NOT_RUN and leave that size unqualified rather than shrinking the
case, increasing its timeout or moving the missing proof to #193.

## 8. Concrete freeze record still required

Before dependent implementation, record D01 catalog/schema and compatibility,
D02 save/pack/ordinal allocation and abort ownership, D03 bounded transaction
arbitration and collision/publication proof, D04 numeric C/W/memory/load/time
profile, and D05 session identity/deadline/shutdown semantics. These are the
remaining engineering decisions; multiple writers, accepted duplicate bytes and
no durability/resume are not open questions.

This revision requires deliberate C2 work. Removing the service atomic flag or
relabeling a multi-connection refusal test does not implement it. Preserve C1/C2
algorithm ownership, publish explicit integration checkpoints, and qualify affected
public contracts before claiming completion.
