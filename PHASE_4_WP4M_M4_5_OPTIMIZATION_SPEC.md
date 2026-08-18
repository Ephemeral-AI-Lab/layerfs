# Phase 4 WP4-M M4.5 Authority-Correct Changed-Spine Optimization Specification

- Status: controlling subordinate specification for M4.5 implementation and
  evaluation; implementation pending
- Date: 2026-08-18
- Branch: `codex/empty-worktree`
- Repository scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only
- Starting retained implementation: accepted M3 dirty tree
- Starting HEAD: `c96b5396e98db523b9a983df4ec80fdedfa971c1`
- Retained M3 implementation-diff SHA-256:
  `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb`
- Rejected M4 candidate-diff SHA-256:
  `91f394fdcfccca4c3625e7962db56ac0304f2b2b32bc65875089755316d0a139`
- Rejected M4 executable SHA-256:
  `310d63e95a0d5dcbeedd537370c7d875cc0a2d57735e87b6254721de5a9043ad`

## 1. Authority and scope

This document repairs and supersedes the implementation direction, acceptance
rationale, and resource-adjudication procedure of the rejected M4 milestone.
It does not alter any frozen canonical bytes, identity domain, mapping grammar,
receipt bytes, SQLite schema authority, CDC parameters, Phase 3 semantics, or
profile-selection rule.

The controlling documents remain, in descending order where applicable:

1. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_SPEC.md`;
2. `PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md`;
3. `PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md`;
4. `phase_4_algorithm_spec.md`;
5. `PHASE_4_ALGORITHM_COMPLEXITY_ANALYSIS.md`;
6. `phase4-algorithm-test.md`; and
7. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_IMPLEMENTATION_PLAN.md`.

If this document conflicts with one of those records, the earlier controlling
record wins. In particular, M4.5 does not grant the current SQLite candidate
adversarial cross-reopen receipt authority. That remains disabled until WP7
proves protected-key custody, integrity-epoch mutation coverage, and the
required rollback/copy trust boundary.

M4.5 is deliberately narrow:

- optimize pre-COMMIT qualification for an exact same-count file edit;
- preserve the proven changed-leaf and ancestor-spine COW mutation;
- establish valid same-open authority for skipping equal immutable subtrees;
- repair prepublication result binding, lost-ack handling, typed errors, and
  memory accounting;
- rerun only focused correctness tests and the affected 100-MiB same-middle
  comparison; and
- leave `qualification=false`, `promotion=false`, `rejection=false`, and
  profile selection unresolved.

M4.5 is not authority to run the 198-row profile campaign, the 512-MiB rows,
or a promotion campaign. It is not a new engine, new database, new schema,
new receipt format, or new persistence profile.

## 2. Decision

The exact M4 implementation remains rejected. Its algorithmic result remains
valuable and is the basis of M4.5.

The corrected decision is:

> Reject M4's cross-process receipt-backed implementation and incomplete
> prepublication proof. Preserve its measurements as nonqualifying evidence.
> Reimplement the changed-spine optimization with a private same-open witness,
> complete pre-COMMIT result binding, exact live-memory accounting, and real
> durability reconciliation.

M4 was not rejected because its speedup was too small. It demonstrated a real
work-complexity reduction. It was also not proven to have an inherent 7.921%
RSS penalty. The semantic authority defect is independently sufficient to
reject the implementation.

## 3. Frozen M4 evidence

M4 compared the frozen retained-M3 executable with a same-count changed-spine
candidate on the retained 100-MiB same-middle edit. It produced:

| Metric | Retained M3 | Rejected M4 | Change |
|---|---:|---:|---:|
| pre-COMMIT closure wall | 430.182 ms | 0.150 ms | -99.965%, 5/5 wins |
| durable same-middle edit | 433.029 ms | 2.195 ms | -99.493% |
| complete same-middle lifecycle | 1,134.316 ms | 691.663 ms | -39.024% |
| total CPU | 1.13 s | 0.68 s | -39.823% |
| retired instructions | 9,232,918,143 | 5,743,287,643 | -37.796% |
| elapsed cycles | 2,952,802,680 | 1,812,384,870 | -38.622% |
| lifecycle objects authenticated | 16,171 | 10,807 | -5,364 |
| lifecycle canonical bytes authenticated | 316,084,425 | 210,825,999 | -105,258,426 |
| SQL statement acquisitions/executions | 10,986 | 5,622 | -5,364 |

The M4 pre-COMMIT proof reduced 5,380 objects and approximately 105.29 MB of
canonical authentication to 16 objects and 33,821 canonical bytes. It covered
127 equal strong edges, authenticated four changed edges, and completely
traversed one new 18,867-byte subtree. It created the same seven objects,
wrote the same 26,249 canonical bytes, produced the same root and transition,
and used one transaction and one COMMIT.

Logical, apparent, and allocated endpoint storage were byte-identical between
the measured arms. M4 added no schema, table, database, WAL, worker, pool,
cache, source-sized staging buffer, append-only path, or pack format.

These results establish that changed-spine qualification is a high-value
optimization direction. They do not establish admissible publication,
full-create throughput, profile promotion, 512-MiB scaling, or 100-GiB wall
time.

## 4. Corrected M4 finding ledger

### 4.1 P0 correctness blockers

#### F1. Cross-process receipt authority was invalid

The M4 row preparation process created the committed base and persisted its
216-byte snapshot receipt. A later measured process reopened the database and
used that receipt as authority for skipping unchanged sibling subtrees.

The frozen mapping permits adversarial skip authority only for the same-open
immutable generation until WP7 proves the full SQLite custody/epoch model. A
persisted receipt can authenticate its own bytes and bound tuple, but under
the current engine it cannot prove that an out-of-band deletion, replacement,
copy, or rollback did not occur after the earlier process closed.

Concrete counterexample:

1. prepare and close the base process;
2. delete or corrupt an object below an unchanged sibling;
3. open the M4 row process;
4. verify the still-byte-valid persisted receipt;
5. skip the equal sibling and COMMIT the new head; and
6. discover the missing/corrupt object only during post-COMMIT fresh scrub.

This can publish an incomplete closure and is fail-open publication.

M4.5 therefore prohibits a persisted receipt alone from creating skip
authority after reopen. It introduces no new persisted credential. It uses the
same frozen receipt bytes plus a private in-memory same-open witness issued
only after authoritative validation in the current open.

#### F2. Expected edit result was not fully bound before COMMIT

M4 performed exact edited-byte fingerprint and ordered CDC-sequence checks in
post-COMMIT reconstruction. A wrong-but-canonical same-count result could be
published before the benchmark detected the mismatch.

M4.5 must bind the requested edit to independently prepared expected source,
CDC-sequence, root, transition, and operation fields before COMMIT. The
constant-cost pre-COMMIT gate is equality with the independently frozen
expected root and transition IDs, whose manifest entry is itself bound to the
exact source and ordered CDC fingerprints. Post-COMMIT reconstruction must
still recompute and compare the source and CDC fingerprints directly.

This preserves path locality: M4.5 does not add an O(N) pre-COMMIT
reconstruction merely to compare a fingerprint already cryptographically
committed by an independently derived expected root.

#### F3. Root-summary equivalence was incomplete

The M4 paired verifier omitted file-root `mode`. M4.5 compares every frozen
root and node summary field, including role, version, mode, height, child
count, total reference count, total raw length, cumulative ends, and any
profile-bound scalar. Equality of a subset is not subtree equivalence.

#### F4. Actual ambiguous COMMIT outcomes were not fully reconciled

The benchmark's synthetic after-COMMIT-before-ack path did not prove that an
actual SQLite COMMIT error followed the frozen requested/prior/different/
unresolved reconciliation table.

M4.5 routes every ambiguous post-dispatch result through fresh independent
reconciliation of the complete visible head, including the byte-identical
receipt and recomputed idempotency key. No fallible counter, formatting, or
observation error may be returned as operation failure after a known
successful COMMIT.

#### F5. Missing-object errors lost the exact ID

The frozen mapping requires `MissingObject(ObjectId)` at the durable mapping
boundary. A unit missing-object error or raw `QueryReturnedNoRows` is
insufficient. M4.5 preserves the exact expected object ID through SQLite,
mapping, cleanup, and reconciliation error translation.

### 4.2 P1 proof and resource blockers

#### F6. M4's logical-Q claim was not exact

M4 recorded a maximum local semantic pair. It did not sum every allocation
that was simultaneously live across recursive descent. Parent decoded page
vectors, parent canonical payloads, child vectors, active ancestry, generated
SQL, source chunk, output, and other bounded buffers can overlap.

M4.5 must use checked live charge/decharge accounting and report the maximum
sum of all LayerFS-owned live capacities. Allocator metadata, rusqlite/SQLite
internals, SQLite page cache, filesystem cache, and OS memory remain separate
Observed/Unavailable metrics and are never silently included in or replaced
by logical Q.

#### F7. SQL “preparations” were statement acquisitions/executions

The benchmark counter named `sql_preparations` was incremented for statement
use and did not prove a native SQLite prepare. M4.5 must rename or precisely
label that metric. At minimum it reports separately:

- statement-cache acquisitions;
- execute/query calls;
- rows returned/changed;
- BLOB reads/writes; and
- native prepares as Observed only if directly instrumented, otherwise
  `Unavailable`.

#### F8. External RSS evidence was noisy

The official five-row arm medians reported maximum RSS increasing from
16,547,840 to 17,858,560 bytes (+7.921%) and peak footprint increasing
11.872%. Those values exceeded the predeclared protected 5% threshold.

The paired RSS deltas, however, were:

```text
-2,506,752; +1,589,248; -294,912; -65,536; +1,490,944 bytes
```

M4 used less RSS in three of five pairs. The paired median was -65,536 bytes,
the arithmetic-mean difference was only +42,598.4 bytes (+0.245%), the ranges
overlapped, and M4's maximum was lower than M3's maximum. Therefore the frozen
five-row protected median failed procedurally, but the evidence does not prove
an inherent M4 memory regression.

M4.5 keeps the 5% protected gate for any qualifying or promotion claim. For
implementation retention, a mixed five-pair RSS result is `INCONCLUSIVE`, not
causal proof. The predeclared confirmatory procedure in section 13.6 applies;
the original M4 rows are preserved and never overwritten or pooled silently.

### 4.3 P2 scope and interpretation findings

#### F9. The candidate Store is a benchmark shadow

The current WP4-M `Store` and candidate schema live in
`phase4_create_edit_benchmark.rs`; they are not the production `Engine` path.
That is permitted for private profile/candidate measurement, but every M4.5
report must say so explicitly. Candidate success is not production-engine
integration.

M4.5 reuses the shared codecs and walkers wherever they already exist. It does
not create a second public engine, trait, provider, database, or profile API.

#### F10. Complete lifecycle remains linear

The required fresh full scrub and exact reconstruction traverse the complete
reachable file. M4.5 can make edit mutation and pre-COMMIT qualification
path-local while complete lifecycle remains Theta(N). No report may call the
complete lifecycle logarithmic.

#### F11. M4 did not improve full-create throughput

M4 measured only same-middle edit rows, all with
`throughput_measurement_admissible=false`. It does not predict or justify the
100-MiB full-create 200/300-MiB/s target.

#### F12. Count-changing edits remain suffix work

Fixed-ordinal `+1` edits remain bounded-spool Theta(suffix references and
rewritten mapping bytes). M4.5 makes no logarithmic claim for `+1`, prepend,
insert, append, truncate, or any count-changing operation.

## 5. Non-negotiable invariants

M4.5 preserves:

- Phase 1 canonical `Object::Bytes` and `Object::Directory` bytes and IDs;
- Phase 2 FastCDC 8/16/32-KiB min/target/max boundaries and raw `ChunkId`;
- Phase 3 COW, root, delta, ordered operation, and replay semantics;
- the frozen K64/F64 candidate bytes used by the retained row;
- immutable authenticated CAS and no overwrite of an existing ID;
- authentication of complete fetched canonical bytes before semantic use;
- exact checked-u64 lengths, counts, cumulative ends, counters, and offsets;
- bounded caller-thread synchronous execution;
- one SQLite writer, one transaction, one durability-equivalent COMMIT, and
  one complete visible-head publication;
- exact first, cleanup-first, reconciliation, and dominant failure provenance;
- full fresh scrub and reconstruction as independent postpublication checks;
- no source-sized resident staging, unbounded visited map, unbounded cache, or
  unbounded SQL statement;
- honest Observed/Unavailable/NotApplicable resource reporting; and
- SQLite as the sole durable Phase 4 engine authority.

## 6. Explicit non-goals

M4.5 must not add:

- a new public engine, storage abstraction, provider, or factory;
- a new database, schema version, table, column, index, or sidecar;
- a new receipt wire format or a change to the frozen 216-byte receipt;
- WAL, mmap, async, workers, pools, background validation, or speculative
  prefetch;
- an unbounded ID set, subtree cache, byte cache, or source/reference vector;
- append-only, carrier, pack, compaction, or migration code;
- public profile selection or simultaneous production profiles;
- a full-create, directory, `+1`, or remote-storage optimization hidden inside
  the changed-spine patch; or
- promotion, profile selection, or compatibility claims.

## 7. Same-open validation witness

### 7.1 Purpose

The frozen persisted receipt remains part of the complete visible head. M4.5
adds only a private in-memory witness that proves the current process/open has
established the receipt's required validation authority for one immutable
generation.

Conceptually:

```text
SameOpenValidationWitness {
  private_open_identity,
  store_instance_id,
  validation_authority_id,
  integrity_epoch,
  profile_id,
  generation,
  root_id,
  transition_id,
  receipt_bytes,
  single_use_scope,
}
```

This is a semantic field list, not a new serialized grammar. The type has no
public constructor, encoder, decoder, schema representation, or persistence
API. It must not be accepted by a different `Store` opening even if every
persisted tuple field is byte-identical.

### 7.2 Issuance

A current open may issue the witness only after one of:

1. a complete authoritative full scrub in that same open authenticates the
   exact visible head, receipt, root, transition, and complete strong-edge
   closure; or
2. the same open successfully publishes or reconciles a generation whose
   complete new closure was established from an already valid same-open prior
   witness plus fully authenticated changed/new subtrees.

Opening a database and verifying only the persisted receipt is insufficient.
Preparation by a different process is insufficient. Source/manifest identity
is insufficient. A matching head tuple without closure authority is
insufficient.

### 7.3 Lifetime and invalidation

The witness is valid only for the exact immutable prior generation and open.
It is consumed by at most one publication attempt and is invalidated by:

- closing or reopening the Store/connection;
- a store, authority, epoch, profile, generation, root, transition, or receipt
  mismatch;
- any engine-authorized deletion, replacement, repair, or authority mutation;
- an unresolved durability outcome;
- publication of a different head; or
- explicit full-scrub failure.

The implementation may issue a new witness after exact-requested-head
reconciliation proves success. It must not reuse the prior-generation witness
for the new generation.

The current same-open trust model assumes object-table mutation goes through
the engine-controlled path. M4.5 does not claim protection against hostile raw
SQL or filesystem mutation after witness issuance. A caller requiring that
stronger model must perform a full scrub or receive
`ValidationAuthorityUnavailable` until WP7 supplies the required authority.

## 8. M4.5 same-count edit algorithm

### 8.1 Preconditions

The optimized path is eligible only when all of these are proven:

- the operation is an exact same-reference-count edit;
- the prior complete visible head is byte-identical to the witness binding;
- the witness belongs to the current Store opening and remains unconsumed;
- the prior and replacement mapping profiles are identical;
- the edit operation and independent benchmark oracle identify the expected
  resulting root and transition; and
- all checked bounds and mapping invariants remain satisfied.

Otherwise use the ordinary full pre-COMMIT closure path or return the exact
typed authority/correctness failure. Never silently enter the incremental path
on partial evidence.

### 8.2 Required procedure

The implementation executes, in order:

1. authenticate and validate the current complete head and frozen receipt;
2. validate the private same-open witness against the current open and tuple;
3. authenticate and decode the prior and replacement namespace/root objects;
4. compare their complete summaries, including `mode`, count, total length,
   height, profile, and ordered strong-edge descriptors;
5. follow every differing namespace/file edge and authenticate both prior and
   replacement nodes on that spine;
6. at each paired node, compare the complete canonical summary and every
   ordered strong-edge position;
7. treat an equal child ID as prior-witness-covered without fetching it;
8. completely traverse every new or different child ID, authenticating bytes,
   role, partition, summary, counts, cumulative ends, chunk IDs, and closure;
9. validate the complete root and transition against the independent expected
   IDs and operation fields;
10. create the new frozen receipt only after all qualification succeeds;
11. stage the exact complete visible head; and
12. dispatch one COMMIT.

Any missing, corrupt, malformed, wrong-role, wrong-summary, cyclic, overflowing,
or unexpected changed/new object fails before COMMIT dispatch.

### 8.3 Equality is exact, not heuristic

The changed-spine verifier must not use object size, reference count, total
length, ordinal, path, or a cached key as a substitute for `ObjectId` equality.
Equal child IDs are covered only because the same-open witness proves the
prior immutable closure and the exact parent objects are authenticated.

Every decoded file child must validate its declared `cumulative_end` against
the actual decoded child subtree length. Root `total_reference_count` and
`total_raw_length` must equal the streamed/decoded subtree summaries for
arbitrary durable input. K/F partition, nonfinal fullness, minimal height,
and no redundant unary levels remain enforced by the shared validator.

### 8.4 Independent benchmark oracle

The retained fixture manifest must bind:

- exact base source fingerprint and ordered CDC fingerprint;
- exact edit kind, offset, removed bytes, and inserted bytes;
- exact edited source fingerprint and ordered CDC fingerprint;
- exact result root ID and transition ID; and
- exact root/transition/closure identities used by the row.

The expected result must be prepared independently of the measured operation.
The measured process may compare constant-size expected IDs before COMMIT, but
it must not write newly observed values into the manifest. Post-COMMIT fresh
reconstruction recomputes exact edited bytes and fingerprints rather than
comparing a result to itself.

## 9. Publication and durability reconciliation

Before COMMIT dispatch, any failure guarantees no visible-head change. Staged
immutable objects may remain as authenticated unreachable residue only where
the existing transaction/cleanup contract permits it and reports custody
honestly.

After COMMIT dispatch, every ambiguous transport/SQLite result is reconciled
using a fresh independent connection and the retained prior/request tuple:

| Fresh authoritative observation | Required result |
|---|---|
| exact requested complete head and receipt | success; retain first cause as diagnostic |
| exact prior complete head | return the original exact failure; publication is absent |
| a different complete head | `PublicationConflict` |
| requested/prior/different cannot be established | `AmbiguousDurability` |

The idempotency key is recomputed from the frozen prior/request tuple. It is
not stored as a fifth visible-head field. Only the identical operation key may
retry an unresolved publication.

Instrumentation, JSON formatting, report writing, counter conversion, or
external observation occurs after the operation result is durably classified.
Such failures cannot relabel a known committed publication as failed.

## 10. Error and provenance requirements

M4.5 preserves the mapping error taxonomy and the exact object ID wherever
applicable. At minimum, direct tests cover:

- `MissingObject(ObjectId)`;
- `IdentityMismatch`;
- `ChunkIdentityMismatch` and `ChunkLengthMismatch`;
- `WrongLogicalRole`;
- `NonCanonicalPagePartition`;
- `LengthMismatch` and checked `LengthOverflow`;
- `MappingCycle` and `MappingDepthExceeded`;
- `InvalidValidationReceipt`;
- `ValidationAuthorityUnavailable`;
- `PublicationConflict`; and
- `AmbiguousDurability`.

`FailureProvenance.first` remains the first cause under frozen validation/event
order. Cleanup and reconciliation fill only their own fields. `dominant` is
computed by the existing frozen precedence and never by whichever error is
reported last.

## 11. Complexity contract

Let:

- `N` be total file references;
- `K` be leaf capacity;
- `F` be branch fanout;
- `H = O(log_F(N/K))` be mapping height;
- `X_b` be changed source bytes;
- `X_c` be changed/new chunk occurrences;
- `A_delta` be authenticated changed-spine bytes; and
- `V_delta` be fully traversed new/different subtree bytes.

The same-count mapping mutation remains:

```text
O(X_b + X_c + K + F*H) work
O(H) rewritten mapping objects
```

The minimal M4.5 qualification implementation may use bounded linear active-
ancestry scans. Its honest bound is then:

```text
O(K + F*H + A_delta + V_delta + H^2)
```

The `H^2` term is explicit and comes only from active-cycle membership checks.
Because durable mapping depth is strictly bounded, this is not a resident-
memory violation. A bounded `BTreeSet`/equivalent may reduce that term only if
measurement shows ancestry scans matter; M4.5 does not add it speculatively.

Expected LayerFS-owned live semantic memory is:

```text
O(H + K + bounded node pages + bounded canonical buffers + bounded output)
```

No source-sized or all-reference vector is permitted.

The following remain unchanged:

```text
fresh full scrub       = Theta(reachable canonical/raw bytes)
full reconstruction   = Theta(source bytes + references)
complete lifecycle    = Theta(N + source bytes)
fixed-ordinal +1 edit = Theta(suffix references/objects/bytes)
```

Big-O establishes scaling shape, not 200/300-MiB/s empirical throughput.

## 12. Exact resource and work accounting

### 12.1 Logical Q

Logical Q is the maximum checked sum of every simultaneously live
LayerFS-owned allocation capacity, not the maximum single vector length.
Charge at least:

- raw source/chunk buffers;
- canonical encode/authentication buffers;
- prior and replacement decoded-node vectors and their capacities;
- parent buffers retained across child descent;
- DFS frames and active cycle-detection state;
- branch/page/entry/reference vectors;
- generated bounded SQL/query buffers and parameter arrays;
- bounded BLOB-copy buffers;
- eager range/reconstruction output still live; and
- receipt, head, and failure-provenance working state.

Every charge, decharge, and high-water addition uses checked `u64`. Scoped
guards or one equivalent shared mechanism must ensure early returns do not
leak a logical charge. Tests deliberately hold parent and child allocations
live together and assert the summed high-water.

Logical Q excludes allocator metadata, SQLite/rusqlite internals, SQLite page
cache, mapped pages, filesystem cache, and kernel state. Those are separately
Observed or Unavailable.

### 12.2 W and D

Report exact cumulative:

```text
W = canonical bytes newly written by the operation
D = canonical bytes authenticated - canonical bytes newly written
```

Also report changed refs/chunks, rewritten leaves/branches/ancestors, covered
equal edges, new/different edges, fully traversed new objects/bytes, and
mapping bytes rewritten. All equations must reconcile per row.

### 12.3 SQLite and BLOB metrics

Report exact or explicit Unavailable values for:

- statement-cache acquisitions;
- native prepares;
- query/execute calls;
- rows returned and changed;
- BLOB opens, reads, writes, and copied/borrowed bytes;
- transactions and COMMITs;
- busy/locked events;
- sync/fsync observations;
- logical/apparent/allocated database plus sidecars; and
- process/host physical I/O where available.

Never substitute statement execution count for native prepare count, logical
file length for physical allocation, or zero for unavailable observations.

### 12.4 CPU, RSS, and macOS observation

Each measured row runs in one child process observed by `/usr/bin/time -l`.
On macOS, `maximum resident set size` is already bytes and must not be
multiplied by 1,024. Report total CPU, maximum RSS, peak memory footprint,
instructions, and cycles when exposed. Missing host counters are
`Unavailable`.

## 13. Correctness and benchmark protocol

### 13.1 Required narrow tests before release build

Run focused tests first, then the existing load-bearing suites. New M4.5 tests
must prove:

1. a persisted receipt from a preparation process does not authorize an
   incremental skip in a reopened row process;
2. corruption or deletion of an unchanged sibling before the row's same-open
   full scrub fails before witness issuance and before publication;
3. a same-open full scrub issues authority for the exact tuple only;
4. close/reopen, store/profile/epoch/head/receipt changes, witness reuse, and
   different-generation use invalidate the witness;
5. multiple changed children and a final partial leaf are fully authenticated;
6. wrong count, total length, cumulative end, height, fullness, mode, role,
   root, transition, edited fingerprint binding, or CDC-sequence binding fails
   before COMMIT;
7. missing/tampered changed and new objects preserve exact typed failures and
   object IDs;
8. all four real post-dispatch reconciliation outcomes preserve exact
   provenance;
9. logical Q includes simultaneous parent/child/canonical/query/output
   capacities; and
10. one transaction, one COMMIT, and one complete-head publication remain.

The test must not demand detection of hostile raw-SQL/filesystem mutation after
a valid same-open witness is issued; that is outside the current explicitly
allowed trust model. It must prove the implementation does not claim broader
authority.

### 13.2 Existing regression gate

After focused tests, run the smallest existing suites that cover:

- canonical codec goldens, malformed inputs, identity, exact EOF, and frozen
  FileChild field order;
- file partition/fullness/minimal-height and summary validation;
- zero/cross-chunk/leaf/branch ranges with exact bytes;
- delta decode/apply/replay and root/delta convergence;
- cycle/depth/Q/W/D and immutable reuse/tamper;
- receipt adversary, lost acknowledgement, and provenance;
- path-local same-count counters and bounded-spool `+1` regression;
- directory replacement/insertion regression;
- real Memory-vs-SQLite semantic parity; and
- package/workspace/all-target, format, diff, and clippy gates required by the
  controlling plan.

Build the release benchmark once after correctness passes. Debug self-tests
remain allowed, but debug builds must reject throughput/campaign output.

### 13.3 Frozen M4.5 row

Use only the retained exact 100-MiB K64/F64 same-middle fixture:

- source bytes: 104,857,600;
- source SHA-256:
  `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`;
- retained raw fingerprint:
  `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7`;
- retained result references: 5,284;
- retained result CDC-sequence fingerprint:
  `58b61bbd4f319ecb6011278ca42caf2b5d696e42b4655c054c48b3906d017b83`;
- retained result root:
  `cc8f31adc20eaa56b621744fe45f90f65fb9ac6177446d33b0052d7ebd404560`;
- retained transition:
  `2686d6ffc512b38f64922073dcc191a1ff1c7eacedb1c73e0a72045bf7cf4a92`;
- retained closure digest:
  `7b7142f5e203ae23efd46662efe576a182f8043c4323f487407bbb031b7cc2bb`;
  and
- exact changed source bytes per row: 18,854.

The fixture gate must reject any mismatch; it never rewrites expected values
from newly observed data.

### 13.4 Authority setup and timer boundaries

Each edit row starts from an independently prepared, committed, byte-identical
database/base image copied with every inseparable authority sidecar outside the
measured child operation. The measured child opens that image and performs a
same-open authoritative full scrub before receiving the witness.

Report two distinct scenarios:

1. `same_open_prevalidated_edit`: authority setup is an explicit untimed
   prerequisite performed identically for A and B; its wall/counters are still
   reported separately. The primary M4.5 algorithm metric is
   `precommit_closure_validation_wall_ns` and the operation metric is durable
   edit latency.
2. `first_open_edit_lifecycle`: authority setup is included in the lifecycle
   denominator. This diagnostic shows the cost when no same-open witness
   already exists.

No timer may hide repeated work by nesting and then summing it again. Required
disjoint equations are checked per row:

```text
durable_edit_total
  = mapping_and_cow
  + precommit_closure_validation
  + sqlite_commit_durability

postcommit_verification_total
  = fresh_reopen_head
  + fresh_full_scrub
  + reconstruction
  + range_verification

same_open_complete_lifecycle
  = durable_edit_total
  + postcommit_verification_total

first_open_edit_lifecycle
  = same_open_authority_establishment
  + same_open_complete_lifecycle
```

All Store handles, connections, process-local witnesses, and local state are
dropped before fresh reopen. Post-COMMIT scrub does not use the prior witness.

### 13.5 A/B protocol and primary metric

A is the frozen retained-M3 executable. B is M4.5. Both arms receive identical
same-open authority establishment and cache conditioning. Report cache state
as `warm_or_unknown` unless a real mechanism proves otherwise.

Run:

- one warmup per arm;
- five balanced interleaved measured pairs in isolated child processes;
- exact source, base, authority, executable, source-diff, and manifest
  fingerprints;
- complete raw JSONL and external-resource rows; and
- median, min, max, spread, every paired delta, and win count.

The primary affected metric is pre-COMMIT closure-validation latency. M4.5
passes the speed gate only with at least 5% median improvement and at least
four of five paired wins. Same-middle is an edit-latency row. Do not divide the
whole 100-MiB logical file by edit wall time and call it storage throughput.

### 13.6 Protected resource adjudication

Semantic correctness, one-COMMIT publication, exact Q, and storage bounds are
hard gates. CPU, external RSS, peak footprint, and allocated store are
protected at 5% for any PASS/promotion statement.

Because process RSS is noisy, use this predeclared procedure:

1. preserve the official five-pair result unchanged;
2. if the arm-median RSS or peak footprint regresses by more than 5% but pair
   direction is mixed or ranges materially overlap, label the resource result
   `INCONCLUSIVE` rather than causal `FAIL`;
3. run 15 additional balanced pairs, yielding 20 total pairs, without changing
   source, binaries, command, environment, or hypothesis;
4. report arm medians, paired byte and percentage deltas, pair directions,
   arithmetic means as diagnostics, and ranges; and
5. classify a protected-memory regression as repeatable only when the 20-pair
   paired median exceeds 5% and at least 16 of 20 pairs regress by more than
   5%.

If neither PASS nor repeatable FAIL is established, M4.5 may remain preserved
as nonqualifying/inconclusive research, but it cannot be called accepted,
promoted, or resource-safe. No post-hoc sample deletion, arm pooling, or
replacement of the original five rows is allowed.

Endpoint persistent storage must remain byte-identical or improve for this
optimization, because M4.5 has no authorized persistent-format change.

## 14. Milestone execution and reporting

The implementing agent completes and reports each milestone before starting
the next. Every report updates three ledgers:

- benchmark statistics and raw artifacts;
- implementation work and exact diff fingerprint; and
- algorithmic work/complexity and counter equations.

### M4.5-0 — freeze evidence and baseline

- Preserve M3 and rejected-M4 binaries, hashes, reports, and raw rows.
- Verify branch, HEAD, dirty-tree scope, retained fixture, and no Cargo writer.
- Reproduce no benchmark yet.
- Produce the corrected finding ledger and explicit M4 rejection rationale.

Exit: evidence fingerprints and scope are recorded without source edits.

### M4.5-1 — authority witness

- Add the smallest private same-open witness mechanism.
- Reuse the frozen receipt codec and shared closure walker.
- Add cross-process, reopen, tuple-mismatch, reuse, and invalidation tests.
- Do not restore the rejected M4 patch wholesale.

Exit: invalid cross-reopen reuse is impossible and the exact same-open
issuance/invalidation tests pass.

### M4.5-2 — incremental proof and expected-result binding

- Restore the changed-spine comparison behind the witness gate.
- Compare complete summaries, including `mode`.
- Fully validate all new/different subtrees and shared summary invariants.
- Bind the independently expected root, transition, source, CDC sequence, and
  operation before publication.
- Add malformed and multi-change tests.

Exit: no wrong result or incomplete changed/new closure can reach COMMIT.

### M4.5-3 — durability and typed failures

- Route real ambiguous COMMIT errors through the frozen reconciliation table.
- Prevent post-COMMIT instrumentation failures from relabeling success.
- Preserve exact `MissingObject(ObjectId)` and failure provenance.

Exit: before-dispatch plus all four post-dispatch outcomes pass direct tests.

### M4.5-4 — exact counters and bounded memory

- Replace max-local-vector Q with summed live-capacity accounting.
- Correct SQL prepare/acquisition/execution labels.
- Reconcile auth/hash/object/edge/SQL/BLOB/W/D equations.
- Keep endpoint storage unchanged.

Exit: deliberate simultaneous-allocation tests prove the Q high-water and all
per-row equations are checked.

### M4.5-5 — focused release comparison

- Complete focused and existing regression gates.
- Build release once.
- Run only the prescribed 100-MiB same-middle A/B scenarios.
- Run the predeclared memory confirmation only if triggered.
- Preserve raw JSONL, external observations, commands, binaries, source/diff
  hashes, and summary.

Exit: speed, correctness, CPU, RSS, Q, storage, and publication results have
separate PASS/FAIL/INCONCLUSIVE classifications.

### M4.5-6 — independent audit checkpoint

- Produce `wp04-opt-milestone-4-5.md`.
- State whether code is retained, reverted, or preserved as inconclusive.
- Record exact terminal diff and executable fingerprints.
- Stop for independent read-only audit before any M5, full campaign, profile
  selection, or promotion claim.

## 15. Acceptance table

| Gate | M4.5 PASS requirement |
|---|---|
| authority | skip requires exact private same-open witness; persisted receipt alone never suffices after reopen |
| prepublication correctness | expected root/transition and complete changed/new closure pass before COMMIT |
| codec/mapping | all frozen identities, summaries, partition rules, delta replay, and typed failures pass |
| atomicity | one transaction, one COMMIT, one complete head; all ambiguity reconciled |
| primary speed | at least 5% median improvement and at least 4/5 paired wins |
| CPU | no protected median regression above 5% |
| logical memory | exact summed Q remains within frozen bound with no source-sized state |
| external memory | no repeatable protected regression under section 13.6 |
| storage | no endpoint logical/apparent/allocated overhead for M4.5 |
| semantics | fresh reopen, full scrub, reconstruction, ranges, and Memory/SQLite identities remain exact |
| claims | same-count edit only; no full-create/profile/promotion/100-GiB throughput claim |

Any correctness, authority, atomicity, identity, or exact-Q failure rejects the
implementation regardless of speed. A resource result may be inconclusive
under the frozen diagnostic procedure, but it cannot be promoted to PASS by
relabeling or averaging.

## 16. Aggressive follow-on optimization direction

M4.5 is the highest-value same-count edit repair, but it does not address the
current full-create target. After M4.5 reaches its independent-audit checkpoint,
the next measured full-create work should follow this order:

1. **Transaction-local full-create witness.** Reuse validation already
   performed on newly generated immutable objects inside the same transaction
   and open, avoiding redundant pre-COMMIT rereads without cross-reopen trust.
2. **Bounded batched scrub reads.** Reduce thousands of SQLite API crossings
   while preserving independent canonical authentication, order, duplicates,
   missing-row rejection, and exact closure.
3. **Borrowed BLOB access.** Remove remaining bounded row-to-`Vec` copies where
   SQLite lifetimes make a borrowed callback safe.
4. **Bounded multi-row immutable CAS insertion.** Batch by both object count
   and canonical bytes while preserving insert-or-authenticate-reuse and no
   overwrite.
5. **COMMIT regression diagnosis.** Explain the retained M3 COMMIT increase
   with sync and physical-I/O evidence before changing durability behavior.

Those are separate milestones with their own A/B hypotheses. They must not be
folded into M4.5, because doing so would make the changed-spine result
unattributable and enlarge the correctness surface.

The retained full-create terminal baseline remains approximately:

| Phase | Retained M3 median |
|---|---:|
| canonical CAS mapping/persistence | 410.776 ms |
| pre-COMMIT closure | 388.155 ms |
| SQLite COMMIT | 152.996 ms |
| fresh reopen | 1.155 ms |
| full scrub | 272.815 ms |
| reconstruction | 429.985 ms |
| complete lifecycle | 1,663.449 ms |
| complete-lifecycle throughput | 60.116 MiB/s |

The 200-MiB/s target requires durable capture at or below 500 ms; the
300-MiB/s stretch target requires at or below 333.333 ms. M4.5 makes no claim
against either target because it is an edit optimization.

## 17. Required terminal report

The M4.5 terminal report must include:

- exact HEAD, dirty status, implementation diff hash, executable hash, and
  fixture/manifest hashes;
- every focused/regression/build command and result;
- the authority model and explicit same-open/cross-reopen distinction;
- every raw five-pair row and any predeclared confirmatory memory rows;
- median/min/max/spread, paired deltas, and wins for every phase;
- complete phase equations and work/resource counter equations;
- CPU, RSS, peak footprint, Q, W/D, SQL/BLOB, storage/sidecar, and physical-I/O
  Observed/Unavailable states;
- expected versus actual source, CDC, root, transition, receipt, and closure
  identities;
- implementation-defect versus algorithm-defect conclusions;
- honest asymptotic bounds for mutation, qualification, `+1`, scrub,
  reconstruction, and complete lifecycle;
- retained/reverted/inconclusive decision under section 15; and
- `qualification=false`, `promotion=false`, `rejection=false`.

The agent then stops for an independent read-only audit. It does not continue
into M5, the full campaign, profile selection, or promotion on its own.
