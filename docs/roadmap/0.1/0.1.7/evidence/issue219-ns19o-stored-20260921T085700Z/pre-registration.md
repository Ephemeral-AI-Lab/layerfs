# Pre-registration — #219 round 14: store a payload the codec cannot shrink

Written **before** any edit and before the first run of this arm. This is a **product** change in the
Store's payload framing, taken from the handoff's workstream A.

Control: **N1** `benchmark-results/issue219/ns19-N1-absence-20260921T083530Z` (`35aaf6f0b`, clean tree,
13/13 gates, 14/14 pinned counters, digest `1d6fba29…3857847`): `operation_work_ns` **1473.83 ms**,
CPU 1495.48 ms, `profile_full_ns` **240.85 ms**, `pack_bytes_written` 302,406,480, `pack_appends`
15,534, `packs_created` 1,268, `statements` 16,595, `commits` 284. This arm compares against that row
and **does not re-run it**.

## The one difference

**A payload record whose compressed frame is not smaller than the payload is stored verbatim, and the
decision is made on a bounded 1024-byte prefix instead of on the whole payload.**

Today `encode_representation` (`encoding/full.rs:104`) compresses every whole-file and chunk payload
unconditionally. On this fixture that is 300,000,000 payload bytes and zstd returns **input + 9 bytes**
for every one of them: `profile_full_ns` 240.85 ms is 300 MB at **1.245 GB/s**, which is the codec's
own price for scanning bytes it cannot match, paid in full for 0 % of the benefit.

Two grammar facts make the change contained:

- the record tag byte already exists (`encoding/delta/record.rs:19`, `:21`), carrying `FULL_TAG = 0` and
  `PREFIX_TAG = 1`, so **a third tag costs nothing** on the wire;
- the compact whole-file lane already drops its two length fields at assembly
  (`pack/assemble.rs::append_body`, `WHOLE_FILE_COMPACT_DROP = 8`), so a stored payload's width is
  `payload + 1` where today it is `payload + 9` — the stored form is **7 bytes narrower per record**.

**Versioned, both directions.** `WholeFile` moves 11 → 14, `Native` 10 → 15, `Singleton` 13 → 16. The
new reader still maps 11, 10 and 13 to the same lanes, so every pack this Store has ever written is
still read; a reader that predates this change refuses a new pack by version at `parse_header`
(`UnsupportedPolicy { field: "pack framing version" }`) instead of meeting an unknown tag mid-decode.
`PooledMetadata` (12) and `Ordinary` (9) are untouched: neither lane carries a payload frame.

**The rule, stated once.** A payload is stored verbatim when *either*

1. it is **larger than 1024 bytes and a 1024-byte prefix of it, compressed under the same profile,
   shrinks by less than 1/32**, or
2. it was compressed and the frame is **not smaller than the payload**.

Rule 2 is free — the frame already exists — and it is what makes a wrong guess cost size and never
correctness. Rule 1 is the only new work.

**What this is not.** It is not a codec change: the level was refuted in round 8 (`zstd -1` ≡ `zstd -3`
on this data, identical time and identical output) and the frame parameters are untouched. It is not a
retry: a payload that fails to compress is stored, never re-encoded.

## Price, count and width

The round-8 lesson is that a wave *count* fell while one wave's *width* grew. Both are priced here.

| | count | width per unit | bytes |
| --- | ---: | ---: | ---: |
| today: payloads compressed | 23,910 | 12,547 B (mean) | 300,000,000 |
| arm: payloads probed | 15,977 | 1,024 B | 16,360,448 |
| arm: payloads compressed for real (≤ 1024 B) | 7,933 | 377 B (mean) | 2,991,465 |
| arm: fixed per-call overhead | 23,910 calls | 0.3–0.8 µs | 7–19 ms |

Counts and bytes are read from the control row's own Store (`sample.sqlite`, `objects` grouped by
role): role 1 `WholeFile` 9,444 records / 24,652,248 canonical, role 2 `Chunk` 14,466 records /
275,868,750 canonical; minus the frozen canonical overheads (23 and 21 bytes, `policy.rs:157`, `:155`) that is **23,910 payload records and exactly 300,000,000 payload bytes**. Of those, 7,933 are
≤ 1024 bytes and 15,977 are larger. The codec byte width therefore falls to **6.45 %** of today's;
the call count does **not** fall (it rises by the framing calls), and that is the term the prediction
pays for explicitly.

## Prediction, in the instruments' own units

| instrument | N1 | predicted | derivation |
| --- | ---: | ---: | --- |
| `profile_full_ns` | 240.85 ms | **20–40 ms** | 19.35 MB of codec input at 1.245 GB/s = 15.5 ms, plus 23,910 call overheads at 0.3–0.8 µs |
| `probe_ns` (new) | — | **5–13 ms** | 15,977 probes at 0.3–0.8 µs |
| `stored_records` (new) | — | **≈ 23,910** | the fixture is `fixture::noise`; the pre-flight script classifies every payload |
| `operation_work_ns` | 1473.83 ms | **1255–1280 ms** | −195 to −220 ms from `profile_full_ns` |
| CPU user+system | 1495.48 ms | **1285–1315 ms** | the same work, counted rather than waited |
| `pack_bytes_written` | 302,406,480 | **≈ 302,239,000** | 7 bytes × ~23,910 stored records, not pinned |
| `pack_appends`, `packs_created`, `statements`, `commits`, `inserted` | 15,534 / 1,268 / 16,595 / 284 / 25,245 | **unchanged** | grouping, row count and transaction cadence are untouched |

CPU carries the row-level claim wherever the wall clock is inside its own ±226 ms band; the two
codec counters carry the mechanism.

## What would refute it

1. `profile_full_ns` >= 100 ms. The probe is not doing what this document says it does — most likely
   the fixed per-call overhead is far larger than 0.8 µs, which is the width term this registration
   exists to price. Reported as a refutation, not repaired by re-running.
2. `operation_work_ns` >= 1373 ms (a movement smaller than 100 ms). The mechanism may still be real;
   the row movement would not be, and the arm is reported as a refutation of the *row* claim.
3. `stored_records` < 20,000. The probe misclassifies this fixture and the movement is not the one
   claimed here, whatever its size.
4. Any of the 14 pinned counters moves, or `digest:filesystem_root` != `1d6fba29…3857847`, or the row
   is not PASS 13/13. This treatment changes how a payload is framed, never what is stored: the
   canonical bytes, the object identities, the roles, the grouping and the commit cadence are all
   outside it.
5. `tests/cas_reuse.rs` or `tests/delta_payload.rs` fails. Those are the two invariants the handoff
   names as walls, and a framing change is exactly what could walk into them.
6. `pack_bytes_written` **rises**. The stored form must be narrower than the framed one; a rise means
   the prefix rule is firing in the wrong direction.

## Declared trade, stated before the result

This fixture is `fixture::noise`, the **maximum case** for this change: every payload is
incompressible, so every probe is a pure saving. On compressible content the probe adds one bounded
codec call per payload and then compresses anyway, and where a payload's first 1024 bytes are
unrepresentative of the whole the Store keeps bytes zstd would have removed. That is a declared
size/time trade, not a free win, and the size half of it is bounded by the 1/32 threshold: a payload
the rule stores is one whose own prefix zstd could not shrink by 3 %.

## Pre-flight classification (count-driven, no arm is sampled)

`raw/probe-coverage.py` reads the control row's Store, extracts every payload frame exactly as the
reader does and asks the pinned zstd CLI for the size of a 1024-byte prefix of each. It samples no
arm: it is a byte-in/byte-out instrument over the receipt already on disk. Result and script are
beside this file.
