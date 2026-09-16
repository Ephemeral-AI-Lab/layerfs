# Generic content I/O: logical content and database storage

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Read with the [content-storage design](content-storage-design.md),
[policy/tables](content-storage-policy-and-tables.md) and
[v0.1.6 I/O and memory audit](content-io-memory-audit.md).
The [finalized-object handoff](finalized-object-handoff.md) owns the detailed
output fields, finality, allocation transfer, backpressure and completion contract.

## 1. Scope and objective

This design covers cluster 1 canonical content and cluster 2 physical storage /
SQLite only. Workspace state, capture mode, snapshot transfer, FUSE, host/container
placement, wire protocols and history publication/completion belong to later owners.
The replacement must not assume v0.1.6's Workspace mode. Its observed calls are
reference evidence, not the input model or required deployment architecture.

The objective is a smaller, faster core boundary with fewer actual reads,
transformations, allocations and database calls than v0.1.6, with every live
memory allocation justified and bounded in its stated domain. No optimization or
total-memory bound is claimed as measured by this proposal.

No buffer manager, reader service, transport framework, scheduler or new crate
is introduced here. I/O is part of the canonical object contract, expressed through
ordinary functions and narrowly required existing capabilities.

The direction is finalized-object streaming across the whole content operation,
including many files and directories: no generic payload spill or scratch object
store in the target path. Bounded working memory remains inside each algorithm.
Complete-file construction already supports final-only output; extending that
property to multi-edit construction and namespace reference ordering remains a
proof obligation. This is not a claim that all C1/C2 operations already need zero
temporary storage.

```text
ANY LATER CALLER / ENVIRONMENT
  supplies stable bytes or known edits, policy, authorized identities
  owns its own source acquisition, transport and lifecycle
                           |
                           v
CLUSTER 1                  |               CLUSTER 2
construct / read / COW      |               object storage
canonical bytes + IDs -----+-------------> membership / base acquisition
bounded owned batches                      FULL or DELTA + compression
         ^                                 packing / SQLite
         |                                       |
         +---- authenticated object reads -------+
```

Neither cluster requires a WorkspaceId, LayerStackId, BranchId, logical Commit,
mount, socket or container handle. Storage can be exercised using real object
tables without creating history entities. The caller decides where to execute
the core; component boundaries do not become RPC boundaries automatically.

## 2. Reference baseline

The source comparator is peeled `v0.1.6^{commit}`:
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
The annotated tag object is a different Git object, `dbdf0fed...`.
At review HEAD `a8a1ba848429d5f2fbba83c2de22dada8c29def9`, all root `crates/`
product files are byte-identical to that source. The companion audit records
reproduction and exact source anchors.

The [v0.1.6 verification record](../../../../../release-notes/0.1.6/verification.md)
binds its historical receipts to their actual source/product/harness identities.
It does not supply a new generic object-I/O benchmark or permission to reuse a
historical timing with different inputs, cache state or measured boundaries.

Keep 128 KiB / whole-file depth 8 / chunk depth 4 defaults, frozen CDC and codec
parameters, supported formats, required authentication, batching and transaction
semantics in the first comparison. Configurable transparency remains the goal;
finding faster numerical settings is a separate task.

## 3. Three small capabilities

These describe function requirements, not three new components or finalized APIs.

### A. Stable source bytes

Use borrowed bytes for already resident bounded input and std::io::Read for a
sequential source. Known edits retain base roots and explicit replacement ranges;
do not flatten them into a whole-file stream and lose the COW advantage.

The [file input contract](file-content.md#2-minimal-operation-inputs-and-results)
distinguishes sequential complete input from known edits with stable replayable
replacement ranges. Required changed-range equality can consume input before a
mismatch is known; construction may need to read that range again. Count both
passes rather than retaining an unbounded prefix or promising single-pass edits.
The normalized edit offsets use the current result after preceding edits.

When random ranges are necessary, fill caller-supplied memory directly:

```text
source + offset + destination slice
                 |
                 v
     exact bytes in that destination, or error
```

The caller guarantees source stability for the entire construction/read operation,
across all requested ranges. The core validates range arithmetic and exact
requested byte counts. When a whole input declares a total length, it also
rejects early EOF or bytes beyond that declared length. A range read does not
require EOF at the end of that range.
It must not silently reopen, recapture or substitute a newer source. A future
local/remote adapter can fulfill this same contract without the core importing
its environment types. Adapter I/O required during construction remains in the
measured dependency scope; borrowed data is not a claim of zero acquisition cost.

Return length and root already established during construction. Do not reread
the completed root simply to recover its known logical length.

### B. Authenticated object access

Consolidate the overlapping ObjectRead / ObjectSource / CoreReader responsibilities
into one semantic object-read boundary. Preserve actual bounded batch access:

```text
requested IDs
     |
bounded location lookup
     |
group physical demands -> read/decode required group for those demands
     |
authenticate canonical objects
     |
deliver in required demand order
```

The batch must be bounded by ID count and live byte capacity. A point read can
use a one-ID batch. Production batch access must not silently fall back to a
loop of point queries. Preserve missing-object errors, cardinality, identity,
role and ordering checks. Repeated demand for the same ID need not clone its
payload; borrow from one authenticated owner while its lifetime permits.

Separate explicit construction policy, authorized identity inputs and optional
predecessor IDs and correspondence from the reference's broad mutable ObjectStore context. Hints
carry ordinary IDs/ranges and cannot change canonical identity or require a
history-bound SnapshotReader.

### C. Canonical output

```text
constructor owns canonical allocation
                 |
                 | move existing allocation
                 v
bounded storage admission batch
                 |
       encode -> pack -> SQL
```

Move bytes with ObjectId, established role and needed direct references; derive
byte length from the allocation and keep optional predecessor hints separate.
The [handoff fields](finalized-object-handoff.md#2-object-fields-and-optional-hints)
distinguish checked constructor output from external assertions and C2 dependencies.
Keep unfinished boundaries and parent summaries inside C1 rather than rereading
emitted nodes. Bound count and bytes across the operation, with no whole-candidate
Vec or automatic spill. Supported large objects need a working singleton path.

Acceptance means bounded ownership transfer, not persistence or immediate read
visibility. [Backpressure](finalized-object-handoff.md#5-batching-and-backpressure)
can use direct calls or the existing bounded producer/admission overlap. Preserve
useful overlap until a measured replacement proves better.

### Finalized output replaces candidate staging

```text
REFERENCE PATHS TO REMOVE                TARGET

construct intermediate objects          stable input / ordered changes
             |                                      |
temporary object store                  current chunk + unfinished boundaries
  payload spill / ID and order indexes               |
             |                          finalize an immutable object
reachability walk / payload rereads                   |
             |                          move into bounded admission
real object admission                                |
             |                          FULL/DELTA -> compression -> pack
            SQL                                      |
                                                    SQL
```

The [finality rules and source evidence](finalized-object-handoff.md#3-when-output-is-final)
cover complete-file and sorted-tree support, plus the outstanding multi-edit and
operation-level reachability proofs. Reuse those algorithms and preserve required
duplicate/collision checks. Ordering costs cannot disappear into an unbounded caller.
The [file COW proposal](file-content.md#5-immutable-file-cow-and-final-construction)
replaces already-bounded encoded structural overlays, retaining the existing
split/concat partitions. Its source path already streams payloads; generic payload
staging removal elsewhere must not be counted as a saving on every edit.

Construction returns the root and logical summaries; storage completion covers all required output.
The [completion contract](finalized-object-handoff.md#6-completion-and-failure)
owns bounded early writes, final dependencies, failure ownership and uncertain
remote outcomes. Neither result independently publishes history. Preserve required
atomic final-object/history composition without adding a separate commit.

## 4. Required reductions against v0.1.6

The [audit ledger](content-io-memory-audit.md#2-core-optimization-ledger) distinguishes
source-visible reductions from performance results.

| Target | Required reduction, with equivalent semantics |
| --- | --- |
| Candidate staging | Remove generic payload spill, full-candidate indexes and reachability/readback on proven finalized-output paths; multi-edit boundary equivalence and namespace ordering remain separate proof gates |
| Mixed constructed/persisted reads | Move authenticated owned results into bounded ordering slots; eliminate the intermediate payload clone and repeated hash |
| Whole-file construction | Encode directly into final canonical allocation; remove inner-envelope allocation/copy |
| Large-object handoff / singleton pack | Align producer/admission capacities; replace the singleton temporary-file round trip within supported memory and format limits |
| Authenticated metadata access | Decode already validated payload fields directly instead of recreating an outer envelope to decode it again |
| Metadata group digest | Hash the actual uncompressed group body before compression; remove decompression/clone just to recover those bytes |
| Pack assembly | Choose append/new placement from validated sizes before materializing bytes; assemble selected output once |
| Compact inode batch lookup | Share required ancestor traversal across a bounded key batch instead of independently descending per key |

Keep existing good behavior: <=128-ID lookup pages, packed read-demand grouping,
batched predecessor locations, membership results carrying locators, and absence
checks reused while exclusive ownership keeps them valid. Unexpected invalidation
fails without refresh/reprepare under the [save contract](admission-and-persistence.md#4-save-path-without-an-extra-validation-pass).
A wrapper call is not
automatically a DB round trip; prove improvement using actual work counts.

Do not remove collisions, validation, dependency bounds or necessary repeated
reads in subsequent planned batches. Do not expand global similarity search or add caches
as a shortcut to a favorable number. Current state/capacity can make a delta
ineligible and select FULL normally; unexpected failures do not select another
algorithm or storage implementation.

The initial implementation retains placement-first compatible append. Seal-once
insertion is deferred outside this plan; it would need separate proof of grouping,
visibility, acknowledgement, failure ownership and tiny-operation packing density.
Do not implement both paths or force a pack flush/SQL transaction per file.

## 5. Memory rule: justify ownership, not a new manager

Every retained allocation must identify:

```text
owner -> purpose -> capacity -> lifetime -> simultaneous allocations
      -> admission before growth -> release event -> overlimit behavior
```

The [memory ledger](content-io-memory-audit.md#3-memory-ownership-ledger) is the
review checklist. Borrow or move existing bytes; allocate new bytes for a new
canonical/encoded representation only when necessary. Count capacity and metadata,
not just useful length. A moved buffer changes owner without becoming a second
allocation. During compression, the target, base and alternatives can coexist.

```text
construction: input/probe + current chunk + canonical output + tree state
                                 |
storage: canonical batch + base/chain + codec + encoded alternatives
                                 |
packing: selected groups + chosen pack output + placement metadata
                                 |
SQL: live bind/BLOB/statement + cache/journal/temp + existing product bytes
```

These are lifetime views, not disjoint numeric buckets. Do not sum overlapping
6-MiB data / 2-MiB scratch / batch / codec checks as if they were independent.
The audit gives the allocation-based formula and remaining proof gaps.

Use the existing operation/Store owner to enforce admitted concurrency and
resource allowances before allocating. A per-operation bound multiplies with
concurrent operations; do not derive concurrency silently from CPU count or a
Workspace mode. The benchmark's single construction producer and separate
namespace-init exception remain explicit. A whole-process claim additionally
needs shared state, connection count, allocator and provider/OS accounting.

SQLite's requested page-cache size is not a hard bound on all SQLite memory.
Bound statement/batch/transaction inputs, account for journal/temp/BLOB ownership,
and qualify the resulting profile. Until this and allocator overhead are covered,
claim only the application-owned bounds that are established, not a total RSS cap.

No automatic disk spill is the target's response to full buffers. Backpressure
stops production; an unsupported profile or irreducible resource excess reports
an explicit error. Working memory for chunks, unfinished nodes, delta operands
and codecs remains necessary, with no generic scratch service around it.

### Whole-operation bounds, including workspace-scale workloads

Per-file bounds do not establish a many-file bound. A later caller can supply an
entire workspace's stable inputs without imposing its Workspace representation on
the core. Consume that work incrementally under shared operation allowances:

```text
caller: stable files + known edits + namespace changes
                         |
            incremental work consumption
                         |
       +----- one bounded C1/C2 operation -----+
       |                                      |
       | file construction -> final objects --+--> shared bounded admission
       |         |                            |       encode / pack / SQL
       |         +-> root and inode values    |
       |                    |                 |
       | incremental namespace construction --+
       |    unfinished boundaries only        |
       +--------------------+-----------------+
                            |
                  final constructed root
                  + persistence completion
```

Avoid a Vec of all files, all file results, all objects or all changed paths in
the core. Namespace processing needs compatible ordered input and must consume
file results incrementally; its exact contract remains open. Input normalization,
identity reservation and caller retention still count in complete-operation costs.
Do not replace candidate spill with unlimited source descriptors or a root map.

Small files share bounded object batches, packs and SQL cohorts across file
boundaries. Flush at declared limits or operation completion, with required
visibility respected. Preserve the single construction producer and the separate
namespace-init exception; no fan-out of one builder per file. Admitted concurrent
operations and initialization workers must also fit the declared shared budget.

Reads are demand-driven: opening a root need not load the entire namespace or all
file contents. Full traversal/materialization, when explicitly requested, streams
through bounded read batches. This is a core capability, not a mount policy.

### Remaining ordering obligation

Directory/name order differs from inode-reference order. Hardlinks and changes
across directories may need grouping before reference removals are final:

```text
directory-ordered bindings -> old/new inode events -> grouped inode effects
```

The reference [event journal](../../../../../crates/layerfs-workspace/src/changes.rs#L59)
and [additions-before-removals handling](../../../../../crates/layerfs-workspace/src/changes.rs#L3111)
show the semantic obligation, not a required future Workspace mode or file format.
The [filesystem-tree decision](filesystem-tree.md#5-reference-accounting-and-the-temporary-record-decision)
retains a narrow bounded inode-effects reducer and compact ordering records where
needed, reusing useful tiered merging while removing checkpoint fields and
compact-profile inode conversion passes. Its exact resource/layout contract still
needs qualification. It is not an automatic overflow fallback or a canonical
payload scratch store. No all-files plan or Workspace graph is required: checked
validated retained bindings can arrive incrementally from any caller.

C2's [physical encoding proposal](physical-encoding-and-packing.md#replace-the-disposable-sql-fingerprint-index)
replaces its temporary physical inode-value fingerprint index with a bounded
Store-owned ordered set, preserving exact reuse/window semantics. Resource and
performance qualification remains; removing file overlays alone proves no blanket
zero-temporary-storage claim.

## 6. Pluggable to later environments

```text
A. daemon in Docker -- bounded input --> host: C1 -> C2 -> SQLite

B. host caller -- bounded operation --> daemon: C1 -> C2 -> SQLite

C. host <--> daemon
              |
         bounded input --> cloud owner: C1 -> C2 -> SQLite

D. host <--> daemon
              |
         operation owner: C1 -> C2 codecs/packing
                                      |
                               database adapter
                                      |
                            bounded grouped requests
                                      |
                             remote SQLite service
```

The core does not select a Workspace mode, endpoint, transport, filesystem mount
or process topology. Later adapters establish stable input and translate transport
data at real boundaries. Borrowed references are local; cross-process transfer
is an adapter responsibility. Bounded input/output contracts make batching
possible without requiring a network call per object or tree node.

Checked logical inputs are native C1 entry points, independent of a path-command
front end. C2 independently consumes canonical objects/IDs with a selected backend.
Do not assume the future Workspace resembles the reference or hide its types and
lifecycle inside a renamed generic context. Record-ordering and identity providers
are explicit capabilities whose required work stays visible.

Both host and daemon can remain participants. Keep C1/C2 together where practical;
moving their execution does not change canonical semantics. Placement D requires
a concrete database capability contract, not remote simulation of each rusqlite
call or incremental BLOB handle. Group selected records/ranges and dependency
reads within byte limits; do not fetch every whole pack indiscriminately.

The selected backend must provide atomic pack/descriptor/dependency writes,
declared read-after-write behavior, and exclusive writer authority across all
transactions of the save. Local absence epochs and pack-watermark cleanup are valid only
under their actual ownership assumptions. A remote lost acknowledgement means
failure with unknown persistence outcome. Do not resend, automatically poll or
delete potentially committed data. A separately requested inspection can establish
the outcome. CAS IDs alone do not make pack insertion/cleanup idempotent.
Successful versions remain intact. Enforce the
[single-attempt rule](physical-encoding-and-packing.md#one-attempt-no-retries)
including zero busy timeout and no SDK/transaction retries; invalidated authority
fails without refreshing/repreparing. Planned backpressure remains ordinary work.

Qualify actual provider size/parameter/batch limits and SDK serialization/response
buffers. No per-object/per-chain-edge RPC translation, unbounded in-flight requests
or silent backend fallback. Preserve compatible local transaction behavior; do
not add local round trips to fit an imagined remote API. Exact remote adapter
signatures and backend qualification remain an open C2 decision.
The [persistence boundary](admission-and-persistence.md#8-environment-independent-persistence-boundary)
now specifies the minimal execution requirements, including exclusive ownership
across transactions and explicit final composition. Only the concrete adapter and
its qualification remain; no provider/lease/retry framework is added.
These diagrams are possible placements, not certified backends. The first target
is embedded SQLite with MEMORY journaling and synchronous OFF, zero busy timeout
and no WAL/durability additions. Cloudflare Durable Objects is excluded under the
current no-WAL rule; the [implementation review](implementation-plan.md#6-fuse-daemonhost-and-future-cloud)
records its actual provider constraints and the future integration direction.

Input acquisition, caller retention and transport allocations are not core-owned
by definition. They must still be disclosed and included when claiming complete
operation/system memory or latency. No caller can use this boundary to move
required work outside the declared timer. Deployment-specific performance remains
a later integration qualification, not a claim made by the core design.

## 7. Measurement and completion

### Independent C1/C2 timing is an acceptance requirement

Owner requirement: independently timing C1, independently timing C2, and tracing
their integrated operation are required product boundaries in the first real
implementation slices. They are not later monitoring work. A coupled entry point
that can only time Workspace creation through database completion does not pass.

Use the implemented [layerfs-telemetry API](../../../../../core/crates/layerfs-telemetry/README.md):
the caller starts Timing::record or Timing::disabled; each measured component
receives an injected child TimingScope and executes its real body inside run.
Components choose neither the root recording policy nor an output path. This
adds instrumentation to the same implementation, not a second benchmark algorithm.
The measured paths have no retry, fallback or fsync (including fdatasync and file
sync_all/sync_data). SQLite COMMIT still completes the selected MEMORY-journal,
synchronous-OFF transaction; no extra durability step is added. Timer report output
uses ordinary writes without a sync-to-disk operation. Failures propagate unchanged.

| Required mode | Supplied inputs | What the clock includes | What must not be required |
| --- | --- | --- | --- |
| C1 complete construction only | Stable file input, explicit policy, bounded consuming output | Source reads, canonical construction/framing/hash, finalized output and necessary consumer work | C2 save, SQLite, Workspace, history, FUSE or daemon |
| C1 edit/tree construction only | Stable edits, authorized identities, explicit authenticated base reader and ordering resources | Base/input access through the declared provider, comparison/replay, CDC/COW/tree/reference work and output | Concrete C2, hidden identity allocation or automatic persistence |
| C2 save only | Supplied bounded canonical objects and a real Store with declared initial state | Ownership, membership/collision/dependencies, base acquisition, encoding, packing, SQL and final acknowledgement | C1 file/tree construction or a Workspace/history operation |
| C2 read only | Object IDs/ranges and a real Store | Location/acquisition, decompression/delta reconstruction and authentication | Reconstructing a Workspace or rerunning C1 construction |
| Encoding or packing only | One bounded set of actual target/base bytes or encoded groups and valid placement context | The real selected component body | A substitute codec, fake storage or product test-only API |
| SQL persistence only | Actually ready bounded packs/rows and valid write ownership | Actual required binding, statements and transaction completion under the declared scope | Hidden encoding/assembly relabelled as SQL-only |
| C1+C2 integrated | Stable complete input or edits and real storage | All required construction, handoff/backpressure, storage and acknowledgement | Workspace, FUSE, daemon or logical Commit creation |

For C1-only construction, inject a bounded consumer that receives and releases
final objects rather than saving them. A diagnostic may count/check output in the
external harness; it must not retain every object or replace the construction
algorithm. Consumer work remains inside the declared measurement. For a no-DB
edit diagnostic, supply a declared immutable base provider; using the real Store
reader instead is a separately labelled C1-with-storage-reads measurement.

For C2-only save, canonical fixtures are its explicit input, not an invocation of
C1 inside the timer. Declare whether inputs are already supplied allocations or
read through a stream; include actual stream acquisition in the timed operation.
The integrated measurement includes C1 construction. Do not exclude it from an
end-to-end claim by citing a C2-only number.

Store opening is a separate measurable operation when the save contract takes an
already-open Store. Declare that boundary and cache/index state. Normal save-time
initialization, synchronization and base reads cannot be moved into setup. Excluded
fixture/provider preparation remains visible in the scope description and inside
any larger operation that requires it. These modes define architecture acceptance;
actual benchmark cases still require the existing specification/identity process.

### What a useful report shows

These are illustrative labels, not measured durations or implemented C1/C2 APIs:

```text
C1-ONLY: canonical.construct
           file.build / file.edit / filesystem.apply  (selected operation)
           actual measured substeps where useful
           output.accept                            (bounded, no DB writer)

C2-ONLY: storage.save
           writer.acquire
           membership / dependency.check / base.acquire
           physical.encode
           pack.build
           sql.begin / sql.statements / sql.commit
           storage.finish                           (remaining work only)

C2-READ: storage.read
           locate / acquire / reconstruct / authenticate
```

Instrument actual phases, not every helper or object. Required dependency reads,
comparison passes and SQL remain visible. A function named prepare or finish may
still perform several kinds of work; only timing the actual SQL calls establishes
a SQL-only duration. Encoding on already-supplied operands and complete save with
base acquisition are different measurements, both useful and explicitly named.

### Integrated timing must preserve actual nesting and overlap

For bounded single-object sequential work, construction and save can be sequential
children. Bulk operation must not collect all output merely to obtain that shape.
When C1 invokes the consumer synchronously, the real nesting can be:

```text
content.store
  canonical.construct                      includes synchronous consumer calls
    output.accept
      storage.batch
        physical.encode / pack.build / SQL
    ...                                    only bounded requested detail
  storage.finish
```

With the existing producer/consumer overlap, each worker records locally and
returns a completed report for attachment under the operation that encloses it:

```text
content.store                              end-to-end elapsed time
  canonical.construct                      producer elapsed, includes output waits
  storage.save                             consumer elapsed, includes input waits
```

The two child durations may overlap. Never compute pure C1 time as total minus
storage, or add child durations to manufacture end-to-end time. C1-only and C2-only
runs are the direct measurements of those declared component scopes. Elapsed time
is not CPU time. Time output acceptance/input acquisition where useful and label
those calls honestly: they may include queue work as well as waiting.

TimingScope is synchronous and neither Send nor Sync. Do not pass a live scope
through a worker queue, await with it, or add a global recorder. Create independent
recordings in real workers and attach their completed reports before propagating
errors. Place them under the whole enclosing operation, not a short join call
that did not enclose their execution. Never change production scheduling to fit
the timer. Future async integration must establish its actual execution boundary.

### Bounded detail and root-cause investigation

Bulk default reporting must preserve complete operation/cluster durations with
bounded coarse instrumentation. Do not emit a node per object or SQL iteration.
The existing recorder has 1,024-node/32-level limits. Detailed bounded component
diagnostics can expose inner calls; clipped detail is marked incomplete and cannot
be presented as a complete phase breakdown. Do not silently enlarge the recorder
or let C2 batch detail consume all capacity before recording the required C1 result.
Existing operation-owned accumulated phase timings can be reported separately with
their units/scope; they are not invented contiguous spans or a new Monitor service.

| Observation under matched inputs/state | Next area to inspect |
| --- | --- |
| C1-only time regresses; C2-only stays stable | Input/base access, comparison, CDC/hash/COW/tree construction |
| C2 save regresses in membership/base acquisition | Query count, collision reads, dependency chains and declared cache state |
| Encoding regresses while acquisition is stable | Codec trials, accepted profile, base/target sizes |
| Packing regresses | Group copies, assembly count and pack rewrites |
| SQL calls regress while preparation is stable | Statements, indexes, transaction sizes and actual database I/O |
| Isolated components stay stable but integrated elapsed grows | Handoff/queue scheduling, overlap, copies and real process-boundary work |

Timing narrows the investigation; it does not by itself prove the cause. Use
existing work counts and controlled component diagnostics to check the hypothesis.
No retry of failed product work or best-of benchmark selection is introduced.

The caller retains the returned report in memory, renders text or saves JSON after
the measured operation under its selected fresh output path. Components write no
timing files or telemetry tables. Disabled timing runs the same operation with no
clock/node work; missing/clipped detail is never reported as zero duration. Original
errors and unknown storage outcomes remain failures; no recovery-to-success path.

### Mandatory implementation checks

1. C1-only complete construction succeeds with no database/history/runtime, using
   the same constructor as integration; a no-DB edit case declares its base provider.
2. C2-only save/read uses real storage on supplied canonical objects without invoking
   file/tree construction, Workspace or history operations.
3. Integrated and isolated operations produce matching canonical outputs and real
   authenticated readback under equivalent declared inputs/profiles.
4. Recording on/off preserves output and error behavior; external tests check timing
   labels, nesting, result propagation and completeness, not fragile wall-clock limits.
5. Report scopes include required work and acknowledgement, remain bounded across
   many files/batches, preserve overlap and record failures without retries.

A component slice cannot be called complete without these independent execution
and measurement boundaries. Timer availability alone is not proof of decoupling.

### Comparator and proof obligations

First register a fair v0.1.6 component comparator with identical canonical inputs,
base availability, database/cache state, policy, codec, worker count, transaction
semantics and output oracle. Some reference admission entry points are private;
the reference access/harness method must be declared before collection. Existing
unit fixtures prove feasibility, not an existing public standalone benchmark.
Do not reimplement the reference algorithm in a synthetic control or add test-only
core entry points. Baseline setup may supply fixtures outside the timer; intrinsic
object/base reads, hashing, scratch and persistence stay inside their scope.

Require all of:

1. Actual reductions in candidate staging/readback, clones/hash passes,
   intermediate decoding and duplicate pack assembly. Preserve query batching.
2. Default-case latency no worse than the matched v0.1.6 component operation,
   with positive measured gains on targeted I/O cases before claiming a speedup.
   No invented percentage target or inherited v0.1.5 slowdown allowance.
3. Application-owned memory within declared caps, accounted coexistence, bounded
   event-ordering resources if required and qualified SQL/allocator attribution
   for broader claims. Prove the bound across many files and concurrent operations.
4. Exact canonical/readback correctness, supported transitions, failures and
   immutable successful stored data. Failure cleanup is limited to the failed
   unpublished storage attempt; logical version publication is outside this scope.

Cases include FULL save/read, exact reuse, both payload delta roles, shared-group
and duplicate-ID reads, slow consumers, late EOF/input failure after persisted
cohorts, uncertain remote outcomes, pack boundaries/density, namespace key batches,
fragmented edits, hardlink reference ordering, many small files, cutoff transitions
and accepted configuration overrides. Compare reference spill traffic with target
staging removal where that reference route actually spills; already-direct paths
do not receive fictional savings. New schema/configuration errors without a matching old
path get correctness/resource checks, not invented paired speed ratios.

Freeze measurements under [repository rules](../../../../../AGENTS.md) and
[benchmark rules](../../../../general/benchmark_rules.md): one sample per case/arm,
append-only receipts, equal declared cache state, separate verification, no
resource-sensitive overlap or relaxed limits. Historical release receipts are
reusable only when their exact identity/scope requirements apply. Document unrun
or unattributable cases plainly. This proposal registers no campaign or result.

## 8. Explicitly deferred

Workspace representation, capture/frontier pagination, snapshot RPCs, remote
completion/cancellation and client connection lifecycle were inspected as
v0.1.6 integration context. Their improvements belong to a later design and are
not required implementation steps or claimed gains for this core work. The core
must not retain their concrete types merely because they supplied the old inputs.

The seven content/storage responsibilities and existing telemetry utility remain
unchanged. I/O and memory ownership are folded into the canonical object contract;
the next deliverable is its exact minimal signatures/lifetimes and the reference
comparison access plan. First extract the existing complete-file finalized-output
route: empty/whole/chunked construction -> real save -> authenticated readback,
with backpressure and late failure. Then extend to sorted namespace updates,
range edits/transitions, proven multi-edit boundaries and reference ordering.
