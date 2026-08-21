# 2026-08-21 Phase 4 full grind roadmap

Status: execution roadmap after CP-0009. This document organizes the remaining
Phase 4 optimization work; it does not itself authorize a candidate, format
change, benchmark campaign, promotion, WP5 work, or a Phase 4 completion claim.

## Executive decision

Phase 4 has four independent product-performance lanes:

1. durable full create;
2. same-open edits, especially count-changing edits;
3. first authenticated operation after reopen;
4. first-time and repeated materialization.

They must not be bundled into one implementation or one performance claim.
Research may proceed in parallel, but measured candidates advance serially
against the latest accepted control:

```text
CP-0009
  -> terminal H05/H05b/H05c findings preserved; H05 not promoted
  -> canonical-v2 shadow, publication repair, and complete validation PASS
  -> freeze exact fresh-store canonical-v2 baseline
  -> H09 edit-locality simulator and candidate as a separate lane
  -> materialization qualification and one-variable candidate as a separate lane
  -> residual SQLite/durability work
```

Reopen authority remains a separate product/security decision. It does not
block canonical-v2 research, H09 simulation, or honest materialization
measurement.

## Controlling baseline

CP-0009 is the exact historical v1 comparison control:

```text
HEAD                              febc20f046bba84ccdce1256363d77799eabf2db
control source diff               b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84
release executable                9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
durable 100-MiB submit            640.109209 ms
construction / mapping            504.215417 ms
proof consumption                   0.038542 ms
COMMIT                             135.855250 ms
same-open same-count edit            9.737250 ms
authority before that edit         245.330416 ms
warm logical materialization       425.800708 ms
fresh logical materialization      433.512791 ms
authenticated returned 1-MiB range   3.171209 ms / 315.337 MiB/s
reopen / visible head                3.007750 ms
```

The materialization cache state is `warm-or-unknown`, and phase-local CPU is
unavailable. Future candidate claims require adjacent balanced A/B against the
exact current control; historical subtraction is not evidence.

Canonical-v2 is now the accepted fresh-store optimization baseline. Its exact
source/executable/profile are `16e9beed...e120` / `f3dd4c94...0280` /
`94a03ba7...d13b`. The complete campaign measured 667.652021 ms CP-0009
versus 512.214000 ms v2, or 23.281293% faster. Both adjacent pairs won and all
29 lifecycle/semantic rows passed. CP-0009 remains the v1 rollback authority;
automatic nonempty v1-to-v2 migration remains deferred.

The below-500-ms / above-200-MiB/s create milestone remains an intermediate
target. This complete-validation sample is 12.214 ms above it, so the baseline
freeze is a correctness-backed relative win, not a claim that create work is
finished.

Controlling documents:

- [CP-0009 report](test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md)
- [Current baseline manifest](baseline/current-baseline-v1-manifest.tsv)
- [Canonical-v2 frozen baseline](baseline/canonical-v2-baseline-v1.md)
- [Canonical-v2 baseline manifest](baseline/canonical-v2-baseline-v1-manifest.tsv)
- [Optimization decision map](../../research/phase-4/decision-map.md)
- [Hypothesis ledger](../../research/phase-4/foundations/hypothesis-ledger.md)
- [Benchmark method](../../research/phase-4/foundations/benchmark-and-evidence.md)
- [Invariant matrix](../../research/phase-4/foundations/invariant-matrix.md)

## Permanent constraints

Every retained candidate must preserve the applicable contracts:

- exact CAS, CDC, COW, root, transition, delta, and object identities unless a
  separately authorized versioned profile explicitly changes them;
- exact errors and failure precedence;
- authenticated incumbents and no authority laundering;
- bounded owned memory with exact capacity accounting and terminal `Q=0`;
- one writer transaction, one publication COMMIT, and atomic visible-head
  publication;
- caller-thread execution unless a separately authorized execution profile
  changes that contract;
- rollback-journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`, and
  `mmap_size=0` for the current SQLite profile;
- fresh reconciliation for ambiguous COMMIT outcomes;
- no inference of physical I/O, sync, cold-cache state, or CPU attribution
  from wall time or logical byte counts.

## Lane 1 — durable full create

### Goal

Reduce the 640.109-ms durable 100-MiB create, first by attacking the
504.215-ms construction path and only later the residual 135.855-ms COMMIT
path. The work order is canonical identity and hashing, CDC execution, then
SQLite physical behavior.

### C1 — H05 canonical-occurrence witness

H05 is the immediate create optimization. Its benchmark-private candidate is
already implemented, validated, built once, hash-frozen, and preregistered.
Its frozen private screen is now terminal `H05 MEASURED NO-GO` /
`REVERT / H05 LOCAL NO-GO`. The candidate won 3/3 measured pairs with a
16.655343% paired median durable improvement, but all four pairs missed the
prospectively frozen exact allocated-storage equality gate. No full H05
campaign or integration is eligible. CP-0009 remains the accepted create
control.

It replaces one private full-source construction digest:

```text
control input:    104,857,600 source bytes
candidate input:  repeated(u32be(raw_length) || canonical_object_id)
candidate bytes:  5,284 * 36 = 190,224
net reduction:    104,667,376 hash-input bytes
```

It retains the external source fingerprint, current v1 raw-ID sequence,
current durable bytes, roots, schema, profile, transaction, COMMIT, and
durability behavior. The hash-input reduction is a direct-counter prediction,
not speed evidence, and it does not remove the source scan required by CDC.

Completed screen:

1. 7/7 protected candidate smoke rows passed;
2. one uncounted `AB` warmup and measured `AB / BA / AB` pairs completed in
   83 seconds;
3. exact semantics/resources and counters passed except allocated-storage
   equality;
4. the negative evidence is preserved under
   `target/phase4-h05-canonical-witness-screen-20260821-v1/screen-results-v7`;
5. no full campaign was run.

Had the H05 screen passed, it would have authorized only a full adjacent
balanced campaign, not integration or a new baseline. The actual terminal
result authorizes neither.

See [H05 preregistration](experiments/h05-canonical-witness/preregistration.md).

### C2 — compact canonical-v2

**Terminal disposition: PASS / FROZEN.** Native-v2 completed the full static
and 29-row lifecycle validation. It retains 5,284 CDC occurrences, 5,372
created objects, 5,381 SQL calls, 10,748 BLOB writes, one transaction, one
COMMIT, and terminal Q zero while reducing mapping bytes from 365,262 to
196,174. The balanced durable-create result is 667.652021 to 512.214000 ms
(23.281293%). Canonical-v2 is closed as the baseline; further work must be a
separate candidate derived from it.

H05 has supplied local cost evidence for the mandatory ordered canonical
commitment, though its exact candidate is terminally rejected. Investigate a
versioned occurrence representation:

```text
v1: raw_id[32] + length[4] + canonical_id[32] = 68 bytes
v2: length[4] + canonical_id[32]               = 36 bytes
```

Exact retained-fixture gross effects before topology changes:

```text
5,284 references * 32 bytes       = 169,088 fewer mapping bytes
mapping bytes                     = 365,262 -> 196,174
full K64 leaf                     = 4,380 -> 2,332 bytes
raw ChunkId hashing interval      = 95.185147 ms gross removable budget
```

The accepted profile changed authority, rejoin, receipt, mapping, and
transition semantics prospectively. It did not create a bridge format or
rewrite retained history. Automatic migration remains unsupported.

### C3 — exact CDC/hash execution

After identity work, measure the remaining FastCDC boundary loop. F4-A
attributes 128.723024 ms to CDC-exclusive work, while F4-A2 showed only
3.701583 ms of removable scanner materialization/carry. Therefore:

- do not pursue another buffer-copy abstraction;
- test only an exact-boundary hot-loop mechanism with direct boundary and byte
  counters;
- treat a larger chunk profile as a versioned locality/dedup/range tradeoff;
- keep worker or multicore execution as a separately authorized profile.

### C4 — SQLite physical profile

Only after canonical and CDC work, compare fresh 4-KiB, 8-KiB, and 16-KiB
database profiles with byte-fixed cache settings. Protect create, same-count
and count-changing edits, scrub, reconstruction, returned ranges, Q, RSS,
storage, and residue.

The current profile reports approximately one final database image of dirty
page bytes. Larger pages may reduce page events and overflow/B-tree work, but
they cannot be assumed to reduce physical bytes or wall time.

### C5 — residual COMMIT and durability

Optimize only evidenced work before the same durability barrier. Do not weaken
`synchronous=FULL`, remove the publication COMMIT, add a second transaction,
or infer sync behavior from wall time. If canonical, CDC, and page-profile
work leave no safe removable budget, retain the current durability design.

## Lane 2 — same-open edit scaling

### Current evidence

Same-count edits are already strong and must be protected:

```text
100-MiB same-open same-count edit  9.737250 ms
```

CP-0008 exposes the count-changing suffix slope:

```text
500-MiB early +1 edit              27.140916 ms
500-MiB middle +1 edit             15.102042 ms
100 -> 500 MiB suffix/mapping work approximately 5x
```

The fixed-radix representation passes the historical `<50 ms through 500
MiB` policy. The stronger product objective is now practically size-insensitive
same-open local edits across 1, 10, 100, and 500 MiB.

### E1 — H09 history-independent prolly simulator

Before changing the durable mapping format, simulate the exact CP-0008 edit
sequences over 1/10/100/500-MiB mappings. A qualifying design combines:

- subtree byte lengths for direct offset lookup;
- bounded local CDC rejoin;
- content-defined mapping-node boundaries with hard minimum and maximum sizes;
- persistent COW reconstruction of only affected nodes and paths;
- no global ordinal or absolute-position field that relabels the suffix;
- deterministic, history-independent roots for identical final content.

Advance to a benchmark-private candidate only if the simulator shows:

- at least 95% fewer rewritten mapping bytes for early and middle
  count-changing edits;
- direct affected-work counters that remain approximately flat across file
  sizes apart from shallow-tree `O(log N)` growth;
- hard node-size bounds and deterministic roots;
- no more than 5% full-build or same-count regression.

The aspirational result is approximately 10–15 ms for equivalent same-open
count-changing edits from 10 through 500 MiB. This is a target, not existing
evidence. H09 does not optimize full create and must not regress it.

## Lane 3 — reopen authority

### Separate the operations

Opening the database and reading the visible head is already fast:

```text
reopen / visible head               3.007750 ms
```

The expensive operation is the first authenticated mutation after reopen:

```text
500-MiB first-after-reopen          1.228564–1.262772 s
```

This is dominated by full closure authority, not count-changing mapping.
Neither H05 nor H09 materially removes it.

### R1 — explicit authority decision

Choose one terminal policy before implementation:

1. retain the secure `Theta(stored closure)` scrub after an untrusted reopen;
   or
2. authorize a trusted authority boundary with a non-replayable store
   generation, mutation mediation, cross-process writer fencing,
   rollback/downgrade protection, and crash/ambiguous-outcome reconciliation.

A receipt, inode, size, mtime, sidecar, or database-local generation cannot
alone prove freshness because it can be rolled back with the database. The
current read-only research disposition is `RETAIN_FULL_REOPEN_SCRUB`; no fast
implementation is authorized without a stronger trust primitive.

See the [reopen-authority report](../../research/phase-4/after-cp-0009/reopen-authority/report.md).

## Lane 4 — materialization

### Separate the workloads

1. First-ever materialization to an empty destination is `Omega(file size)`:
   every output byte must be produced.
2. Repeated materialization to an authenticated existing destination may be
   changed-byte proportional through an authenticated parent-to-child delta.
3. Repeated same-volume materialization to a new destination may benefit from
   a verified native seed and APFS clone, but that is platform-specific and
   does not replace the no-seed path.

The existing 425.801/433.513-ms warm/fresh values use a logical reconstruction
sink with `warm-or-unknown` cache state. They are not native cold-output
evidence.

### M1 — qualify cold and hot materialization

Before implementing an optimization, build one evidence boundary that
separates:

- cold versus warm source-cache state where the platform can support the
  distinction;
- empty destination versus authenticated existing destination;
- logical reconstructed bytes;
- authenticated SQLite/CAS bytes read;
- destination bytes written;
- logical, apparent, and allocated endpoint bytes;
- first materialization versus delta/incremental materialization.

Every unsupported observation must be marked unavailable with its reason. Do
not use wall time, RSS, logical length, or allocation as a physical-I/O proxy.

### M2 — select one measured mechanism

Rank only after M1 exposes the removable budget:

1. one-pass authenticated bounded streaming if duplicated reads are measured;
2. authenticated parent-to-child delta application for a proven destination;
3. verified APFS native seeds for repeated same-volume output;
4. canonical-v2 read-side simplification if C2 establishes its authority.

Foreground compression and Git-style delta packing remain rejected for the
retained fixture: adaptive zstd saved only about 4.32% while the exploratory
per-object encode screen cost about 147.8 ms.

## Fast research-to-evidence loop

Every candidate follows the same cadence:

```text
measured removable budget and authority analysis
  -> prospective one-variable preregistration
  -> <=120-second kill screen
  -> REVERT or RETAIN-FOR-FULL-CAMPAIGN
  -> complete adjacent balanced A/B
  -> independent recomputation and final read-only audit
  -> new accepted checkpoint or terminal revert
```

Rules:

- never reinterpret a failed screen as a pass;
- never selectively rerun or remove measured rows;
- never compare a candidate only to a historical standalone median;
- promote at most one variable at a time;
- rebuild the next candidate from the latest accepted control;
- pause CPU-, memory-, filesystem-, and disk-intensive parallel work during
  timed rows;
- preserve rejected evidence and document why the mechanism failed.

## Parallel work policy

Read-only research and non-measured simulation can proceed in parallel across
create, edit, reopen authority, and materialization. Candidate builds,
filesystem preparation, and timed campaigns must be coordinated so they do
not contaminate one another or the benchmark host.

Implementation remains sequential at promotion boundaries:

```text
candidate
  -> evidence
  -> retain/revert
  -> freeze next control
  -> next candidate
```

This preserves fast iteration without batching speculative ideas into a large,
unattributable final rewrite.

## Immediate queue

```text
DONE      H05/H05b/H05c: terminal no-go/closure; all evidence preserved
DONE      canonical-v2: complete validation PASS; fresh-store baseline frozen
CONTROL   canonical-v2 f3dd4c94...0280; CP-0009 retained as historical v1 control
NEXT      choose one separate lane: exact CDC loop or H09 simulator
PARALLEL  materialization/reopen work may remain read-only research when authorized
LATER     SQLite physical profile and residual COMMIT/durability evidence
```

Do not run parallel load during any future timed candidate rows.

## Phase 4 completion conditions

Phase 4 closes only when every lane has an explicit evidence-backed terminal
disposition:

| Lane | Required disposition |
|---|---|
| Durable full create | Accepted optimized control or evidence-backed retain-current decision |
| Same-open edits | Size-scaling goal met, or its remaining suffix-linear limit explicitly accepted |
| Reopen authority | Trusted fast path accepted, or secure full scrub explicitly retained |
| Materialization | Cold/hot and first/repeated behavior measured; one candidate accepted or current path retained |
| SQLite durability | Physical profile accepted or current profile retained from complete evidence |
| Global correctness | Identities, errors, authority, Q, durability, storage, reconciliation, and custody all pass |

WP5 begins only after those Phase 4 dispositions are closed and the Phase 4
handoff is frozen. None of this roadmap starts WP5.
