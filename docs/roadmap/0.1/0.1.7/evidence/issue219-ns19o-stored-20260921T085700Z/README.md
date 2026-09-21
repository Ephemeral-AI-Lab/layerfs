# #219 round 14 — store a payload the codec cannot shrink: the mechanism is 165 ms, the row is 93 ms

Pre-registration: `pre-registration.md` (written before the edit and before the run). Row:
`benchmark-results/issue219/ns19-O1-stored-20260921T091015Z`, **PASS, 13/13 gates, 14/14 pinned
counters** (`golden_matches: true`), `source_commit` `4d8e6e2ab`, `source_dirty` false, root digest
`1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`. Landed as `4d8e6e2ab`.
Control: **N1** `ns19-N1-absence-20260921T083530Z`, not re-run.

## 1. The movement, and the prediction that missed

| instrument | N1 | O1 | movement | pre-registered |
| --- | ---: | ---: | ---: | --- |
| **`operation_work_ns` (the row's formula)** | 1473.83 ms | **1380.70 ms** | **−93.13 ms, −6.32 %** | 1255–1280 ms — **MISSED** |
| **CPU user+system** | 1495.48 ms | **1397.14 ms** | **−98.34 ms, −6.58 %** | 1285–1315 ms — **MISSED** |
| `operation_ns` (inclusive) | 1551.04 ms | 1475.87 ms | −75.16 | — |
| complete command | 2376.25 ms | 2312.15 ms | −64.10 | — |
| **`profile_full_ns`** | 240.85 ms | **75.65 ms** | **−165.20 ms, −68.6 %** | 20–40 ms — **MISSED** |
| `diag_probe_ns` (new) | — | **44.04 ms** | — | 5–13 ms — **MISSED** |
| `stored_records` (new) | — | **23,910** | — | ≈ 23,910 — **HIT** |
| `pack_bytes_written` | 302,406,480 | 302,074,398 | −332,082 | ≈ −167,000 — widened |
| `packs_created` | 1,268 | **1,265** | −3 | — |
| `pack_appends` | 15,534 | **15,532** | −2 | — |
| `statements` | 16,595 | **16,590** | −5 | — |
| `commits` | 284 | **284** | 0 | pinned, held |

**Refutation clause 2 fired.** It read *"`operation_work_ns` >= 1373 ms (a movement smaller than
100 ms) … the arm is reported as a refutation of the row claim"*, and 1380.70 ms is above it. The row
claim of this round — a ≥100 ms movement — **is refuted** and is recorded as refuted, not repaired by
a second run. What is *not* refuted is the mechanism: clause 1 (`profile_full_ns` ≥ 100 ms) did not
fire at 75.65, clause 3 (`stored_records` < 20,000) did not fire at exactly 23,910, clause 4 did not
fire (PASS, 14/14, digest unchanged), clause 5 did not fire (`cas_reuse` and `delta_payload` are green
inside the 223 passing storage cases), and clause 6 did not fire (`pack_bytes_written` fell).

## 2. The mechanism, confirmed at exactly the registered count

`stored_records` **23,910** is every payload record in the Store — 9,444 whole-file and 14,466 chunk —
and it is the number `raw/probe-coverage.txt` predicted before the run by classifying every payload
from the control row's own `sample.sqlite`:

| | records | how decided |
| --- | ---: | --- |
| payload ≤ 1024 B | 7,933 | the post-hoc rule: the frame the codec produced is not smaller than the payload |
| payload > 1024 B | 15,977 | the probe: every one returned a 1033-byte frame for its 1024-byte sample, i.e. 9 bytes of frame overhead and nothing found |
| total | **23,910** | every payload record of both lanes |

`raw/frame-widths.txt` is the independent half: over all 23,910 records the frame the old code wrote
was **13–16 bytes wider than the payload it described** (median 14), so the post-hoc rule alone would
have stored every one of them. The probe is therefore not what makes this fixture correct — it is what
makes it *fast*: it is the only thing that skips the whole-payload scan.

The compressed width fell to **6.45 %** of the control's 300,000,000 bytes (16.36 MB of samples plus
2.99 MB of payloads at or below the probe width), and `profile_full_ns` fell by 68.6 %, which is that
arithmetic arriving.

**The store is smaller, and by the amount the grammar says.** 23,910 records × 13.89 bytes =
332,082 bytes fewer handed to the pager; three fewer packs created, and `space.pack_bodies_bytes`
falls by exactly **786,432 = 3 × 256 KiB**, the three pack allocations that no longer happen;
`space.page_count` 82,129 → **81,937**.

## 3. The two width terms the registration priced wrong

Round 8's lesson was that a *count* can fall while a *width* grows. Here both terms were counted and
**both widths were still wrong**, in the same direction: the price per call was under-estimated.

1. **The probe's fixed cost is ~1.9 µs, not the 0.3–0.8 µs registered.** `diag_probe_ns` 44,040,823 ns
   / 15,977 probes = **2,757 ns per probe**; the sample's own 1024 bytes at the rate this row measures
   for zstd (300,000,000 B / 240.85 ms = **1.2456 GB/s**) is **822 ns**, so **1,935 ns is per-call
   overhead** — `ZSTD_CCtx_reset`, six `setParameter` calls, `getCParams`, `estimateCCtxSize`, a fresh
   output allocation and a second reset. Priced at the registered 0.8 µs the bucket would have been
   ~57.5 ms instead of 75.65, so **≈18 ms of the 165 ms the mechanism won was given back to its own
   instrument**. The refutation clause said in advance that this was the likely failure ("the fixed
   per-call overhead is far larger than 0.8 µs, which is the width term this registration exists to
   price"); it was, and the clause is recorded as having named it.
2. **The frame overhead is 13–16 bytes per payload, not the 7 the handoff's summary carried.**
   `pack_bytes_written` therefore fell 332,082 rather than ~167,000. The pre-registration's arithmetic
   inherited the 7; `raw/frame-widths.txt` measures the real distribution. Nothing in the row turned on
   it — the direction was right and the magnitude was under-stated by 2×.

## 4. Where the other 72 ms went, and what is not claimed

`profile_full_ns` gave 165.20 ms and the row's formula recorded 93.13 ms. The difference is not a
rounding: it is rises in terms the treatment made **strictly smaller**, in a window whose own untimed
witness says the machine was slower.

| term | N1 | O1 | movement |
| --- | ---: | ---: | ---: |
| `diag_commit_total_ns` | 416.39 ms | 455.52 ms | **+39.13** (bucket `profile_commit_ns` +39.12) |
| `teardown_ns` = `diag_finish_drop_ns` (excluded from the formula, inside the accept span) | 74.65 ms | 91.73 ms | **+17.08** |
| `profile_sql_ns` (contains `insert_objects_ns` +9.25) | 318.86 ms | 331.94 ms | **+13.08** |
| `diag_seal_total_ns` | 360.07 ms | 376.75 ms | +16.68 |
| `diag_wave_ns` | 63.87 ms | 69.31 ms | +5.44 |
| `diag_write_pack_total_ns` | 200.52 ms | 204.88 ms | +4.36 |
| `profile_group_ns` | 18.43 ms | 21.14 ms | +2.71 |
| `diag_validate_ns` | 47.63 ms | 50.11 ms | +2.49 |
| **untimed witness: `construct_ns`** | 447.67 ms | 458.17 ms | **+2.34 %** |
| **untimed witness: `construct_noise_ns`** | 113.11 ms | 118.74 ms | **+4.97 %** |

The two witness lines are the **same harness code on the same fixture, inside neither timer**, and they
say this window was 2.3–5.0 % slower. Every store-side term above also did *less* work in this window:
332 KB fewer bytes to the pager, 2 fewer pack writes, 3 fewer packs, 5 fewer statements, the same
25,245 rows and the same 284 transactions. **No mechanism in this change raises any of them, and this
round attributes none of it** — exactly as round 13 declined to attribute its own unexplained
`diag_commit_total_ns` rise of 331.2 → 416.4 ms. `teardown_ns` is the operating system's price for the
pages the process dirtied and this lane has measured it at 6.7 → 469.9 ms on identical source.

Stated as arithmetic and labelled as inference, not measurement: at the witness's own 2.3–5.0 % the
five rising store-side terms would account for roughly 20–35 ms of their 65 ms, leaving 30–45 ms
unexplained by drift and unclaimed by this round.

## 5. What the treatment is, and what it is not

`STORED_TAG = 2` joins `FULL_TAG` and `PREFIX_TAG` in the payload record grammar
(`encoding/delta/record.rs:29`). A payload is stored verbatim when a bounded **1024-byte prefix**
compressed under the same profile shrinks by less than 1/32, or when the frame that came back is not
smaller than the payload it describes. The stored record is the tag and the payload, with no base and
no frame; a reader that meets it returns those bytes and checks the declared raw length against them
rather than trusting either.

**Both directions of the read path, versioned.** Whole-file 11 → 14, native 10 → 15, singleton 13 → 16.
The new reader still maps 11, 10 and 13 to the same lanes and the old tags keep their meaning, so every
pack this Store has written is still read; a reader that predates the change refuses a new pack by
version at `parse_header` with `UnsupportedPolicy { field: "pack framing version" }` rather than
meeting an unknown tag mid-decode. `Ordinary` (9) and `PooledMetadata` (12) are untouched because
neither carries a payload frame.

**The declared trade stands and is not walked back.** This fixture is `fixture::noise`, the maximum
case: 100 % of payloads are incompressible, so every probe is a pure saving. On compressible content
the probe adds one bounded codec call per payload *and then compresses anyway*, and where a payload's
first 1024 bytes are unrepresentative of the whole the Store keeps bytes zstd would have removed. The
size half is bounded by the 1/32 threshold; the time half is ~2.8 µs per payload, which on a
compressible fixture is a cost this round did not pay and cannot measure.

## 6. Checks as run, and what was not run

- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast` — **629 passed / 0 failed**
  (round 13's tree: 621; the new `tests/stored_payloads.rs` adds 8).
- `-p layerfs-storage` alone: **223 passed / 0 failed** across 33 binaries, including `cas_reuse`,
  `delta_payload`, `pack_watermark`, `multi_writer`, `visibility`, `persistence_failure`,
  `pack_locator`, `physical_formats` and the new `stored_payloads`.
- `clippy --locked --manifest-path core/Cargo.toml --all-targets` clean;
  `fmt --manifest-path core/Cargo.toml --all --check` clean;
  `core/tools/check_product_boundary.py` **PASS** (194 production files).
- Harness build: `cargo +1.85.1 build --release --locked --manifest-path
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml`, from the repository root, on the commit
  before the run. Harness suite: **120 passed / 3 failed**, the three being the pre-existing
  `registry_negative` cases of the handoff's §7 (registry 221 rows against a frozen 220), untouched.
- The row: one sample, one arm, fresh `--out`, `--verify full --no-build`, `samples_per_case_per_arm`
  1, `cache_state` "declared per row; never pooled".
- **Not run:** the reference `crates/` workspace; any other harness case or lane; any second sample of
  this arm. The harness's `registry_negative` repair is not attempted here.

## 7. Two existing cases changed with the format, and are reported rather than adjusted

1. `delta_chains.rs::a_corrupt_intermediate_is_rejected_during_reconstruction` builds its chain from
   **compressible** content now. Its claim is exact — a damaged *frame* is refused by the frame's own
   checksum — and `noise` is no longer a frame on this path. The stored form's half of that guarantee
   is a new case in `stored_payloads.rs`: a damaged stored payload is refused by the dependency
   identity, and is never served.
2. `pack_locator.rs::a_whole_file_record_is_stored_in_its_own_compact_pack` asserts the **lanes**
   (`parse_header(...).lane == PackLane::WholeFile` and `lane.version()`), not the literal version that
   names the compact framing, because that version moved. The versions themselves are enumerated in
   `physical_formats.rs`, which now carries all eight implemented framings.

## 8. Production LOC

**31426 -> 31558 (delta +132).** Method `python3 tools/production_loc.py --root <tree>`, first parent
`70366dd81` against the committed tree `4d8e6e2ab`. Scope: first-party product implementation
(`core/crates/*/src` and shipped runtime SQL). The harness driver change and the new test file are
outside that scope and contribute 0.
