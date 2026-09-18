# Stage 6 handoff: qualify the complete C1/C2 core

> **Status:** Active implementation routing, 2026-09-19. This is the executable
> assignment for the Stage 6 agent. It supersedes stage-level routing in
> [`stage-5-handoff.md`](stage-5-handoff.md),
> [`stage-5-continuation-handoff.md`](stage-5-continuation-handoff.md) and
> [`stage-5-terminal-handoff-20260917.md`](stage-5-terminal-handoff-20260917.md);
> those remain the historical record of Stage 5 and are not edited.
>
> **Issue:** [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) — open.
> Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). Next child
> after this one is [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)
> (Stage 7, runtime integration), which is **not** yours.
>
> **How you work:** you are **one agent, working alone**.
> **No subagents.** Do not launch, delegate to, or fan out to any other agent,
> background agent or workflow. **No codex.** Do not invoke `codex` or any external
> coding agent or CLI at any point; all work happens in this session. This is a
> deliberate departure from Stage 5, which used verification subagents — section 7
> replaces that mechanism with receipts a later reader can re-run mechanically.
>
> **Terminal condition (the only definition of done in this document):** every
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) acceptance checkbox
> is satisfied **by a receipt that reproduces on the committed tree**; every one of the
> 217 registered cases is `PASS`, or `FAIL`/`INCOMPLETE`/`INELIGIBLE`/`NOT_RUN`
> with its measured state and a written reason; the four unmeasured rows of section
> 3.10 are recorded in one place; every commit's production LOC is reproducible; and
> **#171 is closed** with a final comment naming the tree, the receipts and the
> matrices.

---

## 1. What to read

Read in this order. Do not start writing code before the first block is done.

### 1.1 Mandatory — the rules that bind you

| # | Document | Why |
| --- | --- | --- |
| 1 | [`AGENTS.md`](../../../../../AGENTS.md) | Sections 1-3 are the measurement contract: a warm cache must never credit a timed phase, reuse setup never measurement, one sample per case per arm, budgets. Section 4 carries the production-LOC rule and the no-CI rule. |
| 2 | [`core/AGENTS.md`](../../../../../core/AGENTS.md) | Product-source purity, the `Content | Location` table that says where a benchmark harness may live, the 999-line and 200-line file ceilings, and the exact checks to run. |
| 3 | [`docs/general/benchmark_rules.md`](../../../../general/benchmark_rules.md) | The normative benchmark contract. §1 (freeze the claim), §5 (pure timing boundaries), §6 (separate setup/perf/verification/cleanup), §7 (coherent families — the rule that shapes your file layout), §10 (honest memory attribution), §11 (freeze gates before measuring), §13 (evidence custody), §15 (fast loop and terminal admission). |
| 4 | [`docs/general/release-policy.md`](../../../../general/release-policy.md) | What a qualification claim may and may not assert. |
| 5 | [`docs/general/documentation-policy.md`](../../../../general/documentation-policy.md) | How to state measured facts, limits and open rulings. |
| 6 | [`benchmark/AGENTS.md`](../../../../../benchmark/AGENTS.md) | Benchmark-tree specifics. |
| 7 | [`benchmark/fs-bench-pro/QUICKSTART.md`](../../../../../benchmark/fs-bench-pro/QUICKSTART.md) | The v0.1.6 harness's build/reuse/run mechanics — the reuse source you are lifting from. |

### 1.2 Mandatory — the frozen specification you are implementing

All seven files under
[`core/docs/benchmark/fs-bench-pro-storage-content/`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/).
`CONTRACT.md` is the index and the normative record; read it first, then the one
that owns the work you are doing:

| Document | Owns |
| --- | --- |
| [`CONTRACT.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md) | The claim (`structural-complexity`), the frozen 217-case registry, the seven owner decisions D1-D5, evidence layout, and what is explicitly not claimed. |
| `c1-families.md` | The 11 C1 families and their 127 cases. |
| `c2-families.md` | The 9 C2 families, the boundary sub-lane and `pipeline.*` — 90 cases. |
| `memory_cpu_space_support.md` | The four measurement axes: time, memory, CPU, space — and the instrument for each. |
| `test_setup_and_cache_discipline.md` | The copy ladder R0-R3, de-warming, the prepared-input cache and the compatibility digest. |
| `gates_and_oracles.md` | Gate classes G1-G7, oracle classes O1-O7, status vocabulary, and the bands. |
| `implementation_estimate.md` | The file structure, the LOC estimate, the reuse ledger and experiments E1-E4. |

The specification headers address themselves to two family sub-issues of #171:
[#182](https://github.com/Ephemeral-AI-Lab/layerfs/issues/182) (C1 families) and
[#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183) (C2 families). They
carry no separate acceptance criteria — #171 is the acceptance issue — but they are
where a family-level question is discussed.

### 1.3 Mandatory — why you exist (what Stage 5 handed you)

| # | Document | Why |
| --- | --- | --- |
| 8 | [`stage-5-terminal-handoff-20260917.md`](stage-5-terminal-handoff-20260917.md) | The Stage 5 closure loop and its ledger. §4 rows show what was remediated and what was deferred to you; **`VF-6` (complete-operation comparison) is yours**. |
| 9 | [`stage-5-report.md` §16](stage-5-report.md) | The final Stage 5 matrices: **81 PASS / 0 FAIL / 0 PARTIAL-INCOMPLETE / 1 NOT_RUN with an owner disposition / 1 NOT_APPLICABLE of 83**, and **34 PASS / 2 owner-WAIVED of 36** cumulative. |
| 10 | [`stages-1-5-review-20260917T230700Z.md`](stages-1-5-review-20260917T230700Z.md) | The governing review. **§10 action 10 is your row**; §11 "Final answers" states in its own words what Stage 6 must supply. |
| 11 | [`complexity-and-roundtrip-research-20260917.md`](complexity-and-roundtrip-research-20260917.md) | Per-area complexity versus the reference and a risk-tiered optimisation register. **Source-read only — no measurement.** It is a hypothesis list, not evidence. |
| 12 | [`parallelism-and-batching-study-20260918.md`](parallelism-and-batching-study-20260918.md) | Bounds what to measure first (worker pools, SQLite tuning, statement batching). Also source-read only. |

**Four traps in the Stage 5 record.** They are bookkeeping defects in a closed stage,
not live work, but each one can mislead you:

- **`VF-6` is labelled `PASS` in one evidence file.** The verdict column of
  `../evidence/stage-5-terminal-20260918T120000Z/verify-VF5-VF6-F5.md:16` reads
  **PASS** for a row the matrices record as `NOT_RUN` under an owner disposition. The
  matrices govern (`stage-5-report.md` §16), and the sibling verifier
  `verify-close-evidence-limits.md:33` records the correct reading. Do not cite that
  `PASS` column, and do not treat the row as closed.
- **The terminal closing comment overstates its own evidence.** Comment `5721219925`
  says the falsifier's "final verdict table marks every item SATISFIED"; the committed
  table marks item 10 **PENDING BY DESIGN**
  (`verify-close-terminal-checklist.md:500`), and the tree the comment names
  (`249d2b917`) did not yet contain that table. Cite the report, not the comment.
- **Two Stage 5 criteria were met under a disclosed re-reading**, and both are recorded
  in the tree: terminal item 6 (per-commit LOC reproducibility — seven of 27 Stage-5
  commits are documented as not reproducing, `verify-close-terminal-checklist.md:169,179-188`)
  and `R2-F14` (its named clipped-run receipt is unreachable; the row passes on wiring
  plus telemetry tests, `verify-R2-F1-F2-F3-F14.md:205-218`). Read the qualification
  before repeating either as a clean pass.
- **The closure-boundary file count is recorded two ways** — 115 in
  `stage-5-completion-report-20260917.md` §7/§7a, 116 in the closure logs and
  `verify-close-cumulative.md:127`. Harmless to you, but do not quote either as fact.

### 1.4 Read before you write the affected code

| Area | Documents |
| --- | --- |
| C1 content | [`content-io.md`](content-io.md), [`canonical-content.md`](canonical-content.md), [`file-content.md`](file-content.md) |
| C2 storage | [`admission-and-persistence.md`](admission-and-persistence.md), [`object-storage.md`](object-storage.md), [`physical-encoding-and-packing.md`](physical-encoding-and-packing.md), [`content-storage-policy-and-tables.md`](content-storage-policy-and-tables.md) |
| Filesystem tree | [`filesystem-tree.md`](filesystem-tree.md), [`stage-5-verification.md`](stage-5-verification.md), [`stage-5-verification-addendum-20260917.md`](stage-5-verification-addendum-20260917.md) |
| Architecture pins | [`core/docs/architecture/`](../../../../../core/docs/architecture/) — describes the product source at a pinned commit. It is a description, not a contract, and carries no performance claim. |

### 1.5 Working habits the repository enforces

- Read the module you are about to extend before extending it. Several Stage 5
  defects were a comment or a doc claiming something the code did not do.
- Every number you publish must name its unit, its basis and the counter that
  produced it. A configured ceiling is not observed use; a lifetime high-water mark
  is not a phase peak; a source-derived budget is not a measured RSS.
- Prefer a public behaviour check to a new accessor. If you find yourself wanting a
  counter in product `src/`, stop — see section 5.2.

---

## 2. Where you are starting

| Field | Value |
| --- | --- |
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` |
| Branch | `main` |
| Issue | [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) — open, not started |
| Predecessor | [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) — **closed** `2026-09-17T21:05:53Z`, comment `5721219925`, final HEAD `249d2b917`. An earlier component-scope closure at `2026-09-17T07:58:02Z` (comment `5711002673`, source of record `f2de7810e`) was reopened by the round-2 review; **it is not the closure of record**, and a document citing only that instant is describing the superseded one. |
| Frozen specification | committed under `core/docs/benchmark/fs-bench-pro-storage-content/` (7 files) |
| Target directory | `core/benchmark/fs-bench-pro-storage-content/` — **exists and is empty** |
| Existing harness to reuse | `benchmark/fs-bench-pro/` (v0.1.6, the reference campaign) |
| Product under test | `core/crates/layerfs-content` (C1), `core/crates/layerfs-storage` (C2), `core/crates/layerfs-telemetry` |

Rebuild the exact numbers yourself before trusting any of them:

```sh
git log --oneline -1
python3 tools/production_loc.py
python3 core/tools/check_product_boundary.py
```

Stage 5 is closed at its implemented scope. Read its closure record for the
qualifications it carries; do not re-open a Stage 5 row, and do not promote a Stage 5
`NOT_RUN` or owner-WAIVED row into your evidence.

---

## 3. What to implement

Eleven work packages. Order matters: WP-0 unblocks the build, WP-1 to WP-7 build the
harness, WP-8 to WP-11 are the qualification itself.

### 3.0 WP-0 — Workspace and seals (do this first; it blocks everything)

The harness is its **own Cargo workspace** (owner decision D3 in `CONTRACT.md` §5).

Files: `Cargo.toml`, `Cargo.lock`, `.gitignore`, `shared/test_lock_parity.py`.

- `Cargo.toml` **must carry an empty `[workspace]` table** or Cargo fails with
  *"current package believes it's in a workspace when it's not"* — `core/Cargo.toml`
  lists three members and no `exclude`. The alternative (`exclude = ["benchmark"]` in
  `core/Cargo.toml`) is an edit to a product manifest; prefer the empty table.
- `license = "MIT"` **inlined beside `publish = false`** — `license.workspace = true`
  does not resolve outside a workspace.
- `shared/test_lock_parity.py` **is mandatory before the first receipt.** Every
  package present in both `core/Cargo.lock` and the harness lock must match version
  and checksum. Without it the harness can link a different dependency graph than the
  product seal names.
- Its own `target/`: share the build with core via `CARGO_TARGET_DIR` and record it as
  `dependency_reuse`, or accept a full rebuild per seal change.

**Exit condition:** `cargo build` in the harness directory succeeds, and
`python3 shared/test_lock_parity.py` exits 0 with the compared-package count printed.

### 3.1 WP-1 — Registry, families and the frozen cardinality

Files: `src/registry.rs`, `src/families/` (18 files), `tests/golden/registry.tsv`.

- `benchmark_rules.md` §7 requires **one canonical definition module and one thin
  runner per family**. Do **not** collapse the family layer into a single TSV: that
  needs an owner waiver that was not granted. Keep the family modules thin (~40-60
  lines each) with the bodies in the shared drivers.
- `registry::self_check()` must assert the frozen cardinality array
  `[4,4,12,12,32,7,20,12,4,12,8,5,10,20,21,14,6,4,4,2,4]`, summing to **217**.
- The case table ships as a **generated golden** `tests/golden/registry.tsv`, rendered
  by `registry::render_tsv()` and compared with `include_str!`.
- `component.primitives` is **3 diagnostic cases, not one of the 217**, and is excluded
  from admission. The registry holds **220 rows = 217 admission + 3 diagnostic**.
- Honour the cardinality parsing rule: a bracketed profile list in a case ID is
  **one** case, never one per profile.
- `--smoke` selects one tier per family = **20 cases**.

### 3.2 WP-2 — Fixtures, the copy ladder and cache state

Files: `src/fixture.rs`, `shared/fixtures.py`, `shared/copyladder.py`,
`shared/residency.py`.

- Three generators plus an expected-result model; the prepared-input cache is keyed by
  a compatibility digest and validated once per acquisition.
- The copy ladder R0 (read-only master, mmap, no copy) / R1 (`apfs-clonefile-cow-v1`) /
  R2 (`closed-quiescent-byte-copy`) / R3 (`regenerated-in-process`).
  `--setup clone` keeps its byte-copy meaning; add `--setup reflink` and
  `--setup auto`.
- **R1 is forbidden wherever `st_blocks` gates** (`c2.footprint`): a COW clone's
  `st_blocks` double-counts blocks shared with the master.
- **De-warm order is fixed:** `mincore` → `msync(MS_INVALIDATE)` only if
  `resident_first > 0` → `mincore`, requiring `resident_pages == 0`. Never
  touch-every-page: that is why v0.1.6 paid a measured **18.57 s** for a 100k-file
  fixture.
- ENOSPC preflight: `free_bytes >= 1.05 * master_logical_bytes`.
- **For C1 filesystem families the prepared artifact must be a serialized
  `FilesystemInput`**, not a 100k-file tree — the product never walks a directory in
  the timed phase. `FilesystemInput` is fully public, so this needs no product change.

### 3.3 WP-3 — Instruments

Files: `src/support/instruments.rs`, `shared/space.py`.

Implement, each with its own self-check:

| Instrument | Measures | Note |
| --- | --- | --- |
| counting `GlobalAlloc` | `current`/`peak`/`allocs`/`charged` | the **only** valid gate for the O(1)-memory claim |
| 10 ms RSS sampler thread | `proc_pid_rusage` (macOS) / `/proc/self/status VmRSS` | **not** `ps -o rss=`. Cannot cover phases under ~200 ms — a G4 bound, not a gate |
| `getrusage` FFI bracketing | user/system CPU | wrap, do not copy the sketch in `memory_cpu_space_support.md` §4.1 (it overran its buffer) |
| `mincore` | residency, for `INELIGIBLE` decisions | cannot distinguish a cache-served from a device read |
| `st_blocks * 512` vs `st_size` | space | never pool the two |
| `PRAGMA page_count` / `freelist_count` | SQLite space | |
| pack SQL | `object_packs.data` bytes | watch the `COALESCE` hazard in `c2-families.md` |
| `disk_read_bytes` | **device attestation** | a row claiming cold or de-warmed must show `disk_read_bytes >= 0.9 x requested` or it is `INELIGIBLE` |

Two clocks, not three: `CLOCK_MONOTONIC_RAW` (id 4) everywhere, Rust and Python.
Do not add a third domain.

### 3.4 WP-4 — Operation drivers and family bodies

Files: `src/ops.rs` (the shape drivers — the only place operation bodies live),
`src/main.rs` (argv → one op; writes `trace.jsonl`), `src/lib.rs` (re-exports, so
`tests/*.rs` has a seam — a binary crate cannot be imported).

Implement the ten shape drivers once and have every family call them. Reproduce the
real production paths: C1-only construction, C2-only save/read, and the integrated
`pipeline.*` cases.

### 3.5 WP-5 — Trace, time window and analysis

Files: `src/support/trace.rs`, `src/support/window.rs`, `shared/trace.py`,
`shared/analyze.py`.

- `layerfs-trace-v1`: a flat JSONL record writer. The product's frozen telemetry
  writer **cannot carry a sibling `resources` key** (`timer/json.rs:29-89`), so the
  resource axes live in your own trace, keyed to the product receipt.
- `window.rs` reconstructs the envelope from `clk(4)` brackets and checks balance.
  `Σ self_ns == root.elapsed_ns` is a **tautology**, not an `attach` detector — do not
  present it as one.
- `analyze.py` renders ladders, bands and the four-axis `report.txt`.

### 3.6 WP-6 — Gates and oracles

Files: `src/gates.rs` (pure, the highest-value test target),
`src/workload/oracle.rs`, `src/workload/providers.rs`, `src/workload/digest.rs`.

- Gate classes G1-G7 and oracle classes O1-O7 as frozen in `gates_and_oracles.md`.
- **`elapsed_ns` never gate-decides.** The established same-binary wall spread is
  **+17.6 %**, which makes the O(n) time band overlap O(n log n). Scaling gates read
  counters, heap and disk.
- `workload/oracle.rs` is the **independent** oracle: census, coverage and read-back.
  It must not share code with the operation it checks.
- `providers.rs` supplies `TreeStore` — the only provider that authenticates on read.
  Do not reuse the examples' `Provider` (a linear `Vec::find`, O(n²) at 500 MiB).
- `digest.rs` must be SHA-256, because the Python side re-checks it; blake3 will not do.

### 3.7 WP-7 — `runner.py`

Files: `runner.py`, `shared/receipt.py`.

Verbs: `list | prepare | perf | verify | report | self-check | calibrate`.

- `layerfs-core-receipt-v1`; fresh `--output` per run; **append-only** receipts.
  Never overwrite a prior receipt, report or failed attempt.
- Hold the measurement lock for every `perf`/`verify` invocation.
- Report `acquisition_wall_ns` as its **own field**, outside every operation timer and
  outside the row's admission decision (owner decision D2).
- Enforce the budgets: complete command **≤ 15 s**, declared exceptions **≤ 25 s**,
  verification **≤ 60 s**.
- Export and assert `LAYERFS_CONSTRUCTION_WORKERS=1` in every receipt.

### 3.8 WP-8 — Experiments E1-E4 (before any collection)

Cheap, untimed, ~2 s each, read-only. None has been run.

| # | Settles | Pass condition |
| --- | --- | --- |
| **E1** | clone write isolation, inode distinctness, block sharing | distinct `st_ino`; master unchanged after writing the clone; used-bytes growth far below the file size |
| **E2** | clone residency independence; whether `MS_INVALIDATE` on a clone evicts the master | clone resident 0 after reading the master fully. **Decisive: if this fails, `prepared-master-dewarmed` is a lie for R1 and the reflink rung must be withdrawn.** |
| **E3** | cloning a quiescent Store | `quick_check == ok`, page count and watermark intact, master digest unchanged after mutating the clone |
| **E4** | tree-clone fidelity | mode, mtime, symlinks, xattrs, `st_nlink == 1` across the tree |

Record each outcome in the round's evidence, including a failure.

### 3.9 WP-9 — The three acceptance areas that have no instrument yet

[#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) requires these and no
design exists. They are **your work, not waivers**:

| Area | How to verify without touching product source |
| --- | --- |
| Failure / unknown-outcome / cleanup | **No product fault injection.** Use external perturbation only: a declared process kill, or a hand-edited watermark so `Store::open` refuses with `Integrity`. |
| Concurrency / visibility | Two `begin_save` on one Store; the second must fail `OwnershipUnavailable`. The busy timeout is zero, so this is observable without a race loop. |
| Zero retry / fallback / fsync / WAL | A sealed call-graph or manifest status **plus** observable runtime tripwires. **A fabricated zero is forbidden**: a counter that cannot fail an assertion is not evidence. |

If any of these cannot be verified without a product change, that is an owner
decision — stop and report (section 10). Do not add a test-only branch, a hook, a
feature flag or a fault-injection surface to `core/crates/*/src`.

### 3.10 WP-10 — Record the unmeasured rows in one place

The governing review's §10 action 10 is explicitly assigned to the Stage 6 owner.
Write one section of the Stage 6 qualification plan that lists, with a reason each:

1. the **complete-operation comparison** you inherit from Stage 5 (`VF-6`);
2. cold-cache rows;
3. pack-footprint rows;
4. process-memory rows.

It must **not** carry the Stage 5 component rows as a substitute for any of them.

Four further inheritances belong on the same list, because Stage 5 scoped them out by
name and no later stage has picked them up:

5. **Derived-unverified limits** — `MAXIMUM_LEVELS` 32, the 256-group pack ceiling and
   the level-31 tree are arithmetic, not observed. Either exercise each or carry the
   label `derived, unverified at scale` with the arithmetic.
6. **Storage-side fixed work budgets** — scoped out of Stage 5 but named
   (`stage-5-report.md` §16). Report them or say why they are out of scope.
7. **The sixth integration route**, "measured route with real payloads"
   (`stage-5-report.md` §16) — it follows `VF-6`, so it inherits that disposition.
8. **The two Stage 3-4 owner waivers (`S3-6`, `S4-5`) stay waived, unmeasured and
   unpromoted.** They are not yours to re-open and not yours to evidence.

**And you must record the D1 owner decision explicitly.** The governing review
(§11) requires Stage 6 to supply *"a frozen complete-operation comparator (or an
explicit owner decision that none will exist and the claim is withdrawn)"*. The
second branch is the one taken: `claim_kind = structural-complexity` (`CONTRACT.md`
§1, decision D1) withdraws the comparative claim, because only `component.primitives`
is a matched pair and `pipeline.filesystem`/`pipeline.c2` are `NOT_RUN`. State that
in the plan so the absence reads as a decision and not as an omission.

### 3.11 WP-11 — Documents, evidence and the issue

- `README.md` for the harness: how to run, what each verb guarantees.
- Evidence under `benchmark-results/fs-bench-pro-storage-content/` (gitignored,
  development) and, for admission, a dated append-only directory under
  `docs/roadmap/0.1/0.1.7/evidence/`.
- **The ledger entry is mandatory, not optional.** `AGENTS.md` §3 item 6 requires every
  measurement round to append an entry carrying **exact numbers, limits, the arithmetic,
  the identities, the reproduction command, and every non-passing line**. For Stage 6
  the successor ledger is the round's own
  `docs/roadmap/0.1/0.1.7/evidence/<stamp>/README.md` — declare it as such the first
  time you write one, so the successor is named rather than assumed. Section 7's receipt
  rule does not carry limits or arithmetic on its own; the ledger entry does.
- One comment per round on [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171):
  what changed, the evidence path, the row deltas, and every command that failed or
  did not run.
- Update the release register (`docs/roadmap/0.1/0.1.7/README.md`) and this document's
  status when you close.

---

## 4. LOC expectation

### 4.1 What you will write

| Bucket | Estimate |
| --- | ---: |
| Rust source (30 files) | 4,255 |
| Python (7 files) | 2,470 |
| Data + docs (`CONTRACT.md`, `README.md`, manifests) | 625 |
| **Harness authored** | **7,350** (range **6,800 - 8,200**) |
| Harness tests (29 Rust targets + 6 Python modules) | 6,700 |
| **Total new code** | **~14,050** |
| *Already committed* | *~1,400* (the seven specification docs) |

Test-to-harness ratio ≈ **0.91**. The product's own ratio is 25,203 test lines to
19,294 implementation lines = **1.31**, so this plan is below the repository's
existing standard, not above it. Do not shrink the tests to hit the estimate.

`~`7,200 inspected lines from v0.1.6 must **not** be carried: all Docker/cgroup/image
handling (~580), the native-tree fixture verification (376), the SDK edit schedules
(~1,080), `cold.py`'s tree-`acquire`/`compare` (227), the examples' `Provider`, three
more copies of the counting allocator, and `measure_filesystem.rs` (630).

### 4.2 Production LOC delta

**Zero is the expected figure, and the honest one.** `AGENTS.md` excludes benchmark
harnesses, development tools and fixtures from the production count, and
`core/tools/check_product_boundary.py:90-95` scans only `core/crates/*/src` and
`core/crates/*/sql` — `core/benchmark/**` is outside it. So every commit in Stage 6
reports:

```text
Production LOC: <before> -> <after> (delta 0)
```

with scope and counting method. ~14,050 new lines and a delta of 0 is the correct
outcome, not a contradiction.

**If you find yourself needing a product change**, section 5.2 governs. Report the
before/after honestly; never claim a delta of 0 for a commit that has one.

---

## 5. Rules

### 5.1 The line ceilings

| Scope | Rule |
| --- | --- |
| Production Rust under `core/crates/*/src` | **fewer than 1,000 physical lines — maximum 999**, including comments and blank lines |
| Product `lib.rs` and `mod.rs` | **maximum 200 physical lines**, declaration/delegation only |
| Shipped runtime SQL under `core/crates/*/src` or `core/crates/*/sql` | counts toward the 999 ceiling |
| **Benchmark Python files** | **exempt from any line-count limit** — an owner clarification, recorded here. `core/benchmark/**` is not product source and the boundary guard does not scan it. |
| Benchmark Rust files | likewise outside the guard; keep them focused anyway, and keep the largest (the oracle) near its ~620-line estimate |

The ceilings count **physical lines, including comments and blanks**. They are
different from production LOC: do not use one as the other. Enforced by
`python3 core/tools/check_product_boundary.py`.

### 5.2 Product source is closed to you

- **No bench-driven addition to product `src/`**: no instrumentation, hook, counter,
  accessor, feature flag or visibility change. `Store::path()` is already public
  (`cas/store.rs:173`), which is why no `Store::size()` is added.
- **No product change at all is expected.** If a #171 acceptance row genuinely cannot
  be satisfied without one, **stop and report to the owner** with the row, the exact
  blocking artifact and the minimal change you need. Do not decide it yourself.
- Two counter-attribution defects and `Store::create`'s ~4.6 ms fixed cost are already
  known and belong to [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
  **not to you**. Document them as scope restrictions; do not cite them in a gate.
- **Do not modify `layerfs-telemetry`.** It is frozen: time-only, std-only,
  `forbid(unsafe_code)`. It emits exactly `name`, `elapsed_ns`, optional `outcome`,
  optional `incomplete`, `children`, and cannot carry a sibling `resources` key.
  Its limits — `MAX_NODES = 1_024`, `MAX_DEPTH = 32`, `MAX_LABEL_BYTES = 128` —
  clip rather than fail, and clipping marks the node **and all its ancestors**
  incomplete. `TimingScope` is `!Send`/`!Sync`.
- Do not edit the root `crates/` reference tree.

### 5.3 Rules that can never be relaxed

- **No retry, no busy handler, no error-driven fallback, no alternate backend.**
  One operation, one attempt. An unknown persistence outcome is a failed result —
  never resend, never delete on a guess.
- **No `fsync`/`fdatasync`/`sync_all`/`sync_data`** anywhere in product or report
  paths, no WAL, no added crash-durability. SQLite stays MEMORY journal,
  `synchronous = OFF`, zero busy timeout.
- **No third-party patches, forks, vendoring or registry edits.** Builds stay
  `--locked`. An incompatible dependency is a reported limitation, not permission to
  patch it.
- **No fault injection** in product source, and no dependency patch to obtain one.
- **No `tools/preflight.sh`, no CI, no aggregate gate, no replacement for either.**
  Verify with the explicit commands in section 6 and report exactly which ran.
- **No warm-cache credit, no extra workers, no hidden spool, no best-of selection,
  no fabricated PASS.** A clean machine that had just booted must pay the same price
  inside the timed phase.
- **No shrinking a workload, relaxing a limit, inflating a timeout or changing worker
  counts to turn a number green.** A selection that cannot fit the budget is recorded
  `NOT_RUN` with its measured wall time.
- **No rewriting, re-labelling or promoting a historical receipt**, and no
  force-push to make a disclosure match.

### 5.4 Agent rules

- **No subagents.** You are the only agent. Do not launch, delegate to, or fan out to
  any other agent, and do not describe a plan that assumes one exists.
- **No codex.** Do not invoke `codex` or any other external coding agent or CLI.
- **One writer:** you. Do not stage, commit or push work you did not author. If the
  tree contains another workstream's uncommitted work (a concurrent
  [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) phase may leave files
  under `core/crates/layerfs-storage/src/`), leave it alone and never stage it.
- Commit only your own files, and only when their checks pass.

---

## 6. Checks to run

Per round, from the repository root:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py
git diff --check
```

Plus, in the harness workspace:

```sh
cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked
python3 core/benchmark/fs-bench-pro-storage-content/runner.py self-check
python3 core/benchmark/fs-bench-pro-storage-content/shared/test_lock_parity.py
```

Report every exit code, every command that failed or did not run, and the discovered
test counts. `fmt` has no `--locked` flag. **Never claim CI is green.**

A measurement round additionally requires: a declared cache state enforced equally in
both arms, a fresh `--output` path, one sample per case per arm, the measurement lock
respected, and the complete command inside budget.

---

## 7. Evidence, and how to replace the verifier you do not have

Stage 5 closed its rows with read-only verification subagents. **You have none.**
That does not lower the bar — it changes what carries it. No row may be closed on
your own word.

A row closes only when **all four** hold:

1. **A command exists** in the repository that produces the row's evidence, and it is
   named in the receipt.
2. **The receipt is raw and append-only** — the command, its exit code, its output,
   and the identity of the tree it ran on.
3. **The number is re-derived, not re-typed.** `runner.py verify` re-reads the raw
   artifacts and recomputes every published figure independently of the run that
   produced it. A figure that only the collecting process can produce is not evidence.
4. **A test would fail if the row regressed.** If nothing in the harness breaks when
   the behaviour changes, the row is `INCOMPLETE`, not `PASS`.

Write the assertion down. When a row is closed, state the exact command a later
reader should run to reproduce it, and run that command once from a clean shell
before you believe it.

**Receipts are the verifier.** They are also the reason the `--output` discipline is
absolute: a rerun that overwrites the evidence destroys the only witness you have.

Layout:

```text
benchmark-results/fs-bench-pro-storage-content/     gitignored, development runs
  prepared/<compatibility-digest>/                  immutable master + manifest
  <run_id>/<case_id>/
    timing.json        byte-verbatim product receipt (never edited)
    trace.jsonl        the harness trace
    receipt.json       derived: identity, gates, statuses, budget
    report.txt         derived: the four-axis human view
docs/roadmap/0.1/0.1.7/evidence/<stamp>/            admission evidence, append-only
```

Every retained file is hashed into a run manifest. Failures, `INELIGIBLE` rows and
discarded attempts stay on disk with their exit codes.

---

## 8. Terminal checklist

Close [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) only when **all**
of the following hold on one identified tree:

1. The harness builds, and every check in section 6 exits 0 with its counts recorded.
2. The registry self-check passes against the frozen 217-case array, and
   `tests/golden/registry.tsv` matches.
3. All nine [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) acceptance
   checkboxes are satisfied, each naming the receipt that decides it.
4. Every registered case reports a status; **0 unowned rows**; every `NOT_RUN`,
   `FAIL`, `INCOMPLETE` and `INELIGIBLE` row carries its measured state and a reason.
5. The three no-instrument areas of section 3.9 are verified, or reported as
   `NOT_RUN` with the blocking constraint and the owner decision you need.
6. The four unmeasured rows are recorded in one place with the D1 withdrawal of the
   comparative claim stated beside them (section 3.10).
7. E1-E4 are recorded, and the reflink rung's status follows E2's result.
8. Every commit's production LOC is reproducible on the committed tree by the audited
   counter.
9. No product source was modified, or — if an owner decision required it — the change
   is minimal, declared, and its LOC delta is reported honestly.
10. `README.md` and the release register state the final position, and
    [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) is closed with a
    final comment naming the tree, the evidence directory and the exact matrices.
    Nothing is tagged or released, and the release parent
    [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165) is **not** closed by
    this work.

Do not start [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172).

---

## 9. Anti-patterns that will fail

- Closing a row because the code "obviously" does it, without a receipt.
- Marking a row `PASS` from a smoke run, a passing suite or an attractive interface.
- Letting a timed phase read pages that setup, an earlier sample or another arm left
  resident — the canonical 19 GB/s-versus-2.1 GiB/s error.
- Moving work outside a timer, enlarging a timeout, shrinking a workload or changing
  worker counts to make a number green.
- Quoting a lifetime high-water mark as a phase peak, or a configured ceiling as
  observed use.
- Treating a counter that cannot fail an assertion as proof of zero retry/fsync/WAL.
- Adding a hook, counter or accessor to product `src/` so the harness can see
  something.
- Folding the 3 `component.primitives` diagnostic cases into the 217.
- Rewriting a receipt, or promoting a Stage 5 `NOT_RUN` or owner-WAIVED row.
- Declaring "all passed" while any row is `FAIL`, `INCOMPLETE` or unowned.
- Assuming a subagent will check your work. None exists.

---

## 10. If you cannot finish

Stop and post to [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) with:
the row, the exact failing artifact, the command and its output, the constraint that
blocks it, and the disposition you need from the owner (extend scope, waive in
writing, or change the contract).

An honest `INCOMPLETE` or `NOT_RUN` with a reason is an acceptable round outcome. A
green-looking matrix with an unresolved row is not. Do not self-waive a criterion: a
waiver needs the owner, in writing, on the issue.
