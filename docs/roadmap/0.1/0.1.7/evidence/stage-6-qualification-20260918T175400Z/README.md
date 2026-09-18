# Stage 6 round evidence — 2026-09-18T175400Z

> **Status:** one measurement round of Stage 6
> ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)). Append-only:
> nothing here is edited after the fact, and a later round gets its own directory.
>
> **The round does not close #171.** Section 5 states exactly which acceptance
> checkboxes are satisfied and which are not.

## 1. Identity

| Field | Value |
| --- | --- |
| Tree | `14cf16d83` plus the WP-9 driver change and these documents (see §6) |
| Harness root | `core/benchmark/fs-bench-pro-storage-content/` |
| Harness binary (full run) | `e01d4a3…` — recorded per receipt in `run-full.json` → `identity.harness_binary_sha256` |
| Product lock `sha256` | `run-full.json` → `identity.product_lock_sha256` |
| Harness lock `sha256` | `run-full.json` → `identity.harness_lock_sha256` |
| Registry golden `sha256` | `run-full.json` → `identity.registry_tsv_sha256` |
| Workers | `LAYERFS_CONSTRUCTION_WORKERS=1`, exported by the runner and inherited by every child |
| Toolchain | `cargo +1.85.1`, `--locked`, release |
| Samples | one per case per arm; no n3, no best-of |

## 2. Reproduction

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
python3 $H/runner.py self-check
python3 $H/runner.py perf     --lane full --out benchmark-results/fs-bench-pro-storage-content/run-20260919T-full
python3 $H/runner.py verify   --run …/run-20260919T-full --tag verification-2.json
python3 $H/runner.py report   --run …/run-20260919T-full
python3 $H/runner.py calibrate --out …/run-20260919T-wp9
```

The full lane ran **220 cases in 322.0 s** wall on this host.

## 3. Matrices

### 3.1 Registered rows

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **122** | **3** | **92** |
| Diagnostic (`component.primitives`, excluded from the 217) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **122** | **3** | **95** |

**0 unowned rows.** Every registered case reported a status, and every non-`PASS`
row carries its measured state and a reason in its own `receipt.json`.

### 3.2 Per family

| Family | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: |
| `c1.construct.whole-file` | 4 | | |
| `c1.construct.chunked` | 4 | | |
| `c1.cdc.chunk-count` | 9 | 3 | |
| `c1.edit.length-preserving` | 11 | | 1 |
| `c1.edit.length-changing` | 32 | | |
| `c1.transition.boundary` | 7 | | |
| `c1.many-tiny` | | | 20 |
| `c1.tree.construct-traverse` | | | 12 |
| `c1.tree.namespace-mutation` | | | 4 |
| `c1.change-locality` | | | 12 |
| `c1.fs.build-scale` | | | 8 |
| `c2.lifecycle` | 5 | | |
| `c2.reuse.cross-file` | 6 | | 4 |
| `c2.delta.cdc-locality` | 10 | | 10 |
| `c2.delta.boundaries` | 21 | | |
| `c2.reuse.workspace` | | | 14 |
| `c2.footprint` | 5 | | 1 |
| `c2.delta.small-file` | 4 | | |
| `c2.read.waves` | 4 | | |
| `c2.pool.cold-warm` | | | 2 |
| `pipeline.*` | | | 4 |
| `component.primitives` | | | 3 |

### 3.3 Why the 95 non-`PASS` rows are non-`PASS`

| Cause | Rows | Detail |
| --- | ---: | --- |
| `driver-unimplemented` | 79 | eight families have no shape driver yet: `many-tiny` 20, `reuse.workspace` 14, `tree.construct-traverse` 12, `change-locality` 12, `fs.build-scale` 8, `tree.namespace-mutation` 4, `pipeline.*` 4, `component.primitives` 3, `pool.cold-warm` 2 |
| `budget-complete-command` | 4 | 27.98 s, 28.66 s, 29.27 s, 29.62 s — over the ≤ 25 s declared-exception limit. Their gates all held; the rows are `NOT_RUN` on the budget alone and are **not** shrunk to fit |
| product error: `UNIQUE constraint failed: objects.object_id` | 5 | §4.1 |
| product error: `Integrity("group ordinal")` | 4 | §4.2 |
| product error: `CapacityExceeded { pack.assembled_length, limit 262144 }` | 2 | §4.3 |
| product error: `InvalidRecord("mapping coverage")` | 1 | §4.4 |
| fixture direction refuted (`FAIL`) | 3 | §4.5 |

## 4. Findings

### 4.1 Offering the same canonical object twice in one save operation

`dedup-cdc-{insert,delete}-{10,100}` and one more row abort with
`SqliteFailure(ConstraintViolation, extended_code 1555)` —
`UNIQUE constraint failed: objects.object_id` — where the declared case is a
legitimate save of a member set that overlaps the stored base.
Reproduce: `fs-bench-storage-content --case dedup-cdc-insert-10 --out <fresh>`.
The harness offers each member's finalized objects in insertion order, exactly as
a real C2 caller would.

### 4.2 An identical member set inside one save

`dedup-cross-file-identical-{10,100,500}` and `dedup-cross-file-mixed-10` abort
with `Integrity("group ordinal")`. The registry declares for the `identical`
profile that *"every member after the first is reused"*; the measured behaviour is
a refusal, so the declared equation is not what happens.

### 4.3 A pack over its own declared limit

`dedup-cdc-insert-1` and one further row abort with
`CapacityExceeded { what: "pack.assembled_length", limit: 262144, actual: 262147 }`
— three bytes over `PACK_LIMIT`. The rows are retained with their exit codes.

### 4.4 Mapping coverage

One `c2.delta` row aborts with `InvalidRecord("mapping coverage")`.

### 4.5 The three `FAIL` rows are a harness-fixture defect, and this round did not hide it

`overwrite-fixed-64k-chunk-count-decrease-{10m,100m,500m}` FAIL
`g2.chunk-count-direction`: the extent count does not move.

```text
decrease-1m   initial 53   -> final 52    payloads_created 2   PASS
decrease-10m  initial 536  -> final 536   payloads_created 2   FAIL
decrease-100m initial 5417 -> final 5417  payloads_created 2   FAIL
decrease-500m initial 26972-> final 26972 payloads_created 2   FAIL
```

**Diagnosis.** The replacement is 64 KiB of zeros and the chunker emits exactly
**two** 32 KiB payloads for it at those offsets, while the noise it replaced also
occupied exactly two extents there. The direction is therefore unobservable at
those three tiers: the count is measured correctly and simply does not move. The
1 MiB tier shows the effect (53 → 52) because its noise seed chunks differently.

**Disposition.** This is a fixture-alignment defect in the harness, not a product
defect, and it is recorded as `FAIL` rather than refitted after the fact. The
correction — a replacement whose local chunk count provably differs at
`START = 147,456` for `LEN = 65,536`, or a direction assertion on a counter that
moves — belongs to the next round and must be re-run into a **new** output
directory. The gates, the limits and the cardinality are untouched.

## 5. Acceptance checkboxes — what this round does and does not satisfy

| # | Checkbox | State |
| --- | --- | --- |
| 1 | Canonical identity/profile and Store compatibility across complete inputs, edits/transitions, **trees, pooling**, supported formats | **PARTIAL.** Identity, profile, complete inputs, edits and transitions are gated and green (`c1.construct.*` 8, `c1.edit.*` 43, `c1.transition.boundary` 7). Trees and pooling are `NOT_RUN` (no driver). |
| 2 | C1-only, C2-only save/read, bounded diagnostics, **integrated timing** on real production paths | **PARTIAL.** C1-only and C2-only both run real production paths and are green. `pipeline.*` (integrated) is `NOT_RUN`. |
| 3 | Existing-or-better performance where a matched baseline exists; unmatched features get correctness/resource checks, not invented ratios | **SATISFIED as withdrawn.** Owner decision D1 withdraws the comparative claim; the only matched pair (`component.primitives`, 3 cases) is registered, receipted and `NOT_RUN`. **No ratio is invented anywhere in this round.** |
| 4 | Total retained storage/index/pack footprint, occupancy, query/read/write/copy/hash/assembly work, simultaneous memory/disk reported | **PARTIAL.** `c2.footprint` runs 5 of 6 rows with the pack accounting gated and no `COALESCE`; the 100k/500 MB control is `NOT_RUN` on the complete-command budget. Four axes are reported per row (time, heap, RSS, space). |
| 5 | Failure/cleanup/unknown-outcome, concurrency/visibility, zero retry/fallback/fsync/WAL, without fault injection or dependency patches | **PARTIAL, and the two designed arms pass.** W1 (`SATISFIED`): a hand-edited watermark makes `Store::open` refuse while the unperturbed control still opens. W3 (`PASS` in `lifecycle-begin-save`): a second `begin_save` on one Store fails `OwnershipUnavailable`. W4 (`SATISFIED`): a sealed call-graph scan over 120 product source files finds no `fsync`/`fdatasync`/`sync_all`/`sync_data`, no WAL mode, no retry/resend/backoff call site and no non-zero busy timeout, and the runtime tripwires agree over 138 Stores. **Not done: the declared-process-kill arm** — no child mode pauses inside a save, so no kill point exists yet. |
| 6 | Distinct-reuse receipt revision explicit and consumers updated together | **NOT APPLICABLE.** No receipt revision was made; historical receipts are unchanged. |
| 7 | One sample per case/arm, append-only outputs, equal declared cache state, no warm credit, no extra workers, no resource-sensitive overlap | **SATISFIED.** One sample per case per arm; every output path fresh and refused if it exists; `prepared-dewarmed` rows gate `resident_pages == 0` and `disk_read_bytes`; the measurement lock is held for every `perf` and `verify`; `LAYERFS_CONSTRUCTION_WORKERS=1` is exported by the runner and asserted in every receipt. |
| 8 | Every non-`PASS` preserved with identities and repro commands; no dropped cases, relaxed limits or best-of | **SATISFIED.** 95 non-`PASS` rows retained with their measured state and reason; four budget overruns recorded with their wall times rather than shrunk. |
| 9 | Affected-workspace core tests/examples, formatting, Clippy, boundary/tool checks pass individually; per-commit LOC reports complete | **SATISFIED.** §6. No aggregate gate was run and none was created. |

**Therefore #171 is not closed by this round.** Checkboxes 1, 2, 4 and 5 carry
`NOT_RUN` work that is stated above rather than waived, and §4 records three
findings that need an owner disposition.

## 6. Checks run in this round

| Check | Result |
| --- | --- |
| `python3 core/tools/check_product_boundary.py` | exit 0 — 120 production Rust/SQL files scanned |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | OK, 6 tests |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | OK, 17 tests |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | exit 0 |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | exit 0, 68 suites `ok` |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | exit 0 |
| `python3 tools/production_loc.py` | core 19503 / reference 65417 / combined 84920 |
| `cargo +1.85.1 test --manifest-path $H/Cargo.toml --locked` | 2 passed |
| `python3 $H/runner.py self-check` | PASS — residency, space, receipt, trace, copyladder, invariants, lock parity (46 entries, 0 mismatches), registry self-check, golden |
| `python3 $H/shared/test_lock_parity.py` | exit 0 — 46 shared entries compared, 0 mismatches, 0 product entries the harness does not link |
| `python3 $H/runner.py verify --run … --tag verification-2.json` | 220 cases, **0 disagreements**, call-graph PASS, tripwires PASS |

`tools/preflight.sh` was not run: it is permanently retired. No CI, no aggregate
gate and no replacement for either was created.

## 7. E1-E4, and the refutation that matters

| # | Outcome | What it settles |
| --- | --- | --- |
| E1 | **REFUTED** | The master is unchanged after writing the clone and the inodes are distinct, so R1 isolates writes. But the clone's `st_blocks * 512` equals its apparent size (67,108,864 for a 64 MiB clone), so `st_blocks` **cannot distinguish shared from exclusive allocation on this volume at all**. The contract's stated reason for forbidding R1 on `c2.footprint` ("double-counts blocks shared with the master") is not what this host measures; the measured fact is stronger and the prohibition stands. |
| E2 | **SATISFIED** | After the master is read fully, a fresh clone is 0-resident (clone residency is independent of the master's), and `msync(MS_INVALIDATE)` on the clone de-warms it to 0 while the master keeps all 4096 pages. **The decisive question passed, so the reflink rung is not withdrawn** — it remains forbidden where `st_blocks` gates, on E1's measured basis. |
| E3 | **SATISFIED** | Cloning a quiescent Store preserves `page_count` and `quick_check == ok`, and mutating the clone leaves the master's digest unchanged. |
| E4 | **SATISFIED** | Mode, mtime, symlink target, extended attribute and per-file `st_nlink` all match across a directory clone. |

Two over-broad checks were corrected during E1-E4 and are recorded rather than
quietly fixed: `st_nlink == 1` is asserted for **files** only, because a
directory's link count is structural and asserting 1 for it would refute every
faithful clone; and extended attributes are compared through libc `setxattr`,
because this build's `os` module has neither.

## 8. The four unmeasured rows and the D1 withdrawal

Recorded in one place, with a reason each, in
[`stage-6-qualification-plan.md`](../component-decoupling/stage-6-qualification-plan.md):
the inherited complete-operation comparison (`VF-6`), cold-cache rows,
pack-footprint rows and process-memory rows. The D1 owner decision — under which
`claim_kind = structural-complexity` **withdraws the comparative claim** — is
stated beside them, so the absence reads as a decision rather than an omission. No
Stage 5 component row is carried as a substitute for any of the four.

## 9. A verifier defect this round found in its own verifier

`verification.json` (pass 1) records **4 disagreements**: four `c2.delta` 500 MiB
rows whose gates all held but whose complete command overran the declared
exception, so the runner published `NOT_RUN`. The first verifier compared the
trace's gate set against the published status without applying the budget rule.
The verifier was corrected to re-derive the budget classification from the
recorded wall time and worst-case it with the trace status; `verification-2.json`
reports **0 disagreements**. **Both files are retained.** A verification that
overwrote the pass that found its own defect would destroy the only record of it.
