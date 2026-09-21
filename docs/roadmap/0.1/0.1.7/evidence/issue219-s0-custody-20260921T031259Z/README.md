# S0 — custody and pairing feasibility for `namespace-10000` (#219)

> **Status:** S0 deliverable. Inventory + two confirmed harness mechanics + the
> pairing decision. **No product change.** No performance claim is made by this page.
> Every number below is read from a raw `perf.jsonl` on disk; the reader is
> `inventory.py` in this directory and its output is `inventory.tsv`.

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch
`codex/219-ns10000`, based on `origin/main` `b0260df3a2ffc371773cd062feafd4b5e435bf1e`.

## 1. Receipt inventory — every `namespace-10000` row located

Search (recorded; run from the main worktree, which holds the historical results
root `benchmark-results/`):

```sh
find benchmark-results -name perf.jsonl | wc -l          # 1689
grep -rl "namespace-10000" benchmark-results --include=perf.jsonl
python3 inventory.py benchmark-results --json            # emits inventory.tsv
```

`inventory.py` walks every `perf.jsonl`, takes each `kind=sample` record whose
`identities.case` is an `init_namespace` case, pairs it with the `kind=summary`
record naming the same `timer`, and emits the case, arm, route, timer, median,
setup, cache contract, verification status and identity fields.

Eight `namespace-10000` rows exist in the whole tree. All eight are
`source_arm = candidate`; **zero are `baseline`**.

| median (ms) | arm | route | timer | setup | `cache_contract` | `verification_status` | status | source commit | source dirty | disk read | receipt |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: | --- |
| 402.721 | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `448a74bf` | **true** | 0.0 MB | `benchmark-results/worktree-archives/issue49-live-integration-ba7f7fbe1/host-store/campaigns/issue38-main-refresh-1788696142088555000-corrected/performance/init_namespace/namespace-10000/perf.jsonl` |
| 407.598 | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `95508a3d` | **true** | 0.0 MB | `benchmark-results/nine-family-fast-baseline/72f408b467840389/performance/init_namespace/namespace-10000/perf.jsonl` |
| **578.245** | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `56fc884e` | **true** | 0.0 MB | `benchmark-results/worktree-archives/issue49-live-integration-ba7f7fbe1/host-store/campaigns/issue38-main-refresh-1788696142088555000/performance/init_namespace/namespace-10000/perf.jsonl` |
| 928.022 | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `e9951fc1` | true | 0.0 MB | `benchmark-results/host-store/issue118/20260912/namespace-qualification/candidate-small10000-corrected-input/perf.jsonl` |
| 1020.422 | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `1ff1f2dd` | false | 337.4 MB | `benchmark-results/host-store/issue120/performance/init_namespace/namespace-10000/perf.jsonl` |
| 1100.711 | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | PASS | `8b5e0955` | false | 322.2 MB | `benchmark-results/issue152/g1/namespace-10000-r4/perf.jsonl` |
| — (FAIL) | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | FAIL | `e9951fc1` | true | — | `benchmark-results/host-store/issue118/20260912/namespace-qualification/candidate-small10000/perf.jsonl` |
| — (FAIL) | candidate | namespace | `layerstack_init_ns` | fresh-output | `null` | `NOT_RUN` | FAIL | `8b5e0955` | false | — | `benchmark-results/issue152/g1/namespace-10000/perf.jsonl` |

This reproduces the plan's table exactly: the same six medians, the same two
`INCOMPLETE` rows. It also settles the plan's "Recorded baseline rows: **none**
(8 candidate / 0 baseline)" — **confirmed**, and the two FAIL rows are the
`INCOMPLETE` pair.

`admission_eligible` is not a signal: the runner hard-codes
`"admission_eligible": False` in every summary it writes
(`benchmark/fs-bench-pro/shared/runner.py:922`). Rows predating that field simply
lack the key. It must not be read as a per-row verdict.

### 1a. The plan's target row is `SOURCE_DIRTY=true`

The three fastest rows — 402.721, 407.598 and the **578.245 ms** figure the bar
names — all carry `LAYERFS_SOURCE_DIRTY: "true"`. A dirty source seal cannot be
compared against a sealed arm (`AGENTS.md` §4: "a dirty source seal is recorded and
cannot be compared against a sealed arm"). The target is therefore not reproducible
from a clean checkout as recorded, independently of any speed question.

### 1b. The three fastest rows read nothing from storage

`initialization_disk_read_bytes` per row, against `scanned_bytes = 300 MB` in every
one of them:

| median (ms) | disk read during `layerstack_init_ns` | read per byte scanned |
| ---: | ---: | ---: |
| 402.721 | 0.0 MB | 0.000 |
| 407.598 | 0.0 MB | 0.000 |
| 578.245 | 0.0 MB | 0.000 |
| 928.022 | 0.0 MB | 0.000 |
| 1020.422 | 337.4 MB | 1.125 |
| 1100.711 | 322.2 MB | 1.074 |

The rows that report a plausible per-byte cost (≈1.1 MB read per MB scanned) are
the slow ones; the rows the bar names as the target served all 300 MB from the OS
page cache. CPU does not explain the split — the fast rows spend *more* CPU
(1.6–1.8 s user+system) than the slow ones (1.8–2.2 s) while finishing in
402–578 ms. The three fastest rows are cache-credited.

## 2. The two harness mechanics — both confirmed by reading the code

**2a. `--source-arm baseline` selects nothing for this family's performance runs.**
`benchmark/fs-bench-pro/src/infra.rs:499` parses and validates
`LAYERFS_BENCH_SOURCE_ARM`, and
`benchmark/fs-bench-pro/shared/runner.py:737` exports it — but for `init_namespace`
the performance branch calls

```rust
namespace_init_diagnostic(&work, &payload, scenario, seed.into(),
    &field(&fixture, "fixture_digest")?, profile, Some(&ContainerId(container.into())))
```

(`benchmark/fs-bench-pro/src/main.rs:2936-2944`), whose signature
(`main.rs:2935`) takes **no arm argument**. The arm is passed only to the verify
path, `namespace_verify_case(..., &source, ...)`. **Confirmed: a "baseline" row
produced by passing `--source-arm baseline` on a current build would run the same
code as the candidate and be mislabelled.** The only thing the flag does on the
performance path is stamp the string into the receipt.

Consistent with this, the tree's only `baseline` `init_namespace` rows are
`namespace-100-compact-v3` (3 rows) and `namespace-100000` (15 rows) — no
`namespace-10000` baseline row exists anywhere.

**2b. This case has no cold contract.**
`benchmark/fs-bench-pro/shared/cold.py:22-25`:

```python
def applies(selection):
    return (not selection.get("sequence")
            and selection.get("family", selection.get("family_id")) == "init_namespace"
            and selection.get("case", selection.get("scenario_id")) == "namespace-100000")
```

`namespace-10000` fails the `case == "namespace-100000"` test, so
`cold.applies()` is `False` and `runner.py:1337` writes
`"cache_contract": cold.CONTRACT if cold.applies(selection) else None` — i.e.
**`null`**. **Confirmed: the harness has no cold/residency contract for
`namespace-10000`.** The only cache declaration available for this case is the
record-level `fixture_cache_profile`, whose four permitted values all end in
`-uncontrolled` (`main.rs:1611-1615`, `2390-2393`).

## 3. Can a v0.1.6 reference row be produced? — the pairing decision

**Answer: a v0.1.6 build for this case can be produced, but it is not a distinct
arm, because v0.1.6 and current `main` compile the same product.**

What was checked, in order:

1. **The tag contains the case.** `git show v0.1.6:benchmark/fs-bench-pro/families/init_namespace/mod.rs`
   is **byte-identical** to the current file (sha256 `288656cae0d6a317…`, `diff` exit 0),
   and defines `namespace-10000` with `NAMESPACE_ANCHOR_BYTES = 100_000_000`.
2. **The tag contains the harness.** `src/main.rs`, `src/infra.rs`, `shared/runner.py`,
   `shared/runtime.py`, `shared/cold.py`, `families/init_namespace/{mod.rs,perf.sh,verify.sh}`
   are all **byte-identical** between `v0.1.6` and `HEAD`. (`v0.1.6` lacks
   `QUICKSTART.md` and `verify-selected.py`; neither feeds `harness_identity()`,
   which covers `runner.py`, `runtime.py`, `cold.py`, `verify-selected.py` — so a
   v0.1.6 run would carry the *same* `harness_identity` as the candidate arm.)
3. **The tag compiles the same product.** Over the `crates/` tree — the exact input
   set of `LAYERFS_PRODUCT_SEAL` (`runner.py:281-302`: every `.rs/.toml/.sh/.py/.sql`
   file under `crates/`, `target`/`__pycache__` excluded) — `v0.1.6` and `HEAD` have
   **216 production files with zero content differences**. `HEAD` adds exactly five
   files, all examples/tests, which are not part of the release binary:

   ```text
   + crates/layerfs-content/examples/rope_edit_oracle.rs
   + crates/layerfs-content/examples/rope_edit_timing.rs
   + crates/layerfs-content/examples/stage5_component_reference.rs
   + crates/layerfs-content/tests/stage5_reference_fixtures.rs
   + crates/layerfs-workspace/tests/deep_history_diagnostic.rs
   ```

   `git diff --stat v0.1.6..HEAD -- 'crates/*/src/**'` is **empty**. The only other
   change in the comparison is `Cargo.toml` gaining `exclude = ["core"]`, which does
   not affect the reference workspace's compilation.

   So `LAYERFS_PRODUCT_SEAL` differs between the two trees (`818bdad0…` vs
   `d20ab905…` under a replica of the runner's algorithm) **only because the seal
   hashes test and example files**, not because the product differs.

**Decision.** A v0.1.6 arm for `namespace-10000` is **constructible but not
meaningful**: it would be a second measurement of the same compiled product, so the
pair would report build/host noise, not a v0.1.6-vs-current difference. The bar's
part 2 ("pair the same case against the v0.1.6 reference arm") therefore cannot be
satisfied as written for this case, and must not be satisfied by the two forbidden
shortcuts: passing `--source-arm baseline` (mechanic 2a — mislabelled, same code) or
pointing at the historical rows (different product, different harness, different
images, and every one of them `cache_contract: null`).

For contrast, the rows the tree does call `baseline` are genuinely different
products, and were measured that way:

| baseline rows | source commit | product seal | image |
| --- | --- | --- | --- |
| 10 rows, `namespace-100000` | `f8bb6dcb` (2026-09-12, v0.1.4-era) | `f8db9e4ea64708ff…` | `sha256:32fa0eca8fbd…` |
| 3 rows, `namespace-100000` | `441be212` (2026-09-11) | `760eb0f2093488a6…` | `sha256:92096c1dc217…` |
| 2 rows, `namespace-100000` | `e56be75a` (2026-09-12) | `a43bba2a7863d662…` | `sha256:e49386809671…` |

Those commits **do** differ from `HEAD` in production source (34, 42 and 37
differing files respectively), i.e. they are real reference arms — unlike v0.1.6.

## 4. What this changes for S1/S3

- S1's bar "at or under 578.245 ms with a declared cache contract" cannot be met by
  reproducing that row: it is a dirty-tree, cache-credited measurement (1a, 1b).
  S1 must instead report a clean, sealed, cache-declared row and say plainly what it
  is — which is what
  [`../issue219-s1-reproduction-20260921T031259Z/`](../issue219-s1-reproduction-20260921T031259Z/)
  does.
- S3's pair cannot be built from v0.1.6 (§3). A defensible pair needs a reference
  commit whose production source actually differs, which is a separate owner ruling
  because it changes the arm the bar names.

## 5. Reproduction

```sh
# inventory (read-only)
cd <main worktree>            # holds benchmark-results/
python3 <this dir>/inventory.py benchmark-results --json > inventory.tsv

# pairing checks
git diff --stat v0.1.6..HEAD -- 'crates/*/src/**'
git show v0.1.6:benchmark/fs-bench-pro/families/init_namespace/mod.rs | diff - benchmark/fs-bench-pro/families/init_namespace/mod.rs
```

Files in this directory: `inventory.py` (reader), `inventory.tsv` (all 55
`init_namespace` rows), `inventory.stderr.txt` (row/file counts).
