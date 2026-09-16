# Cluster 1 and 2: overall architecture

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Read the [integrated content-storage design](content-storage-design.md) for
algorithms, packing/compression, transitions, dependency cleanup, Git comparison
and qualification. This document remains the overall component map.

The [generic content I/O contract](content-io.md) folds byte access, authenticated
reads and output ownership into ordinary core calls. Its
[audit](content-io-memory-audit.md) targets logical content/SQLite costs against
v0.1.6 without assuming a future Workspace or transport mode.

This document records the agreed overall architecture for the v0.1.7 replacement:
seven responsibility and measurement boundaries, with three components for
canonical content and four for physical storage. These are components, not seven
predetermined crates. The [Stages 0–2 handoff](stages-0-2-handoff.md) selects
layerfs-content and layerfs-storage; only layerfs-telemetry is implemented.
Detailed component proposals are now documented;
concrete APIs/package boundaries and qualification remain implementation work.

The owner deferred the Comparison/Reconciliation component, public logical diff,
conflict handling and conflict resolution to [v0.2.0 issue #164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164).
See the [deferral review](diff-conflict-deferral.md) for exact omission candidates
and retained internal equality/path-resolution/publication obligations.

Read the [source audit](cluster-1-2-source-audit.md) for current implementation
evidence, complexity/resource limits and exact removal candidates. The
[joint co-design](content-storage-co-design.md) owns the shared design decisions;
[issue #160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160) tracks the work.

## Agreed overview

| Cluster | Component | Main responsibility |
| --- | --- | --- |
| 1: Canonical content | [1. Canonical objects](canonical-objects.md) | Common framing, identity, shared references and checked encoding/decoding primitives |
| 1: Canonical content | [2. File content](file-content.md) | Small content, CDC, extents, immutable file COW and range reads |
| 1: Canonical content | [3. Filesystem tree and metadata](filesystem-tree.md) | Directories, inode values, metadata, path lookup, reference accounting and immutable tree updates |
| 2: Physical storage | [4. Object store](admission-and-persistence.md) | Save/read coordination, membership, exact reuse, dependencies and session ownership |
| 2: Physical storage | [5. Physical encoding](physical-encoding-and-packing.md) | Base-selection policy, FULL/delta encoding, compression and reconstruction |
| 2: Physical storage | [6. Packing](physical-encoding-and-packing.md) | Group/pack framing, size accounting, placement planning and record locations |
| 2: Physical storage | [7. SQLite persistence](admission-and-persistence.md) | Database ownership, schema validation, queries, writes and transaction mechanics |

Use **Filesystem tree and metadata** as component 3's architectural name. The
existing [init_namespace benchmark family](../../../../../benchmark/fs-bench-pro/families/init_namespace/mod.rs#L1)
names an initialization workload, not this component. It can exercise file content,
tree construction and physical storage together. Source terms such as namespace
root or namespace COW retain their technical meaning; the component label does
not rename existing APIs, benchmark IDs or historical evidence.

The two clusters meet through the [finalized-object handoff](finalized-object-handoff.md)
and authenticated reads. It owns object fields, finality, allocation transfer, backpressure
and accepted/constructed/stored completion meanings. Exact implementation APIs and
qualification remain open.
Structural checks live in the appropriate checked decoder/constructor; there is
no separate role-validation component. File/tree codecs remain with their owners.
layerfs-telemetry provides shared optional timing; the operation owner selects
recording and saves completed reports. Workspace/snapshots, workflow publication,
runtime adapters and public interfaces remain surrounding responsibilities.

LayerStack, Branch and logical Commit belong to the history/workflow layer,
outside both clusters. Cluster 2's SQLite transaction COMMIT is a storage
mechanism, not a logical Commit. Its save/read path must require no history
entities. Existing atomic object/history transactions may remain composed in one
database; history owns its commands, tables and publication decisions.

The agreed constraints are independent measurability, environment-independent
algorithms, bounded ownership, no hidden execution fallback, and preservation of
canonical correctness and required complexity/resource behavior. Public logical
diff, conflicts and conflict resolution belong to v0.2.0.

Finalized-object streaming and whole-operation bounds are the current direction;
no generic payload spill/scratch layer is planned. File multi-edit finality still
needs proof. Filesystem construction retains compact reference ordering with
explicit ownership; C2 physical value-index backing needs its own qualification.
Zero temporary storage is not established.

No future Workspace structure is assumed. Checked logical inputs are native C1
entry points; path-command adapters are optional. C2 accepts canonical objects
without a filesystem workflow. Each cluster can run independently with declared
providers and resources, across caller-selected environments.

Exact function signatures, buffer ownership/lifetimes, batching/backpressure,
ordering contracts, transaction composition, supported format policy, instrumentation
granularity and package layout remain open. The detailed sections below are
working notes for those discussions, not approved APIs or completed extraction.

## 1. Whole-system placement

```text
             Public SDK / CLI / application integrations
                                 |
                      Workflow and lifetime owner
               acquisition / identity reservation / history
                    publication / completion / recovery
                                 |
                  stable inputs + explicit resources
                                 v
+------------------ CLUSTER 1: CANONICAL CONTENT ------------------+
|                                                                |
|  1. Canonical objects          2. File content                   |
|  3. Filesystem tree and metadata                                |
|                                                                |
+-------------------------------+--------------------------------+
                                |
             canonical objects and references in bounded batches
             authenticated reads; unfinished nodes stay in C1
                                |
+------------------ CLUSTER 2: PHYSICAL STORAGE ------------------+
|                                                                |
|  4. Object store/admission     5. Physical encoding              |
|  6. Packing                    7. SQLite persistence             |
|                                                                |
+-------------------------------+--------------------------------+
                                |
                    explicitly owned database/backing

Runtime adapters supply placement and input access:
  local calls / FUSE / daemon / container / existing remote transport

Shared timing:
  operation-owned scopes -> completed report -> caller-owned output
```

The arrows show ownership and data flow, not a mandatory sequence of fully
materialized intermediate results. Production may stream bounded batches and
overlap construction with admission. No whole-candidate buffer is implied.

## 2. Cluster 1: canonical content

### Internal relationships

```text
        [2. File content]    [3. Filesystem tree and metadata]
         small content        directories / inode values
         CDC / extents        metadata / namespace COW
         immutable file COW
                |                     |
                +----------+----------+
                           | uses
                           v
                  [1. Canonical objects]
               framing / IDs / references / validation
                           |
                canonical read/output contracts
```

| Component | Owns | Explicit inputs and results | Independently measurable work |
| --- | --- | --- | --- |
| **1. Canonical objects** | Object kinds, canonical framing/encoding/decoding, identity and reference semantics | Role + fields/bytes -> canonical bytes + ObjectId + references; supplied bytes -> validated object | Construct, identify, decode, validate |
| **2. File content** | Small-content representation, CDC, extent mapping, immutable file COW and range access | Stable byte/range reader, selected profile, optional immutable base and final edits -> file root/length + bounded objects | Build a file, splice edits, read a range |
| **3. Filesystem tree and metadata** | Directory bindings, inode values, metadata and immutable tree updates | Base root, sorted semantic changes, metadata, reserved identities, reader/finalized output -> tree root + bounded objects | Lookup, apply sorted changes, build tree/metadata |

These components use explicit readers, bounded unfinished state and final output. They do
not import a concrete Store, writable SQL connection, live Workspace, mount,
daemon connection or Docker configuration. A supplied reader may perform I/O;
that cost belongs to the declared operation where it happens.

Ordinary path resolution, local equality/no-op checks, tree reuse and mutation
reference accounting stay with these three components. They do not form a public
diff/conflict feature, and physical delta matching remains in cluster 2.

The [filesystem-tree proposal](filesystem-tree.md) reuses the existing sorted
engine and compact inline inode values. Directory bindings name scoped inode
identities: a file-content/attribute edit changes its inode-table path, not every
ancestor directory's name map. Attribute trees are logical C1 data; physical
inode-value pooling and metadata delta belong to C2.

### Where CDC and COW belong

CDC is an algorithm within file content. It can have a callable helper and a
timing scope without becoming a separate crate or service.

The [file-content proposal](file-content.md) covers small and large files together:
whole-file reconstruction, replacement-only CDC for known large-file edits,
current-result edit coordinates, range replay for required no-op checks, and
finalized output with whole/chunk delta hints. Reuse split/concat/partition logic;
replace the two encoded structural overlays only after bounded decoded-boundary
finality and canonical equivalence are proved. They already stream payloads and
bound deferred structural data; they are not whole-file payload buffers.

Immutable file COW and namespace COW remain with their respective structures.
The live editable Workspace and its piece/backing lifetime are a separate owner:

```text
live Workspace edits / frozen generation     outside clusters 1 and 2
                         |
                  stable semantic input
                         |
          +--------------+--------------+
          v                             v
   immutable file COW           immutable namespace COW
       component 2                  component 3
```

## 3. Cluster 2: physical storage

### Internal relationships

```text
                [4. Object store and admission]
                 public save/read operation boundary
                 membership / exact reuse / dependencies
                 one admission/session lifetime owner
                             |
          +------------------+------------------+
          |                  |                  |
          v                  v                  v
 [5. Physical encoding] [6. Packing]    [7. SQLite persistence]
  base-selection policy  groups/packs    queries / row mutations
  FULL/delta/compression locations       transactions / connection
  reconstruction        framing        database ownership

Write data: canonical bytes -> encoded records -> packs/locations -> SQL
Read data:  SQL locations -> selected records -> reconstruction -> authentication
```

| Component | Owns | Explicit inputs and results | Independently measurable work |
| --- | --- | --- | --- |
| **4. Object store and admission** | Save/read coordination, membership, exact reuse, dependency checks, session ownership and admission failure/retention | Canonical batches or IDs + selected Store/session -> acknowledged save or authenticated canonical bytes | Complete save/read, membership/reuse, bounded admission |
| **5. Physical encoding** | Eligible-base/representation policy, FULL/delta choice, compression and reconstruction | Supplied canonical target/base bytes and policy -> encoded records; encoded records/base bytes -> canonical bytes | Encode, reconstruct, compare eligible representations |
| **6. Packing** | Group/pack framing, exact size accounting, placement planning and record locations | Encoded records/groups + valid placement context -> pack bytes and locations; pack ranges -> records | Build/extract/validate pack data |
| **7. SQLite persistence** | Connection/database ownership, schema validation, queries, mutations and transaction mechanics | Valid session-owned operations and bytes/locations -> rows/results and transaction outcome | Membership/location queries, writes, BEGIN/COMMIT/ROLLBACK |

The admission owner obtains authenticated base data and coordinates the other
components. Pure codec entry points operate on supplied bytes. Placement and
final validation can depend on same-Store session state; a prepared handle is
not automatically detached or serializable.
The [physical encoding and packing proposal](physical-encoding-and-packing.md)
defines these components' terms, candidate/metadata policies and placement-first
compatible append. The bounded fingerprint-index replacement and copy reductions
have explicit equivalence/resource gates. Operations use one attempt; SQL/SDK
retries, stale-state refresh/reprepare and failed-write replay are forbidden.
The [save/persistence proposal](admission-and-persistence.md) completes components
4/7: one enforced writer across bounded transactions, four tables, required
indexes, explicit visibility and cleanup once. Admission is the object store's
save coordination, not a separate service or validation framework.

One session owner does not mean one implementation file or one object implementing
all algorithms. Keep lifecycle authority coherent while encoding, packing and SQL
remain focused and independently callable.

History policy remains with the workflow: for example, which expected Branch head
may advance. SQLite persistence executes the required atomic transaction, which
may include both object and history mutations. Do not split an atomic publication
just to separate component labels.

### CDC and delta are separate stages

```text
CLUSTER 1                                  CLUSTER 2

small file -> whole payload object ---+
                                     +--> CAS reuse / one payload delta policy
large file -> CDC -> chunk objects ---+                |
                 -> mapping objects --------> required role encoding
                                                      |
                                               packs / SQLite
```

Existing CDC chunks can also use physical prefix delta. Preserve that behavior,
the frozen CDC profile and exact identities under matching canonical profiles.
Here small means a nonempty file below the configured cutoff. A FULL chunk
does not mean a complete large file; chunked construction emits mappings and
chunks without an additional whole-file CAS payload. See the
[role/encoding examples](content-storage-policy-and-tables.md#whole_file-applies-below-the-configured-cutoff).
The [cutoff/depth design](content-storage-co-design.md#file-cutoff-and-delta-depth)
retains configurable defaults of 128 KiB cutoff, 8 whole-file delta links and
4 chunk delta links. The
[simplified payload model](content-storage-co-design.md#simplified-payload-storage-model)
uses shared physical selection/reconstruction code with limits supplied by role.
Byte/work and format capacities remain enforced; shared code does not require
equal numerical limits. Non-default values require an explicitly supported and
qualified profile; 1 MiB/50 remain experimental candidates. Normal FULL selection
when delta is ineligible or not beneficial is part of the algorithm, not failure
recovery.

## 4. Shared contracts and deliberate coupling

```text
canonical construction                         physical object store
          |                                              |
          +--- finalized bounded object batches -------->|
          |                                              |
          |<--- authenticated canonical batch reads -----+
          |
          +--- bounded unfinished boundaries, retained privately
```

The handoff proposal supplies these rules; exact signatures and proofs remain open:

1. **Identity:** which exact canonical bytes, role and references define an object.
2. **Ownership:** borrowed versus owned buffers, validity periods and when output
   becomes finalized; no borrowed data can outlive its producer.
3. **Batching:** bounded object/byte groups and backpressure, preserving existing
   batched reads/writes rather than generating one SQL/RPC call per object.
4. **Finality:** retain unfinished nodes internally and return already-known root
   summaries. Remove generic staging only after final-only output is proven; resolve
   multi-edit and namespace reference ordering without unbounded caller state.
5. **Profile and identities:** supported canonical choices and authorized inode
   reservations are explicit, identical when comparing canonical results.
6. **Failure/finality:** incomplete construction is not publishable; same-session
   checks, transaction abort/owned cleanup and acknowledgement preserve their real
   guarantees. A condition requiring retry fails; unknown outcomes never authorize
   deleting or replaying a potentially successful write.

Acceptance can leave bytes buffered; construction returns the root and logical summaries; storage
completion accounts for all required output under the backend contract. Required
final-object/history atomicity can share the final transaction without an extra
commit. See the [completion rules](finalized-object-handoff.md#6-completion-and-failure).

CAS has two sides here: component 1 defines identity and validation; component 4
implements physical lookup, reuse, storage and authenticated retrieval using the
other physical components. SQLite is its persistence mechanism.

Reader/output contracts belong with their canonical semantics. The physical
provider implements them; logical algorithms do not gain a reverse concrete
dependency on the storage implementation. Ordinary functions and narrow existing
capabilities are the starting point, not a plugin registry or service locator.

## 5. Independent measurements and timer wiring

Operation names below are proposed measurement labels, not existing SDK methods.

```text
ISOLATED CONSTRUCTION
  declared payload/profile -> component 1 -> canonical object + ID
  no Workspace, FUSE, daemon or database required

ISOLATED STORAGE
  supplied canonical batch -> components 4/5/6/7 -> real save/readback
  no Workspace, FUSE or daemon required

COMPOSED OPERATION
  same constructor -> same storage entry point -> same bytes and IDs
```

File/tree construction can use readers with I/O. Prove a claim of no canonical
Store writes separately from the stronger claim of no SQL provider. Retain required
reads, output handling and ordering costs inside the stated scope. Apply budgets
across all files, with shared batches/packs and bounded concurrent operations;
see [whole-operation bounds](content-io.md#whole-operation-bounds-including-workspace-scale-workloads).

The existing [timer](../../../../../core/crates/layerfs-telemetry/README.md)
can record real component boundaries:

```text
object.create
  canonical.construct               component 1
  storage.save                      component 4
    membership / dependency checks
    physical.encode                 component 5
    pack.build                      component 6
    persistence                     component 7 plus remaining placement work
      connection.wait
      sql.begin
      sql.statements
      sql.commit
```

SQL transaction lifetime is different from SQL execution. An existing cohort may
remain open while later objects are produced or encoded. A BEGIN-to-COMMIT elapsed
measurement includes that intervening work; do not call it SQL-only or add it to
overlapping phase durations. Keep the actual transaction and acknowledgement
boundaries. See the [audit's transaction analysis](cluster-1-2-source-audit.md#6-independently-measurable-operations).

For existing worker boundaries, scope handles stay on their originating thread:

```text
operation owner                         existing worker
  parent scope                           |
       +--- ordinary work + timing flag -> choose record / disabled
       |                                 run the component
       <--- Result + completed report ---+
  attach report, then propagate Result
  format/save after the outer operation
```

Do not add workers, move borrowed scopes into Send futures or change scheduling
to fit the timer. Use coarse phase/batch scopes and preserve visible incompleteness
at the timer's limits. The timer measures time; other work/resource counters remain
explicit component evidence. No request/reply model or transport codec belongs in
the timing component.

## 6. Environment-independent wiring

```text
                       selected operation binding
                                  |
                +-----------------+-----------------+
                |                                   |
        direct local call                  existing bounded transport
                |                                   |
                +-----------------+-----------------+
                                  |
                         SAME COMPONENT BODIES
                                  |
                    explicit database / input / resource owner
```

Local, container and remote are placement choices outside the component bodies.
Both host and daemon remain possible participants. Use embedded SQLite or a
qualified remote database adapter with grouped reads and atomic bounded writes.
The [placement contract](content-io.md#6-pluggable-to-later-environments) covers
writer authority and uncertain outcomes. Keep construction
and physical admission colocated by default; a tiny function boundary must not
automatically become a tiny network call. A local adapter can invoke a typed body
directly; a transport adapter decodes/validates and invokes that same body.

FUSE is a projection/runtime adapter. A local FUSE handler can reach the same
components without a container. Live mutable Workspace state remains its own
owner and supplies stable inputs when construction is requested.

## 7. What this design preserves and removes

```text
PRESERVE                              REMOVE / CONSOLIDATE
--------                              --------------------
canonical bytes and IDs               encode -> decode round trips
CDC / COW / tree algorithms           repeated ancestor/frontier scans
exact reuse and authentication        temporary inode write -> read cycles
bounded codec / tree / batch state    generic candidate payload staging
transaction and recovery guarantees   repeated ownership wrappers
declared profile and placement        hidden error-driven alternate paths
```

No hidden fallback means a selected supported operation succeeds or reports its
error. Pre-plan bounded chunks and capacity; do not rewind after failure into a
different algorithm or silently switch executors/versions. Preserve ordinary
representation choices, cache misses, structural tree cases and required cleanup.

Active physical encodings must not be removed merely because their version number
is smaller. Historical-format support and any format conversion remain explicit
compatibility decisions, described in the [source audit](cluster-1-2-source-audit.md#5-no-hidden-fallbacks).

Keep time/space/memory complexity and actual work/resource budgets at least as good
under the agreed acceptance contract. Source inspection does not prove equal
performance: verify identities and algorithms, then qualify matched-input/cache/
worker/topology measurements. Do not claim isolated component time is full Commit
latency. Tests remain outside product src/, and no new crate inventory is fixed here.

## 8. Details for the next discussion

The [ordered co-design checklist](content-storage-co-design.md#remaining-co-design-decisions)
tracks the implementation decisions and proofs for all seven documented component
proposals: exact signatures/capacities, canonical equivalence, bounded memory,
schema/profile compatibility, receipt units and qualified backend execution.
The [fuller consistency/placement review](implementation-plan.md) is recorded.
Next implement the complete-file
construction -> standalone save -> authenticated readback slice.

This does not introduce new components/crates or claim implementation completion.

The detailed source findings, complexity caveats, removal candidates, recovery
risks and coverage limits remain in the [source audit](cluster-1-2-source-audit.md).
This component map adds no implementation or performance evidence.
