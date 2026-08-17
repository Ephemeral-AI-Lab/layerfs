# Phase 4 append-only first-implementation findings

Status: **exploratory; Phase 4A SQLite remains authoritative**
Date: 2026-08-17
Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
Branch: `codex/empty-worktree`

## Final same-source proxy evidence retained for rollback

The later same-source campaign ran five measured rows per lane on the exact
100 MiB proxy source. The append-only median was 3,164,894,458 ns or
31.596630 MiB/s. The deliberately conservative SQLite-control median was
2,833,641,750 ns or 35.290276 MiB/s, making append-only 331,252,708 ns or
11.69% slower in wall time.

The source was exactly 104,857,600 bytes with raw BLAKE3
`0855eedd9498bf31a1eafb5a2f00bf84f646db5153cc86632fcb0cc0e180fb36`,
logical-v1 BLAKE3
`52ce153eab81e33a0243a25a47a8805a86ba9bec125a27bee3c50de647cdafbc`, and
historical expected SHA-256
`27f82e57f589b7ed79f28a8cef02acd2db82682fbccb35cdd6b48a136d98a7d6`.
The proxy workload had 4,801 chunk occurrences, 263 unique chunks, 4,803
object submissions, 265 creations, and 4,538 reuses.

Both lanes explicitly had `full_logical_workload=false`,
`phase3_semantic_persistence=false`, `promotion_authorized=false`, and
`target_attainment_authorized=false`. These are exploratory non-promotion
diagnostics, not valid 200 or 300 MiB/s full-logical-workload rows.

## Executive finding

The first append-only implementation is not yet a faster engine than Phase
4A. It is also not evidence that the append-only direction has failed.

There is enough evidence to justify one bounded optimization cycle, but not a
large rewrite or a promotion decision. The raw carrier append is promising;
the current index traversal, repeated authenticated replay, and full-carrier
reopen scan are consuming the advantage.

The performance goal is now explicit:

- **Minimum target:** sustain at least 200 MiB/s for a fair, durable, full
  logical-capture benchmark.
- **Stretch target:** reach 300 MiB/s under the same benchmark conditions.
- **Acceptance requirement:** these numbers must include the same semantic
  work, source bytes, CDC/object profile, publication, durability boundary,
  and equivalent verification as the Phase 4A comparison. A scanner-only row
  does not count.

The current data makes 200 MiB/s plausible for fresh capture after focused
optimization. It does not yet make 300 MiB/s credible enough to promise. The
next measurement must decide that question rather than architecture intuition.

## What the implementation is doing well

### 1. It has the right physical direction

The candidate uses one append-only carrier instead of one file and a metadata
operation sequence per object. That directly addresses the failure mode in the
older LayerFS experiment, where per-object filesystem work multiplied the
complete-path cost.

The current 100 MiB diagnostic produced:

```text
logical input:       104,857,600 bytes
carrier output:      106,327,544 bytes
carrier overhead:      1,469,944 bytes
overhead ratio:              1.4018%
```

That physical overhead is good. It suggests the carrier is not wasting large
amounts of space on the current object/index framing.

### 2. Ingest remains streaming and bounded

The implementation does not stage the source, all objects, or the whole index
in memory. The diagnostic reports a peak in-flight bound of 262,157 bytes and
uses fixed-size buffers, including a 64 KiB carrier write buffer.

That preserves the requirement that objects remain disk-backed. A future
optimization may cache bounded index pages or authenticated locator metadata,
but it must not become an unbounded object map or a source-sized staging
buffer.

### 3. The common publication shape is simple

The candidate has one append stream and one logical capture transaction. The
normal publication order is:

```text
validate parent and closure
  -> publish/reuse immutable objects
  -> publish index records
  -> publish delta and child root
  -> append one commit marker
  -> flush and perform one durability sync
  -> expose the new visible root
```

There are no hidden workers, Rayon fan-out, async runtime, retry storm,
general connection pool, WAL mode, rollback feature, or checkpoint feature.
That makes the cost model inspectable.

### 4. The durability fence is not currently the main bottleneck

The final incremental-digest diagnostic reports approximately:

```text
commit root/publication/authentication/flush/sync:  8.1 ms
marker sync:                                       8.0 ms
```

The exact timings vary by run, but the conclusion is stable enough for
prioritization: changing SQLite journal modes or trying to shave a small
amount from the single commit sync is not the first optimization target.

### 5. The implementation has useful direct instrumentation

The diagnostic records CDC, object hashing, object validation, carrier writes,
carrier reads, flushes, index lookups, index page reads, cache hits/misses,
commit publication, marker sync, reopen scanning, and reopen authentication.

That instrumentation makes it possible to optimize based on measured work
instead of repeating the old scanner-versus-complete-path mistake.

### 6. The authenticated publication design is directionally sound

The candidate treats the index as a locator accelerator rather than an
integrity authority. Immutable object reuse authenticates the incumbent before
reuse, and the last valid commit marker controls the visible root. Unmarked
residue is not silently promoted to visible state.

The correctness audit found gaps in the test coverage and error policy, but it
did not find a need to abandon the basic immutable-object and one-marker
publication model.

## What is not good yet

### 1. The current benchmark cannot support a speed claim

The current benchmark is explicitly marked `scanner_admission_only` and
`phase4a_comparable=false`. It streams CDC chunks into the carrier but commits
an empty-directory root with a fixed benchmark delta rather than a root whose
authenticated member graph references and verifies the complete scanned
source closure.

It also uses a generated LCG source while the historical SQLite experiment
used a different xorshift fixture, and the current wall timer includes source
generation while the historical SQLite timer did not.

Therefore the current approximately 59.6 MiB/s whole-diagnostic number is not
an engine throughput result, and the approximately 515 ms `engine_put` row is
not a qualified 200 MiB/s claim even though it is numerically near that
threshold for 100 MiB.

Before comparing against SQLite, both implementations must consume the same
pre-generated source file and fingerprint, publish the same logical root and
member/closure graph, and perform equivalent full reopen verification.

### 2. The index has too much I/O amplification

The final 100 MiB diagnostic reports:

```text
index lookups:       5,363
index page reads:   55,240
index cache hits:      561
index cache misses: 55,240
```

That is about 10.3 page reads per lookup. The carrier is sequential on
publication, but the index path is not yet behaving like a compact disk index.
This is the clearest fresh-ingest optimization target.

The first optimization should be a measured page-local layout: pack multiple
immutable locator entries into fixed-size pages, retain enough page-level key
range information to select a page, binary-search within the page, and use a
small bounded page cache. It should not introduce a general B-tree or an
unbounded in-memory map without measurements proving that the extra structure
earns its complexity.

### 3. Reopen reads far more physical data than the logical carrier size

For a 106,327,544-byte carrier, reopen read approximately 427,887,475 bytes,
or about 4.02 times the carrier size. The same diagnostic reports roughly:

```text
reopen scan:             505 ms
reopen validation read:  284 ms
reopen object auth:      539 ms
reopen object hashing:   258 ms
```

This is a major ceiling. An append-only design cannot win end-to-end if every
reopen pays for a full historical scan and repeated full-object
authentication.

The next design question is whether the current authenticated marker/index
format can discover the latest visible state with bounded work. If not, a
small authenticated tail locator or equivalent format mechanism may be needed.
It must accelerate discovery without becoming an unauthenticated source of
truth, and it must remain compatible with the one-commit-marker/no-rollback
requirements.

### 4. Authentication is correct in spirit but repeated in the replay path

The current ingest path has one engine object-hash pass:

```text
engine object hash bytes: 104,927,319
canonical bytes streamed: 104,927,319
identity hash passes:      one engine pass
```

That is good and should not be duplicated. However, reopen still reauthenticates
the carrier closure and rereads the same physical data multiple times. The
optimization opportunity is a bounded verified-locator/page cache and reuse of
authentication receipts within their exact immutable carrier identity,
generation, locator, and byte range.

This must never turn an index key match into trust. Payload authentication is
still required whenever the contract requires proving canonical bytes.

### 5. Correctness qualification is incomplete

The correctness audit identified these concrete gaps:

- A torn tail before the first valid marker is returned as an opened but
  poisoned engine instead of the declared typed recovery error.
- Actual partial header/payload/padding writes, marker append failure, flush
  failure, and several sync boundaries are not exercised by tests.
- The poisoning policy is too broad for some semantic errors and inconsistent
  for some carrier-read failures.
- Short-write counts and some first/dominant error provenance are lost.
- The public reuse path needs stronger tests for tampered checksum, payload,
  metadata, kind, length, and locator cases.
- Clean complete residue followed by a new append is not covered by a
  load-bearing reopen test.
- Independent bounded read-only handles are not implemented; this remains an
  explicit deferred qualification row rather than a passing feature.

These issues are not reasons to add more performance machinery. They are the
minimum correctness work required before trusting a performance result.

## What went wrong in the previous LayerFS experiment

The older LayerFS work produced fast scanner rows but a much slower complete
durable path. Representative 8 MiB complete-path medians were approximately:

| Candidate | Wall time | Objects | Direct CAS reads | CAS bytes read | Read amplification |
|---|---:|---:|---:|---:|---:|
| NF | 691 ms | 433 | 35,027 | 86.1 MiB | 10.26x |
| OF | 646 ms | 433 | 35,027 | 86.1 MiB | 10.26x |
| OS | 893 ms | 645 | 53,500 | 87.3 MiB | 10.40x |

The old 64 MiB scanner anchor was approximately 328–347 MiB/s, but those
rows excluded the complete pack, CAS admission, closure validation, COW/root
work, publication authority, and durability sync. They were not complete
engine throughput.

The main failure modes were:

1. Per-object filesystem metadata operations multiplied open/stat/read/close
   work.
2. Locator and catalog validation was repeated across layers.
3. The benchmark mixed a fast streaming scanner with complete durable work.
4. One candidate produced more objects, multiplying every object-level cost.
5. The durable evidence did not include the required sync cost.

The append-only candidate is worth continuing because it directly removes the
first failure mode with one carrier and direct offsets. It must not repeat the
second and third failure modes: the packed index and full-closure benchmark
are mandatory, not optional polish.

## What the earlier Rust + SQLite experiment does and does not prove

The earlier experiment at
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment`
reported roughly 300 MiB/s for Rust + SQLite versus roughly 100 MiB/s for
Node + SQLite under that experiment's conditions.

That result remains useful evidence that Rust's lower runtime/binding overhead
can matter. It does not prove that a new append-only layout will automatically
beat the current Phase 4A engine:

- the experiment compared a controlled SQLite-shaped operation;
- it did not have the same complete LayerFS physical path, recovery rules, or
  full authenticated closure requirement;
- the old LayerFS failure was dominated by storage-layout and metadata
  amplification, which changing Rust versus Node cannot remove;
- a fair Phase 4A/Phase 4B comparison must equalize source, CDC, object count,
  reuse, logical graph, durability, cache state, and timer boundaries.

The prior 300 MiB/s number is therefore motivation and a useful stretch
reference, not an acceptance baseline for this candidate.

## Why continuing optimization is justified

There are four concrete reasons to run one more optimization cycle.

### 1. The raw carrier is not the failure

The 100 MiB diagnostic appended approximately 106 MiB of physical data in
about 192 ms of carrier append time and about 75 ms of flush time. That is
consistent with the carrier having enough raw sequential bandwidth to support
the 200 MiB/s goal if metadata and replay amplification are reduced.

This is not a promise: the row is a single warm-or-unknown APFS run and is not
the full logical workload. It is evidence that the experiment is not blocked
by an obviously inadequate append primitive.

### 2. The largest measured costs are structurally reducible

The index performs about 10 page reads per lookup, and reopen reads about 4x
the carrier size. Those are layout and authentication costs, not immutable
properties of append-only storage. A packed index and bounded receipt/page
cache have a plausible path to reducing them while keeping objects on disk.

### 3. The desired 200 MiB/s threshold is close enough to test honestly

For a 100 MiB input, the target corresponds approximately to:

```text
200 MiB/s: 500 ms or less
300 MiB/s: 333 ms or less
```

The current `engine_put` diagnostic is about 515 ms, but it is not a valid
full-capture comparison. Being near the first threshold makes a focused
optimization experiment worthwhile. The current full diagnostic includes
about 505 ms of reopen scanning and is not a reason to conclude that the raw
append direction cannot reach 200 MiB/s.

### 4. The candidate has a simpler cost model than SQLite

The intended advantage is not that every append-only implementation is faster.
It is that this workload can use:

- one append stream;
- direct immutable locators;
- no SQL parser or statement dispatch on the object hot path;
- no SQLite pager/B-tree mutation for each object;
- one publication marker and one durability sync per capture;
- bounded disk-backed index state.

If the complete logical workload still does not beat SQLite after the current
amplification is removed, the measured result will be a useful and defensible
reason to stop. Before that measurement, abandoning the direction would be
premature.

## Recommended bounded work sequence

### Step 1: close correctness gaps

Fix the typed no-marker torn-tail behavior, define invalidating versus
recoverable errors, and add direct tests for partial writes, flush/marker
faults, public-path immutable reuse, and clean residue continuation.

Do not redesign the format in this step.

### Step 2: establish the fair baseline

Use one pre-generated 100 MiB source file and fingerprint for both engines.
Publish a complete source-referencing root/member graph. Exclude source
generation from both timers. Include one durability-equivalent commit and
full logical reopen verification. Run at least three iterations and report
median and spread, with cold/warm APFS state stated explicitly.

### Step 3: optimize the index only

Measure the current layout against a packed page-local locator layout. Keep
the page size and cache capacity bounded and report:

- page reads per lookup;
- cache hit/miss rate;
- index bytes written;
- ingest wall time;
- physical carrier bytes;
- memory high-water.

Keep the old layout available as the A/B control during the experiment, but do
not promote either layout without the full benchmark.

### Step 4: optimize authentication reuse only where valid

Add a bounded verified-locator/page receipt cache keyed by exact immutable
carrier identity, generation, locator, and range. Measure hash bytes and
carrier bytes before and after. Never cache all objects and never skip required
payload authentication.

### Step 5: measure reopen separately

If fresh capture reaches the target but reopen remains dominant, investigate
authenticated tail/index discovery. If fresh capture does not improve, stop
before adding more recovery machinery.

## Stop conditions

Keep Phase 4A as the production/reference implementation if any of these
conditions holds after the bounded experiment:

- the full-closure append-only row remains slower than Phase 4A;
- the result only beats SQLite when closure publication or reopen verification
  is omitted;
- the index optimization does not materially reduce page reads or wall time;
- the authentication optimization would require trusting unverified index
  entries or storing an uncontrolled amount of object data in memory;
- recovery, immutable reuse, or durability correctness regresses;
- the result is not repeatable across at least three equivalent runs.

The right outcome may be that append-only is retained as a later conditional
Phase 4B experiment and SQLite remains the chosen engine. That is preferable to
claiming 200–300 MiB/s from a reduced workload.

## Evidence references

- Phase 4A decision and current benchmark status:
  [`PHASE_4A_DECISION_RECORD.md`](PHASE_4A_DECISION_RECORD.md)
- Append-only design and non-goals:
  [`PHASE_4B_APPEND_ONLY_SPEC.md`](PHASE_4B_APPEND_ONLY_SPEC.md)
- Current diagnostic rows:
  [`PHASE_4B_BENCHMARK_REPORT.jsonl`](PHASE_4B_BENCHMARK_REPORT.jsonl)
- Earlier LayerFS complete-path evidence:
  `/Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs`
- Earlier Rust + SQLite versus Node + SQLite experiment:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment`
