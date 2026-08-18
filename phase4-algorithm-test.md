# Phase 4 algorithm test and benchmark specification

- Status: candidate test contract; WP4-M implementation and measurements pending
- Date: 2026-08-17
- Branch: `codex/empty-worktree`
- Applies to: WP4-M through WP14

## 1. Purpose and authority

This document defines the tests, benchmark campaigns, baseline sequence, timer
boundaries, evidence labels, and decision gates for the Phase 4 CAS + CDC + COW
integration. It is the executable companion to `phase_4_algorithm_spec.md`; it
does not define another storage format or algorithm.

Requirements apply in this order:

1. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_SPEC.md` controls the Phase 4
   direction and acceptance targets.
2. `PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md` controls candidate and promoted
   canonical bytes, identities, roles, bounds, and profile-selection rules.
3. `PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md` is the sole authority for
   the narrow SQLite schema-v1 to schema-v2 transition required by the complete
   visible-head tuple. It does not authorize a general migration framework.
4. `phase_4_algorithm_spec.md` controls the algorithms and ownership
   boundaries under test.
5. This document controls test cases, campaign order, timer boundaries, and
   evidence classification.
6. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_IMPLEMENTATION_PLAN.md`
   controls work-package order and ledger completion.

The source-artifact fingerprints when this test specification was created are:

| Artifact | SHA-256 |
|---|---|
| rollback specification | `d8f59b476f40511564c3dedfc6f2646d149e4c7c141bfbd5538cc148a35eebd4` |
| rollback implementation plan | `f5097601cb8dd8ec24b3fa608d019de6b38c470cd1c41ae7bf4078b87a0e91dc` |
| WP4-C logical mapping | `3e94b054e6bf0eb198f6b04287d8a6cb209fb2925450b6c6bc6a69c84ab63e06` |
| SQLite visible-head migration authority | `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` |
| complexity analysis | `33879535b0a2ddaf8a4f77a61c47844be9b1ae39d3b5486b51890882f58f2ee2` |
| Phase 4 algorithm specification | `223f6fd4e88a56eda227a7c1163376516de49408f0d32ce90d1471cfcacaac98` |

If an authority artifact changes materially, its semantic delta must be
reviewed before updating these fingerprints or relying on results produced
under the old contract.

## 2. Evidence classes

The following evidence classes are distinct:

| Evidence | Purpose | Selects format | Qualifies 200/300 MiB/s |
|---|---|---:|---:|
| existing custody baseline | Prove inherited Phase 1-3 and SQLite behavior remains healthy | no | no |
| direct WP4 correctness tests | Prove candidate bytes, identities, errors, bounds, and semantics | no | no |
| WP4-M profile-selection campaign | Select one file K/F and one directory page ceiling | yes | no |
| post-WP4-P unoptimized baseline | Establish the fair final-profile Memory/SQLite starting point | already selected | SQLite row may be reported, but is not the final optimized decision |
| optimization A/B | Prove a named optimization changes its expected cost without weakening semantics | no | only the complete qualifying row counts |
| WP14 final campaign | Make the final Phase 4 decision on unchanged source | no | yes, SQLite only |
| later native materialization campaign | Measure projection to destination files/directories | no | separate milestone |

No result may be promoted from one evidence class by relabeling it.

## 3. Historical evidence classification

### 3.1 Retained Phase 2 evidence

The retained Phase 2 counters remain the algorithmic CDC/edit baseline:

| Workload | Source | CDC bytes scanned | Reused chunks | Created chunks |
|---|---:|---:|---:|---:|
| new/full scan | 100 MiB | 104,857,600 | 0 | 5,284 |
| new/full scan | 512 MiB | 536,870,912 | 0 | 27,162 |
| one-byte middle edit | 100 MiB | 1,060,505 | 5,283 | 1 |
| one-byte middle edit | 512 MiB | 1,070,912 | 27,161 | 1 |
| append | 100 MiB | 14,196 | 5,283 | 1 |
| truncate | 100 MiB | 4,785 | 5,280 | 1 |

These rows prove bounded CDC rejoin behavior. They do not include durable
mapping, SQLite publication, reopen, closure, or materialization.

Rerun them before WP4-M only if their source changed, their deterministic
fixtures cannot be reproduced, their artifacts are unavailable, or a new
same-machine RSS observation is required.

### 3.2 Historical SQLite rows

The older approximately 217.8-MiB/s SQLite experiment used a different schema
and timer boundary. It establishes plausibility only.

The later approximately 35-MiB/s SQLite proxy row used a directory-of-chunks
graph and explicitly lacked final Phase 3 semantic persistence. It is a
diagnostic historical row only.

Neither row is the baseline for the promoted mapping. The deleted append-only
carrier is not rerun and is not a candidate.

## 4. Test and baseline sequence

Run evidence in this order:

```text
pre-WP4-M custody check
    -> focused WP4-M correctness tests while implementing
    -> non-qualifying profile-selection A/B
    -> WP4-P selects one profile and deletes alternatives
    -> final promoted-format goldens and full correctness suite
    -> unoptimized Memory/SQLite baseline
    -> measured optimization A/Bs
    -> unchanged-source WP14 final campaign
```

The current incomplete implementation is not used as a fake full-workload
performance baseline. The first comparable algorithm benchmark waits for the
minimum WP4-M candidate codec and SQLite measurement path.

## 5. Pre-WP4-M custody baseline

Before production edits, record:

- Git commit and branch;
- dirty-worktree state;
- relevant source SHA-256 fingerprints;
- Rust and Cargo versions;
- target triple;
- macOS version;
- CPU and installed memory;
- SQLite/rusqlite versions;
- discovered test target and test counts; and
- current package, formatting, and diff results.

Run the existing owner tests for:

- Phase 1 canonical objects and `ObjectId`;
- Phase 2 CDC boundaries and fragmentation independence;
- Phase 2 content reconstruction and edit/rejoin;
- Phase 3 COW, roots, and deltas; and
- the current SQLite engine.

The executor must list the real tests first and report a nonzero executed
count. A filter that discovers or runs zero tests is not evidence. If the
recorded WP3 checkpoint has the same production-source fingerprints and a
complete green owner/workspace result, it may satisfy this custody baseline;
do not rerun a broad Cargo wall solely to duplicate unchanged evidence.

## 6. WP4-M direct correctness tests

Correctness is a prerequisite to timing. A failed correctness row produces no
performance result.

### 6.1 File mapping golden vectors

For K64/F64, K59/F101, and K256/F256, test:

- empty file;
- one reference;
- `K - 1`, `K`, and `K + 1` references;
- exactly `F` leaves and `F + 1` leaves;
- every branch-height transition;
- final partial leaf and branch;
- repeated chunk IDs at different ordinals;
- zero-length references where admitted;
- maximum legal scalar fields; and
- synthetic checked-arithmetic boundaries without allocating their logical
  size.

Every success asserts:

- exact canonical bytes;
- exact `ObjectId`;
- exact role and version;
- exact reference order;
- exact counts and cumulative lengths;
- decode then encode byte identity; and
- deterministic repetition across independent constructions.

Each temporary profile has a private, domain-separated candidate profile ID
and its own expected mapping, node, root, and delta IDs. Only raw source bytes,
CDC boundaries, raw `ChunkId` values, and reconstructed logical bytes are
required to match across different profiles. Memory and SQLite must match all
IDs within the same profile.

Pre-promotion vectors are temporary measurement fixtures. After WP4-P, delete
them and independently generate authoritative goldens for the one winner.

### 6.2 File mapping malformed vectors

Test exact typed rejection for:

- truncated outer or inner header;
- truncated record;
- trailing bytes;
- unknown version, tag, or role;
- incorrect declared reference or child count;
- incorrect cumulative end;
- incorrect subtree count, length, or height;
- unsorted or noncanonical descriptor order;
- unnecessary top level;
- illegal nonfinal partial group;
- object, field, or direct-reference limit violation; and
- checked length, count, offset, or derived-layout overflow.

Also include the exact authenticated mismatch cases from mapping section 14.8:
a complete canonical `abc` Bytes object referenced under the raw `xyz`
`ChunkId` returns `ChunkIdentityMismatch`, and a correct chunk with an unequal
declared raw length returns `ChunkLengthMismatch`.

Malformed input must not panic, allocate past the admitted bound, or publish a
visible head.

### 6.3 CDC and edit-rejoin regressions

Rerun the frozen Phase 2 behaviors through the durable mapping path:

- contiguous versus fragmented source delivery;
- exact 8/16/32-KiB profile and chunk-sequence fingerprint;
- new/full file scan;
- one-byte middle replacement;
- equal-length replacement;
- 4-KiB replacement;
- 1-MiB replacement where applicable;
- prepend;
- append;
- truncate;
- EOF no-op;
- exact rejoin success; and
- honest larger-scan/full-replacement fallback when rejoin is not proved.

The mapping must consume the same ordered chunk-reference sequence. It may not
change CDC boundaries to improve storage measurements.

### 6.4 Exact range tests

For every temporary file profile, test:

- empty range;
- first byte and last byte;
- range wholly inside one chunk;
- exact chunk-boundary range;
- cross-chunk range;
- cross-leaf range;
- cross-branch range;
- full-file range;
- range ending exactly at EOF;
- `start > end`; and
- `end > logical_length`.

The routing suite must include the mapping section 5.4 boundaries: one-byte
references around `4095..4097`, and the 4,161-reference vector containing 64
zero-length references in an otherwise empty leaf. That empty leaf and its
zero-length chunks must not be fetched for `4095..4097`. Include leading,
interior, and trailing zero-length references so equal cumulative ends cannot
select an empty interval.

Successful rows assert:

- exact output bytes;
- exact root/branch/leaf/chunk path;
- exact complete objects and canonical bytes authenticated;
- zero payload reads for unselected siblings;
- full authentication of every fetched object; and
- bounded traversal and output memory.

### 6.5 File COW tests

Test these separately:

| Edit | Required mapping behavior |
|---|---|
| same-count middle edit | affected leaf plus file and namespace ancestor spines |
| append | rightmost partial leaf/new leaves plus rightmost spine |
| truncate | boundary leaf plus rightmost spine; old objects remain immutable |
| forced `+1` early edit | measure and validate suffix repartitioning |
| forced `+1` middle edit | measure and validate suffix repartitioning |

Every case asserts:

- final logical bytes and fingerprint;
- final file, root, and delta identities;
- created and reused chunks;
- created and reused mapping objects;
- mapping bytes rewritten and authenticated;
- unreachable immutable residue;
- unchanged object identities; and
- successful authenticated reopen and ranges.

The `+1` cases must not claim path-local complexity. They are the fixed-ordinal
format rejection gate.

### 6.6 Directory mapping tests

For 64-KiB, 256-KiB, and 1-MiB complete canonical page ceilings, test:

- empty directory;
- one entry;
- one page at its exact packing boundary;
- one entry past a page boundary;
- a deterministic 100,000-entry wide directory;
- first, middle, and final point lookup;
- same-size middle child replacement;
- leading insertion and removal;
- duplicate name;
- noncanonical name order;
- malformed page boundary/index;
- adjacent-page boundary authentication and greedy partition recomputation;
- duplicate name split across otherwise ordered pages;
- descriptor first/last-name mismatch;
- a non-greedy split where the next complete entry still fits;
- wrong child role; and
- oversized individual entry.

Record exact pages, index bytes, mapping objects, rewritten bytes, lookup
authentication bytes, and resident memory.

The duplicate cross-page name returns `NameCollision`; a descriptor routing or
non-greedy packing violation returns `NonCanonicalPagePartition`.

### 6.7 Delta tests

Test:

- genesis;
- empty change;
- add, remove, replace, and metadata operations;
- multiple ordered entries;
- repeated paths where Phase 3 admits them;
- a delta crossing a page boundary;
- exact durable parent and child roots;
- encode/decode/replay equality;
- incorrect parent;
- incorrect before or after durable ID;
- reordered entry/page;
- missing embedded node; and
- truncated or trailing bytes.

Encoding may not sort, deduplicate, combine, or reorder Phase 3 delta entries.

Also test the parentage-convergence case: genesis empty root, change to a
nonempty root, then change back to the byte-identical empty content. The final
`RootId` must equal the genesis content root while its transition is distinct.
Genesis is not encoded as a Phase 3 `Delta` and is never passed to
`Delta::apply`. Provisional Phase 3 handles and durable mapping IDs must
round-trip through the exact translation boundary without putting parentage
inside content identity.

### 6.8 Closure, DAG, and cycle tests

Test:

- complete valid closure;
- missing object;
- tampered canonical bytes;
- correct object under the wrong role;
- active-ancestry cycle;
- repeated completed shared-DAG occurrence;
- maximum admitted logical depth;
- one level beyond admitted logical depth;
- bounded DFS frame state;
- bounded resident edge-spool window; and
- file-backed spool cursor resumption without parent-per-child refetch.

A cycle test uses the active ancestry only. It must reject the repeated active
`(ObjectId, role)` before Q charging, lookup, or fetch, including when the
repeated target would otherwise be absent, corrupt, or Q-exhausting. Test the
parser nesting limit of 8 independently from graph depth, logical namespace
depth 256 versus 257, and the maximum candidate physical path of 781 edges /
782 frames with the maximum file spine. A completed shared-DAG occurrence is
valid and is reauthenticated without an unbounded global visited set.

Closure qualification also tests both modes:

- genesis, missing/invalid receipt, and explicit fresh scrub traverse the full
  closure;
- a valid prior snapshot receipt permits incremental validation only after the
  changed ancestor spine is authenticated and compared;
- equal authenticated child IDs cover unchanged sibling subtrees;
- every new or different child is fully traversed; and
- counters distinguish authenticated changed-spine work, covered subtrees,
  fetched occurrences, and complete-object bytes.

### 6.9 Receipt and reopen tests

Test fast reopen and fresh full scrub as separate operations:

- valid same-open receipt;
- receipt from another store;
- wrong integrity epoch;
- wrong generation;
- wrong visible root or delta;
- malformed receipt;
- valid receipt followed by missing accessed object;
- valid receipt followed by tampered accessed object;
- unavailable cross-reopen authority;
- authenticated fast root/path access; and
- independent complete full-closure scrub.

The exact snapshot receipt is the fixed 216-byte
`ValidatedSnapshotReceiptV1`; change each field independently: store instance,
validation authority/key, integrity epoch, generation, child root, transition,
mapping profile, and authenticator. Also test another store/key/authority,
stale and newer heads, authorized mutation with epoch advance, out-of-band
mutation, head-only rollback, combined database/head/key rollback, exact
216-byte acceptance, and 217-byte rejection. By default SQLite permits only
same-open generation reuse; adversarial cross-reopen reuse requires the exact
custody authority frozen by the mapping or returns
`ValidationAuthorityUnavailable`.

A valid snapshot receipt may authorize skipping unvisited siblings. It never
authenticates newly fetched bytes, authorizes incumbent equality, contains a
locator transcript, or turns fast reopen into a fresh scrub. A bad receipt and
a later explicit full scrub are separate operations with separate provenance.

If WP10 later adds an operation-local verified-work receipt, direct tests must
change independently its store instance, validation authority, integrity
epoch, mapping profile, generation, authenticated root/transition, object ID,
locator/row identity, and byte range. They must also test a stale/replaced
locator, exact count/byte high-water, deterministic eviction, and fallback to
complete incumbent authentication on every miss, mismatch, or eviction. A
snapshot receipt must never be accepted as this locator receipt.

### 6.10 SQLite immutable reuse and publication tests

Test actual semantic boundaries for:

- new immutable object;
- authenticated equal incumbent;
- malformed incumbent;
- unequal incumbent;
- missing/replaced incumbent during validation;
- transaction begin failure;
- object insert or read failure;
- closure failure after immutable inserts;
- root, delta, receipt, or head write failure;
- parent-generation conflict;
- commit failure;
- ambiguous durability result; and
- successful exactly-once publication.

For a failure after publication dispatch, test all four reconciliation
outcomes: exact requested complete head means success; exact prior head returns
the original first cause; a different head returns `PublicationConflict`; and
an unresolved authoritative state returns `AmbiguousDurability`. Assert the
exact fixed `FailureProvenance { first, cleanup_first, reconciliation,
dominant }` slots and permit retry only with the byte-identical idempotency key.

Freeze exact idempotency-key vectors for genesis and an existing prior head.
Change the store ID, prior tag, each prior/request head field, and each receipt
independently and require a different key; reject prior tags other than 0 or 1.
The key is recomputed from the retained request during reconciliation and is
not accepted as a fifth persisted head field.

Run the complete table-driven failure set from mapping section 14.8 at the real
boundary: allocator refusal after Q charge; spool no-space, permission, short
write, close, and removal failures; backend short read; transport I/O;
cancellation/deadline before and after publication dispatch; and W/D overflow
before append, receive, or delivery. Every case asserts visible-head state or
typed ambiguity, immutable residue/spool custody, resource release, and exact
first/cleanup/reconciliation/dominant causes.

Every case asserts as applicable:

- exact first cause;
- exact dominant cause and bounded provenance;
- one transaction and zero or one commit;
- zero or one visible-head transitions;
- prior visible head after definite prepublication failure;
- exact unreachable immutable residue or typed unavailable custody; and
- released writer, transaction, lock, and bounded-resource ownership.

Fault injection must occur at the real operation boundary being claimed. A
hook after a successful write, flush, or sync does not prove that operation's
failure behavior.

The SQLite schema migration suite is exactly the one in
`PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md`: fresh schema-v2 creation;
exact section-3.1 version/profile/authority/head/receipt classification;
empty-v1 upgrade; nonempty-v1 exact
`SchemaMigrationRequired` with byte-identical unchanged storage; interrupted
upgrade classification; complete visible-head round trip; 216/215/217 receipt
lengths; one-field lost-ack mismatches; and proof that no second migration path
or alternate schema profile is reachable. Every failed-open case asserts exact
`first`, `cleanup_first`, `reconciliation`, and `dominant`, zero mutation and
publication, and the specified repair/reopen or separately requested full-
scrub behavior; malformed structure must not silently trigger a scrub.

### 6.11 Memory/SQLite semantic parity tests

For identical fixtures and operations, Memory and SQLite must produce equal:

- CDC sequence;
- raw chunk IDs;
- canonical chunk and mapping object IDs;
- file and directory roots;
- ordered delta;
- logical generation;
- created/reused semantic outcomes;
- reconstructed bytes and fingerprint; and
- exact range bytes.

Memory reports durability and process reopen `NotApplicable`; it does not
manufacture zero-cost durable success.

### 6.12 Resource and 100-GiB analytical tests

Run real streamed resource tests at 100 MiB and 512 MiB. Assert:

- no source-sized input or output buffer;
- declared maximum canonical-object window;
- declared CDC, leaf, branch, page, spool, and output windows;
- bounded live allocation `Q`;
- cumulative work/output `W/D` may exceed resident `Q`;
- exact allocation preflight failure for an eager result that would exceed
  its admitted live budget; and
- successful streamed 512-MiB reconstruction with reusable windows.

The exact resource contract is:

- `MAX_DURABLE_LIVE_ALLOCATION = 1,073,741,824` bytes;
- the ordinary canonical/spool/output/receipt/DFS windows total exactly
  33,604,696 bytes before explicitly charged semantic results and backend
  overhead;
- every Q term is charged once, growth is transactional, refusal occurs before
  allocation, release restores the live baseline, and high-water never falls;
- `Q == 1 GiB` followed by a one-byte charge fails before allocation;
- streamed 512-MiB reconstruction grows D while Q remains window-bounded; and
- synthetic W or D growth from `u64::MAX` by one fails before append, receive,
  or delivery.

Logical Q is not RSS, allocator overhead, SQLite cache, or the OS page cache;
those are observed separately and may be `Unavailable`.

Do not generate a 100-GiB local fixture. Use synthetic metadata boundary tests
and checked equations to prove:

- checked `u64` total length and reference count;
- required leaf/branch/root counts at minimum, retained, and maximum chunk
  densities;
- branch height and root-to-chunk path;
- mapping and canonical-framing bytes;
- no arbitrary 2-GiB, 3-GiB, 100,000-reference, cumulative-work, or cumulative-
  output ceiling; and
- typed arithmetic failure at the real representational boundary.

## 7. WP4-M profile-selection benchmark

Every row in this campaign emits:

```text
qualification=false
purpose=profile_selection
```

It cannot support a product, compatibility, or 200/300-MiB/s claim.

### 7.1 Prepared fixtures

Prepare and fingerprint outside all engine timers:

- the identical retained 100-MiB source;
- the identical retained 512-MiB source;
- exact same-count replacement bytes and offsets;
- exact forced `+1` early and middle edits;
- exact prefix, middle, EOF, cross-chunk, cross-leaf, and cross-branch probes;
  and
- one deterministic 100,000-entry wide directory.

The manifest records generator version/seed, raw source fingerprint, logical
fingerprint, CDC-sequence fingerprint, creation/reuse outcomes, range bytes,
expected strong-edge occurrences, expected authenticated-object occurrences,
expected authenticated canonical bytes, and the expected ordered-closure
digest. The benchmark-only closure digest is:

```text
BLAKE3("layerfs/benchmark/ordered-closure/v1\0" ||
       for each occurrence in canonical traversal order:
         role:u8 || ObjectId:[32]byte || canonical_length:u64be)
```

An occurrence enters the digest only after its complete canonical object has
been read, hashed to the expected `ObjectId`, decoded under the expected role,
and its strong edges have been validated. It is a bounded rolling observation,
not content identity, a receipt, or permission to skip authentication.

The manifest also records each candidate's private profile ID and that
profile's expected canonical mapping IDs, roots, and deltas; those IDs are not
assumed equal across K/F or directory-ceiling candidates. The wide-directory
fixture records the exact ordered names, metadata, child IDs, repeated-child
pattern, candidate page partitions, expected closure digest, and lookup/edit
targets, so a candidate cannot change the corpus while changing its ceiling.

### 7.2 File candidate matrix

Compare K64/F64, K59/F101, and K256/F256 on both source sizes. Use four rows per
candidate/size:

| Row | Timed and verified work |
|---|---|
| full cycle | capture, one durable commit, close/reopen, fresh full scrub, full streamed reconstruction, and all exact ranges |
| same-count middle edit | bounded rejoin, publication, reopen, final verification |
| forced `+1` early edit | count change and suffix repartitioning |
| forced `+1` middle edit | count change and suffix repartitioning |

This is:

```text
3 profiles * 2 sizes * 4 rows * (1 warmup + 5 measured) = 144 invocations
```

Range operations inside the full-cycle row retain their own phase latency and
object/byte counters without requiring another complete 512-MiB campaign.
The full-cycle row uses the exact `capture_publish_wall` and
`sqlite_qualification_wall` boundaries in section 9.2; its primary comparison
is `sqlite_qualification_wall`.

Every edit row records two nested timers:

```text
edit_publish_wall:
  actual edit/source bytes read
  + CDC rejoin or declared full-scan fallback
  + raw and canonical identity
  + immutable creation/authenticated reuse
  + mapping COW and required closure qualification
  + root/delta/complete visible-head construction
  + exactly one SQLite durable commit

edit_verification_wall:
  edit_publish_wall
  + drop all handles and operation-local state
  + fresh engine construction from the durable path
  + authenticated head/delta and required closure verification
  + streamed reconstruction/fingerprint and exact ranges
```

Base-store preparation and cloning are outside both timers. The forced `+1`
5% alarm compares `edit_publish_wall` with the same size's new-file
full-capture `capture_publish_wall`; the complete row remains separately
reported and cannot be substituted for that comparison.

### 7.3 Directory candidate matrix

Compare 64-KiB, 256-KiB, and 1-MiB page ceilings on:

- create, commit, reopen/full validation, and point lookups;
- same-size middle child replacement; and
- leading insertion.

This is:

```text
3 ceilings * 3 rows * (1 warmup + 5 measured) = 54 invocations
```

### 7.4 Campaign execution

Build one release benchmark binary once on stable source. Run one warmup and
five measured iterations with:

- source generation and fingerprint preflight outside the timer;
- an isolated SQLite database in the exact scenario starting state per row;
- no compiler or unrelated benchmark contention;
- alternating candidate order to reduce cache/thermal bias;
- identical timer-boundary version;
- one JSON object per run; and
- no human-formatted output in the timed path.

The full-cycle and directory-create rows start from a fresh empty candidate
database. Edit rows start from a separately prepared, committed base image with
an exact fingerprint; preparing and cloning that base is outside the timer, and
each measured row receives a fresh isolated clone. Directory replacement and
insertion rows likewise start from their exact committed wide-directory base.
The clone must be an ordinary byte copy or the base must be regenerated; a
clonefile/reflink cannot support allocated-byte comparison. The database,
protected receipt/key/epoch authority, and any inseparable sidecars are copied
as one snapshot. Record the copy method and starting allocation. If shared
extents cannot be excluded, physical allocation is `Unavailable` for profile
selection.

The per-row precondition is machine-readable:

| Row | Required starting state outside timer |
|---|---|
| full cycle / directory create | fresh empty candidate store |
| same-count / forced `+1` file edit | byte-identical committed base source, mapping, head, receipt authority, and database image for that candidate |
| directory replacement / insertion | byte-identical committed wide-directory base and candidate database image |

The timer starts at the first actual edit/source read, not at base construction,
database cloning, fixture verification, or cache conditioning.

Use one untimed warmup per row, followed by five matched blocks in balanced
candidate order rather than running all measurements of one candidate first.
Preserve raw JSONL and generate summaries afterward. Each candidate database
uses its domain-separated candidate profile ID; isolated paths alone are not
an authentication binding.

Run one measured invocation per process so external high-water RSS belongs to
that row. If the platform cannot observe RSS, record `Unavailable`; because RSS
is protected, an unavailable challenger comparison is inconclusive. Record the
exact conditioning steps and use `warm_or_unknown` unless a controlled
machine-level procedure directly establishes another cache state.

### 7.5 File-profile promotion gate

K64/F64 is the deterministic default. A challenger replaces it only if it
improves the primary 100-MiB complete full-cycle median by at least 5%, wins at
least four of five matched blocks, and regresses no protected 100/512-MiB
complete capture/full validation, range, same-count edit, forced-`+1`, CPU,
allocated-store-delta, Q, or RSS median by more than 5%. Missing observations,
reversed cross-size evidence, or a win explained only by removable per-row SQL
crossings is inconclusive rather than a format promotion.

When statement/BLOB-open counters show that removable per-object SQL crossings
could reverse the ranking, run one private bounded prepared/batch sensitivity
probe with identical semantics and candidate inputs. It is measurement code,
not a production feature or a new abstraction. A reversed or statistically
unclear ranking defers promotion; the probe never overrides correctness or a
protected regression.

Reject fixed ordinal grouping if the forced `+1` edit at 100 or 512 MiB:

- exceeds 5% of that size's unchanged full-capture median; or
- departs from the declared suffix-rewrite byte/row model.

The 5% comparison is an alarm, not the scalability proof. Report the measured
100-to-512-MiB slope, the exact fixed-ordinal suffix-rewrite equation, and its
analytical 100-GiB early/middle-edit projection. WP4-P must not promote a fixed
profile until an explicit 100-GiB middle-insert analytical work budget is
approved over rewritten reference occurrences, leaves/branches/objects,
canonical mapping bytes, and optional rewrite-to-capture amplification. That
budget is an edit-policy gate, not a file-size admission limit. Any projected
100-GiB latency is nonbinding and must state its model and uncertainty.

Only after the local measurement gate or approved analytical work budget fails
may WP4 authorize the narrow deterministic history-independent/prolly fallback
experiment.

### 7.6 Directory-profile promotion gate

The 256-KiB directory ceiling is the deterministic default. The primary is the
complete same-size middle-child `edit_verification_wall`. A challenger must
improve its overall median by at least 5%, be faster in at least four of five
paired matched blocks, and regress none of these protected outcome/resource
medians by more than 5%:

- create and full-validation wall/CPU;
- point-lookup latency;
- same-size replacement `edit_publish_wall` and `edit_verification_wall`;
- leading-insert publication and verification latency;
- allocated-store delta;
- logical `Q` and external RSS.

Mapping/page objects, logical canonical/auth/rewrite bytes, SQL
executions/rows/BLOB opens, and page counts remain mandatory diagnostics but
are not uniform 5%-nonregression guards. Object-count arithmetic alone does not
select the winner. Missing or reversed evidence leaves the 256-KiB default in
place. If removable SQL crossings could reverse the ranking, run the same
private bounded sensitivity probe and defer rule as section 7.5.

The 100-MiB 500.000-ms minimum and 333.333-ms stretch thresholds are reported
as WP4-M credibility diagnostics, not format-promotion blockers. They become
binding product gates only in the unchanged-source WP14 campaign after shared
core and SQLite optimization.

For every 100/512-MiB pair, compare observed leaf/branch/object counts,
canonical bytes, SQL executions/BLOB opens, `Q/W/D`, and rewritten bytes with
the candidate equations. Report absolute and percentage residuals. Use the
validated count/byte equations for the analytical 100-GiB projection; do not
extrapolate a 100-GiB wall time from two local timings.

## 8. WP4-P promotion tests

After selection:

1. delete every losing profile, constant, selector, and candidate-only fixture;
2. ensure no public format/configuration selector remains;
3. independently regenerate exact success and malformed vectors;
4. fingerprint the final promoted vectors and specification;
5. rerun the complete promoted codec, range, COW, closure, receipt, delta,
   resource, and parity tests;
6. run an independent read-only correctness/performance audit; and
7. expose only the promoted profile to WP5+ production integration.

No compatibility-bearing production golden or final performance baseline is
created before these steps succeed.

## 9. Post-promotion unoptimized baseline

After WP5-WP7 complete the shared mapping, Memory lane, and SQLite lane, record
the unoptimized baseline before WP10-WP12 performance changes.

### 9.1 Ordinary fixture matrix

| Size | Required scenarios |
|---:|---|
| 1 MiB | new, unchanged, one-byte edit, 4-KiB edit, full replacement |
| 10 MiB | new, unchanged, one-byte edit, 4-KiB edit, 1-MiB edit, full replacement |
| 100 MiB | new, unchanged, one-byte edit, 4-KiB edit, 1-MiB edit, full replacement, prepend, append, truncate, EOF no-op, scattered edit |

Run Memory and SQLite. The retained 512-MiB fixture remains a profile/scaling
check and does not multiply the complete ordinary scenario matrix.

After the promoted profile is fixed, run one deterministic repeat-heavy
100-MiB diagnostic with the same timer boundary. It must report occurrence
count versus unique-object count, authenticated incumbent comparisons, and
created/reused outcomes. It diagnoses dedup/reuse amplification; it neither
selects the format nor replaces the retained source row.

Every ordinary row uses one of these explicit starting states:

| Scenario | Required starting state outside timer |
|---|---|
| new | fresh empty store |
| unchanged / EOF no-op | exact committed base store and byte-identical source |
| one-byte / 4-KiB / 1-MiB / scattered / prepend / append / truncate | exact committed base store plus the declared edit input |
| full replacement | exact committed base store plus the replacement source |

Memory and SQLite receive semantically identical base state. Base creation,
base verification, store cloning, and cache conditioning are outside the row
timer and are recorded in its manifest. Same-open and reopened-base diagnostics
are distinct labels; neither may silently inherit a receipt or cache owned by
the other.

### 9.2 Timer boundaries

Every create/edit row records two nested wall timers. The common prefix is:

```text
capture_publish_wall:
  actual source/edit read
  + CDC
  + raw and canonical identity
  + immutable creation/reuse
  + file/directory/root/delta construction
  + required closure qualification
  + SQLite: exactly one durable commit and complete visible-head publication
  + Memory: one atomic in-process visible-head swap; durability not applicable

sqlite_qualification_wall:
  capture_publish_wall
  + drop every engine handle, SQLite connection, and operation-local state
  + construct a fresh engine instance from the durable database path
  + authenticated visible root/delta and fresh closure verification
  + full streamed reconstruction and fingerprint
  + exact range verification

memory_qualification_wall:
  capture_publish_wall
  + discard operation-local receipts and caches
  + construct an independent reader/view over the authoritative in-process store
  + authenticated root/delta and fresh closure verification
  + full streamed reconstruction and fingerprint
  + exact range verification
```

Fixture generation, source fingerprint preflight, empty-store preparation, and
summary formatting are outside both timers.

`capture_publish_wall` is the user-facing create/edit latency. Memory labels
durability and process reopen `NotApplicable`. The controlling 100-MiB
200/300-MiB/s target remains attached to the complete SQLite
`sqlite_qualification_wall` so required work cannot be omitted.

For source-sized rows, the reported qualification throughput is exactly:

```text
qualification_mib_per_s =
  (logical_source_bytes / 1_048_576) / lane_qualification_wall_seconds
```

The logical source is counted once. Reconstructed, authenticated, or rewritten
bytes are work-amplification counters, not additional throughput numerator.

A claim of new-OS-process reopen additionally requires a separate child-process
integration row; reconstructing a fresh engine instance in the benchmark
process does not make that claim.

### 9.3 Correct metric by operation

- New and full-replacement rows report MiB/s and latency.
- Small edits report latency and exact work amplification; one-byte-edit
  MiB/s is not meaningful.
- Ranges report latency, returned bytes, complete authenticated bytes, and
  path/object visits.
- Fast reopen and fresh scrub report separate latency and authentication work.
- Memory is the shared-core ceiling; only SQLite can satisfy the durable
  target.

### 9.4 Mandatory observations

Every performance row reports a value or explicit `Unavailable` or
`NotApplicable` for:

- wall and CPU time;
- source, CDC, raw hash, canonical encode/hash, CAS, COW, closure, SQL, commit,
  reopen, scrub, reconstruction, and range phases;
- bytes read, encoded, copied, hashed, authenticated, compared, written, and
  emitted;
- chunk/reference/object/node/edge submissions, creations, reuses, and visits;
- file height, mapping nodes, range path, rebuilt pages, and ancestor nodes;
- SQL preparations/executions, rows, BLOB opens, transactions, commits, syncs,
  query plans, busy events, and locked events;
- logical `Q/W/D`, cache/spool high-water, and external RSS;
- SQLite pre/post logical, apparent, and allocated database, journal, and
  temporary bytes; post-minus-pre allocated-store delta; and peak
  journal/temporary bytes when directly observed;
- process/host read and write deltas when directly observed;
- host physical I/O when directly observable; and
- exact source/store cache conditioning.

Unavailable physical or cache observations are never replaced with logical
bytes or zero.

### 9.5 Repetition and statistics

Use one warmup and five measured runs per required row. Every promotion-bearing
WP4-M, baseline, optimization-A/B, and WP14 engine/scenario/iteration runs in
its own process; warmups use separate invocations, and the external
orchestrator alternates row order. Campaign-wide RSS from a multi-row process
is diagnostic only. Report:

- median;
- minimum;
- maximum;
- max-minus-min spread; and
- every raw row.

Do not report p95 from five measurements. Alternate Memory/SQLite or
baseline/candidate order where temporal state could bias the comparison.

### 9.6 APFS labels

A fresh pathname or database does not prove cold APFS state. Required source
preflight may warm source pages. The primary reproducible label is:

```text
source_cache_state=warm_or_unknown_after_manifest_preflight
store_state=fresh_logical_store_cache_unknown
```

Run a separately named warm-reuse campaign. Report true cold APFS only when a
controlled machine-level procedure directly establishes it; otherwise use
`Unavailable`. `/usr/bin/time -l` may provide external process RSS and host
observations, but unavailable APFS physical media bytes remain unavailable.

## 10. Optimization A/B procedure

The unoptimized final-profile binary is the authoritative performance
baseline. Preserve its:

- Git/source fingerprints;
- executable SHA-256;
- fixture manifest;
- timer-boundary version;
- raw JSONL; and
- environment record.

For each WP10-WP12 optimization:

1. identify the measured dominant counter it should change;
2. run the exact affected correctness test and owner suite;
3. build the candidate release binary from stable source;
4. alternate preserved baseline and candidate executable runs;
5. require equal semantic identities and results;
6. require movement in the expected counter, not wall time alone;
7. inspect CPU, RSS, SQL, physical/logical bytes, and full-row wall time; and
8. remove the optimization if the full workload does not materially benefit
   or a protected scenario regresses.

Do not keep permanent production feature flags or duplicate implementations
solely for A/B. Preserve executables as evaluation artifacts and run them
sequentially against fresh stores.

## 11. Benchmark self-gates

A row is rejected before summary inclusion unless it proves:

- exact fixture and timer-boundary version;
- exact CDC sequence;
- exact canonical, file, root, and delta identities;
- expected object creation/reuse outcomes;
- complete required closure membership and order, including exact expected
  strong-edge and authenticated-object occurrences, authenticated canonical
  bytes, and ordered-closure digest from the fixture manifest;
- for SQLite mutation/full-cycle rows, exactly one write transaction, one
  commit, and one visible publication;
- for SQLite range, fast-reopen, fresh-scrub, and reconstruction-only rows,
  zero write transactions, commits, or publications;
- for Memory rows, zero SQLite actions with durability and process reopen
  reported `NotApplicable`;
- exact reopened generation, root, and delta;
- full reconstructed bytes and fingerprint;
- every exact range probe;
- declared live-memory bounds;
- zero hidden retries, workers, queues, or extra durability boundaries;
- exact cache-state label; and
- no unexpected residue or typed failure.

A failure produces an exact failure record, never a partial-success throughput
row.

## 12. Materialization exclusion and later test

Phase 4 reconstruction streams authenticated raw bytes to a bounded
fingerprinting/counting sink. It is not native materialization.

The later materialization campaign separately measures:

- one large file versus many small files;
- 1, 10, and 100 files;
- full destination creation;
- unchanged rematerialization;
- one small changed file;
- directory and file creation;
- payload writes and metadata application;
- destination publication and optional/required durability boundaries;
- reopened destination verification; and
- honestly labeled warm or cold/unknown APFS state.

Destination filesystem work must not be folded into Phase 4 storage-engine
throughput.

## 13. Final WP14 decision

On one stable promoted source fingerprint, run the complete final Memory and
SQLite campaigns and make exactly one controlling decision:

1. retain SQLite after the qualifying 100-MiB durable row reaches at least
   200 MiB/s;
2. retain SQLite and continue shared-core work because Memory/SQLite evidence
   shows an engine-agnostic limit; or
3. authorize a separate specification for one named third backend because
   optimized SQLite still misses the target and measured SQLite-specific work
   is dominant.

Report the 300-MiB/s stretch result separately. Memory, a microbenchmark, a
profile-selection row, a fast reopen, or a reconstruction-only row cannot
satisfy the durable target.

## 14. Required artifacts

Each campaign preserves:

- checked fixture manifest;
- environment JSON;
- source and executable fingerprints;
- exact commands;
- discovered and executed test counts;
- one raw JSON object per run;
- machine-generated summary;
- correctness/failure ledger;
- logical memory and external RSS observations;
- SQLite and filesystem-size observations;
- cache-state labels; and
- a decision record that cites raw rows rather than copied numbers.

The implementation plan ledger records each work package only after its exact
test and evidence exit condition succeeds.
