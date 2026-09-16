# Cluster 1/2: source audit, isolation and simplification plan

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Reviewed source: `a8a1ba848429d5f2fbba83c2de22dada8c29def9`.
Three delegated read-only reviews covered canonical construction, physical
storage, and runtime/timer integration; the coordinating review covered public
workflows, CLI/materialization and compatibility. No product implementation,
build, benchmark or fault-injection run was performed for this audit.

Related: [joint design](content-storage-co-design.md),
[logical content](canonical-content.md), [physical storage](object-storage.md),
[timer specification](telemetry.md), [repository layout](repository-layout.md),
[design issue #160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

Owner scope update after this audit: [diff, conflicts and conflict resolution](diff-conflict-deferral.md)
move to v0.2.0. The source findings still describe the reference, including its
comparison code; the current target has seven components and excludes that feature
family. Internal equality, mutation accounting and path lookup remain.

Subsequent I/O direction: the [content I/O contract](content-io.md) and
[follow-up memory audit](content-io-memory-audit.md) supersede the original plan
to carry generic read-your-writes spill into the replacement. Use finalized
output, bounded unfinished state and operation-wide budgets. Multi-edit finality
and namespace reference ordering remain proof gaps; source observations of spill
below describe the reference, not a requirement to retain that component.

## 1. Decision

Co-design the canonical-object boundary, bounded ownership and failure semantics.
The later [Stages 0–2 handoff](stages-0-2-handoff.md) selects layerfs-content and
layerfs-storage; only telemetry is implemented. The boxes below remain
responsibilities rather than a one-to-one crate inventory.

```text
Application / SDK / CLI / agent integration
                    |
Workflow owner: input acquisition, identity reservation, publication, recovery
                    |
        stable changes + explicit resources
                    v
+---------------- CLUSTER 1: CANONICAL CONTENT ----------------+
| Object framing / identity / references                      |
| Small-content representation / CDC / immutable extent COW   |
| Directory / inode / metadata trees                         |
| Ordinary reads / local equality / validation               |
+---------------------------+--------------------------------+
                            |
       canonical IDs + bytes + references, in bounded batches
       authenticated reads; unfinished nodes remain inside C1
                            |
+---------------- CLUSTER 2: PHYSICAL STORAGE ----------------+
| One admission/session owner                                |
| Membership + exact reuse + dependency validation           |
| Physical base selection + delta/compression                 |
| Pack framing / placement / locators                        |
| SQLite transactions + authenticated reconstruction         |
+---------------------------+--------------------------------+
                            |
                 explicitly owned database/backing
```

CAS identity and the canonical read/write contract belong to logical semantics;
the physical provider implements storage, reuse and reconstruction. Mutable live
Workspace state, capture generations and checkpoint installation remain outside
these clusters even though workspace-core contains portable data structures.
History chooses publication semantics; physical storage owns the transaction
mechanism. Their work may need the same SQLite transaction.

## 2. What actually crosses the boundary

| Input/resource | Required meaning |
| --- | --- |
| Stable semantic changes | Bytes/ranges, metadata and final edits; no live Workspace object |
| Canonical profile | Explicit supported format/CDC/namespace choices; no provider defaults that silently change identity |
| Identity context | Already-authorized/reserved inode identities where needed; reservation remains charged to the full operation |
| Canonical reader | Authenticated canonical objects, with bounded batch access and clear borrowing/lifetime |
| Input range reader | Exact bounded reads into caller-owned buffers; resource placement belongs to the selected adapter |
| Unfinished algorithm state | Bounded active nodes retained until final; namespace reference ordering needs a separate explicit bound, with its mechanism still open |
| Output destination | Finalized canonical objects delivered in bounded batches; publication only after complete successful construction |
| Storage session | One owner for membership state, encoding resources, transaction cohorts, rollback/retention and cleanup |

An independently returned candidate includes its root, logical file lengths and summaries and
plain completion correlations. It does not expose SQL guards, live mounts, daemon
connections, Docker configuration or writable Store authority to the logical builder.

The old [CandidateInputs](../../../../../crates/layerfs-workspace/src/changes.rs#L594)
holds concrete Store/SnapshotReader/Workspace/spool context. The old
[ObjectStore](../../../../../crates/layerfs-content/src/object/access.rs#L64)
mixes output, format flags, allocation and physical hints. Store also duplicates
read boundaries as [ObjectSource/CoreReader](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L923).
Reduce those capabilities to actual requirements; do not wrap every old type in
another interface. Explicit input/output is the isolation mechanism.

Three claims must remain distinct:

```text
object.construct from supplied bytes          no DB or runtime required
file/tree.construct with supplied providers   no hidden canonical Store writes
file/tree.construct with no SQL provider      qualify reads, ordering and finality
```

The current [scratch index](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L687)
can use SQLite. A no-op sink or an unbounded HashMap does not prove a valid large
construction path: later operations can read objects produced earlier.
The existing complete-file finalized writer already avoids this requirement.
Extend that property with proof; do not merely replace a required store with a
discarding consumer in algorithms that still need intermediate nodes.

## 3. Preserve the actual algorithms

CDC and delta are different decisions. Delta is not exclusive to small files:

```text
LOGICAL CONTENT                              PHYSICAL STORAGE

small file -> SmallContent -----------------> FULL or prefix delta
                                                     |
large file -> FastCDC -> chunk objects ------> FULL or prefix delta
                        extent/tree objects          |
                                              compression / packs / SQL
```

CDC chunks also use native prefix encoding in
[prepare_native](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L574).
Changing to "large means CDC without delta" loses existing behavior.

Notation: B = bytes scanned, N = entries/chunks/pieces, K = edits, H = tree height,
V = pages actually visited, D = frontier IDs, P = frontier pages. These are
source-derived work descriptions, not measured performance guarantees.

| Mechanism | Work/resource contract to retain |
| --- | --- |
| Canonical encoding + identity | O(B) byte work; preserve exact framing and domain-separated identity; avoid additional full copies/hash passes |
| FastCDC | O(B) scan with frozen 8/16/32 KiB min/target/max and exact gear/masks; bounded scanner buffers |
| Streaming file build | O(B + N) work; pending structural descriptors grow with bounded fanout and height, not total payload |
| Extent COW | Scan replacement bytes and touched mapping structure; reuse retained payload slices instead of reading/rechunking unchanged suffixes |
| Sorted namespace edits | Work follows visited pages plus K; preserve subtree identity pruning, 8 KiB nodes, depth <=31 and bounded scratch; worst case can visit the whole tree |
| Logical diff/reconcile (reference; deferred to v0.2.0) | Equal identities prune where valid; hardlink/inode changes and semantic equality can require more work, including full-file comparison |
| Live piece tree | Persistent treap with expected logarithmic navigation/path copying; worst O(N) without a balancing proof; preserve piece and inline-byte limits |
| Physical encoding | Preserve exact eligible-base rules, bounded candidate search, chain-depth/byte limits and codec parameters |
| Storage lookup/write | Preserve batched queries, demanded BLOB ranges, existing absence proofs and bounded transaction cohorts; no per-object full-store scan |

Key sources: [CDC](../../../../../crates/layerfs-content/src/file/cdc/gear.rs#L7),
[streaming builder](../../../../../crates/layerfs-content/src/file/rope/build.rs#L14),
[COW edit](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L110),
[namespace batch](../../../../../crates/layerfs-content/src/tree/batch.rs#L12),
[live piece tree](../../../../../crates/layerfs-workspace-core/src/file_edit.rs#L1178).

### Correct two misleading complexity assumptions

Snapshot capture is not O(1). It clones dirty IDs and incorporates touched directory
edges; retained IDs/state scale with the changed frontier. It avoids full payload
copies, but that does not make all metadata work constant. See
[capture_frontier](../../../../../crates/layerfs-workspace-core/src/frozen.rs#L39).

Snapshot pagination repeats whole-frontier work today:

```text
Current, EACH page:
  collect all D IDs -> scan from beginning -> encode next page -> decode last ID

Proposed, EACH page:
  range seek after cursor -> encode bounded page -> retain last ID while encoding

ID enumeration: O(D * P) -> O(P log D + D)
Temporary ID vector per page: O(D) -> bounded page
```

This excludes per-record encoding/lookups. Sources:
[frontier_ids](../../../../../crates/layerfs-workspace-core/src/frozen.rs#L67),
[SNAP_RECORDS](../../../../../crates/layerfs-fuse/src/live_owner.rs#L2801).

### Source limits to preserve or explicitly qualify

| Resource | Existing source limit |
| --- | --- |
| Canonical admission cohort | <=8,191 objects, canonical bytes <4 MiB |
| Physical preparation batch | <=512 objects, multi-object bytes <=512 KiB; separate bounded singleton case |
| Preparation ownership | 6 MiB canonical/pack allowance plus 2 MiB association/scratch allowance |
| Ordinary physical pack/group | 256 KiB pack, 64 KiB ordinary group; bounded native/small special roles |
| Native prefix chain | <=4 edges, <=1 MiB raw closure, additional encoded/decoded read-work bounds |
| Small-content delta chain | <=8 edges, <=512 KiB canonical closure, <=256 KiB encoded closure |
| Generic delta search | 512 trials / 16 MiB charged matching work per prepared batch |
| Live read windows | <=1 MiB remote fetch, 8 MiB shared remote window, 32 MiB immutable read cache |
| Timer | 1,024 nodes / 32 levels / 128 label bytes per assembled report |

Sources: [cohort](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2306),
[prepare bounds](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L183),
[prepare ownership](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L345),
[native reconstruction](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L1351),
[small chains](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L6),
[search budget](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1677),
[remote windows](../../../../../crates/layerfs-workspace/src/snapshot_input.rs#L60),
[immutable cache](../../../../../crates/layerfs-fuse/src/immutable_read_cache.rs#L8).

These limits are not total process/cgroup peaks. Account for simultaneously live
buffers, metadata, codec workspaces, scratch, SQLite cache, OS page cache and timing
records. Disk-backed scratch can grow even when resident memory is bounded.
Complexity preservation requires work counts and qualified time/space/memory
measurements; matching Big-O alone does not justify doubling scans or copies.

## 4. Aggressive cuts, with the replacement obligation

| Candidate | Source | Proposed removal / preserved obligation |
| --- | --- | --- |
| Batch failure followed by point replay | [directory replay](../../../../../crates/layerfs-workspace/src/changes.rs#L957), [inode replay](../../../../../crates/layerfs-workspace/src/changes.rs#L3040) | Plan bounded chunks before effects; one sorted engine. Preserve success for supported inputs; do not replace formerly valid operations with capacity errors without redesigning the contract |
| Store temporary inode records, then read/decode into inline values | [producer](../../../../../crates/layerfs-workspace/src/changes.rs#L2950), [existing value API](../../../../../crates/layerfs-content/src/tree/batch.rs#L1153) | Pass finalized inode values directly to compact leaf construction; retain final-value/hardlink semantics |
| Repeated compact lookup descent | [lookup_many](../../../../../crates/layerfs-content/src/tree/inode/table.rs#L77) | Bounded waves sharing ancestor/group work |
| Re-encode authenticated payloads just to decode them | [file length](../../../../../crates/layerfs-content/src/file/content.rs#L60), [directory read](../../../../../crates/layerfs-content/src/tree/directory/read.rs#L65) | Use the authenticated canonical reader or precise validated payload decoder |
| Encode pages to discover their size | [directory validation](../../../../../crates/layerfs-content/src/tree/directory/validate.rs#L22), [metadata builder](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs#L82) | Exact checked width accounting; preserve structural/underfill validation |
| Allocate inner content then copy into canonical envelope | [small content](../../../../../crates/layerfs-content/src/file/content.rs#L69) | One correctly sized canonical buffer; preserve bytes/IDs |
| Compress, clone, decompress solely to calculate original metadata digest | [metadata pool](../../../../../crates/layerfs-layerstack-store/src/objects/admission/metadata_values.rs#L120) | Return exact digest while the encoder owns the original bytes; independent decoder proof stays in tests |
| Assemble retained tail, then assemble merged pack again | [tail](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1268), [append](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2417) | Choose exact placement before final assembly; retain packing density and locator correctness |
| Rebuild uniqueness containers after a private owner proved uniqueness | [prepare](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L204), [checked output](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4229) | Carry a sealed invariant internally; preserve exact byte collision checks at untrusted boundaries |
| Workspace/Store arguments repeated through admission wrappers | [WorkspaceAdmission](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4451), [PreparedAdmission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L162) | One admission owner; workflow owns Workspace identity and publication policy |
| Old POSIX forwarding proxy alongside active LiveOwner | [proxy client](../../../../../crates/layerfs-fuse/src/proxy_client.rs), [helper](../../../../../crates/layerfs-fuse/src/bin/layerfs-fuse.rs#L60) | Repository callers of ProxyHost/ProxyClient are exports/tests; do not inherit inactive implementation into core. Review exported compatibility separately |
| Unused batch-to-serial transport fallback | [call_batch](../../../../../crates/layerfs-fuse/src/live_transport.rs#L462) | Remove dormant path; no product caller found in this repository |
| Local call encoded/copied/decoded through wire machinery | [local transport](../../../../../crates/layerfs-fuse/src/live_transport.rs#L570) | Local adapter calls the same typed validated operation body; preserve scheduling and admission |
| Host/remote Any downcasts in input reads | [read_backing_exact](../../../../../crates/layerfs-workspace/src/file_io.rs#L193) | Exact bounded range-reader capability, with placement/caching in the adapter |
| Capture-as-optimization silently abandoned | [capture](../../../../../crates/layerfs-workspace/src/capture.rs#L60) | Candidate for one frozen-input construction route; qualify lost write-time preconstruction overlap before claiming equal latency |

Do not reintroduce already-eliminated work: locators from membership queries can
feed reads; same-session absence epochs can avoid redundant negative queries;
native records can share group-directory reads. Preserve these lookup results across new
interfaces. An interface that hides them may increase SQL/RPC/read amplification.

Outer presentation also has costs to keep outside the clusters: CLI owner probes
open a socket before the actual call ([runtime](../../../../../crates/layerfs-cli/src/runtime.rs#L174));
SDK Diff spools then decodes output ([request](../../../../../crates/layerfs-sdk/src/request.rs#L31));
CLI [Diff](../../../../../crates/layerfs-cli/src/lib.rs#L559) and
[Query](../../../../../crates/layerfs-cli/src/lib.rs#L537) accumulate all pages.
These need their own facade/streaming decisions, not accidental inclusion in a
canonical-construction or SQLite-only measurement.

## 5. No hidden fallbacks

Adopt explicit selection before effects, then success or an explicit error:

```text
selected operation + supported input + reserved bounded resources
                               |
                         one implementation
                         /                \
                     success          explicit error

No catch-error -> switch executor / rewind / try old version / fake success.
```

Concrete removal targets:

- The later owner [single-attempt rule](physical-encoding-and-packing.md#one-attempt-no-retries)
  removes retries as well as alternate execution paths: zero SQLite busy timeout,
  no SDK/transaction replay and no stale-absence refresh/reprepare. Planned
  capacity/representation choices remain; actual execution failures propagate.

- Pack append currently treats every assembly error as a reason to open a new
  pack ([objects.rs](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2417)).
  Compute valid size first: fit -> append, valid too-large -> new pack, invalid -> error.
- Native prefix encoding maps broad Io errors into FULL selection
  ([admission.rs](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L764)).
  Represent planned eligibility/budget explicitly; propagate unexpected failures.
- Docker Engine/CLI selection uses availability
  ([execution](../../../../../crates/layerfs-workspace/src/execution.rs#L195)).
  Validate one chosen execution adapter at setup; unavailable means error.
- History identity creation replaces entropy failure with time/PID/counter hashing
  ([ids](../../../../../crates/layerfs-layerstack-store/src/ids.rs#L153)).
  Supply a valid identity or use a fallible supported identity provider.
- Cache poison or speculative-read failure must not masquerade as an ordinary
  miss ([windows](../../../../../crates/layerfs-workspace/src/snapshot_input.rs#L95),
  [prefetch](../../../../../crates/layerfs-workspace/src/live_backing.rs#L86)).

Normal algorithm decisions remain required: empty/small/chunked content, exact
reuse/new insertion, cache hit/miss, tree split/join and FULL/delta selection.
Generic payload spill is now a removal target, not a required capacity path.
For example, native PREFIX must beat FULL after framing,
and small delta must save at least its base-reference overhead. These are planned
encoding outcomes, not recovery from execution failure.

Likewise, SQL abort, checked owned cleanup and authoritative publication state
remain obligations. Lost acknowledgement fails with unknown persistence outcome;
the operation does not poll/replay. Separately requested inspection may establish
what persisted. Never delete or roll back a successful published generation.

### Versions: remove obsolete modes without misclassifying active roles

Schema 10 currently writes all of these:

| Pack number | Active role | Write source |
| --- | --- | --- |
| 1 | Ordinary canonical objects / bounded oversized singleton | [admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1191) |
| 2 | Native file chunks | [admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L824) |
| 4 | Compact SmallContent | [admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L568) |
| 6 | Pooled compact inode metadata | [admission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1191) |

Their dispatch is in [pack.rs](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L116).
Therefore keeping only the largest number does not preserve today's format.
There are separately historical schema 6-9 modes and read-only whole-compaction
support ([whole.rs](../../../../../crates/layerfs-layerstack-store/src/objects/whole.rs#L1)).

Recommended target: one explicitly supported Store profile, rejected before
mutation when unsupported, with no automatic upgrade/downgrade/fallback. Choose
separately whether active physical roles retain their bytes or gain a unified new
framing. Dropping historical support changes the
[released compatibility contract](../../../../versioned/0.1.6/storage-format.md);
record that exact compatibility disposition in the v0.1.7 plan. An explicit
offline conversion or declared incompatibility is a separate decision; this
audit does not invent a working converter or silently reinterpret old data.

## 6. Independently measurable operations

Construction and persistence need separately callable production operations.
Fixtures and correctness checks live outside product src/. Input preparation
excluded from a component diagnostic remains charged to an end-to-end operation
where it is required. A candidate build without publication is not a completed Commit.

| Operation | Timed work | Independent correctness check |
| --- | --- | --- |
| object.construct | Role/framing, canonical bytes and identity, required output handling | Exact fixture bytes/ID and decode equality |
| file/tree.construct | Declared input/base reads, CDC/COW, metadata, scratch and bounded output delivery | Identical roots and reachable objects under fixed identities/profile |
| codec.encode | Supplied base/target through the actual bounded codec; declared init/reuse policy | Decode to exact target and authenticate identity |
| pack.build | Selected groups through final framing and locator creation | Framing/size/locator validation |
| storage.prepare | Membership, base reads, comparisons, encoding and pool work | Valid same-session preparation; no claim of DB-free execution |
| prepared_batch.persist | Remaining tail/placement work, late validation, actual writes and acknowledgement | Real Store readback and failure behavior |
| sql.begin/statements/commit | Actual SQLite calls inside the existing transaction | Exact transaction/rollback and publication outcomes |
| object.save/read | Whole admission or lookup/reconstruction/authentication | Exact reuse/collision, base closure and round-trip bytes |

### A prepared handle is not a portable data object

[PreparedAdmission](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L162)
retains session authority, absence epochs and same-Store state. It must not become
a serialized plan merely to make a benchmark easy. Supplied canonical bytes/base
inputs are enough to benchmark codecs; a real same-Store session is appropriate
for preparation and persistence.

A bounded final batch can already exercise one real transaction through existing
machinery ([fixture demonstrating the route](../../../../../crates/layerfs-layerstack-store/src/objects/admission/native_tests.rs#L28)).
The new implementation needs a useful public component entry point; copying that
private test harness into src/ or exposing test-only hooks is not acceptable.

### SQL lifetime and SQL execution are different measurements

```text
storage session permit acquired ---------------------------------- release
    |
    prepare A
    BEGIN IMMEDIATE +---------------------------------------------+
    insert A        | SQL transaction remains open                |
    release mutex   |                                             |
    produce B       | includes intervening production/encoding     |
    prepare B       |                                             |
    lock + insert B |                                             |
    ...             |                                             |
    COMMIT          +---------------------------------------------+
```

The cohort is retained across calls by
[AdmissionSession](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2238).
[begin_batch](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2309)
opens/reuses it; [publish_inner](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1319)
can drop the connection mutex without committing; the
[next batch](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4577)
is prepared while that transaction remains open.

Therefore use distinct labels:

```text
storage.save
  session.wait
  prepare.batch
    membership / base.read / encode / pack.prepare
  persist.batch
    connection.wait
    sql.begin                  only where BEGIN actually occurs
    pack.place                 includes any real remaining assembly
    sql.packs / sql.locators    defined statement/binding scopes
    sql.commit                 only where COMMIT actually occurs
```

Existing insert timing includes placement, allocation/sorting and assembly; do
not rename it SQL-only. Optional sql.cohort.elapsed means the real BEGIN-to-COMMIT
or rollback interval, including intervening work. The owning session can retain
an explicitly gated monotonic start and attach a completed duration using the
existing report API, or use a naturally enclosing lexical scope after extraction.
That interval overlaps other work and must not be added to its siblings.
Do not add transactions, buffer whole candidates, replay preparation or change
cohort sizes to obtain convenient timing.

The current connection profile is MEMORY journal / synchronous OFF. A measured
COMMIT is its actual acknowledgement, not a crash-durability claim. Inode serial
reservation is a distinct durable effect and remains visible in operation costs.
See [reservation](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L200)
and the [released storage contract](../../../../versioned/0.1.6/storage-format.md).

## 7. Wire the implemented timer without changing execution

Same-thread components receive pending TimingScope handles and call run around
their actual bodies. Example below is an illustrative future component API:

```rust
let (result, timings) = Timing::record("object.create", |root| {
    let object = construct(input, root.child("canonical.construct"))?;
    storage.save(object, root.child("storage.save"))
});
```

Construction and admission can overlap today. Preserve that pipeline and show
its measured structure honestly:

```text
candidate.build
  input.acquire
  identity.reserve
  construct_and_admit
    producer report             recorded in the existing worker
    consumer report             admission/codec/SQL work
  history.publish
  workspace.complete
```

Those children may overlap; the tree is not an additive cost breakdown. Independent
component runs establish isolated costs using explicit inputs and output providers.

The delivered [scope implementation](../../../../../core/crates/layerfs-telemetry/src/timer/scope.rs)
is synchronous and not Send/Sync. Across an existing worker boundary:

```text
coordinator                           existing worker
  timing requested? ----------------> select record OR disabled before work
  existing wait                       run unchanged production operation
                    <---------------- original Result + owned TimingReport
  attach completed report
  then propagate the original Result
```

Do not transfer borrowed scopes between threads or into Send futures. Instrument
synchronous jobs inside the existing scheduler; general async lifecycle timing
needs separate integration. Keep the required single construction producer for
Commit/capture/snapshot and the explicit namespace-init exception. The current
[default selector](../../../../../crates/layerfs-workspace/src/changes.rs#L572)
still uses available parallelism capped at eight; do not assume source default
already satisfies the owner rule or add workers to obtain a faster comparison.

Use static coarse phase/batch labels. One node per chunk/object/SQL row would
quickly exhaust the timer's 1,024-node aggregate budget. Detailed bounded reports
must expose incompleteness; use separate component diagnostics and existing work
counters for exhaustive counts. Do not add another metrics framework to the timer.

Integration details found by review:

- Branch to Timing::disabled before executing unrecorded work. The illustrative
  engine_write in [USAGE.md](../../../../../core/crates/layerfs-telemetry/USAGE.md#L127)
  currently records then conditionally drops the report; copying that pattern
  would pay timing cost while notionally disabled.
- attach/finalization visits and rebuilds report nodes; it is not zero overhead.
  Qualify recording-on/off cost at the selected real component boundary.
- NodeOutcome records Rust Ok/Err. Ok(HeadMoved) or Ok(Busy) is not a successful
  publication; retain the separate domain result.
- Existing [SDK observe](../../../../../crates/layerfs-sdk/src/client.rs#L500)
  can replace the operation result through self.record(...)? at line 519. Optional
  timing/output errors in the replacement must remain separate from product results.
- Format/save outside the measured root. No telemetry-only RPC, one-record-per-child
  transport, Request/Reply model or distributed trace service is introduced here.

## 8. Environment-independent operation, explicit placement

```text
LOCAL
  workflow --> selected direct adapter --> same C1/C2 algorithms --> local Store

CONTAINER OR REMOTE
  workflow --> existing bounded operation transport
                     --> same C1/C2 algorithms --> Store owned at that location

LOCAL FUSE
  kernel --> FUSE/Workspace owner --> stable input adapter --> same C1/C2
```

No component assumes its owner is called the host; host and daemon can both
remain present. Embedded SQLite owns its local I/O. A selected remote SQL adapter
must provide bounded grouped reads, atomic writes and explicit outcome/ownership
semantics; see the [placement contract](content-io.md#6-pluggable-to-later-environments).
Keep DB guards and prepared session authority inside the storage implementation.

Keep construction and admission colocated by default. If separated, use bounded
operation/batch transfer and authenticated batch/range reads. A network operation
per tree node, base edge or SQL statement would preserve an API shape while adding
unacceptable latency/amplification. Environment independence is a dependency
property, not a promise of equal latency across different hardware/topologies.

The current [start_local](../../../../../crates/layerfs-workspace/src/live_backing.rs#L951)
already runs the live owner locally. The new boundary needs ownership/range-reader
extraction rather than a new cloud framework. Reference scratch path selection
uses [temp_dir](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L816);
removing generic staging removes that path dependency on proven routes. Any
retained namespace ordering resource still needs an explicit owner. Optional timing remains ordinary returned
data, carried by a future adapter through its existing completion path.

Existing benchmark placement and cache contracts remain unchanged by this diagram.
Comparisons must use a declared matched topology and acknowledgement boundary.

## 9. Failure boundaries requiring proof before adoption

These are source-inspection risks, not dynamically reproduced bug claims:

| Boundary | Source observation | Required proof/design |
| --- | --- | --- |
| Frozen snapshot transfer | [snapshot_input](../../../../../crates/layerfs-workspace/src/snapshot_input.rs#L285) ignores returned generation and accepts count=0 without requiring done; no final summary-count comparison found | Validate token/generation, cursor progress, counts and exact completion; reject truncated input |
| Lost completion acknowledgement | [COMPLETE_END](../../../../../crates/layerfs-fuse/src/live_owner.rs#L2709) removes snapshot slot before cleanup/final reply; reference replay begin requires that slot | Target no-retry rule returns failure/unknown outcome; retain authoritative completion state for separately requested inspection, never repeat writes or publish a new candidate because acknowledgement was lost |
| Cancellation failure | [remote_commit](../../../../../crates/layerfs-workspace/src/remote_commit.rs#L149) ignores cancel_snapshot errors before settling retained attempt in several branches | Retain/surface unresolved cancellation; do not silently claim release |
| Admission abandonment | [resolve/rollback](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2493) can delete earlier private cohorts and quarantine on cleanup failure | Preserve private ownership, rollback and write quarantine |
| Different publication routes | [direct](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L326) versus [staged](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L423) finality differs | Preserve required atomicity/head-race/retention while consolidating mechanisms |

No-fallback does not permit losing bytes-before-visibility, frozen revision
protection, authenticated closure, collision comparison, or authoritative
publication/completion state. Test those failures through external production-API
tests; do not recreate test-only implementation branches in core/src/.

## 10. Proposed implementation order and acceptance

1. Agree canonical bytes/ID/reference, explicit profile, exact reader, unfinished
   state, finalized output and session/publication ownership contracts. Record the version
   disposition and no-hidden-fallback rule; choose crates only after boundaries.
2. Extract the complete-file finalized-output path through real bounded storage
   save and authenticated readback under core/. It must also run construct-only and save/read-only
   without Workspace/FUSE/daemon. Use existing timer at these actual boundaries.
3. Extend the same path to duplicate/new objects, small/native delta, metadata
   pooling and bulk cohorts. Preserve all active roles selected for support.
4. Bring through file CDC/extents, immutable COW, sorted namespace/inode updates
   and full candidate construction with explicit acquisition/identity/output.
   Prove multi-edit boundaries and namespace ordering under whole-operation limits.
5. Wire workflow publication and selected runtime adapters after their recovery
   contracts are tested. Remove selected duplicate routes rather than wrapping them.

Preservation gates:

- Canonical fixture bytes/IDs, CDC boundaries and retained suffix identities.
- Randomized range/splice and retained-history correctness, metadata/hardlink
  semantics, no-op behavior and exact authenticated reuse. Diff/reconciliation
  feature proofs belong to the deferred v0.2.0 work.
- Same inputs/profile/identity context produce identical roots across selected
  input adapters; construction-only mode performs no hidden canonical Store writes.
- Bounded producer/tree/codec/DB/report and any event-ordering memory/backing; no whole
  candidate or unchanged-file buffering introduced for measurement convenience.
- Compare scanned/copied/hashed bytes, tree visits, query/batch/RPC counts, decoded
  work, pack occupancy and physical output alongside end-to-end elapsed time.
- Missing/corrupt bases, stale absence proofs, head races, transaction failures,
  uncertain completion and cleanup are exercised through actual product paths.
- Qualify matched-source/cache/topology performance against the declared baseline;
  preserve worker policy and all root benchmark rules. No speedup or memory
  equivalence is established by this source audit.
- Follow core production-only src/, external tests, <=200-line thin lib.rs/mod.rs,
  per-commit production LOC reporting and local preflight rules.

Existing proof starting points:
[canonical fixture oracle](../../../../../crates/layerfs-content/tests/canonical_v2_fixture_oracle.rs),
[CDC shifted stream](../../../../../crates/layerfs-content/tests/fastcdc_shifted_stream.rs),
[extent model](../../../../../crates/layerfs-content/tests/extent_model.rs),
[namespace model](../../../../../crates/layerfs-content/tests/namespace_model.rs),
[compact namespace](../../../../../crates/layerfs-content/tests/compact_namespace.rs),
[admission integrity/rollback](../../../../../crates/layerfs-layerstack-store/src/objects/admission/native_tests.rs),
[small chains](../../../../../crates/layerfs-layerstack-store/src/objects/admission/small_chain_tests.rs),
[publication/head-race tests](../../../../../crates/layerfs-layerstack-store/tests/v4.rs).
Port required tests outside product src/; do not import legacy test instrumentation.

## 11. Coverage and limits of the review

| Reviewer area | Production families inspected |
| --- | --- |
| Canonical | All content and workspace-core module families enumerated: object framing/IDs/references, file roles, CDC, rope COW/read/diff, directory/inode/metadata trees, filesystem apply/diff/reconcile, live pieces/namespace/frozen/checkpoint state; construction and scratch bridges traced |
| Physical | Store boundary, schema/SQL, session/cohort lifecycle, membership/admission, every active pack lane, delta search/chain bounds, native/small/metadata/whole reads, candidate/value indexes, spill and history/staging crossings |
| Runtime/timer | Remote Commit and snapshot input, live backing, actual LiveOwner/FUSE paths, local/remote transport, scheduler, spool/cache, selected daemon and Docker execution paths, SDK/Monitor observation, implemented timer and usage |
| Coordinator | Workspace manifests and public operation mapping, CLI owner/control/output paths, materialization/capture ports, released format/durability/compatibility contracts and measurement rules |

The inventory includes the ten existing product packages and the implemented
timer. Tests/oracles were selected for algorithm, resource and failure contracts;
every historical fixture was not read. Not every daemon supervisor/resource-sampler
branch or initialization/import helper received a line-by-line audit. This is a
deep production-family/call-path review, not formal whole-repository verification.
Third-party SQLite/Zstd/BLAKE3 internals were not audited. No product code changed,
no new candidate crates were chosen, and no performance results were collected.
