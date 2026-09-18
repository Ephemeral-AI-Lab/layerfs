# Implementation shape and estimate

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Consumed by Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> and its two family issues ([#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182),
> [#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183)).
> Siblings: [`c1-families.md`](c1-families.md), [`c2-families.md`](c2-families.md),
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md),
> [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md),
> [`gates_and_oracles.md`](gates_and_oracles.md).
>
> Derived from five independent design reviews (architecture, copy mechanics, trace
> format, testability, reuse). **No figure here is a measurement**; measured values
> are cited to an existing receipt and labelled.

## 1. The rule that shapes the structure

`docs/general/benchmark_rules.md` §7:

> *"Each family MUST own exactly one canonical definition module and one thin runner.
> Shared product lifecycle, process supervision, evidence, and reporting code MUST be
> reused rather than copied into families."*

A row in a case table is a canonical definition but is **not a module**. A review
proposed collapsing the family layer into a single TSV; that would need an explicit
owner waiver, so the default is **family modules whose bodies are shared shape
drivers**. The file is thin (~40-60 lines) because the operation, fixture and oracle
bodies live in `ops.rs`, `fixture.rs` and `workload/oracle.rs`.

The case table still exists — as a **generated golden artifact** at
`tests/golden/registry.tsv`, rendered by `registry::render_tsv()` and compared with
`include_str!`. That keeps whole-registry diffability without making it the source of
truth. A self-referential hash that no test recomputes is decoration; a golden diff
names the drifted row.

## 2. Structure

```text
core/benchmark/fs-bench-pro-storage-content/          47 authored files
├── CONTRACT.md                     250   frozen case spec + claim_kind + the rules below
├── README.md                       120
├── Cargo.toml + Cargo.lock + .gitignore  40   see §6
├── runner.py                       850   list|prepare|perf|verify|report|self-check|calibrate
├── src/                                        33 files, ~4,255
│   ├── lib.rs                       45   re-exports every module — a binary crate cannot be
│   │                                      imported by tests/*.rs, so without this no seam exists
│   ├── main.rs                     140   argv -> one op; writes trace.jsonl; prints facts
│   ├── registry.rs                 120   aggregator over family modules + cardinality
│   ├── gates.rs                    380   the PASS/FAIL decision; pure; highest-value test target
│   ├── ops.rs                      520   the 10 shape drivers — the ONLY place bodies live
│   ├── fixture.rs                  260   3 generators + expected-result model
│   ├── workload/
│   │   ├── providers.rs            250   TreeStore — the only provider that authenticates on read
│   │   ├── oracle.rs               620   Census/coverage/read_back — the independent oracle
│   │   └── digest.rs               150   SHA-256 (Python re-checks it, so blake3 will not do)
│   ├── support/
│   │   ├── instruments.rs          340   counting GlobalAlloc + 10 ms RSS thread + getrusage
│   │   ├── trace.rs                340   flat JSONL record writer
│   │   └── window.rs               190   clk(4) brackets, envelope reconstruction, balance
│   └── families/                             21 files, ~840
│       ├── mod.rs                   60   aggregator + the declared cardinality array
│       ├── c1_construct.rs          90   2 families, 8 cases
│       ├── c1_cdc.rs                55   1 family, 12 cases
│       ├── c1_edit.rs              115   2 families, 44 cases
│       ├── c1_transition.rs         60   1 family, 7 cases
│       ├── c1_many_tiny.rs          70   1 family, 20 cases
│       ├── c1_tree.rs              105   2 families, 16 cases
│       ├── c1_locality.rs           60   1 family, 12 cases
│       ├── c1_fs_build.rs           55   1 family, 8 cases
│       ├── c1_read.rs               60   1 family (vehicle does not exist today)
│       ├── component_primitives.rs  45   the only matched reference pair
│       ├── c2_lifecycle.rs          55   1 family, 5 cases
│       ├── c2_reuse.rs              95   2 families, 24 cases
│       ├── c2_delta.rs             110   3 families, 41 cases
│       ├── c2_footprint.rs          55   1 family, 6 cases
│       ├── c2_delta_small_file.rs   50   1 family, case list still TBD
│       ├── c2_read.rs               60   1 family, 4 cases
│       ├── c2_pool.rs               60   1 family, 2 cases
│       └── pipeline.rs              75   5 cases
├── shared/                                      7 files, ~2,470
│   ├── receipt.py                  250
│   ├── fixtures.py                 320
│   ├── copyladder.py               150   clonefile/copy/regenerate rungs + ENOSPC preflight
│   ├── residency.py                 70   ported primitive + self_check (NOT a Rust module)
│   ├── space.py                    170
│   ├── trace.py                    270   reader + independent re-derivation
│   └── analyze.py                  390   ladders, bands, report.txt
├── tests/                             29 Rust targets + 6 Python modules, ~6,700
└── results/                           gitignored dev runs
```

## 3. The estimate

| Bucket | LOC |
| --- | ---: |
| Rust source (33 files) | 4,255 |
| Python (7 files) | 2,470 |
| Data + docs (`CONTRACT.md`, `README.md`, manifests) | 625 |
| **Harness authored** | **7,350** — range **6,800 - 8,200** |
| Harness tests (35 files) | 6,700 |
| **Total new code** | **~14,050** |
| *Already committed* | *~1,100* (the five specification docs) |

**Test-to-harness ratio ≈ 0.91.** The product's own ratio is 25,203 test lines to
19,294 implementation lines = **1.31**, so this plan is *below* the repository's
existing standard, not above it.

### 3.1 Movement from the first estimate

| | First estimate | Corrected | Why |
| --- | ---: | ---: | --- |
| Harness | 8,000 - 10,500 | **6,800 - 8,200** | the family layer collapses to thin modules over shared drivers; three instrument modules and two Python modules disappear |
| Analysis path | **absent** | +1,190 | `trace.rs`/`window.rs`/`trace.py`/`analyze.py` — without it there is no way to read the numbers |
| Test suite | **absent** | +6,700 | 29 Rust targets + 6 Python modules |

**The first estimate under-counted by omitting two whole subsystems.** It described a
harness that collects numbers and never said how they are read or how the collector
itself is trusted.

## 4. Reuse

Destination-unit ledger: **48.6 % lifted** (~1,470 lines verbatim-class: the counting
allocator, the resource FFI, SHA-256, `Content`, `TreeStore`, `Census`, `read_back`,
`Residency`, `closed_store_copy`, `run()`, `_write`, `publish_receipt`,
`normalize_result`, the `object_packs` SQL), **51.4 % new** — concentrated in the
family layer and the analysis path.

**~7,200 inspected lines must NOT be carried:** all Docker/cgroup/image handling
(~580 in `runner.py`), the native-tree fixture verification (376), the SDK edit
schedules (~1,080 of `ordinary_workloads.rs`), `cold.py`'s tree-`acquire`/`compare`
(227), the examples' `Provider` (linear `Vec::find` — O(n^2) at 500 MiB; use
`TreeStore`), three more copies of the counting allocator, and
`measure_filesystem.rs` (630 — its `--entries` panic already invalidated RUNPLAN
D7-D9).

**Licensing:** nothing under `core/crates/*/src` may be copied, and nothing needs to
be — every source is in `examples/`, `tests/`, `benchmark/` or an evidence directory.
The harness cannot use `license.workspace = true` if it is its own workspace; it must
inline `license = "MIT"` beside `publish = false`.

## 5. Review findings now folded into the specs

| Finding | Folded into |
| --- | --- |
| `disk_read_bytes` is the missing instrument — `mincore` cannot tell a cache-served read from a device read | `memory_cpu_space_support.md` §5, `gates_and_oracles.md` G4 |
| G3 must not gate on time (±17.6 % spread) | `gates_and_oracles.md` §6 |
| The 10 ms sampler cannot gate phases under ~200 ms | `memory_cpu_space_support.md` §3.2 |
| Reflink rung; `--setup reflink`; forbidden for footprint cases; ENOSPC preflight | `test_setup_and_cache_discipline.md` §4.2 |
| `mincore`-first de-warm; serialized `FilesystemInput` as the prepared artifact | `test_setup_and_cache_discipline.md` §5 |
| The frozen writer cannot carry a sibling `resources` key | `memory_cpu_space_support.md` §8 |
| Two clocks, not three (`CLOCK_MONOTONIC_RAW` id 4) | `memory_cpu_space_support.md` §10.2 |
| `Sigma self_ns == root.elapsed_ns` is a tautology, not an `attach` detector | `memory_cpu_space_support.md` §10.2 |
| `BOUNDARY_{BELOW,EXACT,ABOVE}` are not product constants | `c1-families.md` §4 |
| Family count is 20, not 21; parity set is 35, not 34 | `gates_and_oracles.md` §4 |
| `NativeRusage` sketch was a buffer overrun | `memory_cpu_space_support.md` §4.1 |
| `edit_timing_c1` records nothing (`Timing::disabled`) | `c1-families.md` §6 |
| Cardinality parsing rule (bracketed profile = one case) | `c1-families.md` §3.1 |

## 6. Build-blocking corrections

1. **The harness crate must declare an empty `[workspace]` table**, or `core/Cargo.toml`
   must add `exclude = ["benchmark"]`. `core/Cargo.toml:6-11` lists three members and no
   exclude, so a package at `core/benchmark/.../` that is neither a member nor excluded
   makes Cargo fail with *"current package believes it's in a workspace when it's not"*.
2. **Its own workspace means its own `Cargo.lock`** — which can resolve different
   transitive versions (`libsqlite3-sys`, `rusqlite`, `blake3`, `zstd-sys`) than
   `core/Cargo.lock`. `--locked` would then lock a *different dependency graph than
   the one the product seal names*. Either add a `test_lock_parity.py` (every package
   present in both locks must match version and checksum) or make the harness a member
   of the core workspace. **This test must exist before the first receipt**, because it
   decides whether the benchmark links the product the seal claims.
3. **An own workspace gets its own `target/`.** The shared-Cargo-target reuse does not
   cross workspaces — share via `CARGO_TARGET_DIR` and record it as
   `dependency_reuse`, or accept a full rebuild per seal change.

## 7. Owner decisions (frozen)

Ruled on 2026-09-18, before any harness code. The normative record is
[`CONTRACT.md`](CONTRACT.md) §5; this table is the local restatement. None of these
changes the shape described above except where noted.

| # | Question | Decision | Effect on this estimate |
| --- | --- | --- | --- |
| **R1** | Does a case-table row satisfy `benchmark_rules.md` §7's "canonical definition **module**"? | **No — family modules stay.** §7 requires one canonical definition module and one thin runner per family; a TSV source of truth would need an owner waiver and would also make the per-family test exports unwritable. Bodies live in the shared shape drivers (`ops.rs`, `fixture.rs`, `workload/oracle.rs`); each family module is ~40-60 lines of rows plus its runner. The row table is checked in as a **generated golden** `tests/golden/registry.tsv` instead of being the source. | Structure unchanged (+~840 lines, 19 files, already counted) |
| **R2** | Does per-sample cache acquisition count against the <= 15 s complete command? | **It is reported, not counted as a failure.** `acquisition_wall_ns` is its own field, outside every operation timer and outside the row's admission decision; the command status is reported separately. v0.1.6's 18.57 s exclusion is *not* precedent for hiding it, so the `mincore`-first de-warm is now **required** rather than merely recommended | Makes the `de-warm` module load-bearing; no cardinality change |
| **R3** | Own workspace + lock-parity test, or a member of the core workspace? | **Own workspace (the path the owner gave), with `shared/test_lock_parity.py` mandatory before the first receipt.** `Cargo.toml` carries an empty `[workspace]` table so it is not absorbed as a core member. Declined alternative: making the child an example target of `layerfs-storage` (no second lock, `core/Cargo.lock` diff empty) — it is the smaller hazard, but a benchmark-only compile error would fail a core build | +~60 lines for the parity test; own `target/` (see §6.2) |
| **R4** | Cut the over-budget tiers (500 MB, 100k files), or declare them as <= 25 s exceptions? | **Declared, on the <= 25 s exception list, with measured wall times.** Cutting them would remove the tier where the O(1) claim is most convincing and the 100k controls that are the point of `c2.footprint`. Shrinking a workload to fit a budget remains forbidden | Cardinality unchanged; adds the exception list to the group report |
| **R5** | `c2.delta.small-file`'s case list is still `TBD` | **Defined, not deferred** — 4 cases, one per size tier below the cutoff. `benchmark_rules.md` §7 forbids accepting favourable members while moving unfavourable siblings to a later release, and dropping the family would do exactly that. Cost is four rows and one shape driver | C2 **86 -> 90**; total **213 -> 217** |

## 8. Experiments to run before collection

Cheap, untimed, ~2 s each, read-only. None has been run.

| # | Settles | Pass condition |
| --- | --- | --- |
| **E1** | clone write isolation, inode distinctness, block sharing | distinct `st_ino`; master unchanged after writing the clone; used-bytes growth far below the file size |
| **E2** | clone residency independence; whether `MS_INVALIDATE` on a clone evicts the master | clone resident 0 after reading the master fully. **If this fails, `prepared-master-dewarmed` is a lie for R1** |
| **E3** | cloning a quiescent Store | `quick_check == ok`, page count and watermark intact, master digest unchanged after mutating the clone |
| **E4** | tree-clone fidelity | mode, mtime, symlinks, xattrs, `st_nlink == 1` across the tree |

## 9. Production LOC

**Zero.** `AGENTS.md` excludes benchmark harnesses, development tools and fixtures from
the production count, and `core/tools/check_product_boundary.py:90-95` scans only
`core/crates/*/src` and `core/crates/*/sql`. Every commit in this work reports:

```text
Production LOC: <unchanged> -> <unchanged> (delta 0)
```

## 10. Non-goals

- No instrumentation, hook, counter or accessor in product `src/`.
- No Docker, container, cgroup, daemon, FUSE or Workspace.
- No change to `layerfs-telemetry`; no new product dependency.
- No comparative gate beyond `component.primitives`.
