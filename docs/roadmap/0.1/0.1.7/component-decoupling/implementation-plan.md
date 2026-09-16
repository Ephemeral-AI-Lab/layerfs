# C1/C2 implementation plan and architecture review

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

## 1. Proposed folders and implementation rules

```text
<repository>/
  AGENTS.md
  core/                              complete replacement product
    AGENTS.md
    Cargo.toml / Cargo.lock
    crates/
      layerfs-telemetry/              existing, implemented
      layerfs-content/               selected C1 home for Stages 0–2
        src/
          lib.rs                     declarations / delegation only
          object/                    identity, framing, checked access/output
          file/                      complete input, ranges, known edits
            cdc/                     existing bounded chunking algorithm
            mapping/                 extent codec/build/read/edit boundaries
          filesystem/
            directory/
            inode/
            attributes/
            references/              compact effects and ordering
        tests/                       external public-API tests and helpers
        examples/                    only actual runnable examples
      layerfs-storage/               selected C2 home for Stages 0–2
        src/
          lib.rs                     declarations / delegation only
          cas/                       content-addressed save/read, owner, completion
          encoding/                  payload + physical metadata, reconstruction
          pack/                      layout, placement, assembly, locator access
          sqlite/                    connection, bounded queries/writes, cleanup
        sql/                         shipped schema/query text when needed
        tests/
      <runtime-package(s)>/          later workspace/FUSE/transport; no scaffolding
    tools/                           boundary/size checks and product tooling
    benchmark/                       when the actual candidate harness is adopted
    containers/                      when a concrete runtime is integrated
  adapters/                          later application integrations
    layerfs-claude-code/
    layerfs-codex/
    layerfs-dsh/
  crates/                            pinned reference during migration
  docs/ / release-notes/              shared design and immutable evidence
```

This is a responsibility map, not an instruction to create empty folders. The
[Stages 0–2 handoff](stages-0-2-handoff.md) now selects layerfs-content and
layerfs-storage as the independent C1/C2 package homes. Only layerfs-telemetry is
implemented today. Do not create seven crates or a crate per algorithm. A cohesive small
module can remain a named file; use a folder when responsibilities/size require it.
Tests need actual Cargo test targets in the package, not an assumed workspace-root
test directory.

CAS (content-addressable storage) was previously labelled store/ in this map.
Use cas/ for that existing C2 responsibility; this is a naming clarification,
not another layer. C1 object/ defines canonical bytes, ObjectId and authentication.
C2 cas/ looks up, reuses, saves and reads objects by that ObjectId, coordinating
encoding, packing and SQLite. C1 never imports the C2 implementation.

Root means the ObjectId used to open a constructed file or filesystem structure.
Return that ID and concrete required values, such as logical file length. Tree
builders retain counts/heights internally where parent construction needs them;
do not add a generic logical-summary object or expose every internal field. These
results contain no checkpoint ID, timestamp, history record or Workspace lifecycle.

The [core rules](../../../../../core/AGENTS.md) now require:

- Every production implementation file: fewer than 1,000 physical lines, maximum
  999, including blank lines/comments; shipped SQL is included.
- lib.rs/mod.rs: maximum 200 physical lines, declarations/reexports and direct
  delegation only; no algorithms, state, validation or formatting implementations.
- Split by responsibility, such as placement versus reconstruction; no part1/part2,
  renamed god modules, minification or include/macro tricks to evade the limits.
- Product src/ contains product code only. Tests, mocks, fixtures, benchmark drivers
  and executable examples remain external; no test-only public APIs or cfg branches.
- Each commit records actual production LOC before/after/delta under the existing
  counting contract. Physical file length is a separate structural measure.

Use SRP/SOLID as practical boundaries: each module owns one coherent operation or
invariant; callers depend on the small capabilities they use; C1 does not depend
on concrete storage; a replacement provider must preserve ordering, errors and
resource contracts. Pure algorithms use functions and concrete data. Add interfaces
at real input/read/output/backend variation points, not around every function.

The guard checks current Rust and shipped SQL inputs. Extend it when other product
source formats are introduced. It does not prove semantic responsibility, macro
behavior or dependency direction. Existing root reference files are not split just
to satisfy the candidate rule; extract and simplify them as each slice lands.

## 2. Review verdict and strict scope

Three independent reviews covered simplicity/coupling, I/O/performance/memory and
FUSE/process/cloud placement across the existing design set. Reference source and
evidence levels remain in the [v0.1.6 audit](content-io-memory-audit.md#1-source-provenance-and-evidence-levels).
The reviewers traced representative product paths and inspected primary Radish/
Cloudflare material. No product performance run was performed.

| Question | Verdict |
| --- | --- |
| Is the design simpler? | Yes: explicit ownership and fewer passes, copies, queries and wrappers. Exact net production LOC reduction is not measured yet |
| Have all unnecessary round trips/versions disappeared? | The replacement C1/C2 implementation has not landed. Concrete cuts are selected; active formats and necessary repeated reads remain |
| Is it pluggable? | The contracts permit independent C1/C2 use. Concrete adapters still need qualification |
| Is it faster? | There are source-visible work reductions; complete successful latency, storage and memory superiority remains unproven |
| Is all scratch/disk gone? | Generic canonical-payload staging is the removal target. Named ordering work and several conditional removals remain |

Strict implementation policy:

1. One attempt: return success or failure. Unknown persistence outcome is a failed
   result with incomplete acknowledgement, never another success mode. No retry,
   automatic resume, stale-state refresh, decoder trial sequence, backend switch
   or legacy execution after failure.
2. FULL versus DELTA, cache hit/miss and planned new-pack placement remain normal
   successful decisions. A failed codec/read/SQL call does not select another path.
3. No WAL or added crash-durability system now. The embedded profile is MEMORY
   journal, synchronous OFF, memory temporary SQL storage and zero busy timeout.
   Keep runtime transaction atomicity/abort; do not choose journal mode OFF.
   No fsync/fdatasync/File::sync_all/File::sync_data calls, persistent attempt log, recovery/checkpoint service or
   durability claim is added. No operation-wide transaction substitutes for this.
4. No third-party patch, fork, vendoring or registry edit. Use existing pinned
   dependencies and locked builds. An unsupported dependency/provider fails its
   qualification rather than being patched or silently replaced.
5. Defaults remain configurable 128 KiB / whole-file depth 8 / chunk depth 4 under
   supported capacity profiles. No silent budget growth or extra workers.

Active v1 ordinary/singleton, v2 native, v4 compact-small and v6 pooled metadata
grammars have different jobs. Required older readers are explicit compatibility,
not error fallbacks. Freeze which existing Stores are supported/rejected before
retiring readers; no automatic upgrade, conversion or new version per numeric knob.

## 3. Significant reductions, component by component

The diagrams describe specific reference paths. Existing complete-file and sorted
tree streaming, shared prefix codec, batched membership and locators are reused;
they are not newly invented savings.

### Canonical objects and output

```text
REFERENCE GENERIC CANDIDATE ROUTE
construct -> candidate object store / payload staging
          -> index + reachability selection -> payload reread -> C2

TARGET QUALIFIED FINAL-OUTPUT ROUTE
stable input -> unfinished boundaries -> final canonical allocation
                                      -> move to bounded C2 consumer
```

Replace the broad mutable object-store context with explicit policy, supplied
identities, bounded authenticated reads and finalized output. Carry ordinary ID,
role and reference fields; derive length from bytes. Remove unnecessary wrapper
conversions and repeated hashing/parsing across trusted internal ownership moves.
External serialized input still receives required checks.

### Small and large file construction/edits

```text
SMALL, known final size
before: probe + head/tail allocations + inner envelope + outer copy
after:  final canonical allocation -> fill / hash -> emit

LARGE, known edits
before: changed bytes already stream as chunks
        split/concat -> encoded structural overlays -> clone/decode/prune -> emit
after:  same streamed chunks and split/concat rules
        existing subtree IDs + decoded unfinished boundaries -> seal/encode -> emit
```

The old large-file mutation path does not spool every complete file. Its structural
overlays are already bounded. Prove exact partition/root equivalence and decoded
allocation limits before deleting those overlays. Preserve CDC restart points,
unchanged extent slices, subtree reuse, no-op equality and small/large transitions.
Unknown-length complete input still needs a bounded threshold probe.

A planned comparison pass followed by reopening the same stable replacement range
for construction is not retry. A failed read terminates the operation. Fragmented
retained slices may still require whole groups/chunks/delta chains to reconstruct;
small output size is not a bound on physical read work.

### Filesystem trees and attributes

```text
before: typed inode -> standalone encoding -> temporary object/journal ID rewrite
                   -> reread/decode -> inline inode-table leaf
after:  checked sorted typed changes -> sorted tree merge -> final inline leaves

before: each inode key -> separate ancestor descent
after:  bounded key batch -> shared demanded ancestor traversal
```

Retain directory/name and inode-reference semantics. Exact hardlink counts and
additions-before-removals need compact ordering work for general input order.
Remove checkpoint-only fields, synthetic inode hashes and encoding-for-size probes
where their replacements preserve all invariants. Do not replace validation with
assumed caller correctness or a whole-filesystem plan.

### Physical encoding and packing

```text
METADATA GROUP DIGEST
before: build/compress -> clone -> decompress -> hash
after:  build exact group body -> hash -> compress

PACK APPEND
before: assemble incoming -> clone old+new groups -> assemble merged -> write
after:  calculate exact placement -> assemble selected write once -> write
```

Preserve the shared codec and role/profile rules, FULL-size group planning, useful
bounded candidates and value reuse. The BTreeSet replacement for the temporary
fingerprint DB still needs same-window/selection, memory and speed proofs. Catalogue
reopen retains its O(groups) header audit. Compatible append can still rewrite a
current-save pack; seal-once insertion stays outside the initial implementation.

Locators remain stable ordinals:

```text
ObjectId -> objects lookup -> (pack_id, group_number, record_number)
                              |
                 pack directory -> group framing -> record
```

A growing pack directory may move byte offsets without changing existing ordinals.
No separate locator table or per-object offset update is introduced.

### Save coordination and SQL

```text
before: repeated private sets/reference parsing + late membership refresh
        + allocation SQL per batch + catalogue SQL per group + empty final writes
        + nested cleanup attempts

after:  one exclusive owner -> bounded membership/dependencies
        -> checked scalar cursors -> encode/place/write
        -> shared bounded transaction -> one terminal completion/cleanup boundary
```

Preserve combined membership/location pages, cheap presence-only checks, exact byte
comparison and dynamic SQL sizing. Four tables/19 columns and location/base indexes
add real index/descriptor write cost. Indexed reverse-location cleanup removes
retained-row scans without introducing an operation-sized graph. The diagnostic
seen-index cut requires an explicitly revised receipt contract; SDK/Monitor and
benchmark units cannot silently change.

## 4. Logical-to-physical and physical-to-logical I/O

There is no new transport component between local C1 and C2. They use ordinary
calls, ownership transfer and the existing qualified bounded producer overlap.

### Write path

```text
C1 owns canonical allocation
             |
             | move bytes + ID / role / direct references
             v
C2 bounded batch owns the same allocation
             |
     exact reuse / dependency checks
             |
   bounded base + codec + selected records
             |
    place / assemble / bounded SQL write
             |
release each allocation after its last required consumer
```

Acceptance can block when the consumer is full. Dequeueing a buffer does not make
its allocation disappear; ownership moves to the consumer and remains charged.
Do not construct all output first, encode it all next and then write it all. Keep
cross-file batching, ordinary single-producer work and parallel namespace init.

Moving Vec ownership avoids an internal boundary copy. It does not make codec,
framing, SQL binds or network serialization zero-copy. Release losing encodings
and unneeded canonical operands promptly, while keeping required comparison bytes.

### Read path

```text
C1: bounded object-ID / logical-range demands
                    |
C2: batched locators -> group required physical reads
                    |
       decode / reconstruct bounded dependencies
                    |
        authenticate canonical objects
                    |
one owned allocation per live distinct object in the bounded wave
                    |
borrow slices / move owners in requested logical order
```

Preserve repeated-demand order without cloning a payload for every repeated ID.
Do not add a generic readback spool or a whole-operation object cache. A later
bounded wave can legitimately reacquire released data, with that I/O measured.

### Visibility during an unfinished save

SQL COMMIT of an early bounded transaction is not successful-save completion.
Same-save reads may access its pending/transaction/private output under the owner.
Ordinary unrelated reads during that save are restricted to pre-save retained
packs. Capture the allowed pack ceiling once per read and check every acquired
location, including bases, pool groups and supported whole-owner/slice dependencies.
Do not change that read's ceiling midway when a save finishes. Reuse existing
locator/catalogue results where they supply pack IDs; this avoids a second query
without adding a reader-pin table. Cache hits must preserve the same boundary;
owner-bound overlays/caches cannot leak private output into unrelated reads.

A read crossing its permitted boundary fails. Complete save success permits new
reads to see the new retained packs; failure preserves the restriction through
cleanup, and unknown outcome blocks affected exposure/writes pending inspection.
This is an explicit API/visibility change to qualify, since reference readers also
serve overlays. It is not a crash-recovery scheme or a promise that every cache
already retains the required origin/location metadata.

### A real process boundary

```text
sender                bounded transport                 receiver
stable input   -> checked byte frames / operation ->  C1+C2 owner
               <- backpressure / result / timings <-
```

Use a protocol only at this boundary. Bound frame length before allocation, total
in-flight bytes, response size and concurrency. Keep payloads binary, signal EOF
and acknowledgement explicitly, and wake blocked producers on receiver failure.
Receiver authentication remains necessary. No retry/resume/replay. Known-edit
sources must supply stable replayable ranges without hiding an unlimited spool in
the adapter. Count sender, receiver, socket and SDK buffers in integration claims.

## 5. Memory and disk ownership

| Memory owner | Why it exists / limit to qualify |
| --- | --- |
| Input window / small final allocation | Streaming CDC, threshold probe or whole-file output; accepted profile limits |
| Unfinished file/tree boundaries | Canonical partition/finality; prove height/fanout/capacity overlap |
| Reference ordering merge buffers | Exact inode-reference accounting; bounded buffers/readers |
| Partial/blocked slab and queue | Construction/storage overlap; count queue and consumer-owned allocations |
| Pending save objects and references | Membership, collision and dependency checks; bounded bytes and IDs |
| Read waves and delta bases | Required reconstruction, authentication and ordered borrowing |
| Codec workspace and alternatives | Actual FULL/DELTA comparison; preserve useful non-overlapping lifetimes |
| Open groups/tails and assembled pack | Selected compatible append; all active format lanes count |
| Candidate/value caches | Existing bounded reuse, not an unbounded workaround |
| SQLite cache/journal/binds/results | Real database execution; cache setting is not total RSS |
| Transport and timing output | Only actual bounded adapters/scopes; caller-selected report output |

The reference's 6-MiB/2-MiB internal ledgers overlap several owners; do not add them
as extra independent buffers or claim they bound total process memory. Use allocation
capacity and actual simultaneous lifetimes. OS page-cache residency also counts
when the measurement claim includes it. The existing requested 32-MiB SQLite cache
is not an absolute SQLite/process cap.

| Disk use | Target disposition |
| --- | --- |
| Main SQLite DB | Keep: packs, object index, pool catalogue, policy and indexes |
| Caller-selected immutable source/backing | Outside C1/C2 ownership, but included in complete-operation accounting |
| Compact inode-effects records / ordered runs | Keep where the selected algorithm needs them; explicit layout, quota, cleanup and measured I/O |
| Generic canonical payload spool | Remove on qualified final-output paths |
| RAW singleton temporary pack | Remove only after supported in-memory allocation/relocation proof |
| Physical fingerprint scratch SQLite | Replace only after ordered-set equivalence/resource proof |
| Distinct-reuse statistics backing | Conditional removal with explicit receipt compatibility; not silently erased |
| Timer JSON/report | Caller-selected optional file output after the operation |
| WAL / durable operation log | Not part of the current supported implementation |

Ordinary working buffers and SQLite's MEMORY rollback journal are RAM, not a new
generic scratch service. The proposal is not zero temporary disk for every tree
operation. Each retained ordering file needs an explicit bound; moving an unbounded
spool into the caller is not a valid removal.

## 6. FUSE, daemon/host and future cloud

### Local first, then move the operation owner

```text
HOST ONLY
caller -> C1 -> C2 -> embedded SQLite

HOST + DAEMON
host <--- bounded operation/input/result ---> daemon
                  selected side owns C1 -> C2 -> SQLite

FUTURE CLOUD COMPUTE NEAR STORAGE
host/daemon -> bounded operation stream -> cloud C1 -> C2 -> supported DB

REMOTE DATABASE ONLY
caller -> C1 -> C2 -> grouped DB adapter -> remote SQL
                     requires proven cross-transaction ownership/visibility
```

Keep C1/C2 colocated initially. A module boundary is not an RPC boundary. Remote
SQL needs grouped acquisitions/atomic writes, actual provider limits and enforced
ownership across all bounded transactions; per-transaction serialization is
insufficient. No provider registry, distributed lease service or speculative cloud
package is added now.

Portability includes real resource needs: known edits require stable range replay,
and general filesystem updates may require compact ordering records. Supply these
through explicit bounded capabilities or qualified already-ordered input; do not
hard-code a host path or hide their costs outside the operation. An environment
missing a required capability fails setup for that operation. Pluggable does not
mean every operation works unchanged on every provider without qualification.

### FUSE later

```text
FUSE callbacks
      |
later workspace/runtime owner
  mutable state / handles / ordering / permissions
      |
      +--> stable complete input
      +--> ordered known edits + immutable base
      +--> checked directory/inode/attribute changes
      +--> logical read ranges
                       |
                     C1 / C2
```

Do not make each FUSE write a SQL transaction. Live-write accumulation and backing
belong to the later workspace design, which may differ from the reference. C1/C2
import no mount handles, checkpoint records, daemon IDs or OS-specific inode structs.
Kernel/cache/workspace allocations remain part of eventual integrated measurements.

### Radish and Cloudflare

Radish puts a Redis-compatible engine inside a Durable Object with SQLite and an
R2 cold tier. It demonstrates a thin routing boundary and one execution/storage
owner; it is not a generic remote SQLite endpoint. Borrow that placement idea,
not its protocol or storage policy. [Radish architecture](https://radish.dhr.wtf/)

Cloudflare documents WAL-backed SQLite and durability/output gating in its Durable
Objects architecture. Therefore this backend is outside the current strict no-WAL
policy; it is a future study, not a supported deployment or a mode we silently
enable. [Cloudflare storage architecture](https://blog.cloudflare.com/sqlite-in-durable-objects/)

If reconsidered later, its row/BLOB and parameter limits, transaction callback API,
integer representation, operation ownership and retry behavior all need a concrete
adapter. Current published limits include 100 bound parameters and 2-MB row/BLOB
limits, which are not interchangeable with the local profile's accepted singletons.
[Provider limits](https://developers.cloudflare.com/durable-objects/platform/limits/),
[SQLite API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/).
Do not copy Radish's patched dependency setup; third-party patches remain forbidden.
[Radish manifest](https://github.com/Dhravya/radish/blob/main/package.json)

## 7. Implementation sequence and exit criteria

Each slice uses the same production bodies for independent and integrated calls.
Reference source stays isolated until replacement qualification; it is never a
runtime fallback. No placeholder packages or generic transport layer are needed.
Independent C1-only, C2-only and integrated timing are mandatory from their first
real slice under the [measurement acceptance contract](content-io.md#independent-c1c2-timing-is-an-acceptance-requirement).
Do not defer this to Workspace integration or Monitor work. Each slice must expose
the same production operation with enabled/disabled injected timing and explicit
input/output/provider ownership; merely adding a timer around a coupled pipeline
does not satisfy the requirement.

| Step | Implement | Exit evidence |
| --- | --- | --- |
| 0. Guardrails and frozen contracts | File/source rules, chosen package homes, exact default/compatibility profile and small read/output signatures | Boundary guard and external tests; explicit dependencies; no fsync/WAL/retry/fallback/patch paths |
| 1. Canonical + complete file construction | Reuse IDs/framing, empty/small/CDC construction and final-only output | Exact bytes/partitions/roots; bounded incremental consumer; C1 runs without DB/history/runtime |
| 2. First real storage slice | Exact minimal schema/profile and capacities; four tables, ownership, batched exact reuse/collision checks and new-object FULL save, locators, authenticated reads, finish and complete failure/cleanup behavior | Complete file -> real save -> close/reopen/readback; ranges, repeated IDs, slow consumer, late input failure and unknown outcome; no test-only path |
| 3. Complete physical policy | Existing whole/chunk delta, compression, metadata pooling, compatible append and caches; remove verified copies/queries | Same selection/storage quality and independent codec/save/read measurements; new/reuse and corruption/error cases |
| 4. Known edits and transitions | Single edits, then decoded multi-edit frontier after finality proof | Exact no-op/partition/root behavior; localized I/O; small/large/empty boundaries and supported overrides |
| 5. Filesystem tree/metadata | Sorted direct inode construction, attributes, compact reference ordering | Exact bindings/counts/topology; bounded ordering resources; large-directory and shared-key reads |
| 6. Full C1/C2 qualification | Broaden schema/profile, capacity, receipt, failure/cleanup and memory proofs across all completed paths | Matched successful v0.1.6 operations, no missing cases, no retry/fallback, all required product checks |
| 7. Runtime integration later | One concrete host/FUSE/workspace shape, then real process transport/provider if selected | Same core contracts, bounded full-system I/O/memory and correct acknowledgement; cloud remains unqualified until policy/provider fit |

### Current Stage 3–4 adjustment

The initial [implementation report](stages-3-4-report.md) records partial work;
the later [completion report](stages-3-4-completion-report.md) adds pooling and a
reference oracle. D still re-derives the mapping, and the multi-edit harness first
needs aligned coordinates. Use the [current prompts](stages-3-4-continuation-prompt.md)
for D's algorithm/proof and independent pooling coverage/qualification, then combined
acceptance. The [completion handoff](stages-3-4-completion-handoff.md) retains details.

```text
existing foundation -> D: corrected oracle + stored-tree edits ----+
                    -> pooling: coverage + component evidence ----+-> final acceptance
```

D and pooling coverage have separate ownership and proof gates. Independent pooling
measurements need not wait for D; combined edit qualification requires D's exactness.
Passing tests for the implemented subset cannot close missing criteria. Qualification
required by #168/#169 stays in this batch; Stage 6 broadens full-core coverage. The
global stage dependencies remain unchanged; Stage 5 absorbs none of this work.

### Earlier slice and boundary cases

Step 2 covers exact reuse and new-object FULL operations; it does not pretend that delta, arbitrary
edits or filesystem operations are already implemented. Unsupported unfinished
operations fail explicitly. Do not route missing behavior through the reference
or report a partial slice as the completed v0.1.7 product.

First-slice cases: empty input; 1 byte; T-1/T/T+1; repeated IDs; multiple files
sharing batches; incompressible supported singleton supplied as a large canonical
object directly to real C2 (a large CDC file of small chunks does not exercise this
route); bounded slow consumer; late
input failure; exact reopen and range readback. Then qualify eligible/ineligible
delta bases, corruption, long chains, cross-threshold edits, directory/reference
ordering and every accepted non-default profile. The larger-cutoff promise is not
complete until an override above 128 KiB works end to end under declared bounds.

## 8. How superiority will be decided

The [required measurement modes and root-cause guide](content-io.md#7-measurement-and-completion)
are part of implementation acceptance. C1-only uses a bounded non-persisting output
consumer; C2-only uses supplied canonical objects and real storage. Both remain
callable without Workspace, FUSE, daemon or logical Commit creation. Integrated
timing follows actual streaming/worker execution and never infers pure construction
time by subtraction. The shared timer exists; this C1/C2 wiring still must be implemented.

Architectural improvement is concrete: fewer capabilities cross boundaries, one
owner manages each lifetime, and redundant data conversions/queries disappear.
Performance superiority requires complete successful operations, not smaller code
or time subtracted between overlapping scopes.

Measure the existing coarse timer scopes around construction, lookup/dependency
work, encoding, packing, SQL and final drain. Retain actual queries/bytes/copies/
hashes/assemblies/transactions and memory/disk accounting in component/harness
evidence; no per-object trace growth or new telemetry subsystem.

Use matched input/profile, exact source/build identities, cache state, worker
policy, acknowledgement and storage-quality requirements. Construction-only tests
consume output incrementally; they do not accumulate all objects to make the test
convenient. Preserve all failure/NOT_RUN evidence and the repository's benchmark
contract. No warm-cache credit, workload shrink, raised limits, hidden extra worker
or quicker failure substitutes for a successful performance gate.

Remaining high-risk proofs are decoded frontier finality, supported singleton
allocation overlap, inode-effect ordering bounds, new index/column write cost,
ordered-set memory/reopen cost, distinct-reuse receipt changes and remote ownership.
These are implementation qualifications, not invitations to add more components.

No measured speedup, total-memory ceiling or zero-temporary-disk claim is made by
this review. The plan is deliberately to remove known redundant work while keeping
the existing efficient algorithms and proving each change on the real path.
