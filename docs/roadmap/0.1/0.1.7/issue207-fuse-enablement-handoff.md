# Handoff prompt — #207 enabling the live FUSE mechanism: enablement, universality, and pair 3 incorporation

> Status: Research; informative and not a product contract. Dated continuation
> checkpoint, 2026-09-20, revised after **PR #200 merged** (`7b8a7d9d3`, pair 3's
> service/bridge/daemon foundation is on `main`). This prompt carries existing
> measurements and three bounded investigations; it is **not** a new measurement, not
> a design freeze, and not a release claim.
>
> **Read first, in this order:**
> [`06-future-fuse-and-cloud.md`](../../../../core/docs/architecture/proposal/service-daemon-transport/06-future-fuse-and-cloud.md)
> — pair 3's own forward design for pair 1, which is authoritative for the
> responsibilities and file layout below — then
> [`fuse-mechanism-from-source.md`](evidence/stage-6-history-205-save-split-20260920T081022Z/fuse-mechanism-from-source.md)
> for the reference mechanism's shape. **This prompt is subordinate to that
> document**; where they differ, it wins and the difference is a bug in this prompt.

## Mission and decision rule

Continue [#207](https://github.com/Ephemeral-AI-Lab/layerfs/issues/207), which asks for a
**competitive result against v0.1.6 behind FUSE** on matched semantics. That issue
established that the comparison is currently unmatched: v0.1.6's `create` (the FUSE mount)
is **9–11 ms and flat across K10 → K100** but materialises no content, while the core pays
`content` 1.585 s + `filesystem` 5.146 s + `accept_loop` 8.820 s eagerly, and v0.1.6's own
deferred read cost was never recorded (`historical_access` declares performance `N/A` in
all six cases).

The mechanism's shape is already read from source and diagrammed in
[`fuse-mechanism-from-source.md`](evidence/stage-6-history-205-save-split-20260920T081022Z/fuse-mechanism-from-source.md)
and posted to [#207](https://github.com/Ephemeral-AI-Lab/layerfs/issues/207#issuecomment-5749557789).
**Read it first; do not re-derive it.** This prompt adds the three questions that document
does not answer.

The three investigations below are **design-first**. #179's boundary is explicit: *"Design
first, measure second. No claim in this pair is measurable until a mount exists."* Do not
produce a latency claim from any of these. Produce the decision set, and where a decision
needs a number, say which measurement would supply it and on which arm.

## Owner rulings that persist

- **Do not close #190, #205 or #207.**
- **Design first, measure second.** #179 owns the design; the implementation round after it
  owns the numbers.
- **One operation per round.** Do not bundle FUSE enablement, a portability abstraction and
  a pair 3 refactor into one change.
- **Preserve authentication, bounds, error/visibility behaviour and the single construction
  worker.**
- **No durability claim.** MEMORY journal, `synchronous = OFF`, no `fsync`, no WAL. A
  `write()` acknowledgement must not imply otherwise.
- **No cache growth without an owner decision**; no invented cold stance; no pre-touched
  inputs; no shrinking a selection.
- **`init_namespace` keeps its multi-worker exception** and its 2.7 s cold Init target. It
  is not "a commit with a bigger input".

## 0. What is already established (do not re-derive)

| fact | value | source |
| --- | --- | --- |
| v0.1.6 mount (`create_workspace_session`) | **9–11 ms, flat K10 → K100** | `issue154/final-complete-matrix.json` |
| v0.1.6 mount does **not** materialise content | branch pin + dir + FUSE attach + tree-metadata read | `crates/layerfs-workspace/src/lifecycle.rs:452` |
| core eager cost, stride10 | `content` 1.585 s, `filesystem` 5.146 s, `accept_loop` 8.820 s | this lane's receipts |
| core content construction without predecessor reads | 1.039 s, **flat at 24–40 µs per changed path** | `nopred` control arm |
| read tiers | namespace cached; content ≤ 8 KiB prefetched; **> 8 KiB uncached at every layer** | `live_wire.rs:17`, `live_backing.rs:81` |
| the 8 KiB threshold | `IMMUTABLE_PREFETCH_FILE_BYTES = 8 * 1024` | `crates/layerfs-fuse/src/live_wire.rs:17` |
| snapshot cache | 8 MiB FIFO, **skips > 1 KiB `CHUNK_MAGIC` objects by design** | `layerfs-layerstack-store/src/workspace.rs` |
| write path accumulator | three stacked tiers: live piece tree, host spool, capture thread at commit | `crates/layerfs-workspace/src/capture.rs`, `cow_tree.rs` |
| core has no runtime/projection crate | only `layerfs-content`, `layerfs-storage`, `layerfs-telemetry` | `core/crates/` |

**The single most useful unmeasured experiment, ready to run once a mount exists:**
`execve` of a binary **above 8 KiB**, repeated. It is the most common shell operation, it
falls in the uncached band *by construction*, and it is the case where an eager model should
win outright. If the core cannot beat a lazy loader there, it will not beat it anywhere.

## 0.1 What pair 3's forward design already settles — do not re-decide these

Pair 3 landed with its own design for pair 1:
[`06-future-fuse-and-cloud.md`](../../../../core/docs/architecture/proposal/service-daemon-transport/06-future-fuse-and-cloud.md).
It is **authoritative** for the responsibilities and layout below, and it changes three
things this prompt originally treated as open.

**a. The responsibility split is decided.** Workspace owns namespace and inode identity,
open-file lifetime and pending changes, local read-your-writes, bounded backing/cache
ownership, and stable submission generation. It must **not** recreate v0.1.6's combined
`LiveOwner`: borrow its behavioural requirements, and exclude its reverse snapshot-pull
protocol, transport authentication, host-control framing and automatic completion
redelivery. The daemon creates and wires Workspace, the FUSE adapter and the bridge client,
and hosts Workspace without duplicating its overlay or generation registry. The FUSE adapter
delegates to Workspace and **never bypasses it with a remote request per callback.**

**b. The operation mapping largely exists.** The document's callback table already assigns
per-callback work and the possible service side for Lookup/getattr/readlink, Read, Write,
Readdir, Create/rename/unlink/setattr, Save/submission and Flush/fsync. §1.1's first
deliverable therefore becomes **reviewing and filling gaps in that table**, not authoring one
from scratch.

**c. There is a hard C1 constraint that shapes the whole write path.** FUSE can overwrite
bytes written earlier in the same dirty generation, but C1's `EditStream` **rejects** an edit
that reaches into bytes an earlier edit in the same stream introduced —
`InvalidEdit { what: "range inside an earlier replacement" }`
(`core/crates/layerfs-content/src/file/edit/input.rs:10-15`). Therefore Workspace must
**lower its final piece/namespace state to a supported stable operation input**; it cannot
forward the chronological FUSE write list to `apply_edits`. The document is explicit that
this lowering is a Workspace algorithm and that it must **not** be worked around by sorting
or coalescing raw edits in the bridge, nor by silently flattening to a complete file to evade
a replay limit. Note `MAXIMUM_EDITS_PER_OPERATION = 4_096` in the same file — the same
ceiling #179 cites for bindings.

**d. The file layout is specified, with ceilings.** Future work goes under
`core/crates/layerfs-daemon/src/workspace/` and `.../fuse/` in the responsibility folders the
document lists — one FUSE trait implementation delegating to helpers, not competing
implementations. Every production file ≤ 999 physical lines; every `lib.rs`/`mod.rs` ≤ 200
and declaration/delegation only. FUSE and its dependency are **platform-gated**, and
unsupported requested mount capability **fails explicitly** rather than no-opping.

**What this leaves genuinely open** — and what §1–§3 below should now focus on: the numeric
overlay ceiling and flush policy, mount topology, the fate of the 8 MiB cache and 8 KiB
prefetch threshold under a cross-boundary topology, round-trip counts per operation, and the
lowering algorithm's canonical/profile consequences.

## 1. How to enable FUSE

**Question:** what is the minimum path from today's three-crate core to a mounted,
readable workspace?

### 1.1 What must be decided (design, not code)

- **The operation set.** Enumerate every FUSE callback that will be served and name the C1
  or C2 call each becomes — or state why it has no call. `#179`'s acceptance criteria
  require this and it is the first deliverable.
- **Mount topology.** One mount with N workspaces (routes by path prefix, one kernel
  session) or N mounts (isolation, N sessions). The deciding question is whether one
  workspace may block its neighbours — the same question the single-writer discussion
  raised one level down.
- **Where the accumulator lives.** Three tiers exist in the reference; decide which belong
  to the FUSE implementation and which to a separate runtime layer a second projection
  (materialization) could share. **See §3 — pair 3 constrains this.**
- **The overlay ceiling and flush policy, numerically.** Bound unflushed bytes per consumer
  and state the breach behaviour (refuse, force flush, block the writer). Size- vs
  time-triggered, and whether a flush blocks the triggering callback.
- **What `commit` returns**, given nothing is durable. **This is #180's answer to give, not
  yours** — record the dependency rather than inventing a durability posture.
- **The 4,096-binding ceiling.** `rename` needs complete final bindings of both directories;
  a directory whose effective subtree exceeds the ceiling **can never be renamed** and the
  refusal surfaces as a cycle-check limit. A shell runs `mv` constantly. Raise it with
  evidence or document it as a user-visible limitation — do not leave it as an
  implementation bound.

### 1.2 The staging question that decides the rest

```
  today (reference, co-located):        FUSE ──► LiveWorkspace ──► Store   (one process)
  pair 3 topology (cross-boundary):     FUSE ──► daemon ──bridge──► service ──► C1/C2 ──► SQLite
                                          container                     host
```

**The reference's 8 MiB cache and 8 KiB prefetch threshold were tuned with reader and store
co-located.** In pair 3's topology every uncached content read crosses a process boundary and
a transport. That does not make the threshold wrong — it makes it **unjustified**, because the
constant's cost model changed. Decide whether the threshold becomes a function of transport
latency rather than a byte count, and say what measurement would settle it.

### 1.3 Do not

- Do not claim a mount exists before one does.
- Do not produce a latency number from this investigation. `#179`: *"No claim in this pair is
  measurable until a mount exists."*
- Do not move construction into setup to make a timed region look fast. Deferring to read
  time is legitimate design; moving work outside the timer is not.

## 2. How to make the mechanism universal across host, Docker and remote

**Question:** what must the mechanism *not* assume so it works when the daemon is in a
container, on the same host, or remote?

### 2.1 The portability requirements pair 3 already established

Reuse these; do not restate them as new. From [#181](https://github.com/Ephemeral-AI-Lab/layerfs/issues/181):

- requests carry **authorized logical Store identities, roots, stable input and operation
  generations** — not native paths, process pointers or SQLite handles;
- **direct and stream delivery use the same handlers**; remote transport must not introduce
  a second implementation of content or storage behaviour;
- bounded payload delivery defined **independently of native sockets**, permitting a future
  WebSocket/HTTP endpoint, and not requiring a permanently running process or permanently
  open connection for operation identity;
- preserve **missing-content, invalid-input, capacity, ownership and unknown-outcome**
  distinctions; peer authentication and operation authorization are **independent of content
  hashes**.

### 2.2 What the FUSE mechanism must add to that

- **Where the mount lives** in each topology. The kernel mount is necessarily where the
  callers are; state that explicitly rather than assuming the daemon's localhost is the host
  endpoint — pair 3 already warns that it is not.
- **What the accumulator holds when the daemon is remote.** The live piece tree is
  container-local by nature. The host spool is a *host* resource in the reference — decide
  where it goes when they are not the same machine, and whether it is durable enough for the
  role it plays.
- **Round-trip count per operation as a first-class metric.** In the cross-boundary topology
  an uncached content read is a bridge round trip. Enumerate the round trips for
  `lookup` / `stat` / `readdir` / `read` / `execve` and state which are avoidable by batching
  or prefetch. This is the number that decides whether the 8 KiB threshold survives the move.
- **Behaviour under a slow or dead peer.** Pair 3 requires bounded transfer, backpressure and
  connection-failure reporting. State what a FUSE callback does when the host is unreachable:
  the distinction between a timeout and missing content must survive to the caller.

### 2.3 Do not

- Do not build a universal provider registry. Pair 3 explicitly scopes that out: *"neither a
  universal provider registry nor every deployment combination is required."*
- Do not claim a cloud/serverless backend exists. Portability gaps are to be **documented**,
  not implied away.
- Do not assume a container's localhost is the host endpoint, and do not let FUSE callbacks
  acquire SQLite credentials, SQL or physical pack logic.

## 3. How to incorporate with the pair 3 implementation

**Question:** pair 3 has **landed** — PR [#200](https://github.com/Ephemeral-AI-Lab/layerfs/pull/200)
merged as `7b8a7d9d3`, and `core/crates/` now carries `layerfs-bridge`, `layerfs-daemon`
and `layerfs-service` beside `layerfs-content`, `layerfs-storage` and `layerfs-telemetry`.
What does pair 1 consume, what does it add, and what must it not duplicate?

**The boundary this prompt predicted is confirmed in the landed code:** `layerfs-bridge`
contains no FUSE, overlay or accumulation logic, so the constraint in §3.2 stands as
written rather than as a forecast.

### 3.1 The authoritative order is pair 3 → pair 1 → pair 2

From the execution contract: implementation is **pair 3 → pair 1 → pair 2**. Pair 3's issue
states the sequencing rule directly:

> Before implementing pair 3, agree only the **initial operation inputs, results, identity and
> completion semantics** with the later pairs. Their full implementation is not a prerequisite.

And its own forward statement: *"Later, pair 1 adds FUSE and Workspace creation/state to the
daemon."*

### 3.2 The boundary, component by component

| component | owns (pair 3) | must **not** own |
| --- | --- | --- |
| `layerfs-service` (host) | authorized logical operation execution, Store lifetime, C1/C2 composition, storage outcomes | FUSE callbacks, client file handles, **pending Workspace overlays** |
| `layerfs-daemon` (container) | execution-side process, configured service connection, bounded request/result handling and cleanup; **later hosts pair 1** | SQLite credentials, SQL, physical pack logic, guessed history semantics |
| `layerfs-bridge` | shared operation contract and endpoint adapters; versioned framing, bounded transfer, backpressure, failure reporting | filesystem algorithms, history/publication decisions, **automatic mutation replay** |

**Three consequences for pair 1, in order of severity:**

1. **The accumulator cannot live in the bridge.** The bridge must not own "automatic
   mutation replay", so the FUSE accumulator belongs on the daemon side (and in the runtime
   layer pair 1 designs). This is the answer to #179's open item "where the accumulator
   lives" — it is constrained, not free.
2. **Keep C1/C2 together at the host.** Pair 3 requires that canonical provider/consumer
   calls remain **local**: *"do not turn individual canonical-object reads or emissions into
   network messages."* The FUSE read path therefore cannot resolve canonical objects one at a
   time across the bridge; it must request in the units pair 3 defines. State the units.
3. **Consume pair 3's operation surface, do not invent a parallel one.** The initial surface
   is: read/inspect, complete-file construction and save, known file edits and save, and a
   filesystem update against a prepared base. Map the FUSE operation set onto **that**
   surface, and where a FUSE operation has no counterpart, say so explicitly rather than
   adding a private call.

### 3.3 What pair 1 must not duplicate

- Do not re-implement transport, framing, authorization or connection lifetime — pair 3 owns
  them.
- Do not add a second content/storage behaviour for the remote path. Pair 3: *"remote
  transport does not introduce a second implementation of content/storage behavior."*
- Do not depend on private content/storage algorithms, SQL or pack layouts. Pair 3: *"do not
  make integration depend on private content/storage algorithms, SQL or pack layouts."*
- Do not treat saving objects as a logical Commit. Pair 3 is explicit: *"Saving objects
  produces an explicit root and storage outcome, not a logical Commit or branch-head
  update."* Commit semantics are **#180's**.

### 3.4 What to agree with pair 3 before writing code

Only the four items pair 3 names: **initial operation inputs, results, identity and completion
semantics**. Everything else — FUSE callbacks, the accumulator, mount topology — is pair 1's
and does not need pair 3's implementation to be finished.

## 4. Deliverables for this handoff

1. **The FUSE operation set**, each op naming the C1/C2 call it becomes or why it has none —
   #179's first acceptance criterion.
2. **The accumulator decision**: which of the three reference tiers pair 1 owns, where each
   lives in the pair 3 topology, and the numeric overlay ceiling and flush policy.
3. **The mount topology decision**, with the isolation trade stated rather than assumed.
4. **The portability statement**: where the mount, the accumulator and the spool live in
   host / Docker / remote, and the round-trip count per operation in the cross-boundary case.
5. **The 8 KiB threshold's fate**, with the measurement that would justify keeping or
   replacing it, and an explicit statement that it is currently unjustified rather than
   wrong.
6. **The dependency list**: which questions belong to #180 (commit acknowledgement, stage
   durability) and which to pair 3, recorded as dependencies rather than answered here.
7. **An issue update** on #207 and #179 carrying the decision set. Do not close either.

## 5. Measurement and resource protocol

Nothing in this handoff is measured. If a later round measures, the standing protocol
applies unchanged:

- one sample per case per arm; fresh `--output` per run; receipts append-only; failures and
  deferrals retained on disk;
- the two global flocks held for the whole resource command
  (`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`); a
  held lock means defer, never wait or interrupt;
- quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and ≥70 % CPU idle on the
  second of two one-second observations; a busy preflight consumes no sample;
- Rust 1.85.1, `--locked`, no third-party edits or vendoring; no CI, and
  `tools/preflight.sh` is permanently retired;
- **no cost moved into setup to flatter a timed region**; no invented cold stance; no
  pre-touched inputs; no shrunk selection;
- **one construction worker** (`LAYERFS_CONSTRUCTION_WORKERS=1`), no second lane or helper
  worker; `init_namespace` is the only multi-worker exception.

## 6. Do not

- Do not close #190, #205, #207, #179, #180 or #181.
- Do not treat the reference's 8 MiB cache bound or 8 KiB prefetch threshold as justified in
  the new topology; they were tuned co-located.
- Do not let FUSE callbacks hold SQLite credentials, SQL, or pack-layout knowledge.
- Do not put mutation replay in the bridge, or resolve canonical objects one at a time across
  it.
- Do not claim a mount, a latency figure, or a durability guarantee that does not exist.
- Do not answer #180's questions (what `commit` returns, whether a stage is durable) from
  this lane
