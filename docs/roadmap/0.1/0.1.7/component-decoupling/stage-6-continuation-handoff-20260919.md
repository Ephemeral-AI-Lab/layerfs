# Stage 6 continuation handoff — 2026-09-19

> **Status:** Active implementation routing. This is the executable assignment for
> the successor Stage 6 agent, written at the end of round 1. It **supersedes the
> routing** in [`stage-6-handoff.md`](stage-6-handoff.md) — that document's rules
> still bind, but its "you are starting from an empty directory" premise is now
> false — and it supersedes nothing else. Stage 5's records are the historical
> record and are not edited.
>
> **Issue:** [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) — **open**,
> round 1 recorded at
> [issuecomment-5734054433](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171#issuecomment-5734054433).
> Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). Next child
> is [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) (Stage 7) — **not
> yours**.
>
> **How you work:** one agent, working alone. **No subagents. No codex.** Everything
> happens in your session.
>
> **Read first, in this order:** [`AGENTS.md`](../../../../../AGENTS.md) §1-§4,
> [`core/AGENTS.md`](../../../../../core/AGENTS.md),
> [`docs/general/benchmark_rules.md`](../../../../general/benchmark_rules.md),
> the frozen specification under
> [`core/docs/benchmark/fs-bench-pro-storage-content/`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md)
> (seven files), and the harness
> [`README.md`](../../../../../core/benchmark/fs-bench-pro-storage-content/README.md).
> Round-1 evidence:
> [`evidence/stage-6-qualification-20260918T175400Z/README.md`](../evidence/stage-6-qualification-20260918T175400Z/README.md).

## 1. Where the tree actually is

```text
HEAD                     5b6c798e8
harness commits          d471d3c66  the workspace, the locks, the frozen registry
                         14cf16d83  the shape drivers, instruments, oracle, runner
                         5b6c798e8  the full-lane round, E1-E4, WP-9 arms
production LOC          19503 (core) / 65417 (reference) / 84920 combined — delta 0
                         for every commit; core/benchmark/** is outside the guard
```

The harness builds, self-checks and runs end to end. **Start by reproducing that,
not by reading code:**

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
python3 $H/runner.py self-check
python3 $H/runner.py list --lane smoke
python3 $H/runner.py perf --lane smoke --out /tmp/smoke
python3 $H/runner.py report --run /tmp/smoke
```

**The raw round-1 run is not in the repository.** `benchmark-results/*` is
gitignored, so `benchmark-results/fs-bench-pro-storage-content/run-20260919T-full/`
exists only on the machine that produced it. The committed evidence directory holds
the derived artifacts (`run-full.json`, `report-full.txt`, both verification passes,
`experiments-E1-E4-W1-W4.json`). If you need the raw receipts, re-run the lane into a
**new** directory; never into that one.

## 2. What round 1 measured

`runner.py perf --lane full` — 220 cases in 322.0 s:

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **122** | **3** | **92** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **122** | **3** | **95** |

0 unowned rows. One sample per case per arm, fresh output per run, measurement lock
held, `LAYERFS_CONSTRUCTION_WORKERS=1` exported by the runner and asserted in every
receipt. `runner.py verify` re-derived all 220 statuses with **0 disagreements**,
a sealed call-graph **PASS** over 120 product source files and runtime tripwires
**PASS** over 138 stores.

**13 of the 21 registry groups have drivers** — 141 of 217 admission cases. The
other **79 rows are `driver-unimplemented`** (76 admission + 3 diagnostic): 20
`many-tiny`, 14 `reuse.workspace`, 12 `tree.construct-traverse`, 12
`change-locality`, 8 `fs.build-scale`, 4 `tree.namespace-mutation`, 4 `pipeline.*`,
3 `component.primitives`, 2 `pool.cold-warm`.

### Why the 95 non-`PASS` rows are non-`PASS`

| Cause | Rows |
| --- | ---: |
| `driver-unimplemented` | 79 |
| product error | 12 |
| complete-command budget overrun (27.98 / 28.66 / 29.27 / 29.62 s vs the 25 s exception) | 4 |
| **`FAIL` — harness fixture defect** | 3 |

### Estimate versus actual, stated honestly

| Bucket | Estimate | Actual |
| --- | ---: | ---: |
| Rust source | 4,255 (30 files) | **6,683 (34 files)** |
| Python | 2,470 (7 files) | **3,347 (9 files incl. runner)** |
| Data + docs | 625 | 393 |
| **Harness authored** | 7,350 | **~10,459** |
| Harness tests | **6,700** | **274** (36 Rust + 236 Python self-check) |

The Rust overrun is real and is in `src/ops/` — the C2 drivers carry more
declaration than the estimate assumed, because every row has to state its copy
rung, its attribution and its oracle phase. **The test bucket is the one that is
missing, and it is the reason no row may yet be called closed** — see §4.

## 3. The one decision you need before touching the twelve product-error rows

Do **not** fix these by changing the harness until the owner answers. The failing
construct is precisely characterised, and the two readings measure different things.

| Offered inside **one** save operation | Result |
| --- | --- |
| distinct members (`dedup-cross-file-unique-10`) | PASS |
| members sharing a base (`dedup-cdc-insert-10`, `dedup-cdc-delete-100`) | `UNIQUE constraint failed: objects.object_id` (`ConstraintViolation`, 1555) |
| byte-identical members (`dedup-cross-file-identical-10`) | `Integrity("group ordinal")` |

`cas/membership.rs::reuse_or_collide` already reuses a repeat that finds a **stored**
row; the `INSERT` in `sqlite/write.rs::object_insert_sql` has no `ON CONFLICT`. So
the product reuses duplicates **across** saves and collides on duplicates **within
one save wave**. Whether one save is meant to carry one file or many is the
registered semantics of `c2.reuse.cross-file`, whose declared equation is
*"identical profile → reuse for every member after the first"*.

**Two readings, one question:** product fix, or a change to the registered case
semantics (one `begin_save`/`finish` per member)? Writing the second one turns all
twelve green and is a pure harness change — which is exactly why you must not do it
unilaterally. The remaining two errors are separate and smaller:
`CapacityExceeded { pack.assembled_length, limit 262144, actual 262147 }` (2 rows,
three bytes over `PACK_LIMIT`) and `InvalidRecord("mapping coverage")` (1 row).

## 4. The work, in the order that unblocks the most

### WP-A — the test suite. **Do this first; it is the largest gap and the cheapest.**

`src/gates.rs` — which the frozen specification calls *"the highest-value test
target"* — has **zero** tests. That is not a detail: the gate table is what decides
every published status, and nothing currently fails if a band, a severity order or
an aggregation drifts.

Write, in this order:

1. **`tests/digest_vectors.rs` — SHA-256 known-answer tests.** This one is urgent
   and subtle: the O2 read-back gate compares two digests produced by *the same*
   `Sha256` implementation, so **a wrong SHA-256 still compares equal**. The gate
   detects content mismatch; it does not verify the digest. Use the FIPS 180-4
   vectors (`""`, `"abc"`, the 448-bit and 896-bit padding cases, a multi-block
   input) and a streaming-vs-one-shot equality test.
2. **`tests/gates_bands.rs`** — every `(Growth, Metric)` pair against its frozen
   band; `per_doubling` on the real ladders (1→10, 10→100, 100→500 MiB, whose
   doublings are 3.3219 / 3.3219 / 2.3219, **not** 1); `slope` on a known
   exponent; `Metric::Time` never producing `FAIL`; `aggregate` worst-gate ordering
   including `NOT_RUN` outranking `TARGET_MISS`.
3. **`tests/window_containment.rs`** — a child outside its parent, siblings that
   overlap, an unenclosed root, an unknown parent, and a balanced tree that passes.
   Prove that `Sigma self_ns == root.elapsed_ns` is **not** used as an `attach`
   detector.
4. **`tests/oracle_independence.rs`** — `Expectation::spliced` against a naive
   materialised splice, on head/middle/tail/insert/delete/zero-length edges; a
   read-back whose bytes differ must fail the digest gate.
5. **`tests/registry_negative.rs`** — the cardinality array, a duplicated ID, an
   unregistered family name, and the profile-per-tier ruling.
6. **`tests/instruments_selfcheck.rs`** — heap window accounting (peak, charged,
   alloc count) on a known allocation pattern; residency on a file this process
   just wrote; the RSS bundle's fail-closed `peak_is_usable`.
7. Then the Python side: `python3 -m unittest` modules for `space` (the fabricated
   zero), `trace` (flatness, sequence, aggregation), `receipt` (append-only,
   budgets), `copyladder` (R1 refusal, ENOSPC), `residency` and `invariants`,
   wrapping the existing `self_check` functions so `unittest discover` finds them.

**Exit criterion:** `cargo test --locked` in the harness reports the new targets and
`python3 -m unittest discover -s $H/shared -p 'test_*.py'` collects the Python ones.
Only after `tests/digest_vectors.rs` is green may a row's O2 gate be cited as
evidence for a digest.

### WP-B — the six filesystem families and the pipeline: 76 admission cases

`c1.many-tiny` (20), `c1.tree.construct-traverse` (12), `c1.change-locality` (12),
`c1.fs.build-scale` (8), `c1.tree.namespace-mutation` (4), `c2.reuse.workspace`
(14), `c2.pool.cold-warm` (2), `pipeline.*` (4).

This is not blocked on the product. `FilesystemInput`, `FilesystemObjects`,
`FilesystemRoot`, `DirectoryUpdate`, `InodeUpdate`, `PathName`, `InodeValue`,
`InodeScope` and `build_filesystem`/`update_filesystem` are all public — the frozen
handoff says so explicitly. What is missing is a **fixture builder**: a documented
function that turns a recipe (profile, entry count, seed) into the sorted
`DirectoryUpdate`/`InodeUpdate` lists the frozen profile accepts, plus a
`--emit-input DIR` / `--load-input DIR` pair so the prepared artifact can be cached
and re-used instead of rebuilt per sample. Read
[`filesystem-tree.md`](filesystem-tree.md) and
[`stage-5-verification-addendum-20260917.md`](stage-5-verification-addendum-20260917.md)
before writing it, and mirror the ordering rules
(`DirectoryUpdate.changes` strictly sorted by name,
`directories` sorted by parent, `inodes` sorted by serial, `new_inodes` sorted and
unique — `FilesystemInput::check` refuses everything else).

**Exit criterion:** each of the eight groups has a driver, its rows run, and its
oracle is the O4 three-tuple (directory root, inode table, filesystem root) plus a
listing equal to the fixture manifest.

### WP-C — the three `FAIL` rows (a harness fixture defect, recorded not refitted)

`overwrite-fixed-64k-chunk-count-decrease-{10m,100m,500m}` do not move the extent
count (536 → 536, 5417 → 5417, 26972 → 26972, `payloads_created` 2). 64 KiB of
zeros chunks to exactly two extents where the noise it replaced also occupied two;
the 1 MiB tier does move (53 → 52), so the effect is real and unobservable at those
offsets. Fix the **replacement content or the base's local structure**, never the
gate, the limit or the cardinality, and re-run into a **new** output directory.
`crate::families::c1_cdc::{START, LEN, ROTATIONS}` are frozen; the base content is
not.

### WP-D — fit the four budget-overrun rows without touching the timed phase

The oracle replay and the read-back currently sit **inside** the child process, so
they are inside the complete command. `benchmark_rules.md` §6 already asks for
setup, performance, verification and cleanup to be separate phases. Moving the
replay and read-back into a second, unmeasured child invocation (a `verify`-mode
run of the same case) would fit the rows **without** shrinking the workload or
enlarging a timeout — which is the only permitted way to make them fit. If the
owner prefers, the alternative is a longer declared exception; that is an owner
decision, not yours. **Do not shrink a payload tier.**

### WP-E — the process-kill arm

The one WP-9 arm round 1 did not implement, because no child mode pauses inside a
save. A declared wait state in the **harness** child is ordinary harness work: it
touches no product source, no hook, no feature flag and no fault-injection surface
in `core/crates/*/src`. Round 1 asked for a ruling on this out of caution; the
handoff anticipates it by name (*"a declared process kill"*). Implement it, record
the outcome including a failure, and keep the other two arms green:

* **W1** (already `SATISFIED`): a hand-edited `store_policy.retained_pack_ceiling`
  makes `Store::open` refuse while the unperturbed control still opens.
* **W3** (already `PASS`): `lifecycle-begin-save` attempts a second `begin_save`
  on one Store and gates `OwnershipUnavailable`.
* **W4** (already `SATISFIED`): sealed call-graph scan over 120 product source
  files plus the runtime tripwires.

### WP-F — re-run, re-verify, re-publish, then judge closure

Only after WP-A..WP-E: a fresh full lane into a new directory, `verify` with a new
tag, `report`, `calibrate`, a new dated evidence directory, and a fresh matrix. Then
check the nine #171 acceptance checkboxes again — round 1 left 1, 2, 4 and 5
partial, and **#171 stays open until every one holds**.

## 5. Traps round 1 hit, so you do not hit them again

1. **`Cargo.toml` needs an empty `[workspace]` table** or Cargo refuses the
   package. Already done; do not remove it.
2. **Adding any dependency re-opens lock parity.** `shared/test_lock_parity.py`
   found **twelve** shared packages resolved ahead of the product seal on the first
   run; every one was pinned with `cargo update --precise`. Run the parity test
   after *any* manifest change, and prefer no new dependency at all.
3. **`Timing::record` closures need an explicit error type** — write
   `Ok::<_, layerfs_storage::StorageError>(…)`. Inference will not do it.
4. **`classify` is not publicly reachable.** Use
   `FileView::open(...).file_state()` for the representation and
   `FileState::extent_count` for the chunk count.
5. **`apply_edits` cannot take the same store as provider and consumer.** Use
   `PairProvider::new(&result_store, &base_store)`.
6. **The oracle must compare the recipe's logical bytes, not the root object's
   canonical bytes.** Round 1's first smoke run produced a `FAIL` from exactly this
   mistake.
7. **`TreeStore` holds `FinalizedObject`, not `Vec<u8>`.** A C2 row has to offer
   the same objects to a `Store`, and `FinalizedObject::new` takes a **role**;
   re-wrapping bytes under a guessed role stores the wrong envelope.
8. **The runner must not pre-create the case directory.** The child refuses an
   existing one (append-only discipline), so the runner only checks that it does
   not exist.
9. **The runner must set `LAYERFS_CONSTRUCTION_WORKERS=1` in `os.environ`**, not
   only in the child's environment: the receipt records what the runner's own
   environment said, and an empty value fails the assertion.
10. **Python's `mmap` object cannot give a read-only buffer's address**
    (`from_buffer` needs writable, and `ACCESS_COPY` would make `mincore` describe
    a private copy). `shared/residency.py` calls libc `mmap`/`mincore`/`msync`
    directly.
11. **`os.setxattr` does not exist on this build.** `experiments.py` uses libc
    through ctypes. An experiment that silently skipped the attribute comparison
    would be reporting a fidelity it never checked.
12. **`st_nlink == 1` is a statement about files, not directories.** A directory's
    link count is structural; asserting 1 for it refutes every faithful clone.
13. **The page size here is 16,384 bytes, not 4096.** Read it; never hardcode it.
14. **`verify` must apply the budget rule.** Round 1's first verifier compared the
    trace's gate set against the published status and reported four false
    disagreements on budget-overridden rows. The corrected pass re-derives the
    budget classification from the recorded wall time and worst-cases it with the
    trace status. Both files are retained — do not delete the failing one.
15. **`--case` overrides the lane selection entirely**; `DECLARED_EXCEPTIONS` in
    `runner.py` is a hand-maintained ID list and must stay in sync with the case
    IDs if you ever re-render the registry.
16. **This shell's working directory is not stable across invocations.** Use
    absolute paths in every command; `timeout(1)` is not installed.

## 6. Rules that bind you, restated because they are the ones round 1 leaned on

* **No product source change.** Nothing round 1 did touched `core/crates/*/src`;
  production LOC delta is **0** for every commit and must stay so. Every finding is
  reported, not repaired.
* **A timed phase never retains, and every oracle is a second, unmeasured,
  byte-identical operation.** The two roots must agree; a replay that failed to
  reproduce the measured operation is caught by a gate rather than trusted.
* **One sample per case per arm. Fresh output. Append-only everything.**
* **Never shrink a workload, relax a limit, inflate a timeout or add a worker to
  turn a number green.** A selection that cannot fit is `NOT_RUN` with its measured
  wall time.
* **`elapsed_ns` never gate-decides.** Counters, heap and disk do.
* **Do not decide the §3 question yourself.** Do not re-open a Stage 5 row. Do not
  promote a Stage 5 `NOT_RUN` or owner-WAIVED row. Do not run `tools/preflight.sh`,
  restore it, or create an aggregate gate or CI replacement.

## 7. How to check round 1's claims rather than believe them

```sh
H=core/benchmark/fs-bench-pro-storage-content
python3 $H/runner.py self-check                 # instruments, registry, golden, lock parity
python3 $H/runner.py perf --lane full --out /tmp/your-own-run
python3 $H/runner.py verify --run /tmp/your-own-run
python3 $H/runner.py report --run /tmp/your-own-run
python3 $H/runner.py calibrate --out /tmp/your-own-run
python3 core/tools/check_product_boundary.py
python3 tools/production_loc.py
git log --format='%H %s' -3
```

Every receipt carries the identity it ran under, including the harness binary's
SHA-256 — a rebuilt binary invalidates the run it produced, which is why the
round-1 full lane and the round-1 WP-9 lane (built before a later driver change)
are separate directories with separate `run.json` files.
