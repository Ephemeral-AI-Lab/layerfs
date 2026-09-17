## 6. Statistics, efficiency and memory

**Headline: no v0.1.6-versus-candidate matched measurement of any kind exists for this
batch — not time, not storage, not memory — and none is claimed by the implementation
itself.** What exists is (a) a correctness and footprint test suite, (b) one 21-arm
debug-profile wiring round, (c) one single-sample, in-process, Store-free C1 pair, and
(d) one six-phase heap ledger on one shape. Everything below is labelled accordingly. A
missing counter is `null` with a reason; nothing is reported as zero.

### 6.1 Time (measured observations)

| Operation | Value | Unit | n | Includes / excludes | Source |
| --- | --- | --- | --- | --- | --- |
| C1 edit, small case (this review) | 1.388 ms | wall, in-process `file.edit` | 1 | includes base read, no-op compare, construction and consumer wait; excludes any DB | `examples/c1-small.txt` |
| C2 save-to-ack, small case (this review) | 134.467 ms | wall, `storage.save` | 1 | includes `store.create` 102.748 ms, `storage.finish` 11.068 ms, `storage.read` 5.867 ms | `examples/c2-small.txt` |
| Integrated edit-to-ack, chunked (this review) | 11.751 ms | wall, `edit.save` | 1 | the synchronous span includes the consumer; `file.edit` is 7.775 ms of it | `examples/pipeline-chunked.txt` |
| Authenticated readback, chunked (this review) | 16.689 ms | wall, `verify.readback` | 1 | timed separately from the acknowledgement | same |
| Integrated small→large at T = 1 MiB | 218.182 ms | wall | 1 | `content.chunk` 153.355 ms dominates; readback 110.598 ms is separate | `examples/grow.txt` |
| Integrated large→small at T = 1 MiB | 8.052 ms | wall | 1 | readback 56.597 ms is separate | `examples/shrink.txt` |
| Complete command wall, 21 declared arms | 0.047 – 3.155 s | wall | 1 each | debug profile; the wrapper that appends `wall_seconds` is not retained, so this is a declared, not recomputable, metric | timing ledger |
| Matched C1 pair | reference 224 875 · candidate 186 000 | ns | **1 per arm with receipts** | in-memory, no Store/pack/DB; three further observations exist only as ledger prose | matched ledger |

**Do not read a speedup from the last row.** The ledger's own text: "**Latency: the gate
stays unqualified.** The measured ordering is not stable and one sample per arm cannot
support either 'existing-or-better' or 'slower'." The recorded observations interleave
(reference 224 875–351 666 ns, candidate 186 000–468 333 ns) and three of the seven arm
executions have no receipt. No p95, no variance and no statistical-confidence claim is
made or supportable. Cold-cache contrast: not measured. Release-profile arm: not run.

**Timer cost.** Two on/off pairs exist (`e3a-pooled-24-off`, `e3c-pipeline-chunked-off`).
Product output is identical between on and off — independently confirmed, and the three
pipeline arms produced a **byte-identical `store.sqlite`** (`544340225c90…`). Node counts
and clipping: `e1c-pooled-512` is **telemetry-clipped**, with the root marked
`"incomplete": true` and 57.46 % of its scope unattributed (this review recomputed 768
retained children summing 1 320 950 951 ns of a 3 105 519 375 ns root). The round
discloses this and does not relabel it.

### 6.2 Work (source-derived where not stated)

| Quantity | Value | Provenance |
| --- | --- | --- |
| SQL statements / transactions per save | not counted by any tool | **null** — no query counter exists in the product or the examples |
| Source/base/pack reads, scanned bytes | not counted for payload lanes | **null**; `StoreReadCounters` exists but the pooled lane's pack fetches are not counted even internally |
| Canonical copies / hashes / encodes | **source-derived only** — `w3/README.md` labels its encode/decode/hash counts "source-derived, not instrumented" | source |
| Codec trials | exactly one per object by construction (`delta/select.rs`) | source, plus `trials 1` printed by the C2 example |
| Pack assemblies | one selected assembly per pack; `plan_lane` builds the record twice on the oversized path (SIM-7) | source |
| Exact reuse | the C2 example prints `reused`; `--case small-to-large` shows `inserted 65 reused 53` (53 same-save duplicates reused) | measured, this review |
| Delta wins / losses | `--mode c2 --case small` → `prefix records 1 full records 1 trials 1` | measured, this review |
| Chain depths / work actually observed | `edit_bounds` asserts `peak_deferred_bytes` bounds; `w10` records `nodes_read: 85116 … peak_deferred_bytes: 353952` and `nodes_created: 122 … 384112` | measured (closeout round) |

### 6.3 Selection and storage

Candidate and usable counts, FULL/DELTA win reasons and observed chain depths are
exercised by tests but **not emitted as counters** by any example, so no per-case table
exists. What is measured:

* **Retained footprint** (one sample of a deterministic 1 048 583-byte chunked fixture):
  `raw=1048583 canonical=1052271 objects=60 inserted=60 packs=6 pack_bodies=1052818
  largest_pack=259777 database=1204224 files=["store_bytes.sqlite"]` — six pack bodies
  inside the ordinary 262 144-byte limit, all inside one 1 204 224-byte database, with no
  pack, payload or spool file on disk. Pack bodies are reported apart from the database
  that contains them, and no database size delta is presented as write I/O. Verified
  verbatim against `w8/w8-verify.log:112`.
* **Occupancy and written-versus-rewritten bytes:** not reported anywhere. **null.**
* **Total retained value-group bytes and index bytes:** the index reports
  `pool_index_entries`/`pool_index_bytes` (`w7`: 2 400 entries / 57 600 B at that
  fixture's shape; the 3.00 MiB modelled cap at 131 072 entries is derived, not measured).
  Group bytes: **null.**
* **Frame size alone is insufficient**, and is not used as evidence anywhere here.

### 6.4 Memory — two separate conclusions

**(A) Bounded resource use.** A declared allocation ledger exists (15 owners in
`w7/README.md`), and the C1 frontier bound is asserted by **real oracles**:
`peak_deferred_bytes` ≤ 64 KiB at 1–16 edits, < 512 KiB at the 4 096-edit ceiling, and
≤ tree + 8 KiB for 1 024 in-place edits — against a control without release of
`[6308, 12856, 26672, 57184, 129728]`. Per-owner bounds, multiplicities, lifetimes and
release events are documented and mostly enforced by checked arithmetic with fail-closed
errors. **Owners with no constant, no ledger row, or a bound that does not cover the
peak:**
1. `pending_values` (`cas/owner.rs:121,479`) — unbounded per-save map (F-10).
2. The assembled-pack copy overlapping the retained open tail
   (`pack/assemble.rs:175–252`) — ≥2× the lane limit live; ~2 × 16.78 MiB on the singleton
   lane, uncharged (F-11).
3. SQLite's own BLOB copy plus the MEMORY journal of the same write — the journal, not the
   Rust buffers, is the largest writer-side allocation on the singleton lane.
4. Transaction row/byte limits enforced *after* the write, so one group plus one pack can
   overshoot (F-13).
5. The final pack `DELETE` in cleanup is one unbounded transaction (F-12).
6. No `cache_size` or `mmap_size` pragma anywhere; SQLite's page cache, `temp_store`
   b-trees, statement/bind copies and connection count (unbounded, one per
   `begin_save`/`read_batch`/`contains`) are charged to no budget.
7. Derived-but-uncharged growth inside the C1 frontier owner: `committed`, `parent_refs`,
   `detached`, `published` (≈0.5× the charge, unasserted); `settle()` is O(n²) in
   `detached` (CPU, not memory).
8. A `MutationOwner` holds two 4 MiB pack caches simultaneously (`owner.rs:111` and
   `owner.rs:121` → `pool/read.rs:32`).

Also: the memory ledger counts **only the Rust global allocator** — it cannot see
libzstd's mallocs or libsqlite3's heap. **SQLite `cache_size` is not total RSS**, and no
`cache_size` is set at all.

**(B) Memory safety.** C1 and telemetry are `#![forbid(unsafe_code)]` with zero `unsafe`.
All twelve product `unsafe` sites are in `encoding/codec.rs` and were read individually:
static zstd contexts are 8-byte aligned, exactly sized and null-checked, and libzstd
refuses rather than overruns; every decompression allocates the destination to the
declared raw length and validates magic, reserved bits, frame type, `frameContentSize`,
window size, dict ID and checksum **in Rust before** the call, then re-checks the produced
length; `MaybeUninit`/`assume_init` runs only on the zero return of a call that memsets
the struct upstream; both borrowed-prefix installs are cleared on success and on inner
failure, and every entry point resets before use, so no use-after-free is reachable.
Allocation and offset arithmetic is `checked_*` on every untrusted length except one raw
expression (`encoding/decode.rs:186`, `4 * count` on an attacker-declared `u32`) — safe
on 64-bit, a panic/DoS shape on 32-bit only, and untested there. Both chain resolvers are
iterative, so depth cannot exhaust the stack. Intermediate authentication is genuine at
every boundary (pack header → directory → record → canonical length → object identity →
pooled group digest → chronology). **Safe Rust or low RSS does not prove dependency
safety**: C2 crosses raw FFI to libzstd and links the **system libsqlite3**
(`libsqlite3-sys` depends only on `pkg-config`/`vcpkg`; no `bundled` anywhere), and no
test ever calls the codec (F-22). Public-input coverage exists and is meaningful —
including a **searched real 64-bit fingerprint collision pair** for the pooled filter —
but several corruption oracles are union or blanket matches, there is no miri/ASan/fuzz
configuration, and no `#[should_panic]` case exists. One documentation defect is a
safety-adjacent accuracy issue: `codec.rs:6–8` claims "every allocation is charged before
the call", which prefix **decode** violates (one `ZSTD_DDict` malloc per call, bounded to
one live per workspace, invisible to the ledger).

**(C) What is unmeasured.** Peak heap or RSS for **HEAD** — the only instrument is
`examples/memory_ledger.rs` and its only recorded run is from `aa4b5a9e4`, not HEAD. There
is therefore no sampled phase peak for this commit, no lifetime high-water for this
commit, and no peak-sampling coverage statement that would let one be derived. The
131 072-entry `BTreeSet`'s real node and allocator cost is modelled at 3.00 MiB but
unmeasured; `metadata_pool_index.rs:268–294` bounds `bytes <= 131072*16 + 1024` = 2.0 MiB,
which only passes because the fixture retains ≈1 000 entries — it does **not** bound the
cap.

### 6.5 Scaling

Real tested sizes: files 0, 1, T−1, T, T+1, 2T, 1 048 575 and 2 097 151 bytes; cutoffs
128 KiB, 256 KiB, 512 KiB and 1 MiB; edits 1 … 4 096 per operation; pooled values at
24/128/512 leaves in the timing round and 131 072 entries in the in-process window test;
chain depth 50 on the chunk lane. **No memory or work *trend* is available** beyond the
frontier peak vector, because no counter campaign was collected. No extrapolated
performance at any theoretical maximum is stated in this review. The synthetic 8 MB/24 MB
extent fixtures in `edit_localized` support **structural** claims only and are not used
for performance.

---

