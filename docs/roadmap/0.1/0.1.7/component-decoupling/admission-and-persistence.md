# Object save and SQLite persistence

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Components 4 and 7 of [C2 physical storage](object-storage.md).
Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Three read-only reviews covered ownership/failure, schema/query costs and
transaction/placement boundaries against the [pinned reference](content-io-memory-audit.md#1-source-provenance-and-evidence-levels).
This document selects the execution design and concrete cuts. Product code is
unchanged; source inspection and a small SQLite semantics diagnostic are not
performance or product qualification.

[Object fields and handoff](finalized-object-handoff.md) owns canonical output,
allocation transfer and finality. [Physical encoding/packing](physical-encoding-and-packing.md)
owns codecs and pack layout. [Policy/tables](content-storage-policy-and-tables.md)
owns field meanings and format-capacity compatibility. This document connects
them into one save operation, without another service or framework.

## 1. Decisions

1. One C2 save coordinator owns Store writes across bounded SQL transactions.
   Acquire ownership once; unavailable ownership fails immediately. Preserve
   bounded producer backpressure and the permitted C1 producer concurrency.
2. Reuse batched membership, exact collision comparison and dependency checks.
   Carry checked object fields; remove repeated parsing and private bookkeeping
   only when their guarantees are already established.
3. Preserve lazy transactions shared across preparation batches and files. Keep
   one final composition point for authorized external writes; C2 owns no history
   entities. An empty finish with no remaining writes needs no write transaction.
4. Use four C2 tables with 19 declared columns, plus required indexes. No separate
   admission, retry, fingerprint, telemetry or operation-manifest table. Start
   with an inspection query; an inventory view is optional, not required storage.
5. Track pack and metadata-ordinal allocation in the exclusive owner. Append only
   to packs created by the current save. Use bounded reverse-location cleanup
   for definitely failed unpublished saves, with enforced base chronology.
6. One attempt. No busy handler, stale-state refresh, query/transaction/SDK retry,
   alternate write route or repeated cleanup from nested wrappers/destructors.
7. A lost acknowledgement fails with unknown persistence outcome. Stop affected
   writes; do not resend, automatically poll or delete possibly committed data.
8. Keep the same canonical algorithms and storage choices. Successful-operation
   speed, storage efficiency and total memory must meet the agreed gates; faster
   failure is not a performance improvement.
9. No WAL or added crash durability now. Use the selected embedded MEMORY journal /
   synchronous OFF profile and zero busy timeout; retain runtime transaction
   atomicity/abort. No fsync, fdatasync, sync_all or sync_data. No third-party
   patching or implicit alternate provider. SQL COMMIT is still part of completion.

These are proposal decisions, not a claim that extraction is implemented. Exact
Rust signatures, role/profile codes and compatibility SQL follow the already
required format and implementation qualification; they add no new components.

## 2. Terms

| Term | Meaning |
| --- | --- |
| Object store | C2 entry point for saving and reading canonical objects |
| Admission | Existing source name for the save coordination described here; not another public service |
| Save operation | One attempt to account for all accepted objects through exact reuse or acknowledged storage |
| Write ownership | Exclusive authority to mutate this Store across every transaction of that operation |
| Preparation batch | A bounded set of canonical objects prepared together for storage |
| Lookup page | A bounded set of IDs queried together; distinct from a preparation batch |
| Pack | Physical groups/records stored in an object_packs row |
| Locator | pack_id, group_number and record_number identifying a stored record |
| Foreign key (FK) | A database-enforced reference to an existing row; it does not prove every content invariant |
| Index | An additional lookup structure maintained on writes; keep it only for an actual query/constraint need |
| SQL transaction | An atomic database write unit that can span several preparation batches; source sometimes calls it a cohort |
| Accepted | C2 owns the submitted allocation; encoding/writes may remain |
| Read-visible | A specified reader/session can obtain the object; acceptance alone does not establish this |
| Stored | Exact reuse or writes acknowledged with required dependencies satisfied under the backend contract |
| Finish | Stop accepting input and complete all remaining required work once |
| Abort / failed-save cleanup | End uncommitted work and remove only established, unpublished data owned by the failed save |
| Baseline pack ID | Highest pack ID before this save; new owned packs have larger IDs under exclusive ownership |
| Unknown persistence outcome | Acknowledgement is missing, so writes may have committed even though the invocation failed |

SQL COMMIT is not the history component's logical Commit. Successful versions
are never rolled back. Ordinary ID, role, length and reference fields remain on
their existing objects/results; no generic metadata bag is introduced.

## 3. One owner, bounded work

```text
local / host / daemon / later caller
                  |
        acquire Store write ownership once
                  |
      establish pack / ordinal starting positions
                  |
  finalized objects -> bounded membership / exact reuse
                  |                  |
               missing             reused
                  |
       dependencies -> encode -> place -> write
                  |
          bounded SQL commit when full
                  |
          accept further bounded input
                  |
       finish final writes and acknowledge
                  |
             release ownership
```

The reference already has [exclusive SQLite ownership](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L526)
and an in-process [ticket gate](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L103).
Replace the queued save acquisition with one try-acquisition; return unavailable
instead of accumulating waiting save operations. This is a proposed behavior
change, not a claim that waiting was itself always a retry. Within an acquired
save, keep bounded producer queues/backpressure. No per-object lock is added.

One mutation coordinator does not mean one canonical producer in every workload.
[Namespace initialization](../../../../../crates/layerfs-layerstack-store/src/layerstack.rs#L1314)
already feeds parallel construction into one admission owner. Preserve that
exception and ordinary single-producer policy. Pure preparation overlap is allowed
only with proven unchanged ownership/order and bounded simultaneous allocations;
the initial target has one outstanding prepared mutation batch.

Keep operation state small: owner/session, starting pack ID, next pack/ordinal
positions, current transaction counters, bounded input/pending objects, useful
bounded caches and open pack tails. No operation-wide payload or object-ID map.
Use a mutable owner where possible; keep Arc/mutex/atomic state only at genuine
sharing boundaries. This is not permission to consolidate algorithms into one
large object or implementation file.

The baseline is read once. New pack IDs are monotonic and current-save open packs
start empty. Previously retained packs are never reopened for append. Track
successful connection-visible insertions inside the open transaction; after any
failure or unknown outcome, invalidate operation cursors instead of refreshing
and resuming. At no point does a local mutex establish remote writer exclusivity.

## 4. Save path without an extra validation pass

```text
bounded input: bytes + ObjectId + role + direct references
                         |
           checked in-batch duplicate comparison
                         |
            batched membership + locations
                  /                 \
             existing               absent
                |                      |
     authenticated exact equality     direct dependency checks
                |                      |
            exact reuse          selected encoding / packing
                                       |
                                  atomic SQL writes
```

Preserve existing [combined membership/location queries](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L422)
and cheap [presence-only dependency queries](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L393).
Do not fetch locators where only presence is needed, or query again when a valid
same-owner result already establishes availability. No one-query-per-object loop.
Necessary collision/base reads remain real work, not free membership checks.

Checked local construction supplies already-computed identity, role and references.
External bytes and serialized fields still require authentication/validation.
Every duplicate occurrence must compare actual bytes under the collision contract;
an ID or a diagnostic seen-set never proves equality. Keep the useful bounded
authenticated comparison cache, with its actual memory/read cost.

The private missing batch is already unique. Remove its [ID vector and rebuilt
uniqueness set](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L201)
after proving that private precondition. Consume direct references instead of
[parsing every canonical buffer again](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4071).
Validate unresolved references in bounded pages. Same-batch children can be
resolved from bounded pending state; do not query SQL for bytes only accepted
into memory or retain every earlier object to avoid later required lookups.

### Caller-authorized value roots

A declared reference is a direct edge of the object being saved. A value root
inside a 73-byte inode value is not: the compact leaf carries `child: None` per
row, so the page declares no edge for the content root or the metadata root its
values name. The dependency check therefore covers the objects a save actually
declares, and a root persisted by this Store holds, for every inode, a value whose
roots are the ones the operation was given.

Those roots are the caller's authorization, not this layer's membership proof. An
operation is free to name a content or metadata root it did not emit - a caller
may address bytes that already exist elsewhere under an identity this save does
not insert - and `Store` accepts that save. The consequence is explicit: an
acknowledged save can publish a root whose file inode names an object this Store
does not hold, and the first read of that root fails with `MissingObject`. No
earlier check refuses it, and there is no repair pass, retry or fallback.

Two routes keep the guarantee total when a caller needs it. Pass the same value
roots as `FinalizedObject::references()` on the pages that name them, which makes
them declared dependencies of that save and gets them the ordinary missing
dependency refusal; or emit the value objects inside the same operation, which is
what the filesystem builder does for every root it constructs. A later adapter
that reads a persisted root must therefore treat a `MissingObject` from a value
root as an authorized-root failure of its own composition, not as a defect of the
save that acknowledged it.

### Caller-declared object role

A role is the producer's declaration alongside the bytes, not a field C2 re-derives.
Checked local construction establishes identity, role and references together, so
the declared code always names the grammar those bytes are in; the Store writes the
declared code and does not re-parse the inner tag to confirm it. Re-deriving it
would be the separate role-validation pass `canonical-objects.md` removes, paid on
every object of every save for a property the constructing caller already holds.

The two framed roles are the exception, and only because their raw payload has to
be *found*: `raw_payload` checks the `LFS4CHK`/`LFS5SML` framing before a chunk or
whole-file record is built, so a `Chunk` or `WholeFile` declaration that disagrees
with the bytes is refused at admission. Every unframed tree role is stored verbatim
in the ordinary lane, so a declaration that disagrees with the object's own inner
tag is persisted **as declared** and is then refused by that role's own checked
decoder on the read which follows - never reinterpreted, never converted, never
silently repaired, and never repaired by a second decoder chosen from the bytes.

A caller that declares a role must therefore be the component that built those
bytes. An adapter reading a persisted object selects its decoder from the persisted
role and reports that decoder's refusal; it does not fall back to another grammar.
The reproduction is `a_disagreeing_role_declaration_is_refused_by_its_own_decoder`
in `core/crates/layerfs-storage/tests/cas_roundtrip.rs`.

### Placement constraints this implementation accepts

Two properties of this seam are deliberate and are recorded here so an adapter
meets them rather than discovering them.

**The seam is a local filesystem path.** `Store::create` and `Store::open` take a
path and open embedded SQLite on it directly; there is no path-to-handle or
connection-provider seam between the Store and the engine, and a pack body is
read as one whole BLOB by primary key rather than by a byte range. A remote or
object-store backend would therefore need a new seam - a handle provider and a
byte-range pack read - and is not reachable by configuration. Until such a seam
exists, a Store is a local directory and its packs move with it.

**One operation is pinned to one thread per side.** `SaveOperation` owns a raw
Zstandard context, the ordering `FileBacking` is `Rc`-based, and
[`TimingScope`](telemetry.md) handles are neither `Send` nor `Sync`; moving an
operation to another thread moves those with it. A placement adapter must
therefore pin one save, one read and one construction each to a single thread,
and may not hand an in-flight operation across a thread pool or an async runtime.
This is what keeps the "one attempted operation" rule checkable: there is no
second thread that could retry, resume or race the first.

Neither property is a claim that a threaded or remote placement is impossible. It
is a claim about where the work would have to go, and it is stated here instead of
being left implicit in the types.

With one mutation coordinator, the established absence result remains valid until
its selected insertion. Remove the [late epoch refresh and winner-list rebuild](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1295).
Unexpected authority/state invalidation fails; no reread/reprepare. A known own
write is not an unexpected competing publication. Keep planned exact comparisons,
integrity constraints and affected-row cardinality checks.

### Remove diagnostic-only operation-wide indexing

The reference [seen index](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4112)
identifies first preexisting reuse for counters; byte comparison is independent.
Conditional target: remove this index from the save path and report reuse occurrences
plus actual inserted rows. Each occurrence still receives the required exact comparison.

This changes public metric units: CandidateReceipt, CommitOutcome, SDK/Monitor and
benchmark consumers currently use distinct counts and candidate/inserted/reused
equations; see [CandidateReceipt](../../../../../crates/layerfs-layerstack-store/src/telemetry.rs#L282)
and [Monitor checks](../../../../../crates/layerfs-monitor/src/operation.rs#L80).
Globally distinct reused IDs and reuse occurrences are different;
occurrence bytes cannot be called physical bytes saved or compared directly with
old deduplication fractions. An explicit supported receipt-format change and caller
updates are required before this cut; this proposal does not authorize silently
breaking that contract. Do not omit/zero required receipt fields, report occurrence
counts under distinct-count names, or rewrite historical receipts. Preserve the
[measurement contract](../../../../general/benchmark_rules.md) and use the revised
receipt identity when qualification is authorized.
If required compatibility cannot be met, revise the proposal before shipping; no
hidden old/new metric path. Other filesystem ordering uses of temporary sets remain
separate. No new statistics subsystem is introduced.

## 5. Transactions, visibility and finish

```text
preparation batch A --+
preparation batch B --+--> bounded SQL transaction 1 -> acknowledge
preparation batch C --+                     |
                                            v
preparation batch D ------> SQL transaction 2 remains open
                                            |
input succeeds -> final tail -> authorized external final writes, if required
                                            |
                                      final COMMIT once
                                            |
                                        save success
```

Reuse [lazy BEGIN IMMEDIATE and bounded transaction accumulation](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2309).
Do not add a transaction per object, file, pack or preparation batch. Preserve the
[final callback-before-commit ordering](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1338).
The external owner supplies its own history/staging writes where required; C2
needs neither history entity types nor their tables to save/read independently.

Two modes express existing semantics without duplicating the save algorithm:

- Standalone finish acknowledges its final object transaction and retains the result.
- An integrated owner contributes authorized writes to that final transaction
  before acknowledgement when its workflow requires atomic composition.

An already completed standalone save is not undone by a later external failure.
Keep direct and staged reference workflow semantics distinct where required;
neither becomes a required future Workspace model.

Empty finish rules:

| Remaining work | Action |
| --- | --- |
| All objects reused; no open transaction or external final writes | Return successful storage completion without BEGIN/COMMIT |
| Earlier writes left a transaction open | Commit it once, even if the final object batch is empty |
| Required external final writes | Execute them once inside the required final transaction |

Acceptance, connection visibility and acknowledgement are different:

```text
accepted in memory       -> bounded pending state only
written in open SQL tx   -> readable through that transaction/session
COMMIT acknowledged      -> backend's declared read-after-write visibility
```

The reference [same-connection access](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L440)
supports reads within its current transaction. The selected remote adapter must
provide equivalent visibility wherever later preparation needs it. At each
acknowledged transaction boundary, required logical/physical dependencies are
in that atomic write, earlier acknowledged writes, or preexisting protected storage.
No finish-time full graph scan or per-child flush is added.

Early SQL commits do not make unfinished-save output available to unrelated
readers. Same-save reads are owner-bound. An ordinary read captures a retained-pack
ceiling once and applies it to all object/base/pool/slice locations and cache paths;
an active save's baseline is its permitted ceiling. Existing locator results can
supply this check without another query. Complete success changes the boundary for
new reads, not a read already in progress. This [visibility contract](implementation-plan.md#visibility-during-an-unfinished-save)
needs explicit API/cache wiring and qualification; no reader-pin table is added.

### Reference bounds to preserve and reconcile

| Unit | Reference bound / consequence |
| --- | --- |
| Lookup page | 128 IDs |
| Producer slab / queue | 512 objects / 256 KiB; four queue slots |
| Physical preparation | Up to 512 objects; multi-object canonical bytes up to 512 KiB; every batch up to 4 MiB minus 1 |
| Canonical input allocation | Existing 6-MiB capacity check; not an independent extra budget |
| SQL transaction | Up to 8,191 submitted rows and 4 MiB minus 1 canonical bytes |
| Pack INSERT statement | Up to 1 MiB of BLOB data, with separately supported oversized RAW singleton handling |
| Statement rows | Up to 128, reduced for actual parameter/SQL-length limits |

Sources: [producer/transaction constants](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L34),
[preparation](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L183),
[pack statements](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1525),
[statement sizing](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1831).
These constrain different owners and are not additive memory buckets. The
canonical-byte transaction limit does not cap cumulative pack-rewrite bytes,
SQLite journal/cache memory or adapter copies. Account those separately. Reconcile
accepted singleton/profile capacities under the existing [capacity proposal](content-storage-policy-and-tables.md#making-the-whole-file-cutoff-genuinely-configurable),
without shrinking supported input or silently enlarging memory limits.

## 6. Four tables and the necessary indexes

```text
store_policy (5 columns, one row)
  id, format_profile, small_file_threshold_bytes,
  whole_file_delta_max_depth, chunk_delta_max_depth

objects (7 columns)
  object_id, object_role, canonical_length, base_object_id,
  pack_id, group_number, record_number

object_packs (2 columns)
  pack_id, data

metadata_value_groups (5 columns)
  first_ordinal, count, pack_id, group_number, digest
```

Keep [schema-10's](../../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql)
STRICT tables, objects keyed by ObjectId without an extra rowid, and integer pack
primary keys compatible with incremental BLOB access. Policy has id=1; create/open
checks require the row and validate the selected profile. Keep policy typed and
loaded once. Schema version remains distinct from format/profile and policy.

Keep positive lengths, fixed hash widths, valid locator ranges and pool-range
constraints under the selected format. Add compact role codes and nullable direct
base ID, with no self-base. The base column represents direct delta dependencies;
it does not replace logical references, pooled ordinals or historical slice owners.
Pack bytes and descriptor fields must agree. Actual value bytes stay in packs;
the metadata table is their ordinal/location/digest catalogue.

Selected secondary indexes/constraints:

| Index / constraint | Required use |
| --- | --- |
| UNIQUE objects(pack_id, group_number, record_number) | Reverse-location cleanup, pack child lookup and one canonical ID per physical record |
| objects(base_object_id) WHERE base_object_id IS NOT NULL | Direct-base dependent lookup and self-FK checks |
| Existing UNIQUE metadata_value_groups(pack_id, group_number) | Catalogue location uniqueness and pack lookup |

No role index, object-dependency join table, file_chunks table or persisted
chain-depth summary is added by default. Indexes and new descriptor bytes add
write/storage work; their usefulness must be demonstrated with actual query plans
and matched complete operations. Packing-group/catalogue disjointness and ordinal
continuity still need checked insertion; simple row constraints alone do not
prove them.

Use a direct-base self-FK with ON DELETE NO ACTION and immediate checking. No
cascade from deleting a base into canonical dependents. Enable/check FK enforcement
on each connection. Statement-end checking supports a whole cleanup page without
depending on the engine's row deletion order; RESTRICT would reject some same-page
base/dependent deletions earlier. See [SQLite's timing and action rules](https://www.sqlite.org/foreignkeys.html#fk_deferred).
No transaction-wide deferral is required for this cleanup design.

Batch physical catalogue inserts as well as object/pack inserts, preserving ordinal
order and constraints. Adding two object columns changes binding width from five
to seven: derive statement row limits from the actual bindings and backend limits.
Do not keep the old five-column calculation. Read next pack ID and exact next pool
ordinal once under ownership, then track them; the reference's next-ordinal query
is already [bounded to at most 165 rows](../../../../../crates/layerfs-layerstack-store/src/objects/metadata.rs#L72).
Its removal saves repeated SQL, not a previously existing full-catalogue scan.
The repeated reference calls are [pack allocation](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1447)
and [ordinal allocation/catalogue insertion](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1564).

An optional inventory view derives readable labels. It is not required for saving
objects. C2 create/open validates its tables and actually used statements without
requiring the full history SQL manifest. Scope/inode allocation remains an explicit
external identity capability; its table does not become part of C2's four-table count.
The source [manifest entries](../../../../../crates/layerfs-layerstack-store/src/statements.rs#L20)
and [startup compilation](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L569)
show the current coupling; required format/schema integrity checks remain.

Freeze actual role/profile codes, maximum-size checks and old-Store opening rules
before shipping schema SQL. Do not silently rewrite current Stores or classify
all historical CHUNK records as FULL merely because base_object_id is NULL.
These are compatibility gates on this schema, not an excuse to add speculative columns.

## 7. Failure ownership and bounded cleanup

```text
save starts at baseline pack P
             |
retained packs <= P          new save-owned packs > P
never touched by cleanup     current save may append within this range
                                      |
                             definite unpublished failure
                                      |
                             abort open transaction once
                                      |
                       delete owned objects newest locator first
                                      |
                      remove catalogue rows / unreferenced packs
```

Retain [baseline ownership](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2238)
under exclusive authority. This identifies the failed save's committed packs
without retaining an ID for every object. If exclusivity or outcome is uncertain,
the range is not deletion authority. A successful save ends this cleanup ownership.

### An interrupted save: what stays readable and the operator path

A save's output becomes visible only in its final transaction, where
`store_policy.retained_pack_ceiling` advances to name the packs the save created
(`cas/owner.rs`). Every earlier commit in that save is private to it. If the
process dies between a mid-save `COMMIT` and that watermark transaction, the packs
above the watermark belong to a save that never published, and the next
`begin_save` is refused with `UninspectedState { ceiling, highest_pack_id }`
rather than guessing their ownership (`cas/owner.rs`, acquisition).

This is contract-consistent, and **no recovery service is added.** What the
contract does state is what an operator is entitled to do, and what must keep
working until they do it:

- **Reads keep working.** Opening the Store and reading every root at or below the
  watermark is unaffected: the watermark is the read-visible boundary, and the
  unpublished packs are above it. A Store in this state is readable, not corrupt.
  `visibility::an_interrupted_save_leaves_the_store_readable_until_an_operator_decides`
  pins the read, the refusal, and the exit from it.
- **The refusal names both numbers**, so an operator never has to guess which packs
  are unpublished: `ceil(pack_id)` above `retained_pack_ceiling` is exactly the
  set. Nothing else is inferred, and nothing is repaired automatically.
- **The operator path is one of two explicit choices, taken outside the product.**
  (a) *Discard*: delete the object and pack rows above the watermark, which is
  precisely the range the product's own definite-failure cleanup deletes, and
  which is safe because no published row can reference a pack above the ceiling.
  (b) *Publish after inspection*: if the operator has independently established
  that the interrupted save's output is complete and its dependencies satisfied,
  advance the watermark to the highest inspected pack - the "explicit authoritative
  inspection" this contract already permits. Either way the decision is the
  operator's and is recorded as such; the product neither performs it nor
  influences it.

Do not add an automatic repair, a retry, a resend, a WAL, a checkpoint or a lease
for this state. A writer that finds it is unsupported until a human decides, and
that is deliberate.

### New-writer ordering removes the need for a cleanup graph

For every new direct delta, require the selected base locator to precede the
target locator. Native and metadata formats retain their stricter earlier-pack
rule. Whole-file same-pack bases must be in an earlier group. Enforce this during
placement using already acquired base locations, before writing.

```text
base locator < dependent locator

cleanup selection: pack DESC, group DESC, record DESC

newest dependent -> older dependent -> base
```

This is a target writer invariant to prove. Readers retain their supported legacy
rules; do not reject every historical representation under a new cleanup assumption.
New referenced bases must be present in the same valid write order or earlier
protected storage. Arbitrary import/rewrite is not a bypass for this rule.

Select the highest remaining owned locators in descending pack/group/record order,
up to declared page limits, through the location index.
Every remaining dependent of a selected base must be in that page or already gone.
Delete the page in one bounded statement under the immediate NO ACTION FK; do not
rely on IN-list order. Keep its row/parameter limits and bound transaction/byte work.
The next page repeats on older owned rows, not on the whole retained Store. Remove
pack rows only after all owned objects have been removed and remaining required
references are excluded under established exclusive ownership. Page owned packs by
their primary key; retain the catalogue's existing pack-delete cascade and indexed
pack lookup. Bound BLOB/journal bytes and induced catalogue-row deletion work as
well as pack count. This needs no additional whole-Store scan. Successful retained
storage is untouched.

### Cleanup runs once

The reference nests [resolve wrappers](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4049)
and [destructor cleanup](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2579).
Its terminal cleanup state is recorded after successful cleanup, allowing an outer
wrapper/destructor to attempt cleanup again, even when write quarantine prevents
further deletion. Replace this with one error boundary:

```text
first failure
     |
mark save terminal and cleanup attempted BEFORE fallible cleanup
     |
abort/clean established unpublished ownership once
     |
return original error, plus cleanup error if any
```

Abort the current transaction first. If that abort fails or its outcome is
uncertain, stop; do not proceed to deleting earlier committed rows. Classify a
failure as definite only when the backend establishes that outcome. An arbitrary
SQL/network error is not proof that nothing committed.

An otherwise abandoned save may receive one best-effort destructor cleanup; a
previously attempted cleanup is never repeated there. Explicit finish/abort owns
ordinary reporting. Cleanup failure or unknown outcome makes affected Store writes
unavailable until a separately requested inspection establishes safe state.

| Outcome | Disposition |
| --- | --- |
| Definite failure with established ownership | Abort/clean once; preserve prior retained data |
| Lost acknowledgement or lost write authority | Return unknown outcome; no resend, polling or destructive cleanup |
| Cleanup failure | Retain both errors; stop affected writes; no outer/destructor retry |
| Successful standalone save followed by later workflow failure | Keep the completed save |

Four tables do not provide a durable failed-operation manifest. After process loss,
the in-memory baseline may be gone and valid unreferenced objects may remain.
Do not guess ownership or promise automatic crash cleanup. The reference's
[MEMORY journal and synchronous=OFF settings](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L510)
are the selected no-WAL baseline, not a crash-durability guarantee. The RAM rollback
journal remains for runtime atomicity; do not change it to journal mode OFF.
Recovery/retention policy beyond an
explicit authoritative inspection is not added as a new subsystem here.

## 8. Environment-independent persistence boundary

```text
local caller ---------------------> C2 owner -> embedded SQLite

host <-> daemon ------------------> selected C2 owner -> SQLite

local/remote caller -> C2 owner -> bounded DB adapter -> remote SQLite
```

The remote contract requires bounded batch queries, grouped physical reads, atomic
pack/locator/catalogue writes, provider size limits, same-operation read visibility,
acknowledged read-after-write behavior and zero automatic retries. Writer ownership
must survive transaction boundaries. A designated storage owner with exclusive
write authority is sufficient only when the deployment actually enforces it;
per-transaction SQLite serialization or a local mutex alone is insufficient.

Do not implement a distributed lease service, automatic failover or generic database
framework for possible future providers. A provider that cannot satisfy the selected
operation contract is unsupported for that operation. No silent alternate backend.
Group acquisitions above individual [local BLOB calls](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L896);
do not turn every header/offset/record read into an RPC.

Local final transaction composition can reuse the existing one-shot callback shape.
A Rust callback is not sent over a network. A remote integrated workflow needs a
concrete equivalent atomic execution mechanism; standalone storage remains separate.
No Workspace paths, mount state, history entity types or transport endpoints enter
the core encoding/save algorithm.
Provider qualification must also satisfy the no-WAL/no-added-durability policy.
Cloudflare Durable Objects uses WAL internally and is therefore a future study,
not a currently supported provider; see the [placement review](implementation-plan.md#radish-and-cloudflare).

Set SQLite busy timeout to zero and disable SDK/query/transaction retries. A lost
acknowledgement, once detected, returns failure with unknown outcome and preserves
potentially committed data. Detection/timeout latency remains measured. A separate
caller-requested inspection can determine what persisted;
it cannot silently replay the original operation.

### 8.1 Accepted placement constraints (recorded 2026-09-17)

Two structural properties of this implementation are **accepted as adapter
constraints**, not left implicit and not treated as defects to fix later. Both were
re-measured against the tree rather than read from a report.

**The persistence seam is local-path only.** `Store` holds a `PathBuf`
(`cas/store.rs`), `Store::create` and `Store::open` take `impl AsRef<Path>`, and the
connection is opened with `Connection::open_with_flags` on that path
(`sqlite/connection.rs`). There is no byte-range read and no transport trait, so an
engine reached over a network would transfer whole packs per acquisition - the
review measured the pack granularity this implies.

*Accepted, with the consequence stated:* a remote placement is a **different C2
owner behind this same operation contract**, not a second backend selected inside
this one. The adapter that owns that placement is responsible for bounded batch
queries, grouped physical reads, atomic pack/locator/catalogue writes, provider size
limits, same-operation read visibility, acknowledged read-after-write behaviour,
zero automatic retries, and for declaring the pack transfer it causes. This section
grants no transport trait, no connection string, no provider registry and no
error-driven fallback; a provider that cannot satisfy the operation contract is
unsupported for that operation.

**The core is `!Send` as written.** `CompressionWorkspace` holds raw
`*mut ZSTD_CCtx` / `*mut ZSTD_DCtx` handles with no `unsafe impl Send`
(`encoding/codec.rs`), `FileBacking` and `FileRun` share `Rc<Account>` with
`Cell`/`RefCell` accounting (`filesystem/references/backing.rs`), and
`TimingScope` is `!Send` and `!Sync` by construction
(`layerfs-telemetry/src/timer/scope.rs`, `PhantomData<*const ()>`).

*Accepted:* one attempted operation is one thread's work end to end. An adapter that
must move work across an executor owns a **session**, not a live `SaveOperation`:
it completes or abandons the operation on the thread that started it and reopens the
Store elsewhere. No `unsafe impl Send` is added, because the zstd context's thread
affinity is the library's rule and asserting otherwise here would be a claim this
crate cannot verify. Nothing in this decision licenses a retry or a second attempt:
a `SaveOperation` that is abandoned is abandoned, and the caller's next action is a
new operation over the same Store, subject to the ordinary visibility rules.

## 9. Cut list and measurement

Standalone C2 save/read timing is mandatory, using supplied canonical objects and
real storage without C1 file/tree construction or Workspace/history setup. The
[measurement acceptance contract](content-io.md#7-measurement-and-completion)
also requires isolated C1 and integrated reports; this is part of each real slice's
completion, not optional follow-up telemetry work.

| Cut | Replacement and expected work reduction |
| --- | --- |
| Queued save tickets and redundant session sharing | One try-acquired mutation owner; bounded producers remain |
| Rebuilt private uniqueness vector/set | Existing unique missing-batch invariant |
| Reparse object bytes to extract references | Checked bounded direct-reference input |
| Late epoch reread and competing-winner reconstruction | Stable exclusive owner; invalidation fails |
| Per-batch MAX(pack_id) and next-ordinal reads | Once-established, checked owner cursors |
| Per-group catalogue INSERT calls | Parameter/byte-bounded catalogue batches |
| Empty final BEGIN/MAX/COMMIT | No-op write finish only when no real write remains |
| Operation-wide seen index used solely for distinct reuse counters | Explicit occurrence-count receipt semantics, subject to consumer compatibility |
| ObjectId-ordered cleanup over retained rows | Indexed reverse-location pages and earlier-base writer rule |
| Nested resolve and repeated destructor cleanup | One terminal error/cleanup boundary |
| History-dependent statement manifest in standalone C2 open | Required C2 schema checks and actually used bounded SQL |
| SQLite busy handling and SDK retry machinery | One attempted execution; errors remain errors |

Preserve what is already efficient: combined lookup pages, presence-only closure
queries, exact collision comparisons, bounded caches, shared transactions and useful
producer concurrency. The [encoding/packing cut list](physical-encoding-and-packing.md#9-reuse-and-cut-list)
remains separate; do not count its savings twice.

```text
storage.save
  writer.acquire
  membership
  dependency.check
  base.acquire
  physical.encode
  pack.build
  sql.begin / sql.statements / sql.commit
  storage.finish
  cleanup                         only on definite failure
```

These are coarse illustrative scopes around real calls using layerfs-telemetry.
Use actual nesting, bounded report detail and existing accumulated measurements.
Do not add a node per object, syscall or delta edge. Construction can block on
handoff; transaction lifetime can include preparation; final drain is only remaining
work. These intervals cannot be added/subtracted into invented exclusive CPU time.
Persistence-only measurement must receive truly prepared data: the reference
publish method also assembles and validates, so it is not a SQL-only boundary.

Compare matched successful complete saves/reads and independently callable stages.
Cover new/reused tiny objects, large-file edits, supported singletons/transitions,
many files, same-pack whole-file bases, cleanup pages crossing base boundaries,
busy ownership, failed SQL/cardinality checks, lost acknowledgement and slow consumers.
Record actual queries, acquired/written bytes, copies/hashes, pack rewrites,
transactions, cleanup calls and simultaneous memory including SQL/SDK allocations.
No warmed setup, raised worker count, dropped case or quicker failure earns a PASS.
Follow the [existing measurement contract](content-io.md#7-measurement-and-completion).

## 10. Design closure and implementation gates

This completes the proposal-level responsibilities for all seven C1/C2 components.
The [fuller consistency/placement review](implementation-plan.md) is recorded.
Next implement the first complete-file construction -> standalone save ->
authenticated readback path. Do not create
another architectural component to handle these remaining proof obligations:

- Actual role/profile codes, compatible schema opening and supported larger-object
  capacities; no silent migration or truncated accepted input.
- Exclusive ownership and visibility across bounded transactions, including the
  selected remote backend and final external composition when used.
- Earlier-base placement, indexed bounded cleanup, single cleanup attempt and
  preservation of all prior retained objects/versions.
- Receipt consumer compatibility for removing the diagnostic seen index.
- Actual query plans, new index/column cost, singleton allocation and total memory.
- Existing-or-better successful speed/storage/resource behavior and zero retries.

Review validation included an isolated in-memory SQLite 3.51.2 diagnostic:
base-only deletion failed; deleting a base/dependent pair in one statement worked
with immediate NO ACTION and failed with RESTRICT; representative cleanup and
dependency query plans used the proposed indexes. This validates SQL direction
only. It does not qualify the pinned product SQLite build, actual data volumes,
remote provider, index write cost, memory or latency.

No product implementation, release admission or measured speedup is claimed.
