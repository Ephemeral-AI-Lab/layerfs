# CAS, CDC, and Delta Encoding Handbook

> Status: Current general guide.

Written 2026-10-05 against product source
`27d87a2b3` (documentation checkpoint; underlying product unchanged from
`8cbeadef07dc9ac1e79cd59eaee3dca494e2ff87`).
This guide explains the implemented mechanisms and caller obligations. It does
not introduce a new format, configuration, transport endpoint, or qualification.
The linked source contracts govern. For API composition and benchmark evidence,
read the [Cluster One Handbook](cluster_one_handbook.md).

S5 addition after `f5558fc22`, 2026-10-05: the public `construct_runs` API
accepts explicit logical zeros and preserves the frozen CDC/canonical profile.
The all-zero chunk period is 32 KiB; complete zero chunks and mapping subtrees
are reused without scanning their logical length. Bounded edge/threshold work
remains. [Payload streams](core/docs/architecture/31-payload-streams.md) records
root equality/work proofs and the unimplemented sparse localized-edit/Commit
integration. Earlier numerical receipts retain their identities and verdicts.

## 1. The three mechanisms and their boundaries

**CDC chooses pieces, CAS recognizes identical canonical objects, and delta
encoding stores distinct but related payloads using a base.** They solve different
problems and can cooperate in the same Save.

| Mechanism | Question answered | Owner | Result |
| --- | --- | --- | --- |
| Content-defined chunking (CDC) | Where should a large file be split? | `layerfs-content` | Bounded payload chunks and a file mapping tree |
| Content-addressed storage (CAS) | Do we already have this exact canonical object? | Identity in content; admission/reuse in storage | Reuse of a verified existing object or admission of a new one |
| Delta encoding | Can this new payload be stored more compactly using an admitted base? | `layerfs-storage` | Independent encoding or role-specific base-dependent encoding |

Compression is another physical operation: an independent value can compress
without using a base. Delta encoding and ordinary compression are not synonyms.
All three mechanisms are shared across Durable and Disposable profiles. Their
correctness and identities do not depend on relaxing durability.

```text
                         STABLE INPUT FILE BYTES
                                   |
                         canonical file construction
                                   |
                  +----------------+----------------+
                  |                                 |
              below cutoff                     at/above cutoff
              whole-file value                 CDC payload chunks
                  |                                 |
                  |                           extent/mapping tree
                  |                                 |
                  +----------------+----------------+
                                   |
                         finalized canonical objects
                        { ObjectId, role, bytes, refs }
                                   |
                           exact CAS admission
                                   |
                  +----------------+----------------+
                  |                                 |
          identical object exists                 new object
          compare/authenticate                      |
                  |                         physical selection
              reuse it                              |
                  |                   +-------------+-------------+
                  |                   |                           |
                  |           independent encoding         base-dependent
                  |            FULL or STORED              PREFIX encoding
                  |                   |                           |
                  |                   +-------------+-------------+
                  |                                 |
                  |                       immutable pack placement
                  |                                 |
                  +----------------+----------------+
                                   |
                       reference-closed publication
                                   |
                                 SQLite
```

This is a semantic flow, not a requirement that every accepted object immediately
causes one SQL transaction. Save batches work, retains bounded unfinished pack
queues, and publishes reference-closed output incrementally.

## 2. Canonical identity versus physical representation

The implemented object identity is:

```text
  ObjectId = BLAKE3( "layerfs/object/v2\0" || canonical_object_bytes )
                   <----- frozen domain ----> <---- framed bytes ---->

  raw payload bytes ---> canonical framing ---> ObjectId
                              |
                              +--> FULL / STORED / PREFIX physical record
                                               |
                                         group / pack / locator
```

The canonical bytes-object envelope is explicit:

```text
  +----------+--------+------------------+----------------+------------------+
  | LFSO     | kind=1 | payload_len      | value_len      | role-specific    |
  | 4 bytes  | 1 byte | big-endian u32   | big-endian u32 | value bytes      |
  +----------+--------+------------------+----------------+------------------+
                         payload_len = 4 + value_len
```

The decoder checks supported kind/magic, length agreement, bounds, and trailing
bytes. Role-specific values provide the whole-file/chunk/tree grammar. A separately
supplied `ObjectRole` must still agree with admission/format requirements.

`ObjectId` is 32 bytes. It is not simply the hash of raw file bytes. Canonical
framing and structure participate in identity; whole-file and chunk
representations have their own canonical value grammars. A file root is an object
identity describing its canonical representation. A chunked file root references
mapping structure and payload slices, rather than being a raw whole-file checksum.

Physical encoding, compression, delta base choice, pack location, and SQLite
layout do not alter a given canonical object's ID. Decoding must reconstruct the
canonical bytes that authenticate against that ID.

```text
                     ONE CANONICAL OBJECT / ONE ID
                                  |
                +-----------------+------------------+
                |                 |                  |
         independent frame   stored raw payload   PREFIX frame + base ID
                |                 |                  |
                +-----------------+------------------+
                                  |
                          decode / reconstruct
                                  |
                         canonical bytes again
                                  |
                    recompute and verify ObjectId
```

Identical logical file bytes do not imply identical canonical roots across every
construction history. Representation choice and localized edit structure matter.
Advisory predecessor hints do not change the canonical ID of an already finalized
object; they influence physical selection.

Source: [ObjectId](core/crates/layerfs-content/src/object/id.rs),
[canonical envelope](core/crates/layerfs-content/src/object/codec.rs),
[finalized objects](core/crates/layerfs-content/src/object/output.rs).

## 3. CDC: stream construction and chunk boundaries

### 3.1 When it runs

`construct_stream` reads a bounded prefix up to the Store's configured cutoff.
At the default 128 KiB exclusive cutoff:

| Logical file length | Construction route |
| --- | --- |
| Empty | Special empty representation |
| Nonempty and below 128 KiB | Whole-file canonical value |
| At least 128 KiB | Chunked file construction |

The prefix is reused through `Cursor(prefix).chain(source)`; construction does not
rewind or reread the source to start the chunked path. The supported cutoff is a
power of two from 128 KiB through 1 MiB, persisted in policy. Derive policy and
capacities from the Store; do not silently choose a different cutoff per caller.

### 3.2 Frozen chunking profile

The implemented scanner is two-byte rolling GEAR content-defined chunking.

| Parameter | Current value / meaning |
| --- | --- |
| Minimum boundary-search length | 8,192 bytes |
| Target | 16,384 bytes; changes boundary-search mask, not an exact output size |
| Maximum | 32,768 bytes; forces a cut when reached |
| Normalization shift | 2 |
| Seed | 0 |
| Profile identity | Digest of sizes, shift, seed, masks, and frozen GEAR table |

The final EOF chunk may be shorter than 8 KiB. "Minimum" describes ordinary
boundary search, not a prohibition on a short final tail. Read-buffer boundaries
are not canonical chunk boundaries; scanner state continues across `Read` calls.
The rolling GEAR value selects boundaries. BLAKE3 authenticates canonical objects.
These hashes have different roles.

```text
  current chunk length
  0                    8 KiB              16 KiB              32 KiB
  |----------------------|-------------------|--------------------|
    collect initial        search using       search using         forced
    bytes                  small mask         large mask           cut

  any selected cut -> emit chunk -> reset scanner for next chunk
  EOF              -> emit remaining bytes, including a short tail
```

The scanner uses bounded buffers, emits chunks through a synchronous callback,
and stops on callback failure. Its borrowed chunk slice is valid only during the
callback; callers needing retained data must own it. Canonical construction emits
owned `FinalizedObject`s to its consumer, children before referring parents.

A chunk buffer does not describe all construction memory: the input buffer,
threshold prefix, mapping builder, output objects, and downstream Save each have
separate lifetimes. See the main handbook's pipeline bounds for the full path.

### 3.3 Why it helps after an insertion

The following is illustrative; the chunk names/sizes are not measured output:

```text
  VERSION A:   [ chunk A ][ chunk B ][ chunk C ][ chunk D ]
                           ^ insert bytes near here

  VERSION B:   [ chunk A ][ changed region ... ][ chunk C ][ chunk D ]
                    |                              |         |
               exact reuse                    exact reuse exact reuse
```

Content-dependent boundaries can align again after a change, leaving later chunks
identical. The changed neighborhood may span multiple chunks. There is no promise
of immediate resynchronization, a fixed number of changed chunks, or a guaranteed
reuse percentage. High-entropy data may still benefit from exact unchanged-chunk
reuse even when its newly written chunks do not compress well.

Source: [stream constructor](core/crates/layerfs-content/src/file/content.rs),
[policy](core/crates/layerfs-content/src/policy.rs),
[CDC scanner/profile](core/crates/layerfs-content/src/file/cdc/gear.rs).

## 4. CAS: exact reuse and collision checks

CAS operates on finalized canonical objects, including payloads and structural
objects. It does not search arbitrary duplicate substrings in unrelated objects.
Chunking and tree construction determine the object boundaries available to reuse.

```text
  finalized object { id, role, canonical bytes, references }
                             |
             bounded wave membership / locator lookup
                             |
            +----------------+----------------+
            |                                 |
        ID absent                          ID present
            |                                 |
      admit new object               check role and canonical length
            |                                 |
      encode and place               reconstruct/authenticate existing
            |                                 |
            |                       compare canonical bytes exactly
            |                                 |
            |                         +-------+-------+
            |                         |               |
            |                       match          mismatch
            |                         |               |
            |                       reuse          Collision/error
            |                         |
            +-------------> successful Save work
```

Within a wave, repeated IDs are compared for canonical bytes and role. For an
already located object, storage checks role/length and resolves its canonical
bytes for exact comparison. An ID match alone is not the complete reuse decision.
Pending objects in unfinished packs can be sealed when needed to make same-save
resolution possible. Presence and authentication errors remain errors, rather
than permission to overwrite or silently admit conflicting data.

Publication also arbitrates concurrent winners. Another writer may have published
the same object before this operation acknowledges it; publication resolves that
situation under the provider contract and refuses incompatible winners. Callers
should use Save rather than implement an independent SQL upsert protocol.

CAS can save write space/work while still paying lookup, authentication, and read
cost. A reused object need not be a zero-I/O result. Object identity also does not
imply garbage collection: this guide supplies no manual deletion protocol for
objects that a failed candidate appears to leave unused.

Source: [wave exact reuse](core/crates/layerfs-storage/src/save/wave.rs),
[publication arbitration](core/crates/layerfs-storage/src/save/publication.rs).

## 5. Delta encoding: current implementation, not a generic diff

### 5.1 What PREFIX means here

The current payload delta mechanism uses Zstandard compression with a prior raw
payload supplied as a **prefix**. A PREFIX record carries a base identity and a
frame dependent on that base. It is not a stored list of filesystem edits, an
XOR delta, or a byte-range patch script produced by `apply_edits`.

```text
  base canonical object              new canonical object
           |                                  |
     extract raw payload                extract raw payload
           |                                  |
           +-------- Zstd prefix compression -+
                              |
                  PREFIX record { base ID, frame, ... }
                              |
               reconstruct using authenticated base
                              |
                    new canonical object / same new ID
```

An independent alternative is prepared first. For payloads it may be a compressed
FULL frame or a STORED raw value. A bounded incompressibility probe can select
STORED, and an independent compressed frame that does not shrink the payload is
stored raw. This is a policy decision, not an error-triggered fallback.

### 5.2 Which objects can use it

Whole-file and chunk payload roles participate in payload PREFIX selection.
Extent/file-state/directory/inode-branch/filesystem-root/attribute/symlink tree
roles do not use this payload delta route. Inode leaves have separate pooled
metadata encoding and reuse, described below. Do not describe every structural object as a
candidate for file-payload delta encoding.

### 5.3 Candidate sources and one-trial selection

Whole-file selection considers declared advisory predecessors first. When no eligible advisory candidate
is selected, it can consult the bounded content-signature index of admitted
independent winners. This allows proposals based on content similarity, not only
on a caller's explicit predecessor identity.

Chunk selection considers the first supplied advisory candidate under its
eligibility checks (same role, located, base depth strictly below the target role's
cap); it does not search that whole-file similarity index. With no
advisory chunk correspondence, exact CAS reuse still works, but new chunks do
not automatically receive a similarity-index delta base.

```text
  prepare independent FULL/STORED alternative
                       |
             role-specific depth cap zero?
                 yes --+--> independent result
                       |
                      no
                       |
           propose candidate by supported role
                       |
             missing / ineligible? ----> independent result
                       |
                    eligible
                       |
             acquire/authenticate base chain
                       |
           prospective chain exceeds budget? --> independent result
                       |
                    admitted
                       |
                  ONE PREFIX trial
                       |
          PREFIX record strictly smaller than independent record?
                 |                                  |
                yes                                 no
                 |                                  |
             store PREFIX                    store independent
```

The comparison is encoded **record length**, not a guarantee of lower CPU,
lower read latency, or fewer transactions. Ties retain the independent record.
The selector makes one PREFIX trial against the acquired base; it does not keep
trying candidates until one wins. Corruption, provider failure, or failed
acquisition after a candidate is selected propagates as an error. That is distinct
from the declared absent/ineligible/no-benefit decisions in the diagram.

The whole-file signature index is a proposal mechanism, not authentication. It
uses eight folded hashes derived from payload signatures, with fixed 8,192 slots
and 65,536 reference entries, declared index budget 704 KiB. Rows persist through
the storage publication machinery. A poor similarity proposal cannot change
canonical correctness: the base is validated and selection compares encodings.

### 5.4 Separate pooled inode metadata deltas

Inode leaves use value pooling plus a separate leaf-body delta grammar. Repeated
inode values can share authenticated pooled value references/ordinals. The
physical leaf body is reconstructed into canonical form before object identity
is verified; physical ordinal numbering is not a replacement for authentication.

```text
  canonical inode values --> authenticated pooled values / ordinal references
                                             |
                                  physical inode leaf body
                                             |
                                +------------+-------------+
                                |                          |
                           independent FULL          COPY/INSERT program
                                |                    against prior leaf body
                                +------------+-------------+
                                             |
                                reconstruct physical leaf
                                             |
                                 resolve pooled values
                                             |
                                 rebuild canonical leaf
                                             |
                                   verify ObjectId
```

The metadata delta builder uses bounded matching/program work. It selects the
program only when program bytes are strictly smaller than FULL bytes; a tie keeps
FULL. Metadata match-budget exhaustion is a declared no-program selection, while
actual provider/format failures propagate. Metadata depth defaults to eight,
with its own chain budgets; payload chain limits must not be substituted for them.
Value pooling, full-object CAS, and leaf delta encoding are three distinct reuse
mechanisms. This guide's FULL/PREFIX selection diagram describes payloads only.

Sources: [pooled Save](core/crates/layerfs-storage/src/save/pooled.rs),
[metadata delta grammar](core/crates/layerfs-storage/src/encoding/pool/delta.rs),
[pooled reader](core/crates/layerfs-storage/src/encoding/pool/read.rs).

Source: [selector](core/crates/layerfs-storage/src/encoding/delta/select.rs),
[candidate signatures/index](core/crates/layerfs-storage/src/encoding/delta/candidates.rs),
[independent/PREFIX encoding](core/crates/layerfs-storage/src/encoding/full.rs),
[Zstd codec](core/crates/layerfs-storage/src/encoding/codec.rs).

## 6. Dependency chains: storage savings have read costs

A delta depends on an existing object, which can itself depend on a base:

```text
  read object C
       |
       v
  C: PREFIX(base B) ----> B: PREFIX(base A) ----> A: independent
       ^                         ^                       |
       |                         |                       |
       |                         +--- verify A <---------+
       |                         |
       |                   reconstruct + verify B
       |                         |
       +------ reconstruct C <---+
                               |
                    authenticate canonical C
                               |
                    return requested logical bytes
```

The resolver walks iteratively, rejects cycles and invalid chains, and reconstructs
base-first with canonical authentication. A depth is the number of dependency
edges; an independent object has depth zero.

| Policy/budget | Current value |
| --- | --- |
| Default whole-file maximum depth | 8 edges |
| Default chunk maximum depth | 4 edges |
| Accepted configurable depth range | 0–50; zero disables prospective deltas for the role |
| Chain canonical work budget | 512 KiB |
| Chain encoded work budget | 256 KiB |

Increasing a supported depth or cutoff does not increase these separate work
budgets. Payload selection conservatively tests:

```text
  base-chain decoded canonical bytes + target canonical length <= 512 KiB
  base-chain encoded record bytes    + target canonical length <= 256 KiB
```

The second test adds target canonical length, not a predicted delta frame size.
This bounds prospective dependency work before storing a dependent. Reader also
enforces its own declared chain limits. These are dependency-work constraints,
not a blanket 512 KiB maximum on every standalone FULL object; the larger canonical
object limit is a separate bound. Caches can avoid some
repeated acquisition while retained, but eviction can cause rereads.

A 4 KiB logical request can require a larger chunk, a complete encoded unit, and
several delta bases. Whole-file representation requires the whole canonical value
before slicing. Therefore stored-byte savings and logical-read amplification
must be considered together. Neither CAS nor delta promises reads proportional
only to requested logical bytes.

Whole-pack acquisition and selected-unit acquisition have different physical
validation scope. Returned canonical objects are authenticated in both; selected
units do not imply an audit of all unread pack bytes.

Source: [chain resolver](core/crates/layerfs-storage/src/encoding/delta/read.rs),
[storage budgets](core/crates/layerfs-storage/src/policy.rs),
[construction depth policy](core/crates/layerfs-content/src/policy.rs).

## 7. Localized edits versus fresh CDC construction

`apply_edits` works from a canonical base root and a stable checked edit sequence.
For chunked files, it can preserve untouched extents, scan replacement bytes into
new structure, and rebuild the required boundaries. It does not need to rerun CDC
across every unchanged byte solely to reconstruct a whole file.

```text
  BASE:       [ existing left ][ bytes removed ][ existing right ]
                     |                               |
                     |       replacement stream      |
                     |               |               |
                     |           CDC/new objects     |
                     v               v               v
  RESULT:     [ reused extents ][ new extents ][ reused extents ]
                     \               |               /
                      +---- rebuilt tree boundaries -+
                                     |
                                 new file root
                                     |
                     CAS reuse / physical encoding / Save
```

Fresh construction runs its chunker over the whole input stream. Localized edits
preserve existing structure and chunk replacement pieces independently. Equal
logical bytes produced by these routes are not promised identical partitioning
or canonical roots. An unchanged/no-op result can return the original root.
Small-file assembly and representation transitions can require broader input work.

`EditSequence` is replayable and ordered, with current-result coordinates; an edit
cannot reach into replacement bytes introduced by an earlier edit in that stream.
The caller must normalize mutable write history into supported stable inputs.

Source: [edit application](core/crates/layerfs-content/src/file/edit/apply.rs),
[edit input contract](core/crates/layerfs-content/src/file/edit/input.rs).

## 8. Worked example: three different kinds of reuse

This example illustrates behavior; the object labels and sizes are hypothetical.

```text
  Previous file:  [ A ][ B ][ C ][ D ]
  New file:       [ A ][ B*][ C ][ E ]

  A: same canonical bytes / ID ----------> exact CAS reuse
  C: same canonical bytes / ID ----------> exact CAS reuse
  B*: different canonical bytes / ID
      admitted candidate B offered ------> independent vs PREFIX comparison
                                             |
                                   smaller PREFIX may win
  E: different bytes, no candidate ------> independent FULL/STORED encoding

  New mapping/tree objects --------------> exact CAS checks where applicable
                                             |
                                      new file root saved
                                             |
                                  namespace/history publication
```

The B* delta outcome requires admitted candidate correspondence; it is not
inferred just from adjacency in this picture. The result can reuse A/C, store B*
as a delta, and store E independently in the same operation. The new file still
needs its completed root and namespace/history publication as appropriate.
No numerical deduplication ratio or delta speedup follows from this illustration.

## 9. Caller recipe and operating rules

Use the public construction/storage interfaces, not a second chunker or private
SQL encoding implementation:

```rust
let construction = storage.policy().construction();
let capacities = construction.capacities();
let save = storage.begin_save()?;
let built = {
    let mut sink = save.sink();
    construct_stream(construction, &capacities, source, &mut sink, scope)
};
// On failure, preserve the content error and recover save.take_failure()
// when OutputRejected came from storage. Do not automatically retry.
let built = built?;
let file_root = built.root;
let outcome = save.finish()?;
// file_root is saved; logical namespace/Commit publication is separate.
```

This is a call-sequence illustration, with checked `storage`, stable `source`,
`scope`, imports, and error conversion supplied by the application.
`construct_stream` has no predecessor argument. For predecessor-aware byte
construction use `construct_bytes_with_predecessor` and its checked
`PredecessorBase`; for localized changes use `apply_edits`. Do not add a predecessor
argument to a nonexistent stream API or assume fresh streams create chunk hints.

- Keep canonical construction policy compatible with the persisted Store.
- Let the consumer apply backpressure; do not collect all finalized objects first.
- Supply correct predecessor provenance through the public checked contracts.
- Treat candidate absence/ineligibility as selection outcomes; distinguish real
  provider/authentication errors.
- Match typed errors. Preserve uncertain outcomes and exact operation context;
  do not resend, delete bases, or invent recovery on a guess.
- Do not mutate old immutable packs or drop a delta base independently of its
  dependents. No garbage-collection protocol is supplied by these APIs.
- Wait for successful Save finish before treating required storage as complete;
  earlier reference-closed publications can remain after a later failure.
- Keep construction, storage completion, history publication, and checkpoint
  separate, as described in the cluster one handbook.

## 10. Measurement and maintenance

Use actual counters to distinguish mechanisms: construction CDC counters,
`WriteOutcome` reuse/record outcomes, `DeltaCounters` trials/selected/no-candidate/
ineligible/work-exceeded outcomes, and `ChainCounters` dependency work. Counter
scope matters: a handle's cumulative diagnostics are not automatically a phase
measurement. Do not sum overlapping wall observations or claim a whole-process
memory bound from one buffer budget.

The [cluster one benchmark appendix](cluster_one_handbook.md#9-attached-benchmark-results-and-qualification-boundaries)
contains retained Init/history results for both profiles. Those results measure
the integrated paths and do not isolate CDC, CAS, or delta contributions.
There is no new mechanism-specific speed/storage qualification in this guide.

Update this document when the canonical profile, selector, candidate rules,
encoding grammar, or bounds change. Preserve source pins and historical receipts.
A format change is not merely an opportunity to edit constants: it needs the
applicable compatibility/implementation process and public behavior validation.
Follow [Core rules](core/AGENTS.md) and
[documentation policy](docs/general/documentation-policy.md).
