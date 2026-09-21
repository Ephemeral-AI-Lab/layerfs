# #219 — the pack format is the lever: a reserved directory and an in-place append

Worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch `codex/219-ns10000`,
from `9c46930b8`. Pre-registration: `pre-registration.md` in this directory, written before the
first run. Raw receipts: `receipts/`. Re-derivation: `python3 attribute.py` prints every number
below from those receipts.

**One line: the treatment is `T1c` — the group directory moves into a region reserved at the
lane's own width and a pack declares its own length, so an append writes only the bytes it adds
through incremental BLOB I/O. `operation_ns` 3524.0 -> 2776.3 ms (−21.22 %), CPU 3274.3 -> 2533.8 ms
(−22.6 %), pack bytes written 2,292,865,337 -> 302,406,480 (7.58x), row PASS, all 14 pinned
counters and `digest:filesystem_root` unchanged.**

## 1. The identity arms, and why there are two of them

| arm | what it is | store sha256 | `operation_ns` |
| --- | --- | --- | ---: |
| campaign baseline | `ns17-squadA-packcounters-…`, the row the handoff names | `03918d61…` | 3351.0 ms |
| **A0** | this worktree, unmodified tree, fresh `--out` | `03918d61…` **byte-identical** | 3490.3 ms |
| D1a | + the driver's own three spans (diagnostic) | — | 3653.0 ms |
| **A1** | + the uncharged-region instrumentation (diagnostic) | `03918d61…` **byte-identical** | 3524.0 ms |
| T1 | the treatment, before the pack-bytes counter existed | `d8cd2384…` | 2735.5 ms |
| T1b | the treatment + `pipeline.pack_bytes_written` | `d8cd2384…` | 2742.0 ms |
| **T1c** | the treatment at the **committed source**, re-run | `d8cd2384…` | 2776.3 ms |

A0 reproduces the baseline's store **byte for byte** and every pinned counter exactly, so this
worktree is the same product the campaign measured. A1 is the comparison arm for T1: the same
work measured through the same instrument, because D1 added charge sites the baseline row does not
have. The diagnostic instrument costs `3524.0 - 3490.3 = +33.7 ms (+0.97 %)` — stated, not hidden.

Three T1 rows exist because the treatment was measured before (`T1`), with (`T1b`) and after
(`T1c`) the pack-bytes counter was added, and `T1c` is the one whose source is the committed tree.
They are **not** a best-of: 2735.5 / 2742.0 / 2776.3 ms is a 1.5 % spread and **all three are
reported**, with `T1c` quoted as the row. Their stores are byte-identical to each other
(`d8cd2384…`), which is the determinism claim, not a timing claim.

## 2. What was changed

**D1 — the remainder, charged by name (diagnostic, `9c46930b8`+).** `SaveProfile` gains a
`diag: DiagProfile` of **region totals**; nothing is added to `total_ns()`, `SaveOutcome`'s
equality ignores them exactly as it ignores `profile`, and no decision reads one. The driver
publishes them as `pipeline.diag_*` and its own three spans as `pipeline.span_*`.

**T1 — the pack format (the treatment).** Four things, and they are one change:

1. **The directory is reserved.** A pack is now `[control 24][directory entry_len x
   group_count_limit][bodies…]`. The region is allocated whether or not the pack fills it, so a
   body's offset no longer depends on how many groups precede it. Before, the directory grew into
   the pack's front and **every append shifted every body behind it**.
2. **The pack declares its own length** (`USED_OFFSET`, a 32-bit field in the control area), so a
   pack row may be allocated with spare capacity.
3. **An append writes in place**: `zeroblob(capacity)` at creation and
   `sqlite3_blob_write` (rusqlite's `Blob::write_at`) for the new bodies, the new directory entries
   and the control area. Nothing is reassembled and nothing already written is touched.
4. **`SCHEMA_VERSION` 8 -> 9 and the framing versions 1/2/4/6/7 -> 9/10/11/12/13.** A schema-8
   Store is refused at open by the existing `user_version` check, before any pack is read; the old
   framings are refused by the same `UnsupportedPolicy { field: "pack framing version" }` path that
   already refuses versions 0, 3, 5, 8 and 99. Both directions are refusal, not a guess.

**Why the length cannot ride in a companion column.** It is the obvious design and it does not
work: SQLite rebuilds and rewrites the whole row for any `UPDATE`, so `UPDATE object_packs SET
used = ?` on a 256 KiB row costs the rewrite the format exists to remove. Measured directly on a
256 KiB `zeroblob` row (Python's sqlite3, one commit per statement, so the figure is an upper bound
that includes ~10 us of interpreter overhead): `UPDATE used` **72.5 us/op** against a four-byte
in-place BLOB write **11.1 us/op**. The declared length therefore rides inside the BLOB and the row
is only ever read after it is created.

**Why the directory region is mandatory even with BLOB I/O.** Partial writes alone change nothing:
with the directory at the front, appending group *k* moves bodies `0..k` by `entry_len` bytes, so
an append that writes only its own bytes would corrupt the pack. Reserving the region is what makes
an append local; the BLOB interface is what makes it cheap.

**The ownership guard survives, as a read.** The old `UPDATE … WHERE save_id = scope AND EXISTS
(unpublished)` is now a `SELECT` of the same predicate over the row's small columns, run **before**
any byte is written. It is fail-closed (a foreign or published pack is refused, not written into
and rolled back) and it never modifies the row. `tests/persistence_failure.rs`, `multi_writer.rs`,
`pack_watermark.rs` and `visibility.rs` cover it and are green.

## 3. The result, in the instrument's own units

`A1 -> T1c`, same instrument, same work, one sample each:

| instrument | A1 | T1c | movement |
| --- | ---: | ---: | ---: |
| `operation_ns` | 3524.0 ms | 2776.3 ms | **−747.8 ms, −21.22 %** |
| CPU (user+system) | 3274.3 ms | 2533.8 ms | **−740.5 ms, −22.62 %** |
| `complete_command_ns` | 4783.7 ms | 4051.4 ms | −732.3 ms, −15.31 % |
| `accept_span_ns` | 3519.5 ms | 2772.5 ms | −747.0 ms |
| `profile_commit_ns` | 1133.6 ms | 682.5 ms | −451.1 ms (0.602x) |
| `profile_sql_ns` | 780.6 ms | 498.7 ms | −281.9 ms (0.639x) |
| `profile_place_ns` | 78.6 ms | 19.0 ms | −59.6 ms (0.242x) |
| `diag_write_pack_total_ns` | 605.6 ms | 316.0 ms | −289.6 ms (0.522x) |
| `profile_full_ns` | 244.2 ms | 252.1 ms | +7.9 ms (uncharged work unchanged, inside noise) |
| `pack_bytes_written` | not published | **302,406,480 B** | vs the campaign's 2,292,865,337 B = **7.582x** |
| `packs_created` | 1250 | 1268 | +1.44 % (declared tolerance 5 %) |
| `pack_appends` | 15552 | 15534 | −0.12 % |
| `commits`, `inserted`, `statements` | 17378 / 25245 / 16595 | same | unchanged |

**The amplification is gone**: 302,406,480 bytes written for 302,023,232 persisted is **1.0013x**,
against the campaign's **7.5917x**. The 0.13 % above 1.0 is the control area (24 B per pack write)
plus one directory entry per group — the bytes that genuinely have to be written.

Unchanged identity: all **14 pinned counters** reproduce and `digest:filesystem_root` is
`1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847` on every row in this directory.
The row is **PASS, 13/13 gates**.

Declared in advance and now measured — the store **does** differ: `sample.sqlite` is
336,400,384 B against 307,879,936 B (**+9.26 %**), because a non-singleton pack row allocates its
lane's whole 256 KiB limit. Its sha256 is `d8cd2384…`, not `03918d61…`. The claim is that the
*stored canonical identity* is unchanged, not that the file is. `verification_wall_ns` moved
+6.8 ms (+2.0 %) — the read path materialises the padded capacity before truncating to the declared
length; that is the cost of the padding on reads, and it is small here because these packs are
nearly full (302 MB of bodies in 1,268 packs of 256 KiB).

## 4. The remainder, attributed by name (D1)

A1's remainder against the accept span was **1257.6 ms of 3519.5 ms (35.7 %)**, and it is now
attributed. Every figure is a **region total**, so a region containing one of the seven buckets
contains that charge too; the residue is the difference.

| named region, A1 | ns | share of span | note |
| --- | ---: | ---: | --- |
| the driver's own C1 construction, inside the timer | 365.0 ms | 10.4 % | `span_build_ns`; caller work, charged to nothing by the product |
| `BEGIN IMMEDIATE` + pack-watermark read, 17,378x | 202.8 ms | 5.8 % | `diag_begin_ns`; **`SaveProfile`'s own doc claims this is charged to `commit_ns` and it is charged nowhere** |
| `offer` outside full/delta/resolve/seal | 207.7 ms | 5.9 % | framed-length arithmetic, group accounting, `records.push` (~302 MB of copies) |
| per-wave locator query + presence seed, 587 waves | 107.6 ms | 3.1 % | `diag_wave_ns` |
| `validate_candidates` outside its queries | 44.4 ms | 1.3 % | `diag_validate_ns − diag_collision_query_ns` |
| `validate_candidates`' per-row queries, 25,245x | 75.7 ms | 2.2 % | one `SELECT` per row inside a call made once per seal |
| `flush_batch` outside wave + offer | 46.0 ms | 1.3 % | wave bookkeeping, membership |
| the content loop outside `flush_batch` | −24.1 ms | — | residue; negative means the plumbing spans overlap the build span |
| `seal_group` outside its children | ~39 ms | 1.1 % | by subtraction of the charged children |
| `finish` span outside `finish_inner` | 257.0 ms | 7.3 % | the final `flush(remaining)` drain; `finish_inner` itself is 1.4 ms |

Two of these are *product* costs nobody had charged and are the next levers: **`BEGIN IMMEDIATE`
(202.8 ms, 5.8 %)** — matching the campaign's independent 3.9–6.4 % cadence bound, and it is the
cadence contract, so it is not free to remove — and **`validate_candidates` (120.0 ms, 3.4 %)**,
which issues one query per row where one paged query per seal would do.

After the treatment the same remainder is **1294.7 ms of 2772.5 ms (46.7 %)**: it did not grow, but
the span it is measured against shrank, so the *share* did. That is the honest reading, and it is
why the next round should start at `BEGIN IMMEDIATE` and `validate_candidates` rather than at the
write path.

## 5. Verification — exactly what ran, and what did not

Ran, all green, on the committed tree:

- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage` — **33 test
  binaries, 0 failures** (`tests/full-suite.txt` in this directory).
- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` — **the whole core workspace, 108
  `test result: ok` lines, 0 failures, exit 0** (`tests/core-workspace.txt`).
- `cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml -p layerfs-storage --all-targets`
  — clean.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml -p layerfs-storage -- --check` — clean.
- `python3 core/tools/check_product_boundary.py` — `PASS: scanned 194 production Rust/SQL files`.
- The row itself: `--verify full`, 13/13 gates, 14/14 pinned counters, root digest reproduced.

**Not run, and not claimed:** the reference `crates/` workspace's own tests; any other harness case
or lane (only `pipeline-namespace-10000` was measured); the `history.*`, `c2.*` and `pool.*`
families; a durability run (the connection profile is unchanged — `journal_mode = MEMORY`,
`synchronous = OFF`, no fsync — so this is a **format and cost** change, not a durability change);
and any multi-writer concurrency measurement beyond the unit tests named above.

The harness's own `run.json` reports `registry_self_check: FAIL` (`MISMATCH frozen cardinality
array`: expected `[…,2,4]`, actual `[…,2,5]`) on **every** row here including the campaign baseline.
It is a pre-existing registry cardinality drift, it is not a row gate, and it is reported rather
than repaired.

## 6. Cost, and what the change does not do

- **Files touched:** `core/crates/layerfs-storage/src/{pack/layout.rs, pack/assemble.rs,
  pack/placement.rs, pack/mod.rs, sqlite/write.rs, sqlite/lookup.rs, sqlite/schema.rs,
  sql/schema.sql, policy.rs, cas/owner.rs, cas/placement.rs, cas/selection.rs, cas/save.rs,
  cas/collision.rs, cas/lifecycle.rs, cas/store.rs, encoding/full.rs}` and the tests that read pack
  rows (`tests/support/{mod.rs, filesystem.rs}`, `cas_reuse.rs`, `provider_errors.rs`,
  `pack_locator.rs`, `physical_formats.rs`, `metadata_pool.rs`, `cas_roundtrip.rs`).
- **Production LOC: 30963 -> 31247 (delta +284), combined 96380 -> 96664.** Method:
  `python3 tools/production_loc.py --root <tree>` — the repository's own counter, core scope
  (`core/crates/*/src/**/*.rs` plus `<crate>/sql/*.sql`), tests/examples/benches excluded, counted
  against `9c46930b8` for "before" and the committed tree for "after" (`loc.txt`).
- **The read path materialises the padded capacity** before truncating. At this workload's fill it
  costs +6.8 ms on verification. A pack that is nearly empty reads 256 KiB where it used to read
  its own length; that is the price of the format and it is stated here rather than discovered
  later.
- **`packs_created` +1.44 %** because the reserved region costs each pack a fixed number of body
  bytes. Well inside the 5 % the pre-registration declared as a red flag.
- **The store is 9.26 % larger.** A store whose packs are mostly empty would be worse; the fix, if
  one is wanted, is a growth schedule for the capacity, and it is not in this change.

## 7. Prior art: this is the design L62 measured on #209, and why this is not that revert

The reserved-directory format is **not a new idea in this repository and this round did not
invent it**. L62 (`docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`, 2026-09-21,
report in `.../stage-6-history-209-format-20260920T222118Z/`) implemented the same design on the
`history` lane, measured `commit_ns` **2.938 -> 1.388 s (−52.8 %)** and **34.158 -> 3.353 pages per
commit**, and was then **reverted by owner direction to close #209**. That round's own report says
in as many words that the revert is *not* a refutation ("The change worked and was still
reverted"). The retained diff is `stage-6-history-209-format-20260920T222118Z/scratch/format.diff`.

What this round adds, and what it does not:

- **The design is confirmed on a second, independent case.** #209 measured `history` at 48,446
  commits; this round measures `pipeline-namespace-10000` at 17,378. `commit_ns` moved
  **1133.6 -> 682.5 ms (−39.8 %)** here against L62's −52.8 % there, and the pack bytes written
  fell **7.58x**. Same mechanism, different magnitude, because the cases differ.
- **The authority is different.** L62 was reverted to close #209. This round's handoff grants the
  format change explicitly: "L1 — the pack format... This is the one... You have the authority to
  change it", and "accept that a measurement arm produces a different store, a different file hash
  or a different pack count". The store hash moving is declared here rather than treated as a
  failure.
- **One deliberate difference from L62.** L62 pre-allocated the row in `max(32 KiB, pack_limit/8)`
  steps. This round allocates the lane's whole pack limit at creation. The step schedule writes
  less to disk when a pack never fills but **rewrites the row at every reservation step**, which is
  the cost this format exists to remove: at eight steps to 256 KiB a pack would submit
  `32+64+…+256 = 1152 KiB` for a 256 KiB pack, roughly 3.8x the bytes this round submits, in
  exchange for the 9.26 % of file size this round spends. Both are defensible; this one is chosen
  because the row's instrument is bytes written and time, not bytes resident. A store whose packs
  are mostly empty would want L62's schedule, and that is the first thing to change if the file
  growth ever matters.
- **The tamper-helper trap L62 found was found here too, and one more site.** L62 records that four
  helpers corrupted the last byte of the row, which under a pre-allocated row is padding, so they
  silently stopped testing anything. The same class of break appeared here in `cas_reuse.rs`,
  `support/mod.rs` (twice), `support/filesystem.rs` and `provider_errors.rs`, plus two read sites
  that would have inspected padding (`pack_locator.rs`'s framing assertion and `metadata_pool.rs`'s
  pooled-leaf damage). All are corrected to read the pack through `support::read_pack_row`
  (truncate to the declared length) and to write it back through `support::write_pack_row` (pad to
  the row's own capacity, so a tamper cannot break the *write* path by shortening the row).

## 8. Files

| file | what it is |
| --- | --- |
| `pre-registration.md` | the registration, written before the first run |
| `attribute.py` | re-derives every number in this README from the receipts |
| `receipts/<row>/receipt.json` | the runner's receipt, byte copy |
| `receipts/<row>/{run.json,timing.json,phases-perf.json,trace.jsonl}` | the rest of the row |
| `stores.txt` | store size and sha256 per row, including the byte-identical baseline |
| `loc.txt` | the production LOC comparison and its command |
| `tests/full-suite.txt`, `tests/core-workspace.txt` | the test runs quoted above |
| `issue219-status-comment.md` | the status comment for #219 |
