# WP4-P profile-promotion order versus canonical v2

Date: 2026-08-20  
Scope: evidence-backed decision only; no profile selection, format approval, or
implementation authority

Material claims are prefixed **Observed**, **Derived**, **Hypothesis**, or
**Unavailable** under the evidence discipline in the
[Phase 4 research index](../../index.md#evidence-discipline). A label on a
table caption applies to every value in that table unless a cell says
otherwise.

## 1. Executive disposition

**Derived — ordering decision.** WP4-P should not promote a permanent v1 K/F
profile before the proposed 36-byte canonical-v2 reference is accepted or
rejected. Reference width changes the only reason K59 is in the current
candidate set, changes SQLite overflow thresholds nonlinearly, changes the
profile-specific live frontier and authenticated leaf bytes, and introduces a
credible unmeasured v2 candidate at K113/F101. Therefore the terminal v1
campaign can remain valuable evidence, but it cannot by itself freeze the
permanent K/F constants if compact v2 remains open.

**Derived — scope of delay.** Delay compatibility promotion, final profile ID,
final goldens, and WP5's single-profile exit rerun. Do not delay collection and
audit of the already-started WP4-M v1 campaign, and do not discard its
profile-neutral evidence.

**Derived — topology-only allowance.** A width-neutral fixed-radix-versus-prolly
family conclusion may be recorded after WP4-M is terminal only if the fixed
ordinal suffix gate passes the approved 100-GiB work budget under both 68-byte
and 36-byte accounting. That conclusion is not WP4-P completion: it freezes no
K, F, mapping profile ID, codec, golden, root, or migration authority.

**Unavailable.** The terminal WP4-M manifest, final audit, candidate medians,
paired effects, protected-metric table, and 100-to-512-MiB suffix slopes do not
yet exist as authority. This report neither chooses nor predicts their winner.

## 2. Authority snapshot

| Label | Authority fact |
|---|---|
| **Observed** | Repository authority is branch `codex/empty-worktree` at campaign checkpoint `d781173a08ab4092eb539c3a0870056e6c6a77ff`. Tracked specifications were read from that commit whenever relevant; concurrent tracked source edits were excluded. |
| **Observed** | The explicitly assigned research sources are the research-only custody layer described by the [research index](../../index.md) and [decision map](../../decision-map.md). They grant no format or promotion authority. |
| **Observed** | The [`Complete WP4-M profile campaign`](codex://threads/01a01eb9-0d14-73c1-9a1f-e97687f9420b) task was `active`/`inProgress` at the last status-only check. Its latest command was no longer running when this report was written. |
| **Unavailable** | WP4-M has not published its terminal manifest and audit. No partial implementation, row, commentary, or provisional ranking from that task is evidence here. |
| **Observed** | WP4-M measures private candidates only. The committed [rollback plan](../../../../implementation-detail/phase-4/rollback/implementation-plan.md#9a-wp4-m--provisional-profile-measurement-lane) requires `qualification=false` and `purpose=profile_selection`; WP4-P alone may select one file K/F and one directory ceiling, delete losers/selectors, regenerate independent goldens, and obtain the final audit. |
| **Observed** | No public compatibility-bearing codec may begin before WP4-P, and WP5+ consumes only the single promoted profile, as specified by the [algorithm sequence](../../../../implementation-detail/phase-4/algorithm/spec.md#25-implementation-sequence). |

## 3. Verified reference and mapping-size equations

### 3.1 Framing constants

**Observed.** The committed mapping authority defines a Phase 1 Bytes outer
frame of `4 + 1 + 4 + 4 = 13` bytes, a mapping common header of
`8 + 2 + 1 = 11` bytes, and a file-leaf reference count of 4 bytes. The fixed
leaf overhead is therefore `13 + 11 + 4 = 28` bytes. The current reference is
`raw_chunk_id[32] + raw_length[4] + chunk_object_id[32] = 68` bytes; the
proposed compact reference is `raw_length[4] + chunk_object_id[32] = 36`
bytes. These operands come from the committed
[common framing and file-leaf grammar](../../../../implementation-detail/phase-4/mapping/logical-persistence.md#3-common-byte-conventions) and the proposed
[canonical-v2 direction](../../core/canonical/v2-single-identity.md#5-mapping-and-resource-effect).

**Derived.** If v2 retains the same outer Bytes frame, mapping header, and
4-byte count, the candidate equations are verified rather than assumed:

```text
Leaf_v1(K) = 13 + 11 + 4 + 68K = 28 + 68K
Leaf_v2(K) = 13 + 11 + 4 + 36K = 28 + 36K
```

**Derived — required checkpoints.** Every checkpoint agrees with those
operands:

| Checkpoint | Substitution | Complete canonical leaf bytes |
|---|---:|---:|
| v1 K59 | `28 + 68*59` | `4,040` |
| v1 K60 | `28 + 68*60` | `4,108` |
| v2 K59 | `28 + 36*59` | `2,152` |
| v2 K64 | `28 + 36*64` | `2,332` |
| v2 K113 | `28 + 36*113` | `4,096` |
| v2 K256 | `28 + 36*256` | `9,244` |

**Derived.** There is no checkpoint disagreement. The previously recorded v1
K64 value is likewise `28 + 68*64 = 4,380`. A future accepted v2 specification
that adds a new fixed field must replace 28 with its new verified overhead;
that possibility is one reason these are candidate, not compatibility,
equations.

### 3.2 Whole file-mapping equation

**Observed.** Branch and root descriptors remain
`cumulative_end[8] + child_object_id[32] = 40` bytes. A complete branch with
`f` children is `29 + 40f`; a root with `r` children is `49 + 40r`. Reference
width changes leaves, not those descriptors.

**Derived.** For reference count `C`, leaf capacity `K`, fanout `F`, reference
width `R`, `P = ceil(C/K)`, and successive branch counts
`B1 = ceil(P/F)`, `B2 = ceil(B1/F)`, ... until the root child count is at most
`F`:

```text
O(C,K,F)   = P + sum(Bi) + 1
M_R(C,K,F) = R*C + 68*P + 69*sum(Bi) + 49

M_v1 - M_v2 = (68 - 36)*C = 32*C       # when K/F are held fixed
```

**Derived.** The `68*P` term is 28 bytes of leaf envelope plus one 40-byte
parent descriptor per leaf. The `69*sum(Bi)` term is 29 bytes of branch
envelope plus one 40-byte parent descriptor per branch. The final 49 is the
root envelope. This is the width-parameterized form of the committed
[file-mapping equation](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md#7-file-mapping-construction-and-durable-space).

**Derived.** At retained `C=5,284`, K64/F64 file mapping is
`365,143 -> 196,055` bytes, a reduction of
`32*5,284 = 169,088`. The broader sealed research tally is
`365,262 -> 196,174`; it differs by the same width-neutral 119 bytes and has
the same 169,088-byte reduction. The numbers therefore have different stated
scope, not a reference-width disagreement.

## 4. K/F sensitivity at 100 MiB, 512 MiB, and 100 GiB

**Observed.** The sealed inputs are `C=5,284` for retained S1-100,
`C=27,162` for retained S1-512, and the analytical retained-density model
`C=5,410,816` for 100 GiB. The 100-GiB count is a model point, not a fabricated
run, as required by the
[mapping decision gate](../../../../implementation-detail/phase-4/mapping/logical-persistence.md#121-exact-object-counts-under-the-frozen-cdc-profile).

**Derived — profile-sensitivity table.** Cell notation is
`leaves P; branch-node counts; H; objects O; M_v1 -> M_v2`, where `H` is the
number of branch layers between root and leaf. Mapping-byte totals exclude
chunk objects and keep K/F fixed while only the reference width changes.

| Profile | Max leaf v1 -> v2 | 100 MiB | 512 MiB | 100 GiB retained-density |
|---|---:|---|---|---|
| K64/F64 | `4,380 -> 2,332` | `83; 2; H1; 86; 365,143 -> 196,055` | `425; 7; H1; 433; 1,876,448 -> 1,007,264` | `84,544; 1,321,21; H2; 85,887; 373,777,127 -> 200,631,015` |
| K59/F101 | `4,040 -> 2,152` | `90; none; H0; 91; 365,481 -> 196,393` | `461; 5; H1; 467; 1,878,758 -> 1,009,574` | `91,709; 909,9; H2; 92,628; 374,235,091 -> 201,088,979` |
| K256/F256 | `17,436 -> 9,244` | `21; none; H0; 22; 360,789 -> 191,701` | `107; none; H0; 108; 1,854,341 -> 985,157` | `21,136; 83; H1; 21,220; 369,378,512 -> 196,232,400` |

**Derived.** Holding a profile fixed leaves its leaf count, branch counts,
height, root child count, and object count unchanged. It reduces every leaf by
32 bytes per occurrence and therefore reduces complete-mapping bytes by
exactly `32C`. Consequently, a v1 ranking based only on topology counts would
transfer for the same three K/F pairs; a ranking affected by leaf bytes,
SQLite overflow, BLOB/page work, authenticated bytes, Q, or per-leaf scan work
need not transfer.

**Observed.** K59/F101 is in WP4-M because K59 is the largest v1 leaf not above
the complete-canonical-object 4-KiB line and F101 is the largest branch not
above it. The specification explicitly says this is not proof of SQLite page
residence or performance. K256/F256 represents fewer objects/SQL crossings;
K64/F64 is the retained locality default. These are the recorded
[candidate motivations](../../../../implementation-detail/phase-4/algorithm/spec.md#111-candidate-family).

**Derived.** Under v2, K59 is only 2,152 bytes, so its page-fit motivation
disappears. The corresponding exact complete-canonical-object fit is K113:
`Leaf_v2(113)=4,096`, while K114 is `4,132`.

**Derived — omitted-candidate sensitivity.** A v2 K113/F101 profile would have:

| Scale | Topology and exact v2 file-mapping bytes |
|---|---|
| 100 MiB | `47 leaves; no branch; H0; 48 objects; 193,469 bytes` |
| 512 MiB | `241 leaves; 3 branches; H1; 245 objects; 994,476 bytes` |
| 100 GiB retained-density | `47,884 leaves; 475,5 branches; H2; 48,365 objects; 198,078,657 bytes` |

**Hypothesis.** K113/F101 could dominate the old K59/F101 page-fit tradeoff by
roughly halving leaf/object counts at similar complete-canonical leaf size,
but no wall-time, CPU, SQLite, range, edit, Q-high-water, or physical-I/O result
exists for it. Page fit is a candidate-selection rationale, never a winner.

## 5. SQLite page/overflow sensitivity: model only

**Observed.** The committed SQLite object record contains rowid, 32-byte
object ID, kind, canonical length, and canonical bytes; see the
[SQLite BLOB record contract](../../../../implementation-detail/phase-4/storage/sqlite/spec.md#7-phase-4a--sqlite-blob-reference-baseline). SQLite stores an ordinary table row as a record-format payload. Its official
[database-file-format specification](https://www.sqlite.org/fileformat.html#cell_payload_overflow_pages)
defines, for usable page bytes `U` and table-leaf payload `P`:

```text
X = U - 35
M = floor((U - 12)*32/255) - 23
K_sqlite = M + ((P - M) mod (U - 4))
```

If `P <= X`, all payload may be local. If `P > X`, SQLite keeps
`K_sqlite` locally when `K_sqlite <= X`, otherwise only `M`; remaining bytes
use overflow pages of `U-4` payload bytes each. The same primary source defines
the record header, serial types, rowid alias, and BLOB encoding.

**Derived — explicit model assumptions.** Assume a 4,096-byte page, zero
reserved bytes, schema format 4, a Bytes-kind value encoded as integer 1, and
the committed ordinary rowid table. Then `X=4,061` and `M=489`. Around the
4-KiB boundary, the table-record payload for canonical BLOB length `L` is
`P=L+41`: 7 header bytes, 32 ObjectId body bytes, zero kind body bytes, two
canonical-length body bytes, and `L` BLOB bytes. K256 uses a three-byte BLOB
serial-type varint, so its modeled payload is `L+42`.

**Derived — table-leaf overflow sensitivity, not measurement.**

| Leaf | Canonical BLOB `L` | Modeled row payload `P` | Modeled local bytes | Modeled overflow |
|---|---:|---:|---:|---:|
| v1 K59 | `4,040` | `4,081` | `489` | `3,592`, one overflow page |
| v1 K64 | `4,380` | `4,421` | `489` | `3,932`, one overflow page |
| v1 K256 | `17,436` | `17,478` | `1,110` | `16,368`, four overflow pages |
| v2 K59 | `2,152` | `2,193` | `2,193` | none |
| v2 K64 | `2,332` | `2,373` | `2,373` | none |
| v2 K113 | `4,096` | `4,137` | `489` | `3,648`, one overflow page |
| v2 K256 | `9,244` | `9,286` | `1,102` | `8,184`, two overflow pages |

**Derived.** Under these assumptions, the complete-object 4-KiB boundary moves
from v1 K59 to v2 K113, while the stricter no-table-overflow boundary moves
from v1 K58 (`L=3,972`, `P=4,013`) to v2 K110 (`L=3,988`, `P=4,029`); v2 K111
has `P=4,065 > X` and spills. Thus even the meaning of “page fit” depends on
whether it means canonical object bytes or the SQLite table-cell payload.

**Unavailable.** This model does not establish B-tree co-tenancy, page splits,
index pages, cache behavior, journal bytes, APFS allocation, physical reads or
writes, CPU, or elapsed time. Those depend on actual page/reserve settings,
record values, rowids, free space, the unique ObjectId index, transaction
history, and the measured database. No profile may be selected from this
model.

## 6. Q/frontier, range authentication, COW, and directory effects

### 6.1 Q and construction frontiers

**Observed.** The accepted bounded construction proof charges one at-most-K
reference frontier and bounded F frontiers. With root child count `R_root`,
branch height `H`, and `L = H + 1 + 1[R_root == F]`, the committed v1 equation
is documented by the
[accepted F2-v3 complexity analysis](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md#242-exact-live-data-structures-and-bound):

```text
Q_proof_v1 = 4,096
             + K*68
             + L*(24 + F*40)
             + L*8
             + L*(24 + F*64)
             + 80
```

**Derived.** For the same implementation ownership and frontier structure,
compact v2 changes only the reference-frontier term:

```text
Q_proof_v2 = Q_proof_v1 - 32K
```

At retained 100 MiB this gives K64 `21,952 -> 19,904`, K59/F101
`18,748 -> 16,860`, and K256/F256 `48,264 -> 40,072` bytes. The same-K savings
are respectively 2,048, 1,888, and 8,192 bytes at every height.

**Derived.** More importantly, v2 K113 uses `113*36=4,068` live reference
bytes, only 56 more than v1 K59's `59*68=4,012`. With the same F101 and the same
height, its exact proof charge is therefore only 56 bytes higher:
`18,804` versus `18,748` at 100 MiB, `29,364` versus `29,308` at 512 MiB, and
`39,924` versus `39,868` at the 100-GiB model point.

**Unavailable.** Total v2 Q high-water is not derivable from this one frontier
equation. V2 rejoin records, canonical buffers, proof fields, SQLite bindings,
allocator capacities, and ownership lifetimes require an accepted design and
prospective counters. RSS remains independent of Q.

### 6.2 Range authentication and same-count COW

**Observed.** Under a valid receipt, a one-leaf range authenticates the root,
every branch on the path, one complete leaf, and selected complete chunks.
Same-count COW rewrites the touched leaf and the same ancestor spine. The
[algorithm and complexity authority](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md#8-range-read-complexity) protects both behaviors.

**Derived — maximum mapping-path bytes.** For a full selected leaf and actual
root child count, the same numbers bound mapping authentication for a one-leaf
range and mapping bytes rewritten by a one-leaf same-count edit; chunks and
namespace ancestors are excluded.

| Profile | 100 MiB v1 -> v2 | 512 MiB v1 -> v2 | 100 GiB v1 -> v2 |
|---|---:|---:|---:|
| K64/F64 | `7,098 -> 5,050` | `7,298 -> 5,250` | `10,447 -> 8,399` |
| K59/F101 | `7,689 -> 5,801` | `8,358 -> 6,470` | `12,587 -> 10,699` |
| K256/F256 | `18,325 -> 10,133` | `21,765 -> 13,573` | `31,074 -> 22,882` |
| v2 K113/F101 sensitivity | `n/a -> 6,025` | `n/a -> 8,334` | `n/a -> 12,483` |

**Derived.** Same-K/F height and mapping-object count do not change with width,
but authenticated/rewrite bytes do. Changing K changes bounded reference scans,
leaf count, height thresholds, object/SQL crossings, and the probability that
a local edit spans a leaf boundary. V2 also proposes removing the post-canonical
raw-payload hash on ranges and reconstruction; its exact read-side wall is
**Unavailable** in the sealed evidence.

**Observed.** Fixed ordinal count-changing edits remain worst-case
`O(suffix references)`; a width reduction does not change that asymptotic
topology. The current promotion gate requires measured 100/512-MiB slopes and
an approved absolute 100-GiB work budget, not only a local ratio; see
[count-changing edits](../../../../implementation-detail/phase-4/algorithm/spec.md#117-count-changing-middle-edits).

**Derived.** For the same K/F, rewritten occurrence and object counts transfer
from v1, while rewritten canonical bytes fall by 32 per rewritten reference.
Changing K changes the partition and object counts as well. A topology-family
decision is therefore width-neutral only when its budget is expressed in and
passes width-independent occurrences/objects plus the canonical-byte budget
under both widths.

### 6.3 Directory ceilings

**Observed.** A directory entry serializes name length/name, one-byte child
kind, and one 32-byte child NodeId. It contains no file-reference record. The
committed directory equations therefore remain:

| Complete directory-page ceiling | Max pages | Max index bytes | Max mapping objects | Max same-size child rewrite |
|---:|---:|---:|---:|---:|
| 64 KiB | `447` | `131,003` | `450` | `196,628` bytes / 3 objects |
| 256 KiB | `112` | `32,848` | `115` | `295,081` bytes / 3 objects |
| 1 MiB | `28` | `8,236` | `31` | `1,056,901` bytes / 3 objects |

The operands and greedy partition rule are in the
[directory COW model](../../../../implementation-detail/phase-4/mapping/logical-persistence.md#125-cow-amplification).

**Derived.** Directory page sizes, entry counts, index widths, and ceiling
tradeoffs are structurally independent of changing a file reference from 68 to
36 bytes because a file child remains a fixed 32-byte NodeId. They are not
identity-independent: v2 changes each file NodeId, which changes the enclosing
directory-page ObjectId, index ID, wrapper/root ID, transition/delta, receipt,
and all related goldens even though serialized lengths are unchanged.

**Hypothesis.** An independently isolated terminal directory-ceiling ranking
may remain strong comparative evidence under v2 because the byte widths and
operation topology are unchanged. It still needs final-v2 identity/correctness
confirmation and cannot make v1 directory roots or goldens compatibility
authority.

## 7. Which K/F motivations depend on reference width

| Label | Motivation | Width sensitivity |
|---|---|---|
| **Observed / Derived** | Complete-canonical 4-KiB leaf | Directly width-dependent: v1 K59 becomes v2 K113. Branch F101 is unchanged because branch descriptors remain 40 bytes. |
| **Derived / Hypothesis** | SQLite local/overflow payload | Directly and nonlinearly width-dependent. The model changes v1 K64 from one overflow page to no overflow under v2 and K256 from four to two; performance effect is unavailable. |
| **Observed / Derived** | Object/SQL crossing count | K/F-dependent, not directly R-dependent for fixed K/F; indirectly width-sensitive because the justified K candidate can change. |
| **Observed / Derived** | Range and same-count locality | Topology/path object count is unchanged for fixed K/F; complete leaf authentication/rewrite bytes and any raw-rehash work change with v2. |
| **Observed / Derived** | Q/frontier | Directly width-dependent through `K*R`; higher K can consume the saving while reducing object counts. |
| **Observed / Derived** | Count-changing suffix behavior | Fixed-radix `O(suffix)` class is width-independent; rewritten bytes and partitions are width-dependent. |
| **Observed / Derived** | Directory ceiling | Serialized sizes/topology are independent; child/root identities and goldens are dependent. |
| **Observed** | CDC boundaries, raw source, and canonical chunk objects | Independent if v2 retains the frozen 8/16/32-KiB CDC and Phase 1 canonical Bytes objects. |

## 8. Dependency graph and ordering

**Observed / Derived.** The committed work-package order plus the open v2
authority yields this decision graph:

```text
WP4-M v1 candidate campaign
  -> terminal manifest + terminal audit (currently unavailable)
  -> reusable v1/profile-neutral evidence
                         \
                          +--> canonical-v2 authority decision
                                |
                 +--------------+----------------+
                 |                               |
             v2 rejected                     v2 accepted
                 |                               |
      apply terminal v1 predicate      define accepted v2 profile grammar
                 |                     and fair v2 candidate evidence
                 |                               |
                 +--------------+----------------+
                                v
                 WP4-P selects exactly one profile
                   -> delete losers and selector
                   -> freeze one production profile ID
                   -> regenerate independent final goldens
                   -> final read-only audit
                                |
                                v
                 WP5 single-profile exit rerun/finalization
                                |
                                v
                           WP6+ / WP14
```

**Derived.** Promoting v1 before the v2 branch resolves inserts another
compatibility-bearing state between WP4-M and canonical v2. That state is not
needed to complete the already-running measurements and is avoidable if the
decision is delayed.

## 9. Reusability of WP4-M evidence under each ordering

**Observed / Derived.** Reusability is evidence-class-specific; “reusable” does
not mean “may be relabeled v2.”

| Evidence class | If v2 is rejected after waiting | If v2 is accepted after waiting | If v1 is promoted first |
|---|---|---|---|
| Terminal manifest/audit and custody | Direct authority for the v1 selection predicate | Reusable custody/history; not a v2 winner table | Direct v1 authority, but must remain sealed historical evidence after migration |
| Raw fixture fingerprints and source bytes | Reusable | Reusable outside product authority | Reusable |
| Frozen CDC boundaries, counts, and raw/canonical chunk IDs | Reusable | Reusable if v2 keeps Phase 1 Bytes and CDC; v2 ordered commitment is separately named/versioned | Reusable as historical input, not proof that migrated roots match |
| Canonical chunk BLOBs | Reusable | Reusable by canonical ObjectId; mapping/root objects change | Reusable payload objects may survive, while mapping/history authority still migrates |
| K/F topology and boundary correctness for the same K/F | Directly reusable | Structurally reusable after v2 codec adaptation; bytes/IDs/goldens are not | V1-compatible only; later v2 still needs its own codec/identity proof |
| V1 file-profile wall/CPU/SQLite/Q rankings | Direct selection input after terminal audit | Historical prior/calibration only; width, overflow, candidate set, Q, and protected paths changed | Selects v1, then a second campaign may be required for v2 |
| Directory-ceiling structural equations | Directly reusable | Reusable because entry widths are unchanged | Reusable structurally; all identity-derived vectors still change |
| Isolated terminal directory performance ranking | Direct selection input | Potentially reusable comparative evidence if audit proves isolation and equal work; final v2 correctness/identity confirmation remains required | Selects v1 directory capacity but cannot preserve v1 roots/goldens through v2 |
| Workload definitions, timer boundary, AB/BA schedule, counters, custody procedure | Reusable | Reusable and should remain profile-neutral | Reusable, but rerunning them increases campaign cost |
| V1 roots, deltas, receipts, profile IDs, final goldens | Reusable only if v2 is rejected | Not reusable | Created once for v1 and again for v2 |

## 10. Double-promotion, migration, and authority risks

**Derived.** Even if the same numerical K/F wins both formats, v2 changes every
file leaf containing a reference and therefore every affected branch, file
NodeId, directory ancestor, workspace root, transition/delta, receipt binding,
and root/closure golden. A second golden regeneration is unavoidable.

**Observed / Derived.** The current contract binds one exact
`mapping_profile_id` and exposes only one promoted profile. A v1-first then
v2 path must either eagerly rewrite mapping/root/history or authorize a dual
reader/new writer with cross-profile parent-child delta rules, retained v1
raw-ID validation, downgrade rejection, history/GC policy, and exact error
semantics. The accepted
[v2 compatibility analysis](../../core/canonical/v2-single-identity.md#7-compatibility-and-migration-blast-radius)
describes this blast radius; Phase 4 currently authorizes no general migration
system.

**Derived.** V1-first creates avoidable work in four places:

1. final v1 profile ID, independent goldens, fingerprint, and audit;
2. WP5 frozen-format finalization against v1;
3. migration/dual-profile authority needed only because v1 became public; and
4. a second K/F selection campaign because K113/F101 and changed overflow/Q
   tradeoffs were absent from WP4-M.

**Hypothesis.** A short-lived v1 promotion could be justified only by an
external release deadline whose value exceeds those costs and whose product
requires a compatibility-bearing v1 before the v2 decision. No such deadline
or product requirement appears in the governing sources.

## 11. Criteria after the WP4-M terminal report

### 11.1 Terminal-evidence admission

**Observed.** No candidate enters a decision predicate unless the terminal
manifest and independent audit validate complete required rows, identities,
one transaction/COMMIT, ranges, scrub/reconstruction, Q terminal zero, storage
observations, and custody. Missing observations make the outcome inconclusive;
partial rows never enter.

### 11.2 Exact v1 predicate if canonical v2 is rejected

**Observed / Derived.** Let `d` be the current default (K64/F64 for files,
256 KiB for directories), `c` a challenger, `m_x(c,s)` the audited matched
median for metric `x` at size `s`, and smaller be better for the predeclared
wall-time primaries. A challenger is eligible only when:

```text
terminal_manifest_and_audit_PASS
AND m_primary(c,100MiB) <= 0.95 * m_primary(d,100MiB)
AND paired_primary_wins(c,d) >= 4 of 5
AND for every protected median p at every applicable measured size s:
      m_p(c,s) <= 1.05 * m_p(d,s)
AND every required observation is Available
AND no cross-size reversal makes the ranking unclear
AND no removable per-row SQL crossing explains or could reverse the result
AND fixed-ordinal local +1 gate, 100-to-512 slope, and approved
    100-GiB occurrences/objects/bytes budget all pass
```

If no file challenger satisfies the complete predicate, retain K64/F64; if no
directory challenger satisfies its complete predicate, retain 256 KiB. If a
removable SQL crossing could reverse a family, only the preregistered smallest
equal-work sensitivity result may resolve it; otherwise the default remains.
These are the committed
[profile-selection rules](../../../../implementation-detail/phase-4/algorithm/spec.md#21-candidate-profile-selection), not a winner prediction.

### 11.3 Predicate if canonical v2 is accepted

**Derived.** The permanent K/F predicate is false on the v1 table alone:

```text
permanent_KF_selectable_from_WP4M_v1 = false
```

Selection requires a terminal, equal-work v2 table under one accepted v2
grammar and ordered-commitment authority. Its candidate set must include or
explicitly disposition the v2 page-fit sensitivity K113/F101, and the same
5%/4-of-5/protected-median/cross-size/SQL/suffix/Q/RSS criteria must be applied.
V1 rows remain priors and regression references, not paired v2 operands.

### 11.4 Predicate while the v2 decision remains open

**Derived.** WP4-P remains held:

```text
promote_compatibility_profile = false
```

A topology-family-only conclusion is allowed only when:

```text
WP4M_terminal_audit_PASS
AND fixed-ordinal correctness and measured suffix-slope gates pass
AND the approved 100-GiB work budget passes for:
      rewritten occurrences and objects                 # width-neutral
      canonical bytes under R=68 and under R=36         # width-sensitive
AND no unresolved v2 authority question changes grouping semantics
```

If that predicate is false or the result differs by width, even the topology
family remains unresolved. If true, it records only that fixed radix remains
admissible; it does not select K/F or complete WP4-P.

## 12. Profile-neutral work and fair comparison

**Observed.** The [full-create plan](../../../../implementation-detail/phase-4/wp4m/f-series/planning/full-create-plan.md#16-after-f6)
requires every optimization present during selection to be profile-neutral or
applied equivalently to every profile. The accepted one-source CDC/CAS/COW
pipeline, proof authority, one FULL+DELETE transaction/COMMIT, timer boundary,
fixture preparation, balanced release schedule, and correctness/resource gates
remain the common comparison substrate.

**Derived.** If v2 is accepted, every compared K/F must use the same accepted
36-byte reference grammar, ordered commitment, error semantics, Q accounting,
SQLite schema/profile, durability mode, build, source, base image, and
verification work. Comparing an optimized v2 default against v1 challengers,
or a 68-byte same-width bridge against compact candidates, would change more
than K/F and cannot select the permanent capacity.

**Observed.** Profile-neutral optimizations and evidence that remain valid
include removal of the duplicate pre-COMMIT closure replay, bounded F2 proof
folding, exact CDC boundary behavior, canonical CAS authentication, transaction
and reconciliation semantics, workload/timer definitions, and custody
procedures. Candidate-dependent codec bytes, goldens, root identities, leaf
Q, page/overflow layout, and K/F-specific range/edit/storage rows must not be
silently transferred.

**Unavailable.** No prospective compact-v2 end-to-end performance, protected
metric, SQLite allocation, Q/RSS, or paired candidate result exists. The
[pipeline research](../../core/pipeline/full-create-pipeline.md) supplies only
component ceilings and direction, not v2 acceptance or a K/F ranking.

## 13. Exact next handoff

**Derived.** This report hands off one decision boundary, not implementation:

1. Receive the sealed WP4-M terminal manifest, terminal report, and independent
   audit without using superseded or partial rows.
2. Receive the canonical-v2 authority outcome: accepted or rejected, including
   its witness/error/migration decision; “still researching” is neither.
3. If v2 is rejected, apply section 11.2 once and hand the resulting single
   profile to WP4-P.
4. If v2 is accepted, treat WP4-M file rows as v1 evidence, require the
   section 11.3 v2 capacity decision, and only then hand one profile to WP4-P.
5. If v2 remains open, keep WP4-P, final goldens, production profile ID, and
   WP5 finalization pending. A section 11.4 topology-only record may proceed
   without compatibility effects.

**Observed.** After one profile reaches WP4-P, the existing boundary remains:
delete losers and the private selector, freeze one production profile ID,
regenerate independent final goldens/fingerprint, obtain the final read-only
audit, and then rerun WP5's single-profile exit. No step above chooses the
active WP4-M winner or authorizes durable v2 bytes.

## 14. Limitations

**Unavailable.** This report has no terminal WP4-M values and deliberately
contains no inference from its partial campaign.

**Unavailable.** The SQLite section is a deterministic file-format model under
stated 4-KiB assumptions, not a database observation or performance result.

**Unavailable.** K113/F101 has arithmetic/topology evidence only. Its semantic,
Q-high-water, SQLite, range, COW, and wall-time evidence requires a future
accepted-v2 decision path.

**Unavailable.** Canonical v2 itself is neither accepted nor rejected here.
The ordered witness equivalence, exact error mapping, migration authority, and
end-to-end performance remain separate gates documented by the
[canonical-v2 research](../../core/canonical/v2-single-identity.md).

DELAY_WP4_P_FOR_V2
