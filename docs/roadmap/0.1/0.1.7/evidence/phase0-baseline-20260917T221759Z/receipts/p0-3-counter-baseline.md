# P0-3 — per-phase counter baseline (core only)

> **Status:** Collected receipt. Workload set frozen in `../CONTRACT.md` §5 before
> collection. Identities: source `dcedd7ef1`, clean tree over `core/crates`+`crates`,
> `release`, Rust `+1.85.1`, `--locked`, **one worker**
> (`LAYERFS_CONSTRUCTION_WORKERS=1`), one sample per workload. The Phase 1 items
> verify against these numbers.

## 1. How to read this receipt

- Every **counter** below is a single gate sample. Determinism is stated per
  counter in §6 against one labelled diagnostic re-run.
- Every **`elapsed_ns`** figure is **diagnostic-grade**: a single sample, no
  best-of, not a gate, and it moved between identical runs (§6).
- `SortedWork.pages_read` is recorded with the standing caveat that it currently
  **undercounts batched decodes** (#178 **P1-11**). Read it as a lower bound.
- A counter the vehicle does not print is recorded as **`NOT_EXPOSED`** with the
  reason, never estimated — the contract forbids hacking a counter into a harness.

## 2. C1-only tree operations — `filesystem_timing_c1` (D1–D6)

| case | elapsed ns (diag) | objects read | read_waves | bytes read | objects emitted | bytes emitted | dir pages read/created/reused | dir scratch | inode pages read/created/reused | inode scratch | ref rows | ref spilled | ref values | ref removals | ref released | ref peak pending | validation entries | validation objects_read |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `empty` | 28,583 | 0 | 0 | 0 | 1 | 129 | 0/0/0 | 0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | `NOT_EXPOSED` | `NOT_EXPOSED` |
| `directory-update` | 240,125 | 6 | 4 | 19,586 | 1 | 129 | 1/0/1 | 67,708 | 1/0/2 | 67,644 | 1 | 0 | 1 | 0 | 0 | 1 | `NOT_EXPOSED` | `NOT_EXPOSED` |
| `inode-update` | 34,833 | 1 | 1 | 368 | 2 | 497 | 0/0/0 | 0 | 1/1/0 | 2,856 | 1 | 0 | 1 | 0 | 0 | 1 | `NOT_EXPOSED` | `NOT_EXPOSED` |
| `hardlink-move` | 41,583 | 2 | 2 | 423 | 3 | 567 | 1/1/0 | 873 | 1/1/0 | 2,856 | 3 | 0 | 3 | 0 | 0 | 3 | `NOT_EXPOSED` | `NOT_EXPOSED` |
| `subtree-remove` | 228,416 | 6 | 4 | 16,797 | 3 | 298 | 1/1/0 | 605 | 1/1/0 | 67,644 | 202 | 0 | 1 | 201 | 200 | 202 | `NOT_EXPOSED` | `NOT_EXPOSED` |
| `attributes` | 19,083 | 0 | 0 | 0 | 5 | 507 | 0/0/0 | 0 | 0/0/0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | `NOT_EXPOSED` | `NOT_EXPOSED` |

Per-phase **diagnostic** elapsed for the same rows (ns):

| case | validate | directories | references | inodes | cleanup | root.encode |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `empty` | 4,750 | 84 | 292 | 542 | 167 | 1,334 |
| `directory-update` | 102,000 | 72,333 | 750 | 35,250 | 708 | 1,125 |
| `inode-update` | 2,750 | 333 | 459 | 7,792 | 541 | 833 |
| `hardlink-move` | 8,958 | 12,625 | 375 | 4,750 | 542 | 875 |
| `subtree-remove` | 1,625 | 10,042 | 2,916 | 60,917 | 250 | 625 |
| `attributes` | 1,542 | 41 | 167 | 417 | 208 | 708 |

`attributes` is the only case with in-region attribute work; its own row:
`set 1 removed 0 preserved 1 base entries 1 base pages 1 values emitted 1`, with
readback verified (`21 bytes "timed attribute value"`). Its base attribute page and
the inode page it looked the root inode up in are both printed as identities.

**`ValidationWork` is `NOT_EXPOSED` in this vehicle.** `filesystem_timing_c1`
prints no `entries_examined`/`objects_read` pair for validation; the per-phase
`validate` **elapsed** above is the only validation evidence this vehicle carries.
A vehicle that prints `ValidationWork` must be nominated before P1-4 can be
verified against it — recorded as a gap, not worked around.

## 3. `measure_filesystem` — C1 / C2 / pipeline (D7–D9)

| mode | elapsed ns (diag) | counters |
| --- | ---: | --- |
| `c1` | 154,500 | boundary objects_read 6, waves 4, emitted 4, bytes_emitted 7,471; sorted directories pages_read 1, pages_created 1; inodes pages_read 1, pages_created 2 |
| `c2` | 1,586,125 | inserted 12, reused 0, packs 2, **commits 1**, full_records 12, prefix_records 0, pooled 203 |
| `pipeline` | 858,875 (construction + handoff + save ack) | objects_emitted 4, bytes_emitted 7,471, saved inserted 4, reused 0, admitted 4; readback labelled separately at 2,277,542 ns |

All three modes report the **same root**
`681ab7e59a612a51c7ba5fe8699f7e65e54a18c42c744ddb0a1b230069558598` for the same
`directory-update` change set — the C1, C2 and integrated routes agree on the
result for this workload.

## 4. `measure_edits` — C1 / C2 / pipeline, five cases (D10–D24)

**The `nodes_read` anchor.** The vehicle `measure_edits` does **not** print
`nodes_read`, so every `edits.*` row below records it as **`NOT_EXPOSED`**.
The counter exists in the product
(`core/crates/layerfs-content/src/file/edit/tree.rs:39`, incremented at `:154`;
`.../file/mapping/read.rs:46`) and a vehicle that prints it does exist
(`edit_timing_c1`), collected as **D27** below. **P1-6/P1-7/P1-9 must be verified
against D27, whose `nodes_read` is 9**, not against the `edits.*` rows.

### C1 mode — no database, pack or Store

| case | elapsed ns (diag) | emitted objects | emitted canonical bytes | readback |
| --- | ---: | ---: | ---: | --- |
| `small` | 86,875 (`file.edit`) | 1 | 65,559 | n/a (C1 only) |
| `chunked` | 48,792 | 3 | 4,947 | n/a |
| `small-to-large` | 385,417 | 18 | 263,269 | n/a |
| `large-to-small` | 19,375 | 2 | 510 | n/a |
| `batch` | 37,000 | 4 | 1,872 | n/a |

### C2 mode — storage only, fixed stored workload

`--mode c2` prints its own scope statement: *"supplied-object wiring probe —
`--case` selects the fixture only; the stored workload is a fixed truncation to
the whole-file limit plus one fixed patch, so it is not a per-case comparison"*.
All five C2 rows therefore carry the **same** stored workload (base 65,559
canonical bytes, patched 256 bytes at offset 16,384, dependent 65,559 bytes). This
is a property of the vehicle, recorded here so the rows are not misread as five
different workloads.

| case | `storage.save` ns (diag) | save result | readback ns (diag) |
| --- | ---: | --- | ---: |
| `small` | 6,952,000 (store.create 4,606 µs, begin 406 µs, finish 1,154 µs) | inserted 2, reused 0, prefix 1, full 1, trials 1 | 436,709 |
| `chunked`/`small-to-large`/`large-to-small`/`batch` | as above (same stored workload) | as above | as above |

### Pipeline mode — C1 edit → C2 save acknowledgement

| case | `edit.save` ns (diag) | inserted | packs | prefix | full | readback |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `small` | 1,038,000 | 1 | 1 | 1 | 0 | verified byte-for-byte (65,536 bytes) |
| `chunked` | 9,347,000 | 3 | 2 | 0 | 3 | verified byte-for-byte (262,144 bytes) |
| `small-to-large` | 2,655,000 | 15 | 2 | 0 | 15 | verified byte-for-byte (262,143 bytes) |
| `large-to-small` | 825,166 | 2 | 1 | 0 | 2 | verified byte-for-byte (131,072 bytes) |
| `batch` | 6,601,000 | 4 | 2 | 1 | 3 | verified byte-for-byte (194,308 bytes) |

Every pipeline row's readback is verified byte-for-byte against the independent
model inside the vehicle. Raw values for all twenty-four rows are in `../logs/`.

## 5. The two anchor shapes

### `c2.ceiling` and `c2.small` (shared with P0-2)

| workload | rows | canonical bytes | elapsed ns (diag) | commits | packs created | pack appends |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `c2.ceiling` (= S1) | 8,191 | 1,023,875 | 232,899,750 | 31 | 33 | 8,169 |
| `c2.small` (= S3) | 1,023 | 127,875 | 32,111,709 | 4 | 5 | 1,020 |

The ceiling is confirmed as the **declared 8,191-row** limit: at 8,191 objects the
save opens 31 transactions (8,202 rows each including one pack row per group)
rather than one. **P2-1/O1 and P2-2/O2 are anchored here**, and the spilling
determination for this shape is in `p0-2-spilling.md`.

### `order.default` (4,096) and `order.forced64` (D25/D26)

Fixture: 4,000-file base, 2,000 rename pairs, one update operation.

| counter | `order.default` (pending 4,096) | `order.forced64` (pending 64) |
| --- | ---: | ---: |
| `rows_spilled` | **0** | **3,968** |
| `rows_touched` | 2,001 | 4,001 |
| `runs.rows_read` | **0** | **59,007** |
| `runs.rows_written` | **0** | **25,760** |
| `runs.runs_created` | 0 | 124 |
| `runs.merges` | 0 | 61 |
| `runs.peak_run_bytes` | 0 | 568,320 |
| `runs.peak_live_runs` | 0 | 5 |
| `references.peak_pending` | 2,001 | 64 |
| caller backing held / peak | 0 / 0 | 0 / 568,320 |
| `objects.objects_emitted` | 14 | 14 |
| `directories.pages_read/created/reused` | 2/11/0 | 2/11/0 |
| `directories.peak_scratch_bytes` | 376,110 | 376,110 |
| `inodes.pages_read/created/reused` | 1/2/40 | 1/2/40 |
| `inodes.peak_scratch_bytes` | 349,820 | 349,820 |
| `objects.objects_read / read_waves / bytes_read` | 98 / 11 / 400,354 | 98 / 11 / 400,354 |
| elapsed ns (diag) | 11,595,375 | 131,018,834 |

**This is the single most consequential Phase 0 finding.** At the **default**
pending ceiling the ordering machinery does **no work at all**: zero spills, zero
runs, zero ordering reads and writes. The entire 2,000-pair touch set (2,001
pending rows) stays resident. The forced-64 shape is where the cascade appears.

`order.forced64` also **reproduces the Stage 5 round-4 grid exactly** on every
work counter — `rows_read 59,007`, `rows_written 25,760`, `runs 124`,
`peak_run_bytes 568,320`, `spilled 3,968`, `peak_pending 64` — against
`../stage-5-terminal-20260918T120000Z/ordering-scaling.log`. The counters are
therefore stable across the Stage 5 round and this baseline.

## 6. Determinism (labelled diagnostics)

| # | Re-run | Counters | Elapsed |
| --- | --- | --- | --- |
| X1 | `order.forced64` repeat (vs D26) | **all identical** (spilled, rows_read, rows_written, runs_created, merges, peak_run_bytes, peak_live_runs, peak_pending, backing, page counters, scratch) | 131,018,834 → 154,012,875 ns (**+17.6%**) |
| X1b | `order.default` repeat (vs D25) | **all identical** (all zero / 2,001 / 14 / 2,376,110 …) | 11,595,375 → 11,674,250 ns (+0.7%) |
| X2 | `c1.subtree-remove` repeat (vs D5) | **every counter identical** (objects 6/4/16,797/3/298; pages; refs 202/0/1/201/200/202) | 228,416 → 238,791 ns (+4.5%) |
| X3 | `component.primitives` re-run (vs C0) | identities MATCH; ratios 0.908649→0.883379 (`small`), 0.328582→0.382005 (`wide`), 0.519367→0.411692 (`large-few-changes`) | see `p0-1-matched-workload.md` §2 |

**Verdict on determinism, per counter class:**

- **Work counters are deterministic for the fixed input.** Every work counter on
  every re-run workload was **bit-identical**, including the reference's own
  comparison driver from a clean re-launch of both arms.
- **`elapsed_ns` is not a gate.** The same binary on the same input produced
  131.0 ms and 154.0 ms on the forced-64 shape (**+17.6%**) and moved by 0.7–4.5%
  and up to ~28% (component ratios) elsewhere. Every elapsed figure in this
  receipt is therefore **diagnostic-grade**, exactly as the contract declares.
- **Cross-run spread on `small` was comparable to the cross-tree delta** reported
  in `p0-1-matched-workload.md`; the two collections must not be compared as a
  regression signal.

## 7. Coverage and gaps

| Item | Status |
| --- | --- |
| Workloads in the frozen set | **all collected** (D1–D27) |
| `ObjectWork` (`objects_read`, `objects_emitted`, `bytes_read`, `read_waves`) | exposed by `filesystem_timing_c1`, `measure_filesystem`, the `order` probes |
| `SortedWork` directories + inodes | exposed; `pages_read` caveated (undercounts batched decodes, **P1-11**) |
| `ReferenceWork` incl. `runs.*` | exposed by the `order` probes only |
| edit `nodes_read` | **exposed only by `edit_timing_c1` (D27 = 9)**; `NOT_EXPOSED` in `measure_edits` |
| `ValidationWork` | **`NOT_EXPOSED` in every frozen vehicle** — needs a nominated vehicle before P1-4 |
| storage counters (packs, commits, pooled) | exposed by the C2/pipeline modes and the `c2` probe |
| per-phase elapsed | captured wherever the vehicle records a timing tree; otherwise **`NOT_EXPOSED`** |

**Two gaps are reported, not repaired:** `ValidationWork`, and `nodes_read` inside
the edit family. Both are vehicle limitations; adding a print would be a harness
change, which this campaign must not make.
