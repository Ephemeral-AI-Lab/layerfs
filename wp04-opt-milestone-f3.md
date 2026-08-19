# WP4-M F3 — bounded immutable SQLite CAS insertion grouping

## F3-v1 prospective preregistration — frozen before source edits or Cargo

- Date: 2026-08-19.
- Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
  `codex/empty-worktree`.
- Starting HEAD/tree: `f7aff33dc46237ed06a94858c9a3b71bc02e82c8` /
  `d54de4c2aeb87969cd9c9e2863e75b476a8c6886`.
- Starting status: clean.
- Classification: fixed bounded SQLite crossing reduction. Full create remains
  `Theta(B + N)`; the insertion group adds only a fixed-capacity term.
- One variable: only the private genesis full-create canonical-object insertion
  transport changes from one SQLite INSERT statement execution per occurrence
  to a bounded multi-object INSERT group inside the same existing transaction.
  CDC, raw `ChunkId`, canonical bytes, `ObjectId`, mapping topology, the F2
  construction proof, root, transition, closure, schema, publication,
  FULL+DELETE durability, transaction count, and COMMIT count do not change.
- Terminal decision is exactly `PASS / retain`, `FAIL / REVISE`, or
  `FAIL / revert`. F4, profile selection, production integration, backend work,
  and commit are outside this milestone.

### Entry custody

| Item | Frozen value |
|---|---|
| accepted F2-v3 source SHA-256 | `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` |
| F3 A/control executable | `target/wp4m-f2-construction-proof-k64-20260819-v3/binaries/phase4_create_edit_benchmark-f2-v3-candidate` |
| F3 A/control SHA-256 | `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0` |
| retained source / bytes / SHA-256 | `target/wp4m-f2-construction-proof-k64-20260819-v3/S1-100.source` / `104,857,600` / `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| fixture manifest SHA-256 | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| accepted F2-v3 raw SHA-256 | `0452726e74b207dabd77f70aab04ef8be4a3aa81162dd3fdcdb094b13d6de46e` |
| accepted F2-v3 artifact manifest SHA-256 | `a80a81274093c736864beb2f0ac4e59a1dca665f466c7b9669d1d4be3082491b` |
| accepted F2-v3 final audit SHA-256 | `258c4348cff0df6e11c09099d57a33ca4d1205f9f114b2fefb6724973e3614ee` |
| F3 B/candidate source/executable | `NotBuilt` / `NotBuilt`; freeze once after all correctness/static/resource gates |

The accepted F2-v3 control medians are mapping/proof `492.776500 ms`,
pre-COMMIT `0.051458 ms`, standalone outer COMMIT `168.425625 ms`, combined
tail `168.477083 ms`, durable capture `659.592708 ms`, and complete lifecycle
`1,353.840916 ms`. Control Q is exactly `55,325` bytes with terminal zero.
Standalone COMMIT remains a mandatory phase-coupled diagnostic under the
accepted prospective F2-v3 interpretation; combined tail, durable capture,
logical writes, pager/storage equations, FULL+DELETE, and one COMMIT remain
hard gates.

### Cap selection frozen before implementation

Read-only inspection of the accepted F2-v3 result database gives:

```text
object occurrences / unique rows       5,372 / 5,372
canonical bytes                       105,291,554
canonical length min / mean / max      54 / 19,600.066 / 32,781
retained maximum raw chunk             32,768
SQLite MAX_VARIABLE_NUMBER             500,000
SQLite MAX_SQL_LENGTH                  1,000,000,000
```

F3-v1 freezes:

```text
INSERT_GROUP_ROW_CAP                   64
INSERT_GROUP_CANONICAL_BYTE_CAP        1,048,576 bytes (1 MiB)
parameters per distinct ID             4
maximum parameters                     256
maximum generated INSERT SQL text      778 bytes
absolute F3 q_high_water cap            1,310,720 bytes (1.25 MiB)
```

The row cap uses only 256 parameters, below even SQLite's historical portable
999-variable floor and far below this executable's observed 500,000 limit.
The 778-byte maximum SQL is negligible beside the observed 1,000,000,000-byte
limit. The byte cap holds at least 31 worst-case retained canonical objects
(`floor(1,048,576 / 32,781) = 31`) and about 53 mean objects. Thus the row cap
still bounds duplicates, results, and quadratic bookkeeping, while the byte
cap—not a source/reference count—bounds retained canonical ownership. These
caps are frozen for v1 and will not be tuned after a performance observation.

### SQL, duplicate, result, and authentication shape

One group uses this private shape, with one four-parameter tuple for the first
occurrence of each distinct `ObjectId`:

```sql
INSERT INTO wp4m_objects
  (object_id, kind, canonical_length, canonical_bytes)
VALUES (?,?,?,?), ...
ON CONFLICT(object_id) DO NOTHING
RETURNING object_id
```

The group retains every input occurrence in order. A fixed-cap linear scan
maps each occurrence to its first equal ID; no hash map exists. This has an
explicit ceiling of `64*64 = 4,096` ID comparisons per group. Returned IDs are
only the bounded set of first-occurrence rows actually inserted. They are not
the final classification.

After the INSERT statement succeeds atomically:

1. the first occurrence of each returned ID is provisionally `created`;
2. each nonreturned first occurrence is a conflict and each later duplicate is
   provisionally `reused`;
3. every provisional reuse performs the existing complete incumbent query and
   authenticates stored kind, declared length, complete canonical `ObjectId`,
   and byte equality;
4. no `PutEvidence`, mutation-serial advance, created/reused publication
   counter, or proof consumption is issued until all required incumbent
   authentication succeeds;
5. ordered per-occurrence evidence is then issued with consecutive checked
   mutation serials and consumed by the unchanged F2 construction-proof order;
   and
6. any SQL/result/authentication/allocation/overflow failure returns a typed
   error through `transaction_attempt`, rolls the transaction back, invalidates
   pending evidence, and leaves terminal Q zero.

The trusted generated-object path keeps its accepted F2 validation contract:
canonical bytes come from the frozen canonical encoder and the exact ID is
computed over those bytes once. Externally supplied `(ObjectId, bytes)` still
takes complete `validate_identity` before admission. Conflict/reuse always
authenticates the complete incumbent. A returned inserted ID alone never
authorizes reuse or proof evidence.

### Before/after algorithm and bounds

Control:

```text
for every canonical object occurrence:
  validate/generated-canonical proof -> one INSERT execution
  -> on conflict one complete incumbent query/authentication
  -> one ordered PutEvidence
```

Candidate:

```text
for every canonical object occurrence:
  validate/generated-canonical proof -> append to fixed row+byte group
flush at either cap -> one atomic multi-row INSERT/RETURNING
  -> bounded deterministic occurrence classification
  -> complete incumbent auth for every reuse
  -> ordered PutEvidence only after the whole group succeeds
```

With fixed caps:

```text
time       Theta(B + N) before and after
memory     O(K + F*(H+1) + 1 MiB group + bounded buffers)
durable    Theta(B_u + N), unchanged
crossings  Theta(N) with a smaller fixed factor:
           N INSERT executions -> ceil-by-row-and-byte groups
```

No source-sized/reference-sized staging, unbounded SQL/parameter/result list,
object map, cache, spool, table, sidecar, worker, async path, VFS, dependency,
or public batching abstraction is permitted.

### Exact simultaneous-capacity Q contract

Target-layout direct tests must freeze:

```text
size_of::<GroupInput>()                  = 72 bytes
size_of::<PendingConstructionResult>()  = 16 bytes
size_of::<PutEvidence>()                 = 80 bytes (accepted F2 value)
size_of::<ObjectId>()                    = 32 bytes
```

The conservative simultaneous owned-capacity equation is:

```text
prepared expectations                                      14,486
accepted F2 proof/frontier                                  21,952
FastCDC owned chunk Vec capacity                            32,768
group fixed/scalar/iterator envelope                         4,096
64 pending GroupInput descriptors                64*72        4,608
pending owned canonical buffers                         1,048,576
incoming canonical at a pre-admission flush                   32,781
64 pending construction-result slots             64*16        1,024
maximum INSERT SQL text                                       778
64 returned inserted ObjectIds                    64*32        2,048
64 ordered PutEvidence results                    64*80        5,120
one incumbent decoded canonical buffer (payload + charge)     33,024
largest simultaneous mapping inner/encoding buffer             4,352
                                                            ---------
pre-correction analytical all-terms ceiling               1,205,613
hard F3-v1 q_high_water cap                              1,310,720
terminal q_current                                               0
```

The equation deliberately over-sums a few mutually exclusive local lifetimes;
the direct high-water test must exercise the actual largest overlap, report its
exact term values, and remain below the frozen absolute cap. Bind parameters
are borrowed through a fixed iterator over the charged descriptors, so their
variable heap capacity is exactly zero. SQLite-owned page cache and statement
internals are external to logical Q and remain covered separately by RSS,
pager, journal, and endpoint gates. Report/range output is produced after the
group is dropped and therefore has zero simultaneous overlap with it; its
existing independent charged high-water and terminal-zero checks remain.

Every product, sum, next-row admission, SQL length, parameter count, returned
row count, classification count, byte count, mutation serial, and metric update
is checked before the corresponding fallible action. Exact-cap admission
succeeds; the next byte/row refuses or flushes as specified. Allocation refusal
and every injected error must end with Q zero.

### Retained-fixture direct counter prediction

Replaying accepted rowid/insertion order through the frozen caps predicts:

```text
input occurrences / distinct IDs               5,372 / 5,372
canonical bytes                                105,291,554
natural byte-cap flushes                                  101
row-cap flushes                                             0
file-root proof barrier flushes                             1
final workspace+transition flushes                          1
total insertion groups / statement executions             103
maximum group rows / bytes                              57 / 1,048,309
returned inserted rows/IDs                              5,372
incumbent queries/rows/BLOB reads/authentications            0
ordered classifications created/reused                 5,372 / 0
total SQL parameters / BLOB binds                 21,488 / 10,744
row BLOB writes                                           10,744
PutEvidence values / proof edges                     5,372 / 5,371
objects / canonical new bytes                    5,372 / 105,291,554
mapping bytes                                               365,262
writer transactions / COMMITs                                 1 / 1
```

Insertion execution reduction is `5,372 -> 103` (`98.08265%`). Mapping-phase
SQL executes include the existing BEGIN and therefore predict `5,373 -> 104`.
Complete-lifecycle counters predict, relative to the accepted F2-v3 candidate:

```text
statement-cache acquisitions          10,863 - 5,372 + 103 = 5,594
SQL execute calls                       5,379 - 5,372 + 103 =   110
SQL query calls                                               5,581
SQL rows changed                                              5,373
SQL rows returned                     10,780 + 5,372         = 16,152
row BLOB reads                                               10,787
row BLOB writes                                              10,748
```

The 5,372 returned IDs are bounded INSERT results, not incumbent BLOB reads.
Any fixture conflict, duplicate, or count drift invalidates these fresh-store
predictions and must be explained before timing interpretation.

### Focused correctness/resource matrix

Before any release build or performance row, direct tests must cover:

1. empty, one, row-cap minus one, exact row cap, row-cap plus one, and final
   partial groups;
2. byte-cap minus one, exact byte cap, byte-cap plus one admission/flush,
   maximum canonical object near the byte cap, and simultaneous row/byte cap;
3. duplicate occurrence within one group and across group boundaries;
4. exact incumbent reuse and complete malformed, wrong-kind, wrong-length,
   missing-after-conflict, unequal-incumbent, and same-ID/unequal-byte rejection;
5. INSERT execution failure before evidence and failure during incumbent
   authentication, with statement atomicity and zero evidence;
6. allocation refusal, SQL/parameter/result/row/byte/counter/mutation overflow,
   exact-cap success, next-unit refusal, maximum overlap Q, and terminal zero;
7. rollback invalidation of every unconsumed grouped result and exact typed
   first/cleanup/reconciliation provenance;
8. exact mutation-serial/evidence order and all 5,372 F2 proof consumptions/
   edges at retained-fixture scale;
9. exact root, transition, ordered closure, reconstruction, ranges, source,
   CDC sequence/count, object bytes, and topology;
10. exactly one transaction/COMMIT, ambiguous-COMMIT reconciliation, no
    post-COMMIT relabeling, and no full pre-COMMIT replay; and
11. protected F2/M4.5 adversarial and release paths unchanged.

Run focused F3 tests first, then workspace all-target tests, Clippy with
`-D warnings`, rustfmt check, `git diff --check`, status/diff custody, debug
self-test, read-only schema/storage audit, and the smallest release M4.5
regression. No release candidate or performance row exists before all gates
pass.

### Frozen resource, phase, storage, and decision gates

Material performance acceptance requires both affected metrics:

```text
mapping/CAS arm-median improvement       >= 5%
mapping/CAS paired-median improvement    >= 5%, at least 4/5 wins
durable-capture arm-median improvement   >= 5%
durable paired-median improvement        >= 5%, at least 4/5 wins
```

The direct `5,372 -> 103` insertion and `5,373 -> 104` mapping-execute
equations must be exact. Qualification remains one head query and zero replay
BLOB/authentication. Source/CDC/identity/proof/root/transition/closure,
canonical/mapping bytes, objects, SQL changed rows, BLOB writes, one
transaction/COMMIT, publication/reconciliation, FULL+DELETE, schema, and
post-COMMIT results are hard exact gates.

Protected resource gates are total CPU, RSS, peak footprint, and allocated
store no worse than +5% for arm and paired medians with at least 4/5 pairs
within +5%. System-CPU arm and paired-median increases must each be at most
60 ms and at least 4/5 pair increases at most 60 ms, preserving F2-v3's frozen
coarse-resolution envelope. Candidate Q must be `<=1,310,720` with terminal
zero on every row/failure; the exact observed overlap must match direct tests.

Final main-DB dirty writes must not exceed the F2-v3 equation `26,676`, final
spills must not exceed `6,675`, and any decrease must reconcile to the same
logical rows/bytes and final database. Sampled rollback-journal allocation must
not exceed 20,480 bytes; final logical/apparent database bytes must remain
109,268,992, authority apparent bytes 32, and residual journal/WAL/SHM zero.
No unexplained endpoint, allocation, page-write, spill, or COMMIT expansion is
accepted. True journal/temp peaks, VFS calls/bytes, xSync calls/wall, and
physical media I/O remain `Unavailable` unless directly observed.

Standalone COMMIT and nested dispatch remain reported phase-coupled
diagnostics. Combined tail and durable capture are hard. If mapping improves
but durable capture fails the 5% gate, reject or revise; never hide COMMIT or
journal expansion in the mapping result. Tiny unchanged reopen/range phases
retain F2-v3's prospectively frozen 200-us-plus-5% envelope; scrub and
reconstruction retain the ordinary +5% arm/paired/4-of-5 protection.

### Frozen release campaign and custody

Artifact root for this candidate is:

```text
target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v1
```

Freeze A once from the accepted F2-v3 executable and B once from the final
validated source with debug assertions disabled. Use the exact retained source
and fixture manifest. Prepare each pair once outside timers; byte-copy and hash
the database, authority, and expectations to both arms; preflight hashes and
apparent/allocated endpoints; run every child under `/usr/bin/time -l`; and
retain every started row.

The asserted complete schedule is exactly:

```text
pair0 warmup   AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

Retain source/diff/binaries, fixture and manifest, prepared pair bases, raw
JSONL, stdout/stderr, commands, environment/toolchain/build/test/static/M4.5
outputs, preflight, CPU/RSS/Q, SQL/BLOB/group/classification counters,
pager/COMMIT/storage equations, a primary Python analysis, an independently
implemented non-Python analysis, analyzer agreement, schema/storage audit,
versioned complete manifest, and final read-only audit.

No started row is replaced, deleted, selectively rerun, or relabeled. If
source or executable changes after a campaign, preserve v1 as terminal
`FAIL / REVISE`, freeze a new F3-vN preregistration and artifact root, rebuild
once, and run an entirely fresh campaign. A cap change is always a new
experiment. Three consecutive recurrences of the same blocker without a safe
in-scope correction terminate changing work and require user direction rather
than a weakened gate.

### Current preregistered state

F3-v1 is preregistered only. No source edit, Cargo invocation, candidate
binary, artifact root, prepared base, or F3 row exists. F2-v3 remains the
accepted control; F4 is not started.

## Self-reflection A — after preregistration, before code

1. **Does crossing cost dominate?** Not yet proven. F2-v3 mapping is
   492.777 ms with 5,372 object INSERT executions, so the hypothesis is
   measurable and plausible, but M2 already showed that a 96.835% statement
   reduction can yield only 7.169% wall improvement. F3 therefore keeps the
   two independent 5% mapping/durable gates and will not infer speed from the
   counter alone.
2. **Existing patterns to reuse.** `Store::put_authenticated` already owns
   incumbent kind/length/identity/byte authentication; `transaction_attempt`
   already owns rollback/provenance; `FileBuilder` and `ConstructionState`
   already own ordered proof/frontier state; and `for_each_leaf_bytes` already
   demonstrates checked, charged, fixed-bound dynamic SQL with borrowed
   parameters. F3 will extract/reuse only those private pieces. A public batch
   framework is unnecessary.
3. **Caps and SQLite ceiling.** Four parameters times 64 rows is 256, below
   the portable historical 999 floor and the observed 500,000 build limit.
   The exact maximum SQL text is 778 bytes. No limit depends on the fixture,
   even though the fixture distribution predicts byte-cap-driven groups.
4. **No hidden linear staging.** The only new resident collections are two
   64-slot vectors, returned IDs/evidence bounded to 64, and canonical buffers
   admitted under 1 MiB. Duplicate/result mapping is a bounded linear scan.
   The existing K/F frontier remains the only topology state; no all-object,
   all-reference, source, or event transcript is introduced.
5. **Evidence-before-authority failure modes.** The implementation must stage
   classification and checked counter totals, finish the complete INSERT
   result scan, reject unknown/duplicate returned IDs, authenticate every
   conflict and duplicate incumbent, and pre-admit the evidence vector before
   advancing mutation serials or creating the first `PutEvidence`. SQL failure,
   missing-after-conflict, malformed incumbent, allocation failure, result
   overflow, or counter overflow before that point must expose zero evidence.
   Any later proof-fold failure is still transaction-local and invalidates the
   whole ordered result through rollback.
6. **Smallest safe shape.** Group only the F2 genesis full-create transport.
   Keep all ordinary `Store::put` callers and M4.5 paths on their accepted
   single-object implementation. The final workspace and transition can share
   one two-row group; the file-root proof barrier remains one explicit flush,
   producing the preregistered 103 groups without redesigning the F2 proof.

Reflection A finds no authority or bound blocker. It does not establish a
performance result; implementation may begin without changing the frozen
caps, gates, or predicted equations.

### Pre-timing analytical corrections from synthetic review

These corrections were recorded during implementation, before any Cargo
command, release build, artifact root, or performance row. They change no cap,
gate, statement shape, executable, or retained-fixture counter prediction.

- The fixed linear-scan strategy remains `O(row_cap^2)`, but several bounded
  passes perform duplicate discovery, parameter selection, returned-ID
  validation, and classification. The conservative aggregate ceiling is
  `5 * 64 * 64 = 20,480` ID comparisons per group, not 4,096 total. The
  Ponytail ceiling and upgrade rule remain: use a map only if the fixed cap
  grows or the bounded scan is measured material.
- A full K64 file leaf inner buffer is `11 + 4 + 64*68 = 4,367` bytes and its
  canonical wrapper is 4,380 bytes. Their exact simultaneous mapping
  encode-buffer overlap is therefore 8,747 bytes, replacing the preliminary
  4,352-byte line. The conservative analytical all-terms Q sum becomes
  `1,210,008` bytes, still below the prospectively frozen 1,310,720-byte cap.
- The private group constructor enforces the scoped maximum canonical input
  `32,781` bytes and exact `capacity == len`; this makes the incoming-object Q
  premise local rather than relying only on current callers.
- The generated INSERT uses `prepare_cached` and is counted as one statement-
  cache acquisition and one logical write execution even though RETURNING is
  stepped through rusqlite's row API. Returned object IDs have their own
  explicit bounded counter and do not increment canonical row-BLOB reads.

The original preregistration SHA-256 before these additive corrections was
`09879a1924b239224976dcebf3bc1d043f6baeb64a3b6a933f73193d599cd881`.

## Self-reflection B — implementation/focused/full/static gate, before release

Final pre-release source SHA-256 is
`414b9751a8caa0b3ab08c0b72aec923511aea3aa08e395bb7bb9c7197aa23c6c`.
Only this private benchmark source and this new milestone record differ from
the accepted checkpoint. No core codec, CDC, schema, engine library, profile,
durability setting, or dependency changed.

### Diff reread against the one-variable contract

- The ordinary `Store::put`, non-proving `FileBuilder`, edit, M4.5, scrub,
  reconstruction, range, publication, and reconciliation paths retain their
  prior behavior. Shared refactoring is limited to extracting the existing
  complete incumbent authentication and generated-input metric accounting.
- Only `FileBuilder::new_proving` owns the new group and pending proof-result
  slots. Canonical IDs, leaf/branch/root encoders, provisional topology
  summaries, and the accepted F2 final proof fields remain unchanged.
- A group is bound to the active transaction identity at construction and is
  rejected before SQL under a missing/replaced transaction. Canonical input
  length, exact capacity, aggregate bytes, row count, SQL length, parameters,
  returned IDs, result/classification algebra, counter headroom, evidence
  capacity, and mutation serials are checked.
- INSERT results are completely consumed and duplicate/unknown returned IDs
  rejected. Every nonreturned first occurrence and later duplicate performs
  the extracted full incumbent kind/length/identity/byte authentication.
  Evidence serials are constructed only after all those checks succeed.
- File dependencies use provisional bounded summaries only. Before a unary
  collapse reads SQLite, pending objects cross an explicit proof barrier. The
  file-root occurrence crosses the retained fixture's one proof barrier before
  the F2 scope is finalized. Workspace and Genesis transition share the final
  two-row group.
- `InsertionGroup.inputs` drops every owned canonical buffer before its fixed
  charge; `FileBuilder` drops the insertion group and pending results before
  the existing frontier charge. Returned IDs, SQL text, decoded incumbent,
  and evidence results are local to one flush. Every tested success/error path
  returns Q to zero.
- No public framework, worker, async/pool/VFS path, table, sidecar, dependency,
  source spool, all-reference/object list, map, cache, or serialized metadata
  exists. The only deliberate simplification is the commented fixed-cap
  quadratic scan; its aggregate comparison ceiling is recorded above.

### Direct pre-release observations

The focused retained-fixture debug test reproduces the frozen identities and
reports exactly:

```text
groups / INSERT executions                      103 / 103
byte / row / proof / final flushes          101 / 0 / 1 / 1
inputs / unique / returned IDs        5,372 / 5,372 / 5,372
created / reused / incumbent queries        5,372 / 0 / 0
BLOB binds / writes                        10,744 / 10,744
mapping executes including BEGIN                     104
proof evidences / strong edges              5,372 / 5,371
objects / canonical / mapping bytes
  5,372 / 105,291,554 / 365,262
maximum group rows / bytes                 57 / 1,048,309
root / transition                            exact accepted values
q_high_water                                 <= 1,310,720
terminal Q                                             0
```

Validation status on the final source:

```text
focused F3 tests                     4 passed
protected focused F2 tests          13 passed
private benchmark tests             52 passed
workspace all-target tests          117 passed, 0 failed
Clippy all-target -D warnings       PASS
rustfmt --check / git diff --check  PASS / PASS
debug self-test                    PASS; root=f1cfdd7f...d2e42a;
                                   objects=20; auth_bytes=1,054,925
debug schema/storage                3 unchanged tables; objects/meta/head
                                   20/1/1; DELETE; synchronous=FULL;
                                   no journal/WAL/SHM residue
```

The first protected F2 rerun found two fixable sequencing assumptions. The
non-proving root path was incorrectly evaluating proof-only coverage, and the
unary corruption test tried to modify an intentionally pending branch. The
shared fixes moved coverage under the proving branch and made the adversarial
test cross the explicit group barrier before injection. The complete F2 and
workspace reruns then passed. No gate was weakened.

Reflection B finds no remaining correctness, authority, identity, proof,
durability, bound, static, or protected-regression blocker. The release source
may now be frozen and built once; no performance conclusion exists yet.

## F3-v1 terminal result — FAIL / REVISE

The immutable v1 root is
`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v1`.
Its manifest verifies 165 files with SHA-256
`3932031bd789807893908cf1b780db33e823765671b9544f6c24aaf2e627e5aa`;
the final audit SHA-256 is
`e993f7f3693d7e30954f76bec44b92db2d4ff7a7ecb1f022f2d590a5f7a9d1b4`.
Primary Python and independent Ruby analyses agree exactly after deleting
their independently calculated hash field.

V1 proves the intended mechanism: insertion executions are `5,372 -> 103`,
mapping executes including BEGIN are `5,373 -> 104`, group/flush/
classification/BLOB/proof equations are exact, Q is `1,148,837 <= 1,310,720`
with terminal zero, and every semantic/write/pager/storage/M4.5 gate passes.
It fails performance and resource gates:

```text
mapping/CAS       501.872 -> 562.438 ms  +12.068% arm, +12.849% paired, 0/5
durable capture   674.972 -> 745.510 ms  +10.450% arm/paired, 0/5
complete lifecycle 1,377.332 -> 1,453.100 ms  +5.501%
RSS / footprint   +7.155% / +7.213%, 0/5
total CPU         +5.036% arm
```

The counters distinguish a genuine statement/result-shape cost from an
implementation, counter, orchestration, phase-policy, identity, or storage
bug. V1 materializes exactly 5,372 `RETURNING object_id` rows and raises SQL
rows returned `10,780 -> 16,152`; mapping, COMMIT, durable wall, RSS, and peak
all move adversely. V1 is not retained and F4 remains ineligible.

## F3-v2 prospective repair — frozen before v2 source edits or build

Date: 2026-08-19. V1 and every file under its root are immutable historical
`FAIL / REVISE` evidence. V2 keeps the frozen row cap 64, byte cap 1 MiB,
1.25-MiB Q cap, F2-v3 control, fixture, AB/BA schedule, correctness/resource
gates, and one-variable scope. No cap or threshold is tuned.

### One repaired statement/result variable

V1 executes one grouped `INSERT ... ON CONFLICT DO NOTHING RETURNING
object_id`. V2 changes only exact result acquisition inside the same private
group:

```text
bounded pre-insert query for distinct submitted IDs
  -> fully authenticate every returned incumbent and compare every covered
     occurrence
  -> grouped INSERT ... ON CONFLICT DO NOTHING without RETURNING
  -> require changed rows == distinct IDs absent from the prequery
  -> for IDs first created in this group but duplicated later, run one bounded
     post-insert incumbent-authentication query before evidence
  -> issue ordered evidence after every query/statement/authentication succeeds
```

The active `BEGIN IMMEDIATE` writer transaction makes the prequery absence set
stable against other writers. The group remains bound to that exact
transaction identity. Any changed-row mismatch is a typed publication conflict
and rolls back. Returned/existing IDs alone never authorize reuse: stored kind,
declared length, complete canonical identity, and bytes are authenticated.
Each input occurrence receives one classification/evidence in order.

The prequery statement is a bounded `IN` query over first occurrences:

```sql
SELECT object_id, kind, canonical_length, canonical_bytes
FROM wp4m_objects
WHERE object_id IN (?, ..., ?)
```

The insert is the v1 four-parameter VALUES list without `RETURNING`:

```sql
INSERT INTO wp4m_objects
  (object_id, kind, canonical_length, canonical_bytes)
VALUES (?,?,?,?), ...
ON CONFLICT(object_id) DO NOTHING
```

At most 64 ID parameters are used by a query and 256 by an insert. SQL text,
parameters, incumbent/result IDs, owned canonical buffers, duplicate scans,
and evidence remain fixed-cap. Complexity remains `Theta(B+N)` time and
`O(K + F*(H+1) + 1 MiB group + bounded buffers)` memory.

### Fresh retained-fixture v2 counter prediction

The fixture has no duplicate/conflicting ID, so exact v2 predictions are:

```text
groups / insert executions                         103 / 103
pre-insert group queries / rows                     103 / 0
post-insert duplicate-auth queries / rows             0 / 0
returned inserted IDs/rows                             0
inputs / unique / created / reused       5,372 / 5,372 / 5,372 / 0
incumbent BLOB reads/authentications                     0
byte / row / proof / final flushes             101 / 0 / 1 / 1
INSERT BLOB binds / logical BLOB writes       10,744 / 10,744
prequery ID BLOB binds / total group BLOB binds 5,372 / 16,116
mapping execute calls including BEGIN                   104
complete statement acquisitions      10,863-5,372+206 = 5,697
complete SQL execute calls              5,379-5,372+103 =   110
complete SQL query calls                    5,581+103   = 5,684
complete SQL rows returned                            10,780
complete SQL rows changed                               5,373
objects/canonical/mapping bytes             exact F2-v3 values
PutEvidence / proof edges                         5,372 / 5,371
transactions / COMMITs                                  1 / 1
```

Direct group counters must separately expose prequery calls/rows, post-insert
duplicate-auth calls/rows, inserted changed rows, classifications, and zero
INSERT-returning rows. Do not call v2 grouped unless both the `103` insert and
`103` prequery equations are exact.

### Q, tests, release, and decision

V2 reuses the v1 descriptor/canonical/pending/evidence capacities. The former
returned-ID vector is now the bounded preexisting-ID/result vector, so the
conservative `1,210,008` analytical sum and frozen `1,310,720` cap remain
unchanged. Exact overlap and terminal-zero tests must be rerun; no relative-Q
gate is added.

Before release, rerun the full v1 boundary/failure/duplicate/overflow matrix
plus: no-existing fresh group; all-existing group; mixed existing/absent IDs;
unordered query results; duplicate existing and newly inserted IDs; changed-
row mismatch injection; prequery failure; post-insert duplicate-auth failure;
and no evidence after any of those errors. Then rerun all F2/M4.5/full/static/
schema/Q gates.

V2 artifact root:

`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v2`.

Freeze one new release executable after validation and run a fresh complete
`AB/AB/BA/AB/BA/AB` campaign against the original F2-v3 control. V1 rows are
not reused or replaced. Mapping and durable capture must each improve at least
5% for arm and paired medians with at least 4/5 wins; all original hard gates
remain. If v2 still fails, distinguish returned-result cost from multi-row
statement/buffer cost before any third repair. F4 remains blocked.

Current v2 state: preregistered only. No v2 source edit, binary, root, base, or
row exists.

### F3-v2 self-reflection B — before release

Final v2 source SHA-256 is
`5bb2c5f8c2b8548818a540f61775dce0c4f302e0d2b78e6bed2891d47a7afba9`.
The v1-to-v2 diff changes only the private group result/classification shape,
its exact counters/output, and focused tests. Caps, canonical construction,
FileBuilder/proof topology, source/CDC, publication, durability, schema, and
post-COMMIT code are unchanged.

The prequery is bound to the same active transaction as the group. Its
unordered IDs are duplicate/unknown checked and mapped through the bounded
first-occurrence array. Every returned row is kind/length/identity/byte
authenticated and compared with every covered occurrence. Only absent unique
IDs are bound into the INSERT. Changed rows must equal absent IDs. Newly
inserted IDs with later duplicates take the bounded post-insert authentication
query before evidence. Mutation serials/evidence remain the final action after
all query/insert/authentication/counter/Q checks.

No unbounded or source/reference-sized state was added. Two fixed 64-element
index arrays live on the stack; the charged incumbent-ID vector reuses the
v1 returned-ID Q term. The 1.25-MiB cap and terminal-zero rules remain exact.

Final debug observations are:

```text
focused F3 tests                     5 passed
focused F2 tests                    13 passed
workspace all-target tests         118 passed, 0 failed
Clippy -D warnings / fmt / diff     PASS / PASS / PASS
debug self-test                    PASS, accepted root, 20 objects
debug schema/storage               unchanged 20/1/1 rows, DELETE/FULL, no residue
retained debug groups              103
prequeries / insert executions     103 / 103
INSERT-returning rows                0
mapping acquisitions/execute/query 206 / 104 / 103
created/reused/evidence/edges       5,372 / 0 / 5,372 / 5,371
canonical/mapping bytes             105,291,554 / 365,262
Q                                  <=1,310,720, terminal zero
```

Reflection B finds no remaining v2 correctness, authority, proof, bound,
static, or protected-path blocker. The source may be built once in release;
no v2 performance conclusion exists yet.

## F3-v2 terminal result — FAIL / REVISE

The immutable v2 root is
`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v2`.
The frozen source/executable SHA-256 values are
`5bb2c5f8c2b8548818a540f61775dce0c4f302e0d2b78e6bed2891d47a7afba9` /
`e7ea76f1b625a752fca67681a508d90f13b2d73c73761b856a9dadafc77332cb`.
The raw JSONL SHA-256 is
`d8d958385ce22c244023f731ab3551f7e13e3c078673dbc5fc2e0d4da5b1e663`.

V2 removes every v1 inserted-ID result row and proves the preregistered
`103` prequeries, `103` grouped INSERT executions, zero preexisting rows,
zero post-insert duplicate queries, `5,372` created classifications, exact
proof/root/storage equations, Q `1,148,837 <= 1,310,720`, and terminal zero.
Its primary Python and independent Ruby results agree exactly after deleting
their independently computed hash field. Nevertheless both mandatory gates
fail in every pair:

```text
mapping/CAS       485.447 -> 504.308 ms  +3.885% arm, +4.009% paired, 0/5
durable capture   645.434 -> 672.161 ms  +4.141% arm, +4.374% paired, 0/5
standalone COMMIT 160.568 -> 169.046 ms  +5.280% arm, +5.528% paired, 1/5
complete lifecycle 1,336.071 -> 1,359.882 ms +1.782%, 1/5
RSS / footprint   +5.789% / +5.811%, 0/5
total CPU         +1.481%, 0/5
```

All semantic, logical-write, pager, storage, M4.5, Q, and terminal cleanup
gates pass. V2 is genuinely grouped and materially repairs v1's result-row
cost, but it still cannot meet either 5% improvement gate and breaches the
independently protected memory ceiling. V2 is not retained and F4 remains
ineligible.

### Self-reflection C — v2 counter-first diagnosis

Predicted and observed counters agree before wall interpretation, so the v2
failure is not an implementation, counter, orchestration, identity, proof,
phase-policy, or storage bug. V1 showed that collecting one inserted-ID row
per object is expensive; v2 removes those `5,372` rows and most of v1's wall
regression, but replaces them with one mandatory prequery per group. The
remaining exact-classification choices under SQLite are therefore isolated:
inserted-row results or a pre-insert incumbent query.

One untested safe fast path remains. A plain INSERT without conflict handling
can succeed atomically for the all-absent common case and can use SQLite's
ABORT statement atomicity to enter the v2 authenticate-and-retry path only on
the object-ID uniqueness error. This is a new result/statement shape and a
new executable/campaign, not a rerun or reinterpretation of v2. Other
constraint failures remain errors. The formal v2 bounded-scan ceiling is
corrected to `10 * 64 * 64 = 40,960` ID comparisons on success/error paths;
this is still a fixed constant and changes no cap or Big-O result.

## F3-v3 prospective final repair — frozen before v3 source edits or build

Date: 2026-08-19. V1 and v2 roots remain immutable `FAIL / REVISE` evidence.
V3 retains row cap 64, canonical-byte cap 1,048,576, scoped maximum input
32,781, absolute Q cap 1,310,720, the F2-v3 control and retained fixture, the
complete AB/BA schedule, every correctness/resource/storage gate, and the
one-variable scope.

### One repaired statement/result variable

For the distinct first occurrences in one bounded group, v3 first executes:

```sql
INSERT INTO wp4m_objects
  (object_id, kind, canonical_length, canonical_bytes)
VALUES (?,?,?,?), ...
```

It has neither `ON CONFLICT` nor `RETURNING`. Success must report exactly the
number of submitted distinct rows; all first occurrences are created. A later
duplicate occurrence in the same group is authenticated against the newly
inserted incumbent before evidence, as in v2.

Only the exact object-ID primary-key/unique extended constraint error enables
fallback. SQLite ABORT semantics must leave the failed statement with zero
rows changed. Under the same active `BEGIN IMMEDIATE` transaction, v3 then:

1. runs v2's bounded distinct-ID incumbent query;
2. fully authenticates every returned kind, declared length, ObjectId,
   canonical bytes, and every covered occurrence;
3. inserts only absent distinct IDs using the bounded v2
   `ON CONFLICT(object_id) DO NOTHING` statement;
4. requires exact changed rows and authenticates any duplicate newly inserted
   incumbent; and
5. issues ordered evidence only after the entire fast or fallback path and
   all checked counter/Q arithmetic succeed.

Foreign-key, CHECK, trigger, datatype, not-null, generic/unknown constraint,
or nonconstraint SQLite errors never select fallback. Missing/malformed/
wrong-kind/wrong-length/unequal incumbents, changed-row mismatch, query or
retry failure, Q refusal, and arithmetic failure return the existing typed
error and roll back with no evidence. The group remains bound to one active
transaction identity.

This changes no canonical input, CDC/hash, object identity, mapping topology,
proof, root, transition, schema, transaction, COMMIT, durability, publication,
or post-COMMIT work. It adds no durable or unbounded state. All vectors, SQL,
parameters, fallback results, duplicate scans, canonical ownership, and
evidence remain bounded by the frozen caps. A conservative v3 success/error
scan ceiling is `16 * 64 * 64 = 65,536` ID comparisons per group; use a map
only if the fixed row cap changes and measurement proves this ceiling material.

### Fresh retained-fixture v3 counter prediction

The retained fixture has no incumbent or cross-group duplicate according to
both v1/v2 direct rows. Exact v3 predictions are:

```text
groups / optimistic INSERT executions                    103 / 103
object-ID constraint fallbacks                                  0
fallback prequeries / rows                                0 / 0
fallback retry INSERT executions / changed rows            0 / 0
post-insert duplicate-auth queries / rows                   0 / 0
returned INSERT rows/IDs                                        0
inputs / unique / created / reused           5,372 / 5,372 / 5,372 / 0
byte / row / proof / final flushes                 101 / 0 / 1 / 1
BLOB binds / writes                               10,744 / 10,744
mapping statement acquisitions / executes / queries       103 / 104 / 0
complete acquisitions                  10,863-5,372+103 = 5,594
complete execute/query calls              5,379-5,372+103 = 110 / 5,581
complete rows returned / changed                    10,780 / 5,373
PutEvidence / proof edges                           5,372 / 5,371
transactions / COMMITs                                    1 / 1
```

Direct counters must expose optimistic executions, exact object-ID
constraint fallbacks, fallback queries/rows, retry executions/changed rows,
postduplicate queries/rows, classifications, BLOB binds, flush causes, and
max rows/bytes. Group/fallback/counter equations are hard gates.

### Q, tests, release, and terminal rule

V3 reuses v2's group, query-result, evidence, SQL, and canonical capacities.
Fast and fallback SQL are sequential, not simultaneous. The conservative
`1,210,008` analytical sum and absolute `1,310,720` cap remain unchanged;
each allocation is pre-admitted, high-water is asserted, and every success or
injected failure must end at Q zero.

Before release, rerun the complete v2 matrix plus direct tests for: optimistic
all-absent success; exact object-ID constraint fallback with an existing row;
cross-group duplicate fallback; mixed existing/absent retry; within-group
duplicate handling; wrong-kind/length/malformed/unequal fallback incumbent;
non-object-ID constraint rejection without fallback; optimistic statement
atomicity; fallback prequery/retry failures; exact fast/fallback counters;
counter/Q refusal before SQL/evidence; proof order; rollback; one COMMIT and
ambiguous-COMMIT reconciliation; protected F2 and M4.5 behavior.

V3 artifact root is
`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v3`.
After all correctness/static/resource gates, freeze one new release candidate
and run exactly `AB/AB/BA/AB/BA/AB` against the original F2-v3 executable,
retaining every row. Mapping and durable capture must each improve at least
5% by arm and paired medians with at least 4/5 wins; all CPU/RSS/Q/pager/
storage/tiny-phase/M4.5 gates remain frozen.

If v3 repeats the v1/v2 performance blocker, no third exact-classification
primitive remains inside scope: SQLite must reveal inserted identities, reveal
incumbents before insertion, or abort and take the bounded fallback tested
here. The recurrence then closes F3 `FAIL / revert` without a fourth candidate,
without changing caps/gates, and without F4 eligibility. Any source change
after v3 rows instead creates a separately preregistered versioned campaign.

### F3-v3 self-reflection B — before release

Final pre-release v3 source SHA-256 is
`69ee5a615b143cb81c1227b747cf601370e096d482cfe0eb907e7f5903fd65ad`.
Only the private benchmark source and this milestone record differ from the
accepted checkpoint. The v2-to-v3 source change is limited to the private
group statement/classification path, direct counters/output, and focused
tests. Source/CDC/hash/canonical bytes and IDs, mapping topology, F2 proof,
roots, transition, schema, publication, COMMIT/reconciliation, durability,
and post-COMMIT paths are unchanged.

The formal v2 audit found that later same-ID occurrences were compared to an
incumbent by bytes but not by their separate kind descriptor. V3 now rejects
kind, canonical-length, or canonical-byte disagreement among all in-group
duplicates before SQL, and repeats complete per-occurrence agreement after
incumbent authentication. Evidence continues to use each checked occurrence
in original order.

The optimistic statement is explicit `INSERT OR ABORT`. Fallback accepts only
SQLite's exact PRIMARYKEY/UNIQUE extended codes for the schema's sole
object-ID key; trigger, NOT NULL, CHECK, foreign-key, datatype, and other
errors propagate. The failed cached statement leaves scope before fallback.
Fallback additionally requires the same live transaction identity and
`is_autocommit=false`, authenticates at least one incumbent, retries only the
bounded absent set, and requires the exact changed-row count. Direct tests
force the conflict at first, middle, and last VALUES positions and prove that
the failed optimistic statement leaves no partial rows.

No all-object/history map or new abstraction was added. Group input, unique/
absent/duplicate indices, SQL, parameters, query results, canonical ownership,
decoded incumbent, and evidence retain the frozen bounds. Actual INSERT BLOB
binds are now `2 * (optimistic bound rows + fallback-retry bound rows)` while
logical BLOB writes remain `2 * created`. Counter and Q headroom cover the
worst fast/fallback/postduplicate path before SQL/evidence.

Final pre-release validation on this exact source is:

```text
focused F3 tests                       7 passed
protected focused F2 tests            13 passed
workspace all-target tests            120 passed, 0 failed
  core/engine/private/parity/eval      44 / 4 / 55 / 12 / 5
Clippy -D warnings / fmt / diff        PASS / PASS / PASS
debug self-test                        PASS, accepted root, 20 objects,
                                       auth_bytes=1,054,925
debug schema/storage                   unchanged three tables; 20/1/1;
                                       DELETE/FULL; no journal/WAL/SHM
retained debug groups / optimistic     103 / 103
fallback/query/retry/RETURNING rows    0 / 0 / 0 / 0
mapping acquisitions/execute/query     103 / 104 / 0
created/reused/evidence/edges          5,372 / 0 / 5,372 / 5,371
canonical/mapping bytes                105,291,554 / 365,262
Q                                      <=1,310,720, terminal zero
```

The first compile caught an invalid direct comparison of transaction structs;
the minimal repair compares the already-bound identities. The first focused
run then exposed v2 counter/test assumptions and the fact that prequery fault
injection must first force a uniqueness fallback. Both were corrected before
full/static gates, no release binary existed, and no performance row existed.
Reflection B finds no remaining correctness, authority, proof, bound, static,
or protected-path blocker. The source may now be built once in release.

### F3-v3-r1 release-custody amendment — frozen before retry

The once-built v3 release executable SHA-256 is
`1828b55df0e9778cf3c9e40af6c13a4f6afc8ed59827600974ba20653a01bc18`.
The first M4.5 child under the preregistered v3 root was started but never
executed because the custody copy did not preserve executable mode. Its empty
stdout, permission-denied stderr, time record, partial preflight, failure
report, and 23-file manifest are frozen under
`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v3`; manifest
SHA-256 is
`b4578ae6e5e6c1b48468b1d9a681a456d8a655bd7d472f241ead658528de5a92`.
It contains zero JSON candidate rows and is neither performance nor semantic
evidence.

Before any retry, freeze a new artifact root:

`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v3-r1`.

V3-r1 copies the exact same source and executable bytes, explicitly applies
mode 0755 to both control and candidate custody copies, and reruns the complete
four-row release M4.5 regression followed by the complete asserted
`AB/AB/BA/AB/BA/AB` F3 campaign. No source/build/cap/query/statement/counter/
Q/schedule/gate change is permitted. Every v3-r1 started row is retained;
the v3 failure root is never modified or reused.

## F3-v3-r1 terminal result — FAIL / revert

The final measured root is
`target/wp4m-f3-bounded-cas-group-k64-r64-b1048576-20260819-v3-r1`.
Source/executable/raw SHA-256 values are:

```text
source      69ee5a615b143cb81c1227b747cf601370e096d482cfe0eb907e7f5903fd65ad
candidate   1828b55df0e9778cf3c9e40af6c13a4f6afc8ed59827600974ba20653a01bc18
raw         e840a62e68c34c879f25c810bd262f9157a1c9a02fe6f6472324a3f660fe8ce1
```

The mechanism matches the prospective prediction in every candidate row:

```text
groups / optimistic INSERT executions                 103 / 103
constraint fallbacks / fallback queries / retries       0 / 0 / 0
returned INSERT IDs / group query rows                    0 / 0
inputs / unique / created / reused        5,372 / 5,372 / 5,372 / 0
byte / row / proof / final flushes              101 / 0 / 1 / 1
bound rows / INSERT BLOB binds                 5,372 / 10,744
mapping acquisitions / execute / query             103 / 104 / 0
complete acquisitions / execute / query          5,594 / 110 / 5,581
complete rows returned / changed                 10,780 / 5,373
PutEvidence / proof edges                        5,372 / 5,371
maximum group rows / bytes                       57 / 1,048,309
Q high-water / terminal                          1,147,173 / 0
transactions / COMMITs                                 1 / 1
```

The candidate therefore proves the most favorable scoped SQLite grouping
shape: one statement, no incumbent query, no result row, and no fallback for
each bounded fresh group. It still fails both required wall gates and the
independent memory gates:

```text
mapping/CAS       489.054 -> 521.492 ms  +6.633% arm, +6.661% paired, 0/5
durable capture   653.849 -> 693.111 ms  +6.005% arm, +5.767% paired, 0/5
standalone COMMIT 166.676 -> 172.666 ms  +3.594% arm, +3.973% paired, 0/5
complete lifecycle 1,345.911 -> 1,385.098 ms +2.912%, 0/5
RSS / footprint   +5.430% / +5.429%, paired +5.578% / +5.579%, 0/5
total CPU         +2.941%, 0/5
```

All identities, canonical/mapping bytes, roots, transition, ordered closure,
proof counts, logical rows/writes, dirty writes 26,676, spills 6,675,
sampled journal allocation 20,480, FULL+DELETE, schema, final endpoints,
reconstruction/ranges, one transaction/COMMIT, Q, terminal cleanup, and the
release M4.5 regression pass. Measured M4.5 C0/C1 durable edit is
`432.047084 -> 8.506375 ms` with exact Q 2,222,803 and terminal zero. VFS,
xSync, true journal/temp peaks, and physical-media bytes remain Unavailable.

### Self-reflection C — counter-first interpretation

Predicted counters match exactly before wall interpretation. V3 removes both
v1's `5,372` inserted-ID result rows and v2's `103` prequeries/5,372 key probes,
yet is slower than F2 in all five mapping and durable pairs. This is not an
implementation, counter, orchestration, result-materialization, phase-policy,
identity, proof, pager, or storage mismatch. Pair 5's large COMMIT diagnostic
is retained rather than averaged away; the durable arm and paired medians fail
even without relying on that one pair.

The three immutable campaigns exhaust the exact SQLite classification choices
inside the fixed contract:

1. v1 obtains inserted identities from `RETURNING`;
2. v2 obtains incumbent identities before INSERT; and
3. v3 executes the no-result/no-query fast INSERT and uses statement-atomic
   uniqueness fallback only when necessary.

`OR IGNORE` plus a changed-row count cannot map created/reused occurrences on
conflict. A no-op `DO UPDATE RETURNING` changes immutable-row/trigger/write
semantics. Savepoints, hooks, temp markers/tables, sidecars, or per-operation
history either reduce to the measured three shapes or violate the one-variable,
schema, bounded-state, or durability contract. Larger groups worsen an already
failed RSS/footprint gate; smaller groups add crossings after a 98.083%
crossing reduction already failed wall. No cap or gate is tuned post hoc.

### Self-reflection D — terminal disposition

Formal read-only SQLite/counter and regression/code-quality audits find the v2
wrong-kind duplicate defect repaired twice: descriptor agreement is required
before SQL and per-occurrence kind/length/bytes are required after incumbent
authentication. The authority lane's earlier v2 semantic-PASS disagreement is
resolved by the concrete v2 evidence-carrying wrong-kind path and the v3 direct
new/existing wrong-kind tests. Benchmark/storage/custody evidence is evaluated
from immutable rows, not the working source.

This is the third consecutive performance/resource recurrence, and no safe
fourth exact-classification primitive remains. F3 is therefore fully closed
**FAIL / revert**: restore the accepted F2-v3 source byte-for-byte, preserve
all v1/v2/v3/v3-r1 evidence and additive reports, make no commit, and do not
claim F4 eligibility. F4, profile selection, production integration, backend
work, and any cap-tuning experiment are not started.

### Sealed terminal handoff

The deliberate source-only reverse patch restored
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` to accepted
F2-v3 SHA-256
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
HEAD/tree remain
`f7aff33dc46237ed06a94858c9a3b71bc02e82c8` /
`d54de4c2aeb87969cd9c9e2863e75b476a8c6886`. The benchmark source is absent
from final Git status; only this new milestone record and the two required
additive ledger/complexity updates remain. No commit was made.

Sealed evidence hashes are:

```text
v1 manifest / final audit
  3932031bd789807893908cf1b780db33e823765671b9544f6c24aaf2e627e5aa
  e993f7f3693d7e30954f76bec44b92db2d4ff7a7ecb1f022f2d590a5f7a9d1b4
v2 manifest / final audit
  5a6ad9b3c6447e1991ac6b36df2ef308d66726033f13365ce96c8521b1b825a5
  2d49a4d9180a118e10b6ffafba63f76c1879d6748191fba1b39afbc813e2491c
initial v3 orchestration-failure manifest
  b4578ae6e5e6c1b48468b1d9a681a456d8a655bd7d472f241ead658528de5a92
v3-r1 172-file manifest / final read-only audit
  0c71375e2daee6e4ac5bc8b44d9b583b88a9bdbc48e38be8e4eaf12da1d95c54
  49b40eb1ab31eb85251fda6dc75b23e3df4da0d7f30617a2fc6fbbedec06bca9
v3-r1 terminal report / independent-audit synthesis
  008996bfa2fdc92f9d4cd53dc743ba4a4291c9310c2d3245c2d94c0fd058fd40
  f3a3fe863af5539297d9bed1db6db626eac51e7eec1b2083c8e2a116a727c3b3
```

The final read-only audit verifies all 172 manifest entries, all historical
manifests, schedule/base/two-analyzer agreement, semantic authority, Q/pager/
storage/M4.5, restored F2 source, unchanged HEAD/tree, no commit, failed
mapping/durable/RSS/footprint gates, and `f4_eligible=false`. There is no
unresolved F3 midpoint.
