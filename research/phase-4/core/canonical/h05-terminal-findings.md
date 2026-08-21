# H05 terminal findings and canonical-v2 entry

Status: terminal research reconciliation after H05, the H05b allocation
observer, and the exact-work H05c A/A study. This note records evidence and
routes the next research lane. It does not promote H05, authorize durable v2
bytes, integrate production, or change the accepted control.

Date: 2026-08-21

## Executive conclusion

H05 found a real and potentially large full-create optimization signal, but
the exact candidate is terminally rejected under its frozen evidence contract:

```text
H05 v7 performance                    3/3 candidate wins
paired-median durable improvement     16.655343%
semantic/work/Q/durability gates      PASS
exact allocated-storage equality      FAIL
H05 disposition                       MEASURED NO-GO / REVERT
full campaign                         not eligible
accepted control                      CP-0009
```

Two prospective follow-ups did not justify changing that decision:

```text
H05b 16-MiB observer    exact A/A allocation equality; amendment ineligible
H05c 100-MiB A/A       exact equality at PRE/T0/T1 in 6/6 pairs
H05c disposition       H05 CLOSED / A/A EXACT-EQUALITY STABLE
```

The H05 candidate and its rows cannot be promoted, rerun under another storage
rule, or counted as canonical-v2 acceptance evidence. The useful discoveries
carry forward as design and performance priors:

1. the private whole-source construction digest is redundant under the
   specified ordered authenticated canonical evidence;
2. replacing 104,857,600 bytes of construction-source hash input with a
   190,224-byte canonical occurrence commitment produced a strong local timing
   signal;
3. canonical-v2 can reuse that ordered-commitment design while additionally
   removing the separate raw `ChunkId` field/pass; and
4. canonical-v2 is now the next full-create research lane, beginning with a
   nonpersistent shadow model and short exploratory screens.

## Evidence authority

CP-0009 remains the only accepted candidate-comparison control:

```text
HEAD                         febc20f046bba84ccdce1256363d77799eabf2db
control source               3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
control source diff          b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84
control executable           9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
durable 100-MiB create       640.109209 ms
construction                504.215417 ms
proof consumption             0.038542 ms
COMMIT                       135.855250 ms
```

H05 terminal evidence:

- [v7 terminal report](../../../../target/phase4-h05-canonical-witness-screen-20260821-v1/screen-results-v7/TERMINAL-REPORT-v1.md)
- [v7 independent analysis](../../../../target/phase4-h05-canonical-witness-screen-20260821-v1/screen-results-v7/INDEPENDENT-ANALYSIS-v1.json)
- [H05b terminal report](../../../../target/phase4-h05b-allocation-observer-20260821-v1/TERMINAL-REPORT-v1.md)
- [H05c exact-work terminal report](../../../../target/phase4-h05c-aa100-attribution-20260821-v1/TERMINAL-REPORT-v1.md)
- [H05c independent terminal audit](../../../../target/phase4-h05c-aa100-attribution-20260821-v1/INDEPENDENT-TERMINAL-AUDIT-v1.txt)

The H05c terminal manifest has 299 entries and independently rehashes with zero
mismatches:

```text
b5e7a559641b6f5ee9ebbf70fd8a704ecf45cdad40ada93c90c96a37d80343e9
```

## What H05 actually changed

Control construction proof:

```text
BLAKE3(complete 104,857,600-byte source)
+ ordered repeated(u32be(raw_length) || raw_id)
```

H05 construction proof:

```text
BLAKE3 derive-key context
  "layerfs/phase4/h05/canonical-occurrence/v1"
  over repeated(u32be(raw_length) || canonical_object_id)
+ the unchanged v1 ordered raw-ID sequence
```

Direct counter equation:

```text
candidate canonical commitment input
  = 5,284 * (4-byte length + 32-byte canonical ObjectId)
  = 190,224 bytes

net construction witness input reduction
  = 104,857,600 - 190,224
  = 104,667,376 bytes
```

Every candidate row retained the current raw ID, canonical ID, CDC sequence,
mapping bytes, roots, transaction, COMMIT, and persistent database bytes.

## H05 screen result

Measured rows:

| Pair | Order | Control | Candidate | Improvement |
|---:|:---:|---:|---:|---:|
| 1 | AB | 652.479042 ms | 543.806417 ms | 16.655343% |
| 2 | BA | 636.699166 ms | 629.022125 ms | 1.205756% |
| 3 | AB | 692.797000 ms | 562.715250 ms | 18.776315% |

The screen is evidence that the redundant digest can be expensive. It is not a
stable 16.655% forecast: there were only three measured pairs, one pair was
near parity, the cache was warm-or-unknown, and no full campaign followed.

Hard gates that passed included:

- 7/7 protected smoke rows;
- source, CDC, canonical-object, root, transition, closure and range identity;
- exact H05 and unchanged-work counters;
- Q high-water 88,093 in every scheduled row and terminal zero;
- exact SQL, BLOB, pager, transaction and one-COMMIT work;
- byte-identical post database and authority SHA-256;
- exact logical/apparent endpoints and no residue.

The frozen exact allocated-storage gate failed. That failure remains terminal
for H05 even though candidate allocation was favorable in all three measured
pairs.

## Allocation follow-ups and lesson

The artifact called H05b here is the allocation observer, not hypothesis-ledger
entry `H05B` for the FastCDC hot loop.

H05b ran six 16-MiB A/A pairs and found no mismatch. H05c then ran the exact
100-MiB control full-create path:

```text
pairs                         6
order                         AB / BA / AB / BA / AB / BA
control rows                 12
PRE/T0/T1 snapshots          36
PRE DB/store allocated       20,480 / 24,576
T0 DB/store allocated        117,506,048 / 117,510,144
T1 DB/store allocated        117,506,048 / 117,510,144
A/B allocation mismatches     0
T0-to-T1 changes              0
```

Under the prospective H05c rule, exact equality was stable and Phase 2 was
ineligible. This closes H05; it does not prove a universal APFS theorem.

For future experiments, the storage gate must match the proposed variable:

- a current-format candidate promising byte-identical persistence may
  prospectively require exact post-byte and allocation equality;
- a format candidate that deliberately shrinks mapping bytes cannot require
  equality to v1. It needs exact format equations, endpoint caps, no residue,
  and a prospective non-regression rule instead.

Canonical-v2 intentionally changes mapping bytes, so its exploratory screen
must report storage honestly without treating expected shrinkage as a failure.

## Why canonical-v2 is next

Current v1 reference:

```text
raw ChunkId[32] || raw_length[4] || canonical ObjectId[32] = 68 bytes
```

Proposed compact v2 reference:

```text
raw_length[4] || canonical ObjectId[32] = 36 bytes
```

Exact retained-fixture structural model:

```text
references                         5,284
bytes removed per reference           32
mapping bytes removed            169,088
mapping bytes          365,262 -> 196,174
full K64 leaf            4,380 -> 2,332
raw ChunkId hash gross lane       95.185147 ms
```

H05 measured the mandatory ordered canonical commitment rather than leaving it
as a zero-cost assumption. Canonical-v2 can now investigate the larger combined
mechanism:

- retain one complete canonical-object authentication identity;
- remove raw ID from new v2 references;
- use canonical IDs for rejoin and CAS addressing;
- remove the separate raw-ID write pass;
- remove post-canonical raw rehashing from v2 scrub/reconstruction/ranges;
- retain exact source bytes, CDC boundaries and canonical chunk objects;
- introduce an explicit v2 mapping profile, roots, transitions and receipts.

The row-wise F4 optimistic subtraction ceiling remains 427.084-454.849 ms,
median 452.873 ms. It is a ceiling, not a speed promise.

## Canonical-v2 research boundary

Canonical-v2 is not a current-identity-preserving micro-optimization. The
following may remain exact across profiles:

- source bytes and source custody;
- CDC boundaries and lengths;
- canonical chunk object bytes and canonical ObjectIds;
- logical reconstruction and ranges;
- transaction/COMMIT/durability class.

The following intentionally change under the new profile:

- file-reference bytes;
- file leaves/branches and file roots;
- enclosing workspace roots;
- transitions, receipts and mapping-profile ID;
- related goldens and exact v2 error surface.

The first implementation must therefore be a nonpersistent shadow/equivalence
model. It should not write production v2 bytes or invent a general migration
framework before the authority questions close.

## Fast-iteration policy

The canonical-v2 discovery loop is intentionally lighter than a promotion
campaign:

```text
parallel read-only research
  -> nonpersistent shadow model
  -> focused tests only
  -> one benchmark-private variant
  -> <=120-second exploratory screen
  -> retain/modify/stop recommendation
```

Semantic, authentication, bounded-memory, one-COMMIT and cleanup requirements
remain hard. Exploratory performance is graded rather than forced through a
promotion threshold:

```text
negative median or semantic failure    stop/revise
positive median, 2/3 wins              promising signal
>=5% median, 2/3 wins                  strong signal
>=15% median, 3/3 wins                 breakthrough signal
```

These labels authorize research decisions only. They do not promote a profile.
No full workspace suite, 512-MiB scale run, five-pair campaign, multi-host run,
or production migration belongs in the first fast iteration.

## Required transition before candidate work

The live benchmark source is still the rejected H05 candidate. Before a
canonical-v2 source change:

1. freeze the complete current dirty status and H05 artifact hashes;
2. restore only the benchmark-private source to the frozen CP-0009 source;
3. verify source SHA-256 `3284c3bd...70652a` and diff SHA-256
   `b073a7e0...50f84`;
4. preserve every H05/H05b/H05c artifact; and
5. build canonical-v2 from CP-0009, not from H05.

Do not use `git reset`, `git clean`, or a broad checkout.

## Next action

Use the [canonical-v2 agent prompt](canonical-v2-agent-prompt.md). The agent is
authorized to explore broadly with subagents, but it must stop before production
integration or a promotion-grade campaign.
