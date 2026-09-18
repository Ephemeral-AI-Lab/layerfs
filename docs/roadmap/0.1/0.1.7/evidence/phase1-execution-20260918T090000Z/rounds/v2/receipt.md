# V2 receipt — the edit-path memory and shape vehicles

> **Status:** Vehicle receipt (prerequisite V2 of the Phase 1 plan §1). No
> product, test or tool source is touched: one new **example** and two new cases
> in an existing one. Written once, from [`after/`](after/) (commit `582dea9dd`)
> under [`../../CONTRACT.md`](../../CONTRACT.md). V2 has no #178 checkbox of its
> own; its deliverable is the pair of observables P1-9/P1-8/P1-14 are gated on,
> and it is reported on #178 as a prerequisite.

## 1. The two gaps it closes

1. **No memory observable for P1-14.** `measure_edits` prints emitted objects and
   bytes only, and the whole-file route deliberately reports
   `EditCounters::default()` — pinned by `tests/edit_transitions.rs:771` — so a
   product counter would break that pin. P1-14's Big-O target is memory
   (~3n → ~n peak), and nothing in the frozen set printed a byte of it.
2. **No frozen fixture with the discriminating shapes.** Of the five
   `measure_edits` fixtures none is a **pure deletion with a printed
   `nodes_read`** (P1-9's positive anchor; `measure_edits` never prints it) and
   none is a **chunked base with a whole-file result** (P1-8's anchor —
   `large-to-small` deletes to exactly 131,072 bytes, which is *at* the cutoff,
   so it stays on the chunked route).

## 2. What V2 adds (examples only)

| Vehicle | Row | What it prints |
| --- | --- | --- |
| `examples/edit_memory_probe.rs` (new) | `M1` | `baseline_live_bytes`, `peak_live_bytes`, `peak_delta_bytes` under a counting `GlobalAlloc` across one `apply_edits`, plus root, emitted objects/bytes and `nodes_read` |
| `examples/edit_timing_c1.rs --case delete` | `M2` | the frozen vehicle's own fields for a 40,000-byte **deletion** from the same 3.3 MB chunked base |
| `examples/edit_timing_c1.rs --case shrink` | `M3` | the same fields for a deletion that leaves 131,000 bytes: chunked base → **whole-file** result |

The probe is a **requested-byte** ledger, not RSS: deterministic for a fixed
input, which is what a before/after comparison needs. It installs
`#[global_allocator]` in the example binary only; the product has no allocator
hook and no counter.

## 3. The rows (arm `after/`, commit `582dea9dd`)

Identities: clean tree, release, `+1.85.1`, `--locked`, one worker
(`LAYERFS_CONSTRUCTION_WORKERS=1`), one sample; `after/artifacts.txt` records the
four example hashes and the probe client. Elapsed figures are diagnostic.

### M1 — `edit_memory_probe` (the frozen `small` fixture, n = 65,536)

| field | value |
| --- | ---: |
| `cutoff_bytes` | 131,072 |
| `base_bytes` / `final_bytes` | 65,536 / 65,536 |
| `edited_root` | `8ddfe36cd5449f2b720590cb05da552290acaa5546ed065881c832703c650798` |
| `objects_written` / `objects_written_bytes` | 1 / 65,559 |
| `nodes_read` | 1 |
| `baseline_live_bytes` | 132,175 |
| `peak_live_bytes` | 394,503 |
| **`peak_delta_bytes`** | **262,328** |

**The plan predicted ≈ 3n ≈ 197 KB; the measurement is 4n + 184.** The fourth n
is `FileView`'s own copy of the base canonical (`view.rs:103-110`, n+23), which
the plan's estimate did not count — it is a base read, not assembly. The
assembly part is the predicted 3n: retained `out` (n) + `encode_whole_file`'s
value (n+10) + the emitted canonical (n+23). P1-14's gate is therefore
**262,328 → ≈ 2n + 46 = 131,118** (base copy + one pre-sized buffer), and the
handoff's "≤ ~2n" reads correctly against the measured baseline. The plan's
absolute figures are superseded by this row; its *direction* (≈half) is what the
gate uses.

### M2 — `edit_timing_c1 --case delete` (P1-9's positive anchor)

| field | value |
| --- | ---: |
| `edit` | `delete [1650000, 1690000)` |
| `edited_root` | `7d3eb265a63fa3644e08d3e1c97744f4ba384d4654c206137b8b89dab7b55c07` |
| `objects_written` / `bytes` | 4 / 7,174 |
| `nodes_read` | **4** |
| `mapping_pages` | 3 |

The result (3,260,000 bytes) stays above the cutoff, so this is the chunked route
with an empty replacement — the shape `rightmost_payload` runs on before the
`replacement_len == 0` check P1-9 adds.

### M3 — `edit_timing_c1 --case shrink` (P1-8's positive anchor)

| field | value |
| --- | ---: |
| `edit` | `delete [131000, 3300000)` |
| `edited_root` | `4a45d246028fe1edb5f3d7868ca81e39ae1d915733a53b866fe62f8af43ba313` |
| `objects_written` / `bytes` | 1 / 131,023 |
| `nodes_read` | **11** |
| `mapping_pages` | 3 |

One emitted whole-file object (131,000 + 23 bytes of framing) out of a chunked
base: the route P1-8's ordered Retain cursor and P1-14's pre-sized assembly both
land on. `nodes_read` 11 is the R×h demand the cursor is meant to collapse.

### D27 — the frozen shape is unchanged

`edit_timing_c1` with no argument still prints every field it printed before
(`edited_root b6dca354…`, `nodes_read 9`, `objects_written 7`,
`objects_written_bytes 47357`, `mapping_pages 3`, `objects_before 176`) and adds
exactly one line, `case: default`:

```sh
diff <(…v1/after/logs/D27-edit-timing-c1-nodes-read.log | grep -vE '^(elapsed_ns|case:)') \
     <(core/target/release/examples/edit_timing_c1 | grep -vE '^(elapsed_ns|case:)')   # exit 0, no output
```

## 4. No frozen row moved

The `v1` arm (before V2) and this arm (after V2) are the same tree plus V2's two
example files. Every counter-bearing line of D1–D29 is identical between them
(diff after removing timing figures, paths and the added `case:` line: **0
lines**), and the two vehicles' own rows reproduce on the labelled repeats
(`X3` = M1, `X4` = M3) bit for bit: `peak_delta_bytes` 262,328 twice, `nodes_read`
11 twice, same roots. The only textual differences anywhere are timing figures
(`elapsed_ns`, the `measure_edits`/`measure_filesystem` timing trees,
`end-to-end ns`, and the probe's own "preparation untimed" note) — diagnostic by
construction.

## 5. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (436 passed, 0 failed) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

The first `fmt --check` run exited 1 (the new example was not rustfmt-clean);
`cargo fmt --all` was run and the check then exited 0. Recorded rather than
silently re-run.

## 6. Production LOC

`V2 | 0` was the plan's estimate; the actual is **0**.
`core` **18,793 → 18,793 (delta 0)**; `crates/` reference 65,417 → 65,417;
combined 84,210 → 84,210. Method: `tools/production_loc.py` over the first parent
(`fe86f3bdd`, via `git archive`) and the V2 tree; both files are under
`examples/`, outside the production scope.

## 7. Acceptance

- [x] Both gaps closed with additive, non-product vehicles
- [x] The frozen shape keeps every printed field (diff-proven) and adds one line
- [x] The three rows collected with the frozen set; exit codes and wall times recorded
- [x] Labelled determinism repeats (`X3`, `X4`) bit-identical
- [x] Eight checks green; parity untouched (no test file in the diff); LOC delta 0
- [x] The plan's ~3n prediction corrected to the measured 4n, with the fourth n attributed
