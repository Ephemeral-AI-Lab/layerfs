# LayerStack database transaction and Store-to-Store transfer model

This document is the binding implementation specification for database
transactions, Store endpoints, missing-only transfer, bounded error handling,
and their performance bounds.

It is intentionally detailed. The optimization target is minimal production
code and persistent state, not minimal prose. A cold implementation must be
able to follow this document without inventing another protocol, schema,
transaction boundary, cache, session, or fallback algorithm.

LayerStack names the complete architecture. It is not a database entity.

## 1. Binding principles

| Principle | Binding rule |
|---|---|
| Store API, not database API | Applications and other machines call BranchStore, StackStore, or LayerStore operations. They never receive raw SQLite access. |
| Temporary transfer roles | Source and destination are roles for one operation. They are not store kinds, rows, flags, or permanent topology identities. |
| Missing-only movement | The source announces ObjectIds; the destination returns the fixed missing bitmap; the source sends only missing canonical bytes and missing typed facts. |
| Push versus Add | Push transfers data. Add performs three-way evaluation and mutates a StackHistory or LayerHistory head. These boundaries never collapse. |
| Immutable data first | Closure-complete objects and immutable facts may be admitted in bounded transactions before final visibility. |
| Visibility last | Product-visible state begins only when a Branch ref, local history head, or LayerStore copied Stack head reaches admitted facts. |
| Exact CAS | Mutable heads move only from an exact expected value. No last-write-wins, force, blind retry, or hidden merge is legal. |
| One object identity | New bytes use one CDC, canonical encoding, and ObjectId path. Transfer never rechunks or remints identity. |
| One merge implementation | Branch Merge, Add Stack, and Add Layer use the same storage-core three-way implementation. |
| Bounded work | Frontier pages, write batches, metadata, memory, SQLite page cache, and one-shot mutation attempts all have fixed bounds. |
| No compatibility scaffolding | This is a cold implementation. There is no legacy schema, dual route, migration shim, alias API, or alternate algorithm. |

## 2. Stores and topologies

### 2.1 Direct topology

    application or workload
             |
             v
        BranchStore
             |
             | Store endpoint
             v
         LayerStore

Publication is:

    push_branch(branch_id)
        -> missing-only transfer
        -> add_layer(layer_history_id, BranchSource)

There is no hidden StackHistory and no hidden StackStore.

### 2.2 Stacked topology

    application or workload
             |
             v
        BranchStore
             |
             | Store endpoint
             v
         StackStore
             |
             | Store endpoint
             v
         LayerStore

Publication is:

    push_branch(branch_id)
        -> add_stack(stack_history_id, branch_id, commit_id)
        -> push_stack(stack_id)
        -> add_layer(layer_history_id, StackSource)

### 2.3 Local and remote composition

The same domain operations support both deployment shapes:

| Deployment | Call path | Database access |
|---|---|---|
| Embedded local | SDK calls an in-process Store handle | Only that Store handle opens its SQLite file |
| Same-machine process | SDK calls a local Store endpoint | Server-side Store alone opens SQLite |
| Remote machine | SDK calls a remote Store endpoint over a reused stream | Remote client never opens or copies SQLite |

No remote topology is implemented by moving a .db, -wal, or -shm file.
SQLite pages are never the transfer protocol.

### 2.4 Temporary source and destination roles

For each Pull or Push, one Store is the source and one is the destination:

    Pull:
        parent Store = source
        child Store  = destination

    Push:
        child Store  = source
        parent Store = destination

The same LayerStore can be a source for one Pull and a destination for a later
Push. No source_role or destination_role column is permitted.

| Operation | Temporary source | Temporary destination |
|---|---|---|
| pull_branch(source_branch_id, local_branch_id) | configured parent | BranchStore |
| push_branch, direct | BranchStore | LayerStore |
| push_branch, stacked | BranchStore | StackStore |
| pull_commit_history | LayerStore | StackStore |
| pull_layer_history | LayerStore | requesting StackStore read-only dependency copy |
| pull_stack_history | LayerStore | StackStore |
| push_stack | creator StackStore | LayerStore |

Add Stack and Add Layer are not transfer operations. They execute against data
already admitted into the receiving Store.

## 3. Exact public operation boundary

There are exactly fourteen recurring storage operations:

| Number | Operation | Executor | Mutation boundary |
|---:|---|---|---|
| 1 | create_branch_from_layer(layer_history_id, layer_id) | BranchStore | Atomically reuse/insert canonical anchor Commit plus new Branch ref; zero payload copy |
| 2 | create_branch_from_stack(stack_history_id, stack_id) | BranchStore | Atomically reuse/insert canonical anchor Commit plus new Branch ref; zero payload copy |
| 3 | create_branch_from_commit(source_branch_id, source_commit_id) | BranchStore | New local Branch ref only |
| 4 | commit(branch_id, expected_head, changes) | BranchStore | New Commit plus exact Branch CAS |
| 5 | merge(source_branch_id, target_branch_id, expected_target_head) | BranchStore | UpToDate, fast-forward CAS, or one merge Commit plus CAS |
| 6 | pull_branch(source_branch_id, local_branch_id) | Configured parent serves | Pin source, verify upstream base/root, admit missing Commit metadata, then create/advance only the explicit local Branch ref; copy no accepted payload |
| 7 | push_branch(branch_id) | Configured parent receives | Missing facts then same-Branch-ID CAS |
| 8 | pull_commit_history(branch_id) | LayerStore serves StackStore | Missing Commit DAG and required root objects; no Branch ref mutation |
| 9 | create_stack_history_from_layer(layer_history_id, layer_id) | Creator StackStore | New StackHistory plus seed Stack |
| 10 | pull_layer_history(layer_history_id, through_layer_id) | LayerStore serves | Exact Layer prefix and local observed head |
| 11 | pull_stack_history(stack_history_id, through_stack_id) | LayerStore serves | Exact Stack prefix and read-only local head |
| 12 | add_stack(stack_history_id, branch_id, commit_id) | Creator StackStore | One AddResult plus at most one Stack and Stack head CAS |
| 13 | push_stack(stack_id) | Creator StackStore sends to LayerStore | Transfer Stack suffix plus accepted Branch/AddResult/Commit/root provenance missing-only, then verified copied-head fast-forward |
| 14 | add_layer(layer_history_id, source) | LayerStore | One AddResult plus at most one Layer and Layer head CAS |

Object lookup, object send, object receive, conflict detection, genesis
provisioning, and connection management are internal phases. They are not
additional public verbs.

The following combinations remain explicit:

    Direct:
        Push Branch
        Add Layer

    Stacked:
        Push Branch
        Add Stack
        Push Stack
        Add Layer

No publish convenience operation may hide those boundaries.

## 4. Minimum persistent database state

This transfer model introduces no new table.

### 4.1 BranchStore: 3 tables / 9 columns

| Table | Columns |
|---|---|
| objects | object_id PK, bytes |
| commits | commit_id PK, root_id, parent_id NULL, merge_parent_id NULL |
| branches | branch_id PK, head_commit_id, base_id |

### 4.2 StackStore and LayerStore: 8 tables / 24 columns

| Table | Columns |
|---|---|
| objects | object_id PK, bytes |
| commits | commit_id PK, root_id, parent_id NULL, merge_parent_id NULL |
| branches | branch_id PK, head_commit_id, base_id |
| layer_histories | history_id PK, head_layer_id |
| layers | layer_id PK, history_id, parent_id NULL, root_id |
| stack_histories | history_id PK, base_layer_id, head_stack_id |
| stacks | stack_id PK, history_id, parent_id NULL, root_id |
| add_results | source_id PK, result_id |

StackStore and LayerStore use the same table shapes. Their authority differs:

| State | StackStore | LayerStore |
|---|---|---|
| StackHistory created locally | Writable only by its creator capability | Read-only copied history |
| Pulled StackHistory | Read-only | Complete central copy |
| LayerHistory head | Observed/read-only dependency | Authoritative exact-CAS head |
| Branch rows | Selected transferred Branches | Complete central transferred Branches |

There is no transfer table, request table, session table, metrics table,
closure table, object-location table, owner table, lease table, GC table, or
derived read-model table.

BranchStore never acquires Layer, Stack, or history tables. In direct mode,
accepted base facts and payload remain in LayerStore and are resolved through
the configured parent. In stacked mode, they remain in StackStore. Pull Branch
admits only the Branch ref and its required Commit DAG metadata. The Commit
root is verified through the parent and accepted object bytes stay there.
BranchStore's objects table receives only objects created by later local
changes; Pull Branch never clones the accepted parent filesystem.

## 5. Canonical object identity

ObjectId is an untagged 32-byte digest. The object kind is encoded in the
canonical bytes rather than stored in the ID or objects table.

The only payload-object identity path is:

    raw bytes
        -> FastCDC v1 chunk
        -> encode_bytes_object(raw chunk)
        -> ObjectId::for_bytes(canonical bytes)
        -> objects(object_id, canonical bytes)

The ObjectId hash uses the existing object domain. The canonical object frame
contains object kind, length, and payload.

The raw chunk_id(raw_chunk) helper is not:

- a persisted object ID;
- an extent reference;
- a transfer ID;
- a missing-set ID;
- a database key;
- or an alternate deduplication domain.

CDC profile identity remains in FileState/profile metadata. It is not embedded
as a second payload identity.

### 5.1 One encode and one hash

Local new bytes:

    CDC once
        -> canonical encode once
        -> ObjectId hash once
        -> trusted staged pair (ObjectId, canonical bytes)
        -> SQLite admission without re-encoding or rehashing

Remote transfer:

    source reads stored (ObjectId, canonical bytes)
        -> no CDC
        -> no re-encode
        -> no new ObjectId
        -> destination authenticates the missing frame once
        -> SQLite admission without a second hash

If DeferredObjectStore spills to disposable scratch, re-authentication when
the scratch file is reloaded is the sole exceptional extra hash. It is counted
separately in benchmarks.

## 6. Copy-on-write update path

LayerFS v1 preserves authenticated unchanged extent slices and runs FastCDC
only over newly supplied or replacement bytes.

    existing FileState
           |
           | split at replacement range
           v
    unchanged left extents
           +
    FastCDC(replacement bytes only)
           +
    unchanged right extents
           |
           v
    deterministic splice/rebalance
           |
           v
    new FileState root

The edit path must prove:

| Measurement | Required result |
|---|---|
| CDC bytes scanned | Exactly the replacement/new bytes |
| Old suffix payload reads | Zero |
| Unchanged extent IDs | Retained outside the cut |
| Whole-file rechunk | Never |
| Background normalization | Never |

Equal logical bytes reached through different edit histories may have
different valid extent segmentation, FileState roots, and filesystem roots.
Canonicality is per stored object and typed manifest, not a promise of one
global representation for every logical byte stream.

### 6.1 Semantic equality for different COW roots

Three-way uses these regular-file rules:

    ObjectIds equal
        -> equal in O(1)

    ObjectIds differ
        -> compare logical lengths
        -> stream each needed distinct root once through ContentDigestWriter
        -> keep at most three transient digests

    semantic_eq(candidate, base)
        -> choose current representation

    semantic_eq(current, base)
        -> choose candidate representation

    semantic_eq(candidate, current)
        -> choose current representation

    otherwise
        -> first deterministic Conflict

The fallback materializes no complete file and persists no digest or new ID.
Current logical walkers perform S indexed structural-node reads and G payload
batch reads. They do not claim generic 512-ID structural batching.

## 7. Fixed transfer and transaction constants

The cold implementation uses these storage-core constants:

    ID_BATCH_COUNT        = 512 ObjectIds
    OBJECT_BATCH_COUNT    = 128 objects
    OBJECT_BATCH_BYTES    = 4 MiB canonical bytes
    FACT_BATCH_COUNT      = 128 fixed-width rows
    FACT_BATCH_BYTES      = 64 KiB row fields
    FINAL_METADATA_BYTES  <= 64 KiB
    MAX_OBJECT_BYTES      = 16 MiB currently
    FINAL_METADATA_STATEMENTS <= 8

The constants are not user settings or database columns.

### 7.1 Notation

| Symbol | Exact meaning |
|---|---|
| A_t | Typed ancestry/membership rows visited after pruning for one exact typed table t |
| A | Total typed rows, A = sum over tables of A_t |
| H | Homogeneous typed query pages, H = sum over nonempty tables t of ceil(A_t / 512) |
| P_o | Actual 512-ObjectId membership-query pages after known-root pruning |
| P | Actual dependency-ordered wire page turns after mandatory coalescing/piggyback of typed and object announcements; P <= P_o + H |
| J | Actual object insert batches emitted by the greedy count-and-byte packer |
| F | Actual immutable typed-fact batches plus frozen Push Stack Branch-provenance batches emitted by the fact packer |
| D | Final metadata and CAS statements; D is at most 8 |
| C | Merge-base recursive CTE statement count; 1 through 3 |
| S | Actual indexed structural-node reads made by current rope/inode/namespace walkers |
| E | Payload extents read across the operation's logical streams |
| G | Actual 64-entry payload batches, normally sum over streams of ceil(E_i / 64) |
| L | Actual Store-endpoint layered-parent read turns during preflight |

J and F are actual deterministic packer outputs. The expression
max(ceil(row_count / row_limit), ceil(total_bytes / byte_limit)) is only a
lower bound. Mixed object sizes can require more greedy batches.

### 7.2 Deterministic object packer

Process dependency-ordered entries in canonical order:

1. Start an empty batch.
2. Append the next object only if row count stays at or below 128 and canonical
   bytes stay at or below 4 MiB.
3. If the next object would cross either target, close the batch and start the
   next.
4. A valid object above 4 MiB and at or below 16 MiB occupies one singleton
   batch.
5. An object above MAX_OBJECT_BYTES is invalid, not another batch class.

### 7.3 Typed-fact packer

Typed immutable facts and frozen Push Stack Branch-provenance rows use:

- 128-row maximum;
- 64 KiB fact-field target;
- dependency order;
- one widest four-column prepared statement with 512 binds;
- no 256-row fact shape.

## 8. Fixed membership query and missing bitmap

At Store open, require SQLite 3.35 or later and prepare one statement:

    SELECT object_id
    FROM objects
    WHERE object_id IN (?1, ?2, ... ?512)

For every ObjectId frontier page counted by P_o:

1. Source supplies sorted, duplicate-free ObjectIds.
2. Destination binds n IDs, where n is at most 512.
3. Destination binds SQL NULL to unused placeholders.
4. SQLite result ordering is ignored.
5. Destination constructs an in-memory set of returned IDs.
6. Destination maps membership back to the source's original positions.
7. Destination emits exactly 512 bits / 64 bytes.
8. Bit 1 means missing.
9. Unused tail bits are zero.

Malformed order, duplicates, wrong bitmap size, nonzero tail bits, or an ID
whose canonical bytes do not authenticate are Integrity errors.

The prepared statement is reused. V1 must not generate 1 through 512 dynamic
query shapes.

EXPLAIN QUERY PLAN must use the objects.object_id primary key/index. A full
objects-table scan fails the gate.

### 8.1 Typed membership and ancestry pages

H uses the same fixed page width:

    typed IDs per page <= 512
    typed missing bitmap = exactly 512 bits / 64 bytes

At Store open, prepare one 512-placeholder primary-key membership statement
for each relevant typed table used by the Store:

    SELECT typed_primary_key
    FROM exact_typed_table
    WHERE typed_primary_key IN (?1, ?2, ... ?512)

This applies to the relevant commits, branches, layers, stacks,
layer_histories, stack_histories, and add_results primary keys. A Store
prepares only statements for tables in its fixed schema; this is not a dynamic
query registry or new public type.

Each H page is homogeneous to one typed table. Switching typed table starts
another H page. One coalesced wire turn may carry that one typed page alongside
one P_o ObjectId page, producing at most one typed bitmap and one ObjectId
bitmap in the reply.

H includes table-homogeneous membership pages for transferred Commit, Layer,
Stack, history, AddResult, and frozen Push Stack Branch-provenance rows when
those facts are carried. A single operation's current mutable Branch/head,
explicit history/node scope, and Push Stack attestation lookups are point
preflight queries counted by operation_preflight, not folded into H. Therefore
one Commit plus one Stack is H = 2, not ceil(2 / 512) = 1.

For ordered ancestry, use the explicitly named prepared recursive-CTE page for
that history/DAG instead of first materializing IDs in Rust:

    Commit ancestry page:
        one prepared recursive CTE cursor over parent_id and merge_parent_id
        UNION dedup
        step cursor output in pages of at most 512 returned IDs

    Layer/Stack ancestry page:
        fixed recursive CTE over parent_id
        at most 512 returned IDs

For a multi-page Commit DAG, execute the prepared UNION-dedup recursive CTE
once for the live operation and step its SQLite cursor in homogeneous output
pages of at most 512 rows. SQLite owns visited/deduplication state, page-cache
use, and temp-file spill until the cursor closes. Do not rerun a LIMIT/OFFSET
query for each page and do not build a Rust seen set. H counts emitted typed
pages; the source statement count is at most H and is normally one live cursor,
so the existing 2H source-plus-destination bound remains conservative. The
read-only CTE cursor/read snapshot may remain open while its bounded pages are
transported, but no SQLite write transaction or writer gate may span network
I/O.

For every typed membership page:

1. Source returns at most 512 sorted, duplicate-free typed IDs from the fixed
   ancestry page.
2. Destination routes each typed set only to its exact table.
3. Destination NULL-pads unused placeholders.
4. Destination does not rely on SQLite result order.
5. Destination remaps returned IDs to source positions.
6. Destination emits the fixed 64-byte typed missing bitmap.
7. Unused tail bits are zero.

EXPLAIN QUERY PLAN must show the typed table primary key and the named
parent-edge indexes/recursive CTE where applicable. A dynamic 1 through 512
statement family, one query per typed ID, a full typed-table scan, or routing a
typed ID to objects is forbidden.

## 9. Object and typed-fact admission

### 9.1 Object insert

Each destination object batch uses one multi-row statement:

    INSERT INTO objects(object_id, bytes)
    VALUES ... at most 128 validated rows ...
    ON CONFLICT(object_id) DO NOTHING
    RETURNING object_id, length(bytes)

Returned IDs are newly inserted by this statement.

    announced missing IDs
        minus
    returned inserted IDs
        equals
    IDs that lost a race to an already-admitted identical row

The difference is computed in transient memory. There is no per-object
follow-up query and no metrics table.

### 9.2 Children before parents

    leaf:
        authenticate canonical bytes and ObjectId
        -> admit

    parent tree/root:
        authenticate canonical bytes and ObjectId
        -> authenticate child references
        -> require admitted children
        -> admit

    Commit:
        require a closure-certified root
        -> locally admitted, or for BranchStore zero-copy Pull,
           verified through its configured parent without a local object row
        -> authenticate canonical typed manifest
        -> admit immutable fact

    Stack/Layer:
        require locally admitted root closure
        -> authenticate canonical typed manifest
        -> admit immutable fact

    Branch/history/copied head:
        require complete admitted dependencies
        -> exact CAS or insert in final visibility transaction

The global visibility order is:

    child objects
        -> parent objects
        -> closure-complete root
        -> immutable Commit / Stack / Layer facts
        -> AddResult and any frozen transferred Branch provenance ref
        -> mutable Branch ref / observed history ref / copied Stack head
        -> authoritative StackHistory or LayerHistory exact CAS last

Steps may share one SQLite transaction, but their statement and constraint
order must preserve this dependency sequence. No public query may traverse a
ref/head to a missing child, root, typed fact, or required AddResult.

An admitted root is a closure certificate. Normal repeated Add or transfer
does not rewalk the complete descendants of a known root. Full traversal occurs
only during first admission or explicit scrub.

### 9.3 Product visibility

Raw immutable row presence is not product visibility.

    unreachable immutable rows
        objects
        commits
        stacks
        layers
        transferred add_results

                    not visible until

    exposed mutable or observed root
        Branch head
        local LayerHistory head
        local read-only StackHistory head
        LayerStore copied StackHistory head

Public listings and reads begin from those exposed refs and follow reachable
facts.

Stacked provenance has one explicit safe ordering: after its complete
Commit/root closure and BranchId->StackId AddResult authenticate, the frozen
same-ID Branch row may be admitted in a bounded typed-fact transaction before
the copied StackHistory head advances. That Branch is independently complete,
accepted, and servable by pull_commit_history; its AddResult prevents later
same-ID mutation. The copied Stack head remains last. A partial Branch closure
or an unfrozen Branch is never exposed.

## 10. Mandatory P + 1 transfer pipeline

One reused Read/Write stream carries every transfer page. After the first
announcement, the next announcement is piggybacked with the preceding missing
payload:

    source                                         destination
       |                                               |
       | announce frontier page 1                      |
       |---------------------------------------------->|
       |                                               | one set membership query
       | missing bitmap 1                              |
       |<----------------------------------------------|
       |                                               |
       | missing payload 1 + announce page 2           |
       |---------------------------------------------->|
       |                                               | authenticate/admit page 1
       | ack 1 + missing bitmap 2                      |
       |<----------------------------------------------|
       |                                               |
       | missing payload 2 + announce page 3           |
       |---------------------------------------------->|
       |                                               |
       | ack 2 + missing bitmap 3                      |
       |<----------------------------------------------|
       |                    ...                        |
       | missing payload P + final intent              |
       |---------------------------------------------->|
       |                                               | final folded visibility
       | final acknowledgement/result                  |
       |<----------------------------------------------|

On an already-open stream:

    one Pull or Push transfer <= P + 1 RTT

A known requested root can finish in the first reply.

Separate announce and payload-ack round trips per page are forbidden.

A wire page may carry both:

- one bounded typed-ID set, routed to its exact Commit, Stack, Layer, Branch,
  history, or AddResult table;
- and one bounded ObjectId set, routed only to objects.

Destination membership is table-specific. Typed IDs are never queried against
objects, and ObjectIds are never queried against a typed table. The destination
returns separate position-preserving typed and ObjectId missing bitmaps in the
same reply. Coalescing makes P no greater than P_o + H; SQL accounting still
counts P_o and H separately.

### 10.1 Direct publication RTT

    Push Branch transfer     <= P_branch + 1
    Add Layer result         <= 1
    -----------------------------------------
    direct publication       <= P_branch + 2 RTT

### 10.2 Stacked publication RTT

    Push Branch transfer     <= P_branch + 1
    Add Stack result         <= 1
    Push Stack transfer      <= P_stack + 1
    Add Layer result         <= 1
    -----------------------------------------
    stacked publication      <= P_branch + P_stack + 4 RTT

Queue wait and a cold TCP handshake are reported separately. They are not
hidden in the service-time RTT bounds.

## 11. Store operation envelopes

Every remote operation envelope is self-contained:

- operation name;
- typed IDs and explicit history scope;
- expected mutable head when the operation moves a ref;
- exact source/candidate head;
- signed StackHistory tuple when Push Stack applies;
- bounded frontier pages;
- final intent.

There is no semantic session ID. A connection may be reused, but connection
lifetime does not own progress or authority.

The experimental v1 assumes the Store process and operation connection remain
available for the call. A transport error aborts the call and returns an error;
there is no automatic reconnect, resume protocol, lost-ack replay guarantee,
or recovery benchmark. An incomplete frame is never admitted. No session,
request, resume, or connection-bound publication table is added.

### 11.1 Pull Branch ref semantics

The signature is:

    pull_branch(source_branch_id, local_branch_id)

LayerStack pins the source Branch head for the operation, verifies its exact
base/root through the configured parent, and admits missing Commit DAG metadata
without copying accepted payload into BranchStore.

When source_branch_id differs from local_branch_id, local_branch_id must be
absent:

    absent local ID
        -> insert local Branch
           head = pinned source head
           base = exact source base

    existing local ID
        -> HeadMoved {
               expected: existing local head,
               actual: pinned source head
           }
        -> no local ref mutation

When the IDs are the same:

| Local state relative to pinned source head | Result |
|---|---|
| Local Branch absent | Create same-ID local Branch at exact source base/head |
| Heads equal | UpToDate; zero ref writes |
| Local head is an ancestor of source | Exact-CAS local head from local to source |
| Source head is an ancestor of local | UpToDate/local-ahead; never rewind |
| Heads diverge | HeadMoved { expected: local_head, actual: source_head }; no ref mutation |

The same-ID ancestry test uses admitted Commit metadata and indexed ancestry;
it does not compare filesystem timestamps or choose a winner.

To resolve divergence without a hidden Pull merge:

    pull_branch(source_branch_id, fresh_local_branch_id)
        -> merge(
               fresh_local_branch_id,
               existing_local_target_id,
               expected_target_head
           )
        -> push_branch(existing_local_target_id)

Pull never overwrites a divergent local Branch, invokes Merge implicitly,
creates a temporary/ref table, or adds another public operation.

## 12. SQL statement and transaction formulas

### 12.1 Cross-Store receiver

The following receiver formula starts after the source has discovered/paged the
typed ancestry. It is the receiver admission/write phase, not the complete
source-plus-destination operation:

    ObjectId membership SELECTs    = P_o
    typed membership/discovery     = H
    object INSERT statements       = J
    immutable fact INSERTs         = F
    final metadata/CAS statements  = D, D <= 8

    receiver admission statements  <= P_o + H + J + F + D
    durable write transactions      = max(1, J + F), if state changes
    UpToDate write transactions     = 0

The last object or fact admission transaction folds the final ref/head
visibility statements. If no admission batch is required but the ref must
move, use one visibility transaction.

Typed ancestry discovery/membership adds H pages at each side where a
destination membership check is required. The complete indexed read/query
envelopes are:

| Transfer operation | Indexed ancestry/frontier/preflight queries |
|---|---:|
| pull_branch(source_branch_id, local_branch_id) | 1 + 2H + P_o |
| push_branch to StackStore or LayerStore | 1 + 2H + P_o |
| pull_commit_history | 1 + 2H + P_o |
| pull_layer_history | 2H + P_o |
| pull_stack_history | 2H + P_o |
| push_stack | 1 + 2H + P_o |

The leading 1 is the operation-specific Branch pin/current-head,
AddResult/head, or attestation/copied-head preflight. It is absent when the
explicit history/node pair itself is the complete preflight.

For push_stack, H, P_o, F, and J include the frozen Branch rows, exact accepted
Commit DAGs, AddResults, and required root objects for every Stack in the
pushed suffix. Those are part of the transfer dependency closure, not a second
operation or an uncounted central-repair phase.

For zero-copy Pull Branch, accepted object payload remains in the configured
parent, so P_o and J for that accepted payload are zero. H still counts Commit
and typed dependency discovery, and P still counts the coalesced wire turns
needed to expose the Branch metadata safely.

Pull Branch ref transaction outcomes are:

- fresh different local ID: one final Branch insert after verified metadata;
- occupied different local ID: zero Branch-ref writes and HeadMoved with the
  existing local head and pinned source head;
- same ID absent: one final Branch insert;
- same ID and local ancestor: one exact Branch-head CAS in the final
  transaction;
- equal, local-ahead, or divergent: zero Branch-ref writes;
- divergent: return HeadMoved and retain the existing local target unchanged.

Concurrent Pulls targeting the same local Branch ID pass through the one Store
operation queue, so each reads the result of the preceding completed Pull.

Counting source ancestry/fact/payload reads, destination membership/admission,
and final metadata, a conservative complete source-plus-destination SQL
statement bound is:

    2H + 2P_o + 2J + 2F + D + operation_preflight

where operation_preflight is zero or one as shown above. Layered-parent
filesystem preflight reads are counted separately as L.

### 12.2 Pull Commit History

Pull Commit History creates no Branch, history, cursor, or copied-head row.

    ancestry/frontier queries  <= 1 + 2H + P_o
    receiver admission writes  = J + F
    durable transactions       = J + F
    everything already known   = 0 writes

The pinned terminal Commit is admitted in the final fact batch. No synthetic
visibility-only transaction is added.

### 12.3 Local Commit

Local Commit does not query object existence before writing:

    SQL statements      <= J object inserts + Commit insert + Branch CAS
    write transactions   = max(J, 1)

When J is zero or one, objects, Commit, and Branch CAS share one transaction.
When J is greater than one, the first J - 1 closure-complete object batches
commit independently and the final batch includes Commit plus Branch CAS.

### 12.4 Local Branch Merge

Read preflight includes:

    one joined Branch/base snapshot
    + C merge-base CTE statements
    + S indexed structural reads
    + G payload batches

Clean:

    object batches + merge Commit + target Branch CAS
    write transactions = max(J, 1)

Conflict:

    zero live database writes

### 12.5 Add Stack and Add Layer

The Store's single active-operation gate is acquired before reading the
StackHistory or LayerHistory head and remains held through preflight,
three-way, bounded admission, and the final exact CAS. The gate is not a
SQLite write lock: no write transaction is open during reads or computation.

One Add therefore:

    read current head once
        -> evaluate once
        -> exact-CAS once
        -> return

Concurrent callers queue. After the first Add completes, the next caller
enters the gate, reads the resulting head, and evaluates once against that
head. There is no internal CAS retry or re-evaluation loop.

Clean with new objects:

    J object insert statements
    + candidate Stack or Layer insert
    + AddResult insert
    + exact head CAS

    write transactions = max(J, 1)

The last object batch, candidate row, AddResult, and head CAS share the final
transaction.

Equal-root Add Stack for a new Branch source:

    insert one same-root child Stack
    insert source -> child Stack AddResult
    exact-CAS StackHistory head to the child
    one transaction and zero new payload bytes

Every newly accepted Branch event advances StackHistory metadata even when its
merged root equals the current root. This keeps later Push Stack provenance
observable; only a repeated Add for an existing AddResult is a zero-write
no-op.

Existing AddResult:

    one indexed lookup
    zero writes
    return the existing result

Conflict:

    zero live writes

Under the legal one-owner/one-gate design, final head CAS loss is not expected.
The CAS remains a defensive integrity boundary. An injected failure or illegal
external head movement rolls back the final candidate/AddResult/head
transaction and returns HeadMoved immediately. Earlier closure-complete object
batches remain unreachable and reusable; the Store does not perform a second
attempt automatically.

### 12.6 Creation operations

| Operation | Read preflight | Write transaction |
|---|---|---|
| create_branch_from_layer | Verify explicit LayerHistory membership and known root through the configured LayerStore parent | Reuse/insert canonical anchor Commit and insert Branch together; one transaction, zero payload copy |
| create_branch_from_stack | Verify explicit StackHistory membership, base Layer, and known root through StackStore | Reuse/insert canonical anchor Commit and insert Branch together; one transaction, zero payload copy |
| create_branch_from_commit | Verify the source Commit is reachable from the source Branch in the same BranchStore | Insert only the new Branch ref; one transaction |
| create_stack_history_from_layer | Verify the Layer and root are present in StackStore; create signer outside core SQLite | Insert StackHistory, immutable seed Stack sharing the Layer root, and head together; one transaction |
| SDK LayerHistory genesis provisioning | Admit/verify canonical empty-root closure before setup | Insert canonical genesis Layer plus LayerHistory/head together; one transaction |

Creation preflight completes before its write transaction. Random Branch and
History ID collisions, wrong-history membership, malformed typed IDs, missing
roots, and unreachable source Commits reject before mutation. No creation
operation copies a complete filesystem or introduces a transfer session.

## 13. WAL, FULL durability, and visibility folding

Every Store SQLite file opens with:

    PRAGMA journal_mode = WAL
    PRAGMA synchronous = FULL
    PRAGMA temp_store = FILE
    PRAGMA cache_size = benchmark-frozen SDK page budget

These durability settings are mandatory in tests and benchmarks.

### 13.1 Transaction ordering

    network / CDC / encode / hash / authentication
    signature verification
    merge-base discovery
    semantic digest
    three-way
            |
            | all complete before writer transaction
            v
    bounded object transaction(s)
            |
            v
    bounded fact transaction(s)
            |
            v
    final admission transaction
        + at most D metadata statements
        + exact CAS
            |
            v
    acknowledgement

No SQLite write transaction may remain open during:

- network wait;
- CDC;
- canonical encoding;
- ObjectId hashing;
- canonical object authentication;
- StackHistory signature verification;
- semantic digest streaming;
- merge-base CTE result processing outside SQLite;
- or three-way traversal.

### 13.2 Lock-size bounds

| Transaction class | Hard content bound |
|---|---|
| Normal object batch | 128 rows and 4 MiB |
| Oversize singleton | One valid canonical object, at most 16 MiB |
| Typed fact batch | 128 rows and 64 KiB fact fields |
| Final metadata portion | At most 8 statements and 64 KiB |

Standalone visibility-only transaction targets warm-WAL p95 at or below
10 ms. Object/fact batch transaction targets warm-WAL p95 at or below 25 ms.
Automatic-checkpoint spikes are included.

A folded visibility transaction uses its object/fact batch class. It does not
owe an additional impossible 10 ms total bound. Its incremental final metadata
work remains D <= 8, at most 64 KiB, and at most 1.25 times isolated CPU work
for those prepared statements.

A valid oversize singleton is compared to an isolated FULL+WAL transaction
with the same byte count rather than the 4 MiB target.

### 13.3 Checkpoints

P1 benchmarking selects one WAL_AUTOCHECKPOINT_PAGES constant and freezes it
for all stores.

- p95 includes automatic-checkpoint spikes;
- no explicit checkpoint runs inside a storage operation;
- no explicit checkpoint runs around the final CAS window;
- if evidence later requires an explicit checkpoint, only PASSIVE between
  operations is legal;
- no checkpoint worker, checkpoint table, or SDK checkpoint option exists.

## 14. Large-file streaming

Large files are never transferred or reconstructed as one in-memory buffer.

    typed root
        -> known-root test
        -> bounded child frontier
        -> 512-ID membership page
        -> missing canonical object frames
        -> receiver authentication
        -> 128-row / 4 MiB admission batch
        -> next frontier

One valid object up to 16 MiB is a singleton. The protocol does not split a
canonical object merely to meet the 4 MiB target.

### 14.1 Backpressure

At most one active operation working set is admitted per Store handle.
Additional callers queue before transfer buffers are allocated.

The wire carrier owns only:

- framing;
- byte lengths;
- checksums;
- bounded frames;
- backpressure;
- rejection of incomplete byte frames over Read/Write.

It does not understand:

- ObjectId lookup;
- SQLite;
- deduplication;
- closure;
- Branch, Stack, or Layer;
- CAS;
- conflict;
- AddResult;
- or history authority.

### 14.2 Frame atomicity

The receiver authenticates and admits only complete frames. A short,
checksum-invalid, or malformed frame is discarded before object/fact
admission, the operation returns an error, and no automatic reconnect or
resume is attempted. SQLite transaction atomicity and idempotent primary-key
insertion remain the correctness floor for batches that did complete.

## 15. Known-root pruning

Receiver traversal begins with the requested root:

    root exists and is admitted?
        yes -> its complete closure is certified; stop descent
        no  -> request/authenticate its canonical bytes
               -> obtain child IDs
               -> recurse only into unknown children
               -> admit children before parent

N in transfer complexity means the visited frontier after pruning, not the
complete filesystem closure.

A transfer must not:

- enumerate the full closure before checking known roots;
- issue one contains RPC per object;
- replay historical payload operations;
- or copy a complete accepted filesystem into BranchStore.

The 512-ID guarantee belongs to the transfer-specific frontier walker. Existing
rope, inode, namespace, and semantic-digest walkers still perform counted
individual structural reads and 64-entry payload batches.

## 16. Deduplication matrix

Deduplication is scoped to each physical objects table plus missing-only
receiver transfer. V1 has no global CAS shared transparently by independent
SQLite files.

| Boundary | Identity and missing rule | Physical payload result |
|---|---|---|
| BranchStore local Commit | Same canonical ObjectIds; multi-row DO NOTHING | One row per ObjectId in that BranchStore |
| BranchStore local Merge | Reuse current/base/candidate IDs; admit only new clean result objects | Conflict adds zero rows; Clean adds new objects only |
| StackStore local/pulled data | Shared StackStore objects table | Commits, Stacks, and Layers reuse one local CAS |
| LayerStore central data | Shared LayerStore objects table | Branch, Stack, and Layer roots reuse one central CAS |
| Push Branch to StackStore | 512-ID membership then missing-only frames | Existing receiver payload is not sent |
| Push Branch to LayerStore | Same protocol and ObjectIds | Direct mode does not create a second identity path |
| Push Stack to LayerStore | Known Stack/root pruning plus every suffix AddResult's frozen Branch ref, exact Commit DAG, and required root frontier | Only missing Stack and accepted Branch provenance facts/payload move; copied head is visible last |
| Pull Branch(source, local) | Destination verifies exact source base/root and admits missing Commit metadata without copying accepted parent payload | Fresh local ref inserts once; same-ID ref follows exact ancestry/CAS rules and never rewinds or hides divergence |
| Pull Commit History | Destination stops at known Commit paths and roots | No Branch ref is created |
| Pull Layer History | Destination stops at known Layer and root closure | Exact prefix only |
| Pull Stack History | Base Layer first, then exact Stack prefix | Read-only local StackHistory copy |
| Add Stack | Known admitted roots prune; DeferredObjectStore stages only new clean output | AddResult/head last; Conflict zero |
| Add Layer | Same three-way and CAS admission | AddResult/head last; Conflict zero |
| Concurrent identical insert | DO NOTHING RETURNING partitions newly inserted versus raced-existing | Exactly one physical row |
| Independent BranchStore databases | Each is a separate physical CAS placement | Each may retain one private copy |

Required ten-install proof:

    ten Branches
    + one physical BranchStore
    + identical byte streams
    + the same edit path from the same base
        ~= one package payload set
         + O(10) small Commit/Branch/structural metadata

Different edit histories may produce different valid COW structures. The
semantic equality fallback must merge equal logical content cleanly, but v1
does not promise arbitrary edit-history-independent storage convergence.

## 17. Discard versus retain policy

| State | Discard or retain |
|---|---|
| Receiver ID known before transfer | Omit bytes from the outgoing plan |
| Receiver ID races into the DB after negotiation | Drop incoming bytes from the active buffer; create no new row |
| Invalid frame/hash/codec/child closure | Delete receive scratch; expose no typed fact/ref; return Integrity |
| Three-way Conflict | Delete DeferredObjectStore memory/scratch; write zero production rows |
| Earlier valid object batches before local final CAS loss | Retain closure-complete unreachable objects; deferred GC may later reclaim them |
| Valid transferred immutable rows before final transfer CAS loss | Retain unreachable immutable facts; do not expose them |
| Sender objects after any Push | Retain; Push never deletes the source |
| Incomplete wire frame | Discard incomplete bytes; never admit |

Garbage collection is deferred. Retention does not authorize a GC table or a
hidden cleanup transaction.

## 18. DeferredObjectStore and first conflict

Three-way result construction writes to one private bounded
DeferredObjectStore:

    three_way(base, current, candidate)
               |
               v
    memory, bounded within the 8 MiB merge budget
               |
               | spill only when required
               v
    disposable authenticated scratch

Three-way visits paths in canonical bytewise lexicographic order.

The only conflict shape is:

    Conflict {
        path,
        base: Absent | ObjectId,
        current: Absent | ObjectId,
        candidate: Absent | ObjectId,
    }

At the first genuine conflict:

- stop traversal immediately;
- discard DeferredObjectStore memory and scratch;
- write zero object rows;
- write zero Commit/Stack/Layer rows;
- write zero AddResult rows;
- move no head.

There is no conflict Vec, all-conflicts mode, conflict count, truncated flag,
continuation token, or later-path read.

On Clean:

- admit trusted in-memory staged (ObjectId, canonical bytes) pairs without a
  second canonical encode or ObjectId hash;
- if and only if a pair was reread from disposable scratch spill,
  re-authenticate that pair once and record the exceptional counter;
- validate dependency roles and closure ordering without reminting identity;
- admit them in J bounded batches;
- place Commit/Stack/Layer, AddResult when applicable, and exact CAS into the
  final transaction.

## 19. Merge-base discovery

Commit merge-base discovery must not build an unbounded Rust HashSet or
materialize the Commit DAG in application memory.

Use one to three indexed SQLite recursive CTE statements:

    source Commit head ----\
                            -> recursive ancestry through
    target Commit head ----/      parent_id and merge_parent_id
                                      |
                                      v
                               UNION dedup in SQLite
                                      |
                                      v
                               common candidates
                                      |
                                      v
                               remove non-maximal candidates
                                      |
                                      v
                               return at most two IDs

| Final candidate count | Result |
|---:|---|
| 0 | Continue to the closest common Stack, then the closest common Layer |
| 1 | Exact Commit merge base |
| 2 | AmbiguousMergeBase |

Only final candidates are paged to Rust. SQLite owns transient recursive work:

- UNION, not UNION ALL, deduplicates diamonds;
- Commit PK and parent-edge indexes serve traversal;
- temp_store=FILE allows spill;
- benchmark-frozen page cache bounds memory;
- no product or staging table is created;
- no complete ancestry vector crosses the Store boundary.

EXPLAIN QUERY PLAN must show indexed access and transient B-trees only for
recursive dedup/final candidate work. A full commits corpus scan is a failure.

Stack and Layer fallback histories are strict lists and use indexed parent
traversal after Commit candidates are exhausted. NoCommonBase is returned only
when the Branch bases resolve to different LayerHistories or no common Commit,
Stack, or Layer can be proven. Zero common Commit candidates alone is not
NoCommonBase.

## 20. StackHistory writer authority during transfer

Exactly one creator StackStore writes each StackHistory. Pull never transfers
the private signer.

### 20.1 Required stacked provenance closure

For every BranchId -> StackId AddResult whose Stack is in the pushed suffix,
Push Stack includes this dependency closure:

    AddResult(BranchId -> StackId)
        |
        +-- frozen same-ID Branch row from StackStore
        |       base_id = exact Stack base used by Add Stack
        |       head_commit_id = exact accepted Commit
        |
        +-- missing Commit DAG through both parent edges
        |
        +-- required Commit root objects
        |
        +-- resulting Stack manifest/root

The Branch row is frozen for publication because an existing AddResult for
that BranchId prevents another accepted Push Branch update under the same ID.
Push Stack must validate:

1. AddResult maps the exact BranchId to the exact StackId in the suffix;
2. StackStore Branch head is the exact Commit accepted by Add Stack;
3. Branch base belongs to the same StackHistory route;
4. Commit DAG and every required root closure authenticate;
5. the creator StackStore's signed attestation binds the exact predecessor
   Stack, Branch/AddResult, accepted Commit, and result Stack/root;
6. an already-present LayerStore Branch row has the identical frozen base/head,
   otherwise the request is Integrity rather than a Branch overwrite.

LayerStore verifies the signature, exact IDs/relationships, canonical typed
facts, and object closures. It trusts the authorized creator's already-completed
Add Stack semantics and never calls or recomputes three_way during Push Stack.

LayerStore negotiates the provenance using the same H typed pages, P_o object
pages, F fact batches, and J object batches. It admits the frozen Branch ref,
Commit DAG, required roots, AddResult, and Stack facts missing-only before the
copied StackHistory head becomes visible. Until the final exact CAS, admitted
immutable facts may remain unreachable, but the copied head may not expose a
partial closure.

This closure is what makes LayerStore central completeness and later
pull_commit_history(branch_id) possible. It adds no operation, table, column,
or second provenance record.

StackHistoryId commits to the creator verification-key digest. Push Stack
attests:

    history_id
    expected_layerstore_head
    incoming_head
    digest of ordered predecessor->result Stack suffix, AddResults,
        frozen Branch refs, exact accepted Commit manifests/DAG root IDs,
        and result Stack/root IDs
    digest of canonical request and complete typed/ObjectId provenance frontier

LayerStore verifies:

1. public key digest matches StackHistoryId;
2. signature covers the exact tuple;
3. suffix is a linear descendant chain;
4. every Branch/AddResult/Commit/Stack provenance relation above validates;
5. suffix and request/frontier digests match;
6. Branch Commit roots and Stack roots close and authenticate;
7. all missing provenance facts are admitted before visibility;
8. copied head exact-CASes from expected_layerstore_head to incoming_head.

| LayerStore copied head | Result |
|---|---|
| Absent and attested expected Absent | Admit prefix and copied head |
| Equal to incoming | UpToDate |
| Verified descendant of incoming | Delayed replay; UpToDate, never rewind |
| Equal to attested expected and ancestor of incoming | Admit suffix and fast-forward copied head |
| Linearly related but expected is stale | HeadMoved |
| Divergent or reparented | Integrity |

Push Stack does not run three-way, create a Stack, create a Layer, or transfer
writer authority.

## 21. CPU, memory, and byte bounds

Let:

    O = max(OBJECT_BATCH_BYTES, MAX_OBJECT_BYTES)
      = max(4 MiB, 16 MiB)
      = 16 MiB

A transfer holds at most two O-sized buffers:

    one source/send canonical buffer       <= 16 MiB
    one destination/receive canonical buffer <= 16 MiB
    -------------------------------------------------
    object bytes                           <= 32 MiB

Add:

- two bounded ID/frontier pages;
- two fixed 64-byte bitmaps;
- bounded frame headers and checksums;
- bounded typed facts and final metadata;
- small Merkle traversal state.

The non-object overhead is conservatively below 2 MiB:

    transfer working buffers < 34 MiB

Three-way and DeferredObjectStore reserve at most 8 MiB of in-memory
application state before scratch spill:

    transfer buffers                < 34 MiB
    three-way + DeferredObjectStore <= 8 MiB
    -----------------------------------------
    application operation memory    < 42 MiB

SQLite is accounted separately:

    total per active Store handle
        < 42 MiB
        + SQLITE_PAGE_CACHE_BYTES
        + fixed SQLite/runtime overhead

SQLite recursive CTE work spills to temp files rather than expanding the Rust
heap or operation buffers.

Only one active transfer/mutation working set exists per Store handle.
Connected clients do not multiply 42 MiB; they queue before allocation.

### 21.1 CPU bounds

| Work | CPU/query bound |
|---|---|
| Existing ObjectId membership | P_o set-based indexed queries |
| Missing object authentication | Linear in missing canonical bytes |
| Local COW edit | O(replacement bytes + touched extent-tree work) |
| Logical three-way | Linear in S structural reads and E payload extents until Clean or first Conflict |
| Semantic equal-content fallback | At most three streamed roots, O(1) digest memory |
| Merge-base | Indexed recursive CTE over reachable Commit nodes/edges, bounded application result |
| Object admission | J prepared multi-row statements |
| Typed-fact admission | F prepared multi-row statements |
| Final visibility | D statements, D <= 8 |

Core object-size, field-size, child-count, component, tree-depth, and canonical
codec limits are enforced before admission.

No operation may accumulate:

- a complete closure list;
- a complete filesystem;
- a conflict collection;
- a Rust ancestry set;
- an unbounded transfer frontier;
- or per-client copies of the active transfer buffers.

## 22. Search and traversal complexity

| Search | Storage path | Complexity |
|---|---|---|
| Exact ObjectId | objects primary key | Indexed B-tree lookup; one fixed comparison key |
| Exact Commit/Stack/Layer | Typed table primary key | Indexed B-tree lookup |
| Branch current head/base | branches primary key plus joined typed lookup | Constant number of indexed statements |
| Layer/Stack prefix | Recursive CTE over strict parent list | Linear in requested missing prefix, paged |
| Commit ancestry transfer | Bounded frontier IDs and indexed Commit lookups | Linear in visited/missing DAG after known-node stop |
| Commit merge-base | C <= 3 recursive CTEs with UNION dedup | Linear in reachable DAG work inside SQLite; at most two result IDs |
| Filesystem known root | objects primary key | One indexed root test; known root prunes entire subtree |
| Logical structural walk | Current rope/inode/namespace readers | S indexed individual structural reads |
| Payload extent stream | Existing batch reader | G batches of at most 64 extents |
| Add repeated/no-op known root | Typed/root lookup plus AddResult | Constant indexed preflight; zero descendant reads |

N+1 relational parent queries are forbidden for Layer, Stack, and Commit
history transfer. The honest S individual structural reads in existing logical
walkers are not mislabeled as 512-ID batching.

## 23. Bounded error and concurrency model

The experimental v1 assumes no Store-process crash and no network break during
an accepted operation. The correctness floor is SQLite atomicity under
WAL/FULL, idempotent primary-key admission, authenticated complete frames,
ordered visibility, and exact CAS.

| Condition | Required result |
|---|---|
| Incomplete, malformed, or checksum-invalid frame | Admit nothing from that frame and return an error |
| Invalid canonical bytes or child closure | Expose no typed fact/ref; discard transient bytes and return Integrity |
| Wrong history or source route | Perform no mutation and return the typed error |
| Missing immutable dependency | Perform no hidden Pull; return MissingBaseData |
| Forged Stack attestation or mismatched stacked provenance | Do not move copied head; return Integrity |
| Injected or illegal external Add head movement | Roll back final candidate/AddResult/head transaction and return HeadMoved once |
| Add path Conflict | Write zero production rows and return the first deterministic Conflict |
| Same Branch ID concurrent Push | Exact CAS or same-ID validation decides; no overwrite/merge shortcut |
| Different Branch IDs in one Store | Fair serialized queue processes both independently |
| Operations against different Store database files | Execute independently; no global writer or global CAS couples them |
| Second owner of the same SQLite file | Fail promptly with StoreBusy |

Reconnect/resume, lost-ack replay guarantees, server-crash recovery,
kill-point matrices, Store-open scratch recovery, recovery benchmarks, and GC
policy are explicitly deferred. Their absence must not weaken insertion order,
frame admission, primary-key idempotence, or exact CAS.

## 24. SQLite ownership and remote filesystem rules

Each SQLite file has:

- one owning Store process/handle;
- one admitted SQLite connection;
- one fair serialized mutation/transfer queue;
- one active operation buffer set.

Another process that tries to own the same file receives StoreBusy. No
owner/lease table coordinates ownership.

Remote clients and other machines use the Store endpoint. They never open the
SQLite file over:

- NFS;
- SMB;
- FUSE remote mounts;
- shared container volumes that do not provide local SQLite locking semantics;
- or a copied live WAL/SHM set.

The defensive two-connection insert-race fixture exists to verify
ON CONFLICT RETURNING correctness. It does not authorize multiple production
writers or a pool.

### 24.1 Queueing and fairness

Concurrent callers queue before active buffer allocation.
The queue must not starve an accepted caller; it serializes operations for this
database file while other Store database files progress independently.

Measure separately:

- queue wait;
- operation service time;
- throughput;
- peak active memory;
- caller completion order;
- starvation;
- StoreBusy behavior;
- writer-lock duration by transaction class.

RTT and p95 transaction bounds exclude queue wait. Reports must not hide it.

## 25. Crate and responsibility boundary

The cold architecture has four production crates:

| Crate | Transaction/transfer responsibility |
|---|---|
| layerfs-storage-core | IDs, canonical objects, fixed schemas/SQL shapes, admission batches, recursive merge-base, shared three-way, byte-only wire |
| layerfs-branch-store | Branch persistence, Commit, Merge, direct/stacked Branch Push/Pull orchestration |
| layerfs-stack-store | Stack persistence, writer capability, Add Stack, history/Commit pulls, Push Stack orchestration |
| layerfs-layer-store | Central facts, authoritative Layer CAS, copied Stack head verification, Add Layer, transfer serving/receiving |

storage-core wire is the only byte carrier. It owns framing, checksums,
backpressure, and bounded Read/Write transport. Store operations own:

- topology routing;
- membership negotiation;
- deduplication;
- object/fact admission;
- history validation;
- authority;
- CAS;
- and visibility.

There is no layerfs-transfer, layerfs-sync, or layerfs-server crate.

## 26. Explicitly forbidden production designs

| Forbidden | Reason |
|---|---|
| Raw SQLite protocol or shipping database/WAL files | Breaks locking, topology, and transaction semantics |
| Application-facing raw storage connection | Bypasses Store validation, CAS, and authority |
| Transfer/session/request/progress table | Canonical IDs, missing membership, CAS, and AddResult already prove progress |
| Global transparent CAS across independent DB files | Expands v1 topology and authority; physical stores dedup independently |
| Metrics table | Query/byte counters belong to external test instrumentation |
| GC/retention table | GC is deferred and must not alter transfer correctness |
| Connection pool or multiple production writers | One owner/pipeline bounds memory and SQLite contention |
| Async runtime solely for transfer | Synchronous bounded Read/Write pipeline is sufficient |
| Compression layer | Not justified by measured evidence in v1 |
| Bloom filter | Exact 512-ID indexed membership is the one protocol |
| Dynamic 1..512 existence statements | One fixed prepared statement is simpler and faster |
| Per-object SELECT, RPC, or transaction on payload bulk paths | Violates P_o/J SQL bounds and P wire-turn bounds |
| Full closure enumeration before known-root lookup | Defeats pruning |
| Full file or full DB copy fallback | Violates zero-copy/missing-only storage model |
| Alternate CDC/hash/serialization path | Breaks cross-store identity |
| Conflict collection or all-conflicts scan | One deterministic first Conflict is the complete v1 result |
| Rust Commit ancestry HashSet | SQLite recursive CTE owns dedup and bounded spill |
| Network/CDC/hash/merge under writer lock | Produces unbounded lock duration |
| Push that creates Stack/Layer | Push transfers; Add mutates history |
| Add that silently Pulls | Missing dependencies return MissingBaseData |
| Push that deletes sender objects | Sender retention and source integrity are mandatory |

## 27. Terminal verification gates

Terminal pass requires raw evidence for every applicable gate below.

### 27.1 Identity and COW

| Test | Required proof |
|---|---|
| Object identity fixture | Persisted/transferred ID equals ObjectId::for_bytes(encode_bytes_object(raw)); raw chunk_id never appears |
| One encode/hash | Local encode/hash once; receiver authenticates once; SQLite does not repeat; scratch reload exception counted separately |
| COW replace locality | CDC scans exactly replacement bytes, reads zero old suffix payload, retains unchanged extent IDs |
| Independent FastCDC shift | From-scratch original/shifted streams reuse the frozen suffix fixture; fixed-block oracle fails |
| Edit-history representation | Equal logical bytes may have different valid roots |
| Semantic equality | Different roots with equal logical bytes merge cleanly after length plus streamed digests |

### 27.2 Membership and admission SQL

| Test | Required proof |
|---|---|
| SQLite version | Store open rejects versions before 3.35 |
| Fixed statement | Exactly one 512-placeholder existence statement is prepared |
| Short page | Trailing placeholders are NULL; bitmap is always 64 bytes with zero tail |
| Result ordering | Shuffled SELECT result order maps to the correct bitmap positions |
| Query plan | objects primary key is used; no table scan |
| Fixed typed membership | Each relevant typed table has one prepared 512-placeholder PK statement; every H page is <=512 IDs and returns one fixed 64-byte bitmap with NULL padding, result remap, and zero tail |
| Typed ancestry cursor | One prepared Commit UNION-dedup recursive CTE cursor emits <=512-row pages without LIMIT/OFFSET reruns or Rust seen set; SQLite owns dedup/spill, H equals emitted pages, and network transport holds no writer gate/write transaction |
| Typed/Object separation | Typed IDs query only their typed tables, ObjectIds query only objects, and their separate fixed bitmaps share one reply |
| P_o/H SQL counts | Instrumented queries equal actual ObjectId pages P_o and H = sum_t ceil(A_t/512); a mixed one-Commit/one-Stack fixture produces two typed pages, not one |
| Object batch | 128-row/4 MiB packer matches deterministic expected batches |
| Oversize singleton | Valid 16 MiB object occupies one FULL+WAL transaction |
| Fact batch | Widest 128-row/four-column statement uses 512 binds; no 256-row shape |
| Final metadata bound | SQL trace proves D <= 8 and final metadata bytes <= 64 KiB for every visibility-changing operation |
| Insert race | Two test connections partition returned versus raced-existing IDs exactly |
| No per-object fallback | Bulk payload path never uses per-ID SELECT, RPC, or transaction |

### 27.3 Transfer protocol and bytes

| Test | Required proof |
|---|---|
| Pipelined frame order | Payload i and announcement i+1 share a frame; ack i and bitmap i+1 share a reply |
| RTT count | Transfer never exceeds P + 1 on a reused stream |
| Coalesced wire pages | Actual P is no greater than P_o + H and carries typed/ObjectId sets without mixing their SQL tables |
| Direct publication | Never exceeds P_branch + 2 |
| Stacked publication | Never exceeds P_branch + P_stack + 4 |
| Known root | Completes in first reply and does not enumerate descendants |
| Missing-only bytes | Sent IDs equal exactly receiver-missing IDs |
| Pull Branch fresh/occupied local ID | Fresh creates requested local ID at pinned source base/head with zero accepted payload copy; occupied returns HeadMoved { expected: local, actual: source } with zero ref mutation |
| Pull Branch same-ID states | Absent creates, equal is UpToDate, local-ancestor fast-forwards by exact CAS, source-ancestor preserves local-ahead, and divergence returns HeadMoved with zero ref mutation |
| Pull Branch divergence workflow | Pull source into a fresh local ID, Merge fresh source into existing local target, then Push target; no hidden merge/ref table/new verb |
| Sender behavior | Reads stored canonical bytes without CDC/re-encode/re-ID |
| Stacked provenance closure | For every pushed BranchId->StackId AddResult, transfer contains the frozen same-ID Branch at exact accepted Commit, missing Commit DAG, and required root objects |
| Stacked provenance validation | Tampered predecessor/result Stack edge, Branch head/base, AddResult target, accepted Commit DAG/root, or result Stack/root fails signature/Integrity validation before copied-head visibility |
| Push Stack never merges | Instrument shared three_way and prove push_stack invokes it zero times; LayerStore verifies creator attestation, canonical facts, relationships, and closure only |
| Central Commit availability | After Push Stack succeeds, LayerStore pull_commit_history(branch_id) serves the exact accepted Commit DAG and roots without repair |
| Signed provenance frontier | Changing or omitting any Branch/AddResult/Commit/root provenance fact invalidates suffix/request signature verification |
| Large-file streaming | No whole-file buffer; frames and batches remain bounded |
| Incomplete frame | Short/checksum-invalid/malformed frame admits no row and returns an error; no reconnect/resume behavior is tested |
| Byte-only wire | Carrier has no ObjectId, SQLite, dedup, CAS, or history branch |

### 27.4 Transactions and durability

| Test | Required proof |
|---|---|
| WAL/FULL | Every Store/test/benchmark uses both |
| Folded visibility | Final ref/head is in the last bounded admission transaction |
| Pull Commit all-known | Zero writes |
| Pull Commit partial | Exactly J + F durable transactions |
| Constant-size create/setup | Layer/Stack branch anchors, create-from-Commit Branch, StackHistory seed/head, and LayerHistory genesis/head each use one transaction after successful preflight |
| Create/setup rejection | Collision, wrong history, unreachable Commit, or missing root writes zero rows |
| Local Commit | max(J, 1) transactions and no existence pre-query |
| Clean Add | max(J, 1); candidate/AddResult/head share last transaction |
| Serialized Add callers | Each caller reads/evaluates/CASes once under the operation gate; the next queued caller observes the completed head |
| Injected Add CAS loss | Final candidate/AddResult/head rolls back and HeadMoved returns immediately with no internal retry |
| Equal-root new Add | Add Stack: same-root Stack + AddResult + head CAS in one transaction; Add Layer: one conditional AddResult transaction |
| Existing AddResult | One indexed read, zero writes |
| Conflict | Zero row delta in every production table |
| Visibility order | Child objects precede parents, root closure precedes typed facts, immutable facts precede AddResult/ref/head, and exact CAS is last |
| Stacked provenance visibility | Frozen Branch/Commit/root/AddResult facts are complete before copied-head CAS; no query through the copied head can observe a partial closure |
| Writer-lock scope | No network/CDC/hash/signature/digest/three-way inside write transaction |
| Lock p95 | Batch and visibility classes meet fixed/normalized gates with checkpoint spikes |
| Checkpoint | Benchmark-frozen threshold; optional PASSIVE only between operations |

### 27.5 Conflict and merge-base

| Test | Required proof |
|---|---|
| First Conflict | Multiple conflicts supplied out of order return first canonical lexicographic path |
| Conflict scalar shape | Exactly path/base/current/candidate; no collection/count/truncated/continuation |
| Deferred scratch | Conflict deletes scratch and writes zero rows; Clean admits bounded batches |
| Large diamond DAG | C is at most 3; no Rust ancestry HashSet/vector |
| Ambiguity and fallback | Zero Commit candidates continue to closest common Stack then Layer; one selects Commit; two incomparable candidates return AmbiguousMergeBase; NoCommonBase requires different/no common LayerHistory ancestry |
| CTE plan | Commit PK/parent indexes used; UNION dedup; no corpus scan |
| CTE memory | Page-cache high-water stays within bound; temp spill bytes recorded |

### 27.6 Memory, CPU, queue, and load

| Test | Required proof |
|---|---|
| Transfer buffers | Below 34 MiB at current limits |
| Three-way/deferred | At or below 8 MiB before scratch spill |
| Application working set | Below 42 MiB per active Store operation |
| SQLite memory | Page-cache high-water and fixed overhead reported separately |
| Frontier memory | At most one 512-ID ObjectId page plus one 512-ID homogeneous typed page and their two fixed 64-byte bitmaps per coalesced wire turn |
| Structural reads | S individual indexed reads reported honestly |
| Payload reads | G actual 64-entry batches; no synthetic 512 structural claim |
| Ten-caller contention/order | All callers are accepted into one fair serialized queue before buffer allocation; one active working set, PK dedup, child-before-parent admission, correct final heads, no partial visible closure, and measured queue wait/throughput/peak memory |
| Concurrent Pull same local ID | Serialized callers each observe the preceding completed local head and apply the same exact same-ID ancestry/CAS table without overwrite |
| Starvation | Every queued caller completes in finite order without bypassing visibility/CAS rules |
| Independent Store DBs | Concurrent operations on different database files progress independently without a global queue or global CAS |
| Multiple owner | Second owner fails StoreBusy |
| Remote filesystem | Test configuration never opens SQLite through network storage |

### 27.7 Deduplication and bounded errors

| Test | Required proof |
|---|---|
| Every dedup boundary | BranchStore, StackStore, LayerStore, and every Push/Pull/Add follow the matrix |
| Push Stack missing-only provenance | Preseed a subset of Branch/Commit/root/AddResult facts; transferred typed IDs/ObjectIds equal exactly the missing stacked provenance closure |
| Ten identical operations | Same physical Store and same base/edit path approach one payload set plus small metadata |
| Independent DBs | Each may retain its own private payload placement; no hidden global CAS |
| Discard/retain | Preexisting omitted, raced bytes dropped, invalid/conflict scratch deleted, unreachable valid facts retained, sender never deleted |
| Injected final CAS loss | Final candidate/AddResult/head rolls back, earlier closure-complete immutable objects remain unreachable, and HeadMoved returns without internal retry |

## 28. Final implementation rule

The implementation must optimize the one model described here. It must not add
a second local fast path, remote protocol, object identity, transaction model,
or compatibility surface.

The desired result is:

    few public operations
    + few tables and columns
    + set-based SQL
    + missing-only canonical bytes
    + bounded transactions
    + exact CAS
    + deterministic first conflict
    + atomic bounded error handling
    + measured memory and latency

Detailed documentation is not architectural complexity. Extra production
state, duplicate algorithms, and ambiguous transaction ownership are.
