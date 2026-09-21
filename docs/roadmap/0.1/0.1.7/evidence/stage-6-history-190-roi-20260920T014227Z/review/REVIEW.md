# Independent review: one remaining-read attribution step

Status: profile, candidate source, matched diagnostics and final check logs
reviewed. The profile alone establishes no speedup. The reviewer changed only
this document and performed no builds, tests, profiling runs or product edits.

## Attribution arithmetic

The reviewer independently parsed `profile/sample.txt`, reconstructed ancestry
from the call-tree indentation and derived each node's self count as inclusive
count minus its immediate children's inclusive counts. Every self count was
nonnegative. Their sum is **7,307 main-thread stack samples**; the subset with
`StoreProvider` in its ancestry is **1,759**. The independent category totals
match `analyze_profile.py` and `profile-derived.json` exactly:

| Disjoint provider category | Self samples |
| --- | ---: |
| Pack preparation | 43 |
| Pack execution/copy/other | 550 |
| Catalogue preparation | 204 |
| Catalogue execution/decode/other | 420 |
| Value decompression | 32 |
| Value authentication | 53 |
| Value decode/allocation/other | 59 |
| Physical record decoding/other | 41 |
| Leaf reconstruction/other | 44 |
| Other provider work | 313 |
| **Total** | **1,759** |

The classifier assigns pack and catalogue ancestry before value/physical/leaf
ancestry, so nested work is charged only once. The **144** value samples exclude
pack acquisition beneath value loading; this is a disjoint category, not the
complete cost of materializing values. The **313** provider-other samples remain
explicitly unassigned to more specific mechanisms.

## Catalogue teardown is mostly not avoidable preparation

The reviewer further split all **624** catalogue samples with mutually exclusive
ancestry predicates. Preparation is assigned first, then Statement/RawStatement
finalization/drop, then reset, execution and row decoding:

| Catalogue subcategory | Self samples | Expected effect of statement reuse |
| --- | ---: | --- |
| SQL preparation/parser/code generation | 204 | Eligible for reuse on cache hits |
| Statement finalization/drop | 12 | Largely deferred to eviction/connection closure, not abolished |
| Statement reset / Rows teardown | 87 | Still required; must not count as saved preparation |
| Query execution, including lock/pager work | 312 | Still required |
| Row decoding | 5 | Still required |
| Other | 4 | Unknown |
| **Total** | **624** | |

Of the 12 finalization/drop samples, **11** have explicit SQLite finalization
ancestry. The remaining **one**, at sample report line **3203**, is the Rust
RawStatement drop path beneath Statement drop. Examples of explicit finalization
are lines **1576**, **1579** and **2113**. Preparation parser/code-generation
examples occur at lines **185**, **186** and **643**. Reset examples at lines
**615**, **624** and **629** occur beneath Rows teardown and are not equivalent
to recompiling the statement.

Thus the source-supported reuse opportunity is associated with **204 + 12 =
216 samples**, not all 624 catalogue samples or all 420 non-preparation samples.
This is `216 / 1759` of provider observations and `216 / 7307` of main-thread
observations. These fractions are neither predicted elapsed savings nor upper
bounds with a statistical confidence interval. They include sampling and
symbolization limitations; cache lookup/return and eventual finalization also
cost work.

## One justified local experiment

Change only `core/crates/layerfs-storage/src/sqlite/pool.rs::group_for` to obtain
its existing fixed SQL through the connection's existing `prepare_cached`, then
execute the same query, decode the same row and preserve every existing error
mapping and range check. The samples directly locate repeated preparation here,
and the code currently calls uncached `Connection::query_row`.

This is a defensible small experiment because:

- The exact same connection already uses its bounded prepared-statement cache
  for locator SQL in `sqlite/lookup.rs::locations`.
- It needs no new cache object, capacity, lifetime, publication rule, permanent
  timer or counter, and no dependency change.
- The SQL text, bound ordinal, result decoding, missing-row handling and metadata
  range validation can remain identical.
- Locked rusqlite **0.40.2** returns `CachedStatement` to the existing cache on
  drop and clears its bindings on cache return (`src/cache.rs` in the published
  package). The result row is still read and the cursor still reset. Reuse is
  not a cached catalogue *result*: rows published between calls must remain
  visible.

Do not widen this experiment to `pack_bytes` or pooled value/pack retention.
Pack preparation has only **43** observed provider samples; its **550** other
samples do not establish that preparation is its dominant cost. A larger cache
or read-transaction lifetime change would require a different contract review.

The one-helper candidate must pass missing-row/invalid-ordinal/range/corruption
tests, between-call catalogue visibility, and existing bounded read/refusal
tests. A small public test that first queries a missing ordinal, inserts its
valid catalogue row, then queries it again on the same connection is useful:
it detects accidentally caching absence while reusing the SQL statement.

Accept it only if the frozen unprofiled matched pair shows a useful improvement
with unchanged roots, Store bytes and error behavior. If the effect is absent or
ambiguous, preserve and report the failed or inconclusive attempt; do not expand
the scope or repeat samples for a better outcome. Confirm stride3 only under the
campaign's stated continuation criterion. Profile counts alone are not enough
to claim success.

## Protocol and interpretation limits

`profile/profile-receipt.json` identifies child **90748**, `/usr/bin/sample`
with a requested 120 seconds at **5 ms**, the immutable prior candidate binary,
and exit codes zero for child and sampler. The entire profiled command took
**42,414,263,625 ns**, below its declared 120-second diagnostic limit. The child
wall was **42,339,847,333 ns**. These are profiled timings and must not become a
baseline in the unprofiled optimization pair.

The sampler launch gap is **4,679,541 ns**. The textual report timestamp is
**88 ms** after its reported process launch timestamp, but the actual first
sample gap is recorded as unknown. Do not substitute either observed timestamp
for an exact first-sample boundary.

The report contains one main-thread tree, with **7,307** samples. A 5 ms nominal
interval does not justify multiplying sample counts by 5 ms and calling the
result measured CPU or phase time. Stack suspension overhead, interval jitter,
blocked syscalls, inlining and missed frames limit that inference. For example,
catalogue query execution includes `stat`, `fcntl` and pager calls; those are
observed stack residence, not isolated SQL CPU. Provider ancestry may fail to
identify some inlined work, which stays outside this explicitly selected subset.

All observations remain diagnostic with uncontrolled cache residency and no
admission claim. No precise amount of the historical v0.1.6 timing gap is
attributed by this profile. This review supports trying the one-helper statement
reuse; it does not support further architectural optimization.

Reviewer production LOC delta: **0**; no commit or source modification made.

## Frozen candidate source and test review

Reviewed the frozen candidate while root owns the unprofiled measurement. No
blocking source finding. The sole production treatment is in
`core/crates/layerfs-storage/src/sqlite/pool.rs::group_for`: acquire the same
fixed SQL with `prepare_cached`, then call the statement's `query_row` with the
same ordinal and `decode_group_row` callback. The diff is seven physical lines
inserted and eight removed; this is not a substitute for production LOC.

Invalid ordinals are still refused before SQL. Missing rows still return `None`;
query/decode engine errors still become `StorageError::Engine`; catalogue-row
decoding and ordinal-range checks remain untouched. Preparation errors now use
the existing `From<rusqlite::Error>` implementation, which maps to the same
`Engine` variant as the prior query helper's non-missing error branch. The
prepared statement drops on success and every error exit.

Read-only inspection of locked rusqlite **0.40.2** confirms its default statement
cache capacity is **16 entries** (`src/lib.rs:168`). Product source has no cache
capacity override. `CachedStatement::drop` returns the statement to that existing
cache; return clears bindings. No cache/result lifetime, policy, dependency or
format is expanded. Capacity is an entry bound, not a new byte-memory claim.
The architecture note accurately describes this source change.

The new external public-API fixture
`core/crates/layerfs-storage/tests/catalogue_statement_reuse.rs` registers a
SQLite authorizer once after setup. It counts `AuthAction::Select`, a statement
compilation event (including any automatic re-preparation), not query execution.
It verifies the following using the same connection and helper:

1. A first catalogue lookup returns the expected row.
2. A changed digest is immediately visible when a different ordinal selects that
   same row.
3. An ordinal in the next row returns that different row, demonstrating new
   bindings rather than stale parameters.
4. An uncovered ordinal returns the original range error.
5. Deleting catalogue contents makes the next lookup return `None`.
6. Reinserting a valid row after that absence makes it visible on the next call.

The invalid zero ordinal is separately refused without another preparation.
Candidate assertions require one SELECT preparation across the six SQL helper
queries. This directly tests the intended work reduction and guards against
mistakenly caching row contents or absence. Baseline/candidate execution receipts
remain root's evidence; the reviewer did not execute the fixture. Query reuse
can compete with other prepared statements for the existing 16 slots, so the
matched workload remains necessary to determine its practical net benefit.

## Unprofiled stride10: modest operation improvement, wall regression

Independent reads of the two raw timing trees, immutable performance traces,
phase files and complete-command receipts reproduce:

| Quantity | Baseline ns | Candidate ns | Candidate minus baseline ns |
| --- | ---: | ---: | ---: |
| Sum of 17 operation children | 23,491,957,417 | 22,180,444,124 | −1,311,513,293 |
| Provider elapsed, nested in operation | 9,596,396,756 | 8,437,296,469 | −1,159,100,287 |
| Invocation CPU user + system | 33,510,728,000 | 32,304,434,000 | −1,206,294,000 |
| Root timing span | 38,988,364,542 | 37,888,574,917 | −1,099,789,625 |
| Complete-command wall | 39,267,932,708 | 39,909,975,875 | **+642,043,167** |
| Complete wall minus operation | 15,775,975,291 | 17,729,531,751 | **+1,953,556,460** |

Operation reduction is **5.582818279974068%** for this one diagnostic pair. This
is a modest observed improvement, not a stable expected percentage or isolated
SQL CPU attribution. **There is no stride10 end-to-end wall win.** The greater
outside-operation wall more than offsets the smaller operation. The external
wall residual includes more than corpus reading, so it must not be labeled
entirely as corpus-read time. Its growth is not causally attributed here.

All 17 state roots and saved-counter maps match. All recorded provider work
counters match except elapsed time, including the pooled physical/value work,
pack bytes, waves and objects. SQL preparation reduction is established by the
separate public-authorizer fixture, not a new history work counter. Both Store
receipts name the same SHA-256; direct large-file hashing subsequently confirmed
it in the quiet window recorded below.

Internal verification is **6,237,682,875 → 5,489,793,375 ns**; complete verification
commands are **6,268,105,792 → 5,511,295,791 ns**. Both satisfy the 10-second work
target and 60-second hard budget. Full stride3 confirmation and final artifact
reconciliation are recorded below; no further optimization is recommended or authorized
by this review.

## Unprofiled stride3 confirmation

Independent reconstruction from the 53 raw state children and provider trace
counters gives:

| Quantity | Baseline ns | Candidate ns | Candidate minus baseline ns |
| --- | ---: | ---: | ---: |
| Sum of 53 operation children | 64,870,176,420 | 59,039,478,665 | −5,830,697,755 |
| Provider elapsed, nested in operation | 36,676,151,222 | 31,155,004,681 | −5,521,146,541 |
| Invocation CPU user + system | 77,940,045,000 | 71,847,405,000 | −6,092,640,000 |
| Root timing span | 87,968,203,917 | 81,432,593,750 | −6,535,610,167 |
| Complete-command wall | 88,637,897,000 | 82,042,953,250 | −6,594,943,750 |

The operation reduction is **8.988256355662305%** for this diagnostic pair. Here
the complete command also improves, with outside-operation wall decreasing by
**764,245,995 ns**. This does not erase the stride10 wall regression.

All 53 state roots and saved-counter maps match. Per-state comparison across
both selections confirms **every** provider work counter matches between arms
except elapsed time; this is stronger than equality of aggregate totals.
Product identity maps differ only at `sqlite/pool.rs`. Harness-file and lockfile
identity maps are identical. Direct Store/binary hashes and final verification
status are recorded below.

Focused execution logs, read but not rerun by the reviewer, record **five SELECT
preparations for five baseline queries** and **one preparation for six candidate
queries**, including the additional insertion-after-absence check. Both fixture
executions pass. The correct common-query comparison is five versus one for the
first five queries; the candidate's sixth query demonstrates continued reuse and
fresh results rather than a different performance workload.

The evidence supports retaining this small local change and stopping the
optimization round. It does not justify widening cache ownership or starting a
second optimization. The gain is modest, based on one sample per arm and
selection, with uncontrolled residency and disclosed desktop activity.

## Final verification receipts and artifact hashes

All four unprofiled verification invocations completed with exit code zero and
zero sampled mismatches:

| Selection / arm | Internal verification ns | Complete verification command ns | Work target |
| --- | ---: | ---: | --- |
| Stride10 baseline | 6,237,682,875 | 6,268,105,792 | PASS ≤10 s |
| Stride10 candidate | 5,489,793,375 | 5,511,295,791 | PASS ≤10 s |
| Stride3 baseline | 22,686,258,458 | 22,708,118,125 | **TARGET_MISS >20 s** |
| Stride3 candidate | 18,912,983,625 | 18,962,225,667 | PASS ≤20 s |

All remain below the 60-second complete verification hard budget. Every trace
still records history O3 counters **INCOMPLETE**, and the receipt cache/admission
status remains **INELIGIBLE**. Sampled verification success does not qualify a
cold or release timing claim.

The first requested direct-hash pass was **DEFERRED without reading any large
file**: the two shared flocks were acquired, then the harness's `O_EXCL` lock
refused an existing empty `.measurement.lock`. Context exit released the shared
flocks. The reviewer did not delete or replace the file and reported the refusal
to root. Read-only inspection found distinct, non-symlink global lock paths and
harness lock inode `824264505`, size zero, mtime `1789869826`; there was no path
alias created by the reviewer's two global `open("a")` calls. Recovery, if any,
belongs to the coordinator's preserved evidence.

Root preserved the empty file and recorded its recovery in
`checks/stale-lock-recovery.json` after proving both shared flocks free and no
open owner. Its origin remains **UNKNOWN**. On the expressly authorized retry,
the reviewer acquired all three locks, independently streamed SHA-256 over the
two executables and five Stores, and released the locks before root began full
workspace checks. All hashes match their identity/performance/verification
receipts:

| Artifact | SHA-256 |
| --- | --- |
| Baseline executable, reused from prior immutable archive | `0cfa938be11a61625da8fd4ef4306657860d6e9e548986983b9f9b3e34b375f8` |
| Candidate executable | `82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11` |
| Profiled Store and both unprofiled stride10 Stores | `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f` |
| Both unprofiled stride3 Stores | `cfb74bc9b1db6c9470129613283a8f4afa3b139c2819ae7bd7a4f14e548ecdb5` |

The profiled Store's identical bytes corroborate unchanged output, not comparable
performance. Its timings remain excluded from the unprofiled pair. Final workspace
check-log review is recorded below; the reviewer does not claim to have executed
those checks.

## Final validation-log review and disposition

The reviewer read each retained command receipt and log without rerunning it:

- Locked core test log contains **494** passing tests across its result lines;
  command exit code zero.
- Locked harness test log contains **117** passing tests; command exit code zero.
- Locked core example-target test command exits zero; these targets contain zero
  runnable example tests, so this is a build/target check, not extra test cases.
- Core all-target warning-denying Clippy and core format check both exit zero.
- Boundary guard reports **122** production Rust/SQL files and PASS. Its Python
  self-test log reports **six** passing tests.
- Final source-check receipt states candidate compilation inputs still match the
  measured candidate. The independently compared measured identity maps already
  show identical harness/locks and one product difference, `sqlite/pool.rs`.

Harness Clippy and formatting were **NOT_RUN again**. The final report correctly
links the unchanged harness's prior 22 Clippy errors and format failure instead
of claiming passes. Those remain unresolved. Nothing here claims CI or the
retired aggregate preflight ran.

The reported production counter result is **85,723 → 85,722 (delta −1)**, split
as unchanged reference **65,417** and core **20,306 → 20,305**. This agrees with
the one-helper treatment's direction and the retained counter receipt. The
reviewer did not execute a new staged-tree LOC comparison; the committing agent
must record the exact first-parent comparison as required.

**Final recommendation: retain this one local prepared-statement reuse and stop
the optimization round.** Source review found no blocking correctness issue;
the external authorizer fixture proves reduced preparation with fresh results;
the two single-sample operation comparisons improve, with byte-identical saved
results and unchanged recorded read work. The stride10 complete-wall regression,
baseline stride3 verification target miss, larger candidate lifetime RSS, unknown
empty-lock origin, uncontrolled cache residency, history O3 gap and unmatched
historical time tripwire remain part of this recommendation. No further cache,
transaction, format, tree-summary or streaming change is justified by this review.
