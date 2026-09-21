# Stage 6 measurement harness — C1/C2 structural complexity

The executable half of [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
It measures `layerfs-content` (C1) and `layerfs-storage` (C2) **through their public
APIs only**, against the frozen case registry in
[`core/docs/benchmark/fs-bench-pro-storage-content/`](../../docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md).

```text
claim_kind = structural-complexity
```

The question is whether the C1/C2 core is algorithmically sound — correct at every
declared boundary, scaling as its architecture claims, and bounded in memory, CPU
and space. It is **not** a comparison against v0.1.6, and no receipt here supports
one. Every gate is absolute and single-arm, and `elapsed_ns` never decides a gate.

## How to run

```sh
H=core/benchmark/fs-bench-pro-storage-content

python3 $H/runner.py self-check                 # every Python self-check + registry + lock parity
python3 $H/runner.py list --lane smoke          # the twenty-case development lane
python3 $H/runner.py perf  --lane smoke --out /tmp/run-smoke
python3 $H/runner.py perf  --lane full  --out /tmp/run-full
python3 $H/runner.py verify --run /tmp/run-smoke
python3 $H/runner.py report --run /tmp/run-smoke
python3 $H/runner.py calibrate --out /tmp/run-smoke   # E1-E4, untimed
```

`perf` builds with `cargo +1.85.1 build --release --locked`, exports
`LAYERFS_CONSTRUCTION_WORKERS=1` itself, holds the measurement lock for the whole
invocation, and **refuses an existing `--out` path**. Every case writes its raw
trace; the runner derives a receipt from it and never edits either.

## What each verb guarantees

| Verb | Guarantee |
| --- | --- |
| `list` | Prints the binary's own generated registry, so what is listed is what is registered. |
| `prepare` | Acquires the prepared artifacts a selection needs, once. A row whose registry declaration names a master is acquired for real into `prepared/<case_id>/`, hashed per file, given a `manifest.json`, keyed by its compatibility digest and sealed read-only; a row that declares none is recorded `not-produced` **with that reason** rather than faked. The declaration is a column of the binary's own registry, so no family list is maintained by hand. |
| `perf` | One sample per case per arm; fresh output; measurement lock held; the complete-command budget enforced per case; a receipt that names the tree it ran on. **The budgeted quantity is a formula** — `declared_ns + 250 ms`, where `declared_ns` is preparation + operation + verification + cleanup — and not the runner's raw process wall, which is still published as `complete_command_ns` but no longer decides (`CONTRACT.md` §4, erratum E4). The verification mode defaults to `full` for a whole-lane run and to `sample` for iteration — `--lane smoke` or any explicit `--case` — and `--verify` always wins over that default. A row whose driver declares `oracle_phase: verify-invocation` is run in phases: the performance invocation is budgeted on its own complete command, and verification is a **second, unmeasured invocation** charged to its own 60 s budget. |
| `prune` | Removes prepared artifacts the current compatibility key no longer accepts — superseded entries, entries sealed under a different key, unsealed directories — and **never** a master the current key still accepts. Holds the measurement lock, supports `--dry-run`, and writes an append-only record naming every entry and its reason. |
| `verify` | Re-reads the raw artifacts and re-derives flatness, sequence, worst-gate aggregation, budget classification, and — for C2 rows — the space and pack accounting read out of the Store file itself. |
| `report` | Renders the ladders, bands and the four-axis view. Time is printed and never decides. |
| `self-check` | Runs every Python self-check, the registry self-check, the golden comparison and the lock-parity test. |
| `calibrate` | Runs the untimed experiments E1-E4 and records each outcome, refutations included. |

## The four measurement phases

`benchmark_rules.md` §6 requires setup, performance, verification and cleanup to use
separate timing and resource scopes. The harness names them:

| | phase | what runs | state |
| --- | --- | --- | --- |
| a | preparation | acquire the fixture — build it, or copy/load a prepared artifact — and de-warm | `preparation_wall_ns`, with the copy and de-warm also as `acquisition_wall_ns` |
| b | **work** | the operation the row claims | `operation_ns`, the product's own telemetry root; the golden benchmark number |
| c | verification | the oracle: a second, byte-identical operation and its read-back | `verification_wall_ns`; a separate unmeasured invocation for rows that declare it, with its own 60 s budget |
| d | cleanup | destroy the per-case copy, close | `cleanup_wall_ns` |

Each invocation writes one `phases-<invocation>.json` beside its trace, and the runner
composes the six published fields from those files and the product's `timing.json`.
`verify` re-derives them and fails closed when the declared phases do not reconcile with
the process wall inside a declared tolerance. `handoff_ns` is published beside
`operation_ns`, so harness work inside the measured region is visible rather than absorbed
into the golden number.

**What is inside the timer is fixed per case shape** by
`test_setup_and_cache_discipline.md` §2.2 — a driver does not choose it:

| shape | inside the timer |
| --- | --- |
| `c1.construct.*` | `construct_bytes` over a slice |
| `c1.edit.*` | `apply_edits` — base read through the provider |
| `c1.fs.*` | `update_filesystem` (+ `Store::open` in pipeline mode) |
| `c2.save.*` (fresh run, no base) | `Store::create` + save |
| `c2.reuse.*`, `c2.delta.*`, `c2.pool.*` | `Store::open` on a sample copy + save |
| `c2.read.waves` | `Store::open` on a sample copy + read wave |
| `pipeline.*` | `update_filesystem` + `Store::open` + save + ack |

So a fixture is an **input**, never part of the measurement. `c2.delta.*` does not
measure the chunking of its members; `c1.edit.*` does not measure the construction of
its base; even in `pipeline.*`, the base is setup and the measured C1 half is the edit.
A driver that builds its fixture inside the timer is measuring setup, which is why a
row handed no artifact fails closed with that reason instead of silently rebuilding.

**A half-built acquisition is never consumed.** An artifact directory that is present
but unsealed is refused with its reason, so an interrupted acquisition cannot be
mistaken for a prepared one.

## Evidence layout

```text
benchmark-results/fs-bench-pro-storage-content/   gitignored, development runs
  prepared/<case_id>/          the acquired artifact, sealed and read-only:
                                 manifest.json   per-file sha256/bytes, compatibility key
                                 objects/pack.bin, objects/index.tsv
                                                 the packed canonical object set: role,
                                                 references and predecessors per object
                                 members.tsv     the offered member set, with expectations
                                 values.tsv      named scalars, identities, expectations
                                 store.sqlite    the base Store, when the row opens a copy
                                 prepared-tree.tsv  the filesystem input, for C1-11
                                 sealed.tsv      completion marker + provenance
                                 acquisition/    the acquisition invocation's own trace
  prepared/manifest-<stamp>.json   append-only acquisition record
  <run>/<case_id>/
    timing.json      byte-verbatim product receipt (never edited)
    trace.jsonl      the harness trace, flat layerfs-trace-v1
    phases-perf.json     the phase spans the performance invocation observed
    phases-verify.json   the same for a deferred verification invocation
    receipt.json     derived: identity, gates, statuses, counters, phases, budget
  <run>/run.json       selection, identity, tally
  <run>/manifest.json  every retained file, hashed
  <run>/verification.json  the re-derivation, append-only
  <run>/report.txt     the four-axis human view
docs/roadmap/0.1/0.1.7/evidence/<stamp>/   admission evidence, append-only
```

## The measurement rules this harness enforces

**A timed phase pays for its own work from a declared cache state.** Every row
declares one of `warm-in-process-fixture`, `prepared-dewarmed` or
`created-in-sample`; states are never pooled. A row claiming a de-warmed read
carries `resident_pages == 0` and `disk_read_bytes >= 0.9 x requested`, because
`mincore` alone cannot distinguish a cache-served read from a device read.

**Timed phases do not retain.** Every measured phase runs with a non-retaining
consumer, so no harness allocation inside the heap window can be charged to the
product, and every oracle is a **second, unmeasured, byte-identical operation**
into a `TreeStore`. The two roots must match: a replay that failed to reproduce the
measured operation is caught by a gate rather than trusted.

**The counted instrument is the allocator, not RSS.** The counting `GlobalAlloc` gates the
O(1)-memory claim and publishes `heap.peak_incremental_bytes` for the measured phase. The
child's peak resident set is published beside it as `rss.process_peak_bytes`, from
`getrusage(RUSAGE_SELF).ru_maxrss` — a **lifetime** figure for the whole child, labelled as
one, which one child per case makes that case's bound. The measured region's own CPU is
`cpu.user_ns` / `cpu.system_ns`, from two `getrusage` reads taken at the phase boundary,
outside the region by construction.

**The 10 ms `RssSampler` is implemented, self-checked, and wired to no row — deliberately.**
An earlier revision of this paragraph claimed a missed sample or an excessive gap makes the
phase peak unavailable and the row `INELIGIBLE`. That was never true, and it should not be:
at a 10 ms interval the sampler cannot cover a phase under ~200 ms, which is most of this
lane, and a sampling thread inside the measured region perturbs the thing it measures. A
per-row sampler would buy a number it could not stand behind at the cost of the
measurement, so `RssBundle::peak_is_usable` stays a tested primitive rather than a gate.

**One clock.** `CLOCK_MONOTONIC_RAW` (id 4), Rust and Python alike. `Sigma self_ns
== root.elapsed_ns` is a tautology of the product's own tree arithmetic and is
**not** presented as an `attach` detector; what `window.rs` checks is containment,
sibling non-overlap and root enclosure, which can actually fail.

**Nothing is written to product source.** No counter, hook, accessor, feature flag
or visibility change — `Store::path()` is public already, which is why no
`Store::size()` exists. All resource observation lives here.

## Registry

222 rows = **219 admission + 3 diagnostic**. `component.primitives` is registered,
runs and is receipted, and is excluded from admission and from every count: under
`structural-complexity` its receipt is diagnostic and cannot gate.

> **Amended 2026-09-21 (#219 round 21).** This paragraph read *"220 rows = 217
> admission + 3 diagnostic"* and was **already stale when round 21 started**: the
> pipeline group held five rows while `FROZEN_CARDINALITY`'s `pipeline.*` entry still
> read `4` and `ADMISSION_CASES` still read `218`, so `registry::self_check` reported
> `frozen cardinality array` and `runner.py self-check` failed. Round 21 registered
> `pipeline-namespace-100000` and moved the constant to `6`, which fixed the
> pre-existing defect in the same edit because the count had to move anyway. The
> arithmetic: 219 admission + 3 diagnostic = 222, and `FROZEN_CARDINALITY` sums to
> 219. `CONTRACT.md` §3 is never re-dated and still states 4 and 217; the registry,
> its golden table and `registry::self_check` are the declaration of record.

`--smoke` is one tier per family: twenty cases. `--smoke` is twenty and not
twenty-one because `c2.delta.boundaries` is a registered sub-lane of a family, not
a family, and `pipeline.*` is a registry group rather than a family.

The golden table's `prepared` column is the registry's own declaration of whether a row's
fixture is a **prepared master** (`-`, `object-set`, `base-store` or `input-tree`). 118 of
220 rows declare one. `runner.py` reads that column to decide what to acquire and what to
hand a measured child through `--load-input`, so no family list is maintained by hand.

### Two readings this harness had to fix, recorded rather than assumed

* **The bracketed profile list is one case.** `c1-families.md` section 3.1 writes
  `[-compact-v2|-mixed-v4]`; the frozen parsing rule says that is one case rendered
  with a tier-selected profile. `families::profile_for_tier` fixes which variant
  each tier takes, and `tests/golden/registry.tsv` names the profile every row
  actually got, so the ruling is visible in a diff.
* **C1-1 and C1-2 share one ID list.** Case IDs are globally unique, so the second
  family's IDs carry a `-chunked` infix, and the families differ by construction
  entry point: `construct_bytes` (the payload is an in-memory slice) against
  `construct_stream` (the payload arrives through `impl Read`). Both run the same
  1/10/100/500 MiB ladder, and the route each row took is a gate, not a label.

## Statuses a row can carry

`PASS`, `FAIL`, `TARGET_MISS`, `INCOMPLETE`, `INELIGIBLE`, `NOT_RUN`. A row whose
driver does not exist yet is `NOT_RUN` with the driver named — never `PASS`, and
never silently dropped from the report.

## Reuse

The harness keeps its own lockfile, guarded by `shared/test_lock_parity.py`: every
entry the harness lock carries must match an identical `(version, checksum)` entry
in `core/Cargo.lock`, a one-sided checksum is refused, and a harness-only registry
package is refused. On the first run it found twelve shared packages that had
resolved ahead of the product seal; every one was pinned back with
`cargo update --precise`, and the comparison now reports 46 shared entries, 0
mismatches, and 0 product entries the harness does not link.


## What is not yet true here — see [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)

Round 5 ([#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)) closed the
accounting half of this section. Round 5b closed the preparation half, in the receipt at
[`stage-6-round5b-20260919T000000Z`](../../../docs/roadmap/0.1/0.1.7/evidence/stage-6-round5b-20260919T000000Z/README.md).
Recorded here so a reader is not misled by the sections above.

**Now true.**

- **The four phases are published.** Every receipt carries `preparation_wall_ns`,
  `acquisition_wall_ns`, `operation_ns`, `verification_wall_ns`, `cleanup_wall_ns`,
  `handoff_ns` and `complete_command_ns`; the child publishes one
  `phases-<invocation>.json` per invocation and `verify` re-derives all six from it and
  from the product's own `timing.json`, failing closed outside a declared tolerance
  (250 ms + 2%). The boundary between setup and the operation is the driver's own
  `Timing::record`, reached through `ops::measure`, so no driver can choose it.
- **`operation_ns` is the report's primary axis and the golden benchmark number**, and
  `sum(operation_ns)` is published for the lane so a row that got faster at another row's
  expense is visible.
- **Every registered row writes `timing.json`.** `ops/fs.rs` never called `write_timing`,
  so 56 of 217 passing rows published no operation time; `ops::measure` now writes it for
  every row that measured something, and `g7.tree-complete` still gates completeness.
- **The frozen oracle's O1 and O3 are pinned constants.**
  `tests/golden/expected.tsv` (1,969 rows, `include_str!`-embedded so the harness binary's
  own sha256 covers it) pins every published counter of every admission case and one
  identity digest per row; `main` gates them and `verify` re-derives them from the row's
  whole trace. The registry self-check asserts the coverage, so the pinned set cannot
  shrink unnoticed.
- **The read-back is gone from the four families whose oracle does not ask for O2**
  (`c2.delta.cdc-locality` is O1 + O3, `c2.reuse.workspace` O1 + O5, `c2.footprint`
  O6 + O1, `c2.pool.cold-warm` O1 + O3). They gate the pinned identity through the
  product's own presence path instead of decoding a whole logical set to answer a question
  the specification never posed. Where O2 **is** required, each distinct
  `(root, expectation)` pair is verified once — complete, not sampled.
- **`--reuse-pass` exists**, on both `verify` and `perf`, and fails closed on schema,
  identity, hard-limit and wall mismatch with `reused_proof_identities` and an explicit
  omission recorded. A reused invocation is named (`phases.reused_invocations`) rather than
  composed as an invocation that published no phases.
- **The mode ladder `full` / `sample` / `none` exists, with the declared default.** The
  sample is deterministic and declared — `max(1, ceil(units/10))`, selected by
  `index % 10 == 0` — and every row in a non-`full` mode is `INCOMPLETE`, never `PASS`, with
  the mode published in the receipt and in the report header. An iteration run is not
  admission evidence. **Quick is the default for iteration** (`--lane smoke` or any explicit
  `--case`) and `full` for a whole-lane run, as #184 section 10.3 requires; `--verify` always
  wins over the default.
- **The prepared-master key is the product identity plus a declared fixture-recipe
  version** (owner ruling 3), now `fs-bench-fixture-recipe-v2`. The producer binary is
  recorded as provenance and published, never part of the key, so a measurement-plumbing-only
  harness change does not invalidate a master. A master sealed under a different key — or
  with no key at all — is superseded rather than consumed.
- **118 of 220 rows are acquired from a prepared master, and the registry declares which.**
  `registry::Preparation` is a column of the golden registry table — `-`, `object-set`,
  `base-store` or `input-tree` — so `runner.py` reads the acquisition decision off the
  binary's own registry and the hand-maintained `PHASE_SPLIT_FAMILIES` set is deleted. A
  family that gains or loses a phase split no longer needs a second edit in Python.
- **The artifact is a packed object set with a manifest and a seal.**
  `objects/pack.bin` plus `objects/index.tsv` replace one file per object; each object's
  role, direct references and bounded advisory predecessors are persisted, so a loaded
  artifact is the same object set the producer built. `runner.py` computes the per-file
  sha256 with `hashlib`, writes `manifest.json`, records the compatibility key and applies
  the seal (`chmod` minus `0o222`). A reused master is checked against its manifest by
  stat-identity — reuse is not an acquisition — and every object is still re-identified at
  load by `FinalizedObject::new`.
- **Expectations are persisted.** Every family that gates a read-back computes its
  expectation once, at acquisition, and every later phase reads it. Recomputing it inside
  the performance invocation cost the harness's scalar SHA-256 a second pass over every
  member — `dedup-cross-file-identical-500` spent 4.317 s of verification hashing 1 GiB to
  re-derive a digest it already had. The two families that gate no read-back at all no
  longer hash their members at all.
- **`c1.construct.*` is not prepared.** The construction *is* the measured operation, and
  preparing it would move the measurement into setup.
- **`c1.fs.build-scale` above the walk ceiling loads its fixture chain rather than
  replaying it.** The chain every batch reads its base from is built once at acquisition;
  its objects are the artifact's packed object set and its per-batch roots are the
  artifact's members. The measured chain is still compared against those roots, and the
  final root is additionally pinned by `tests/golden/expected.tsv`.

**Still not true.**

- **The per-row preparation ceiling is a formula, by owner direction on 2026-09-19**, and
  every row is inside it:

  ```text
  countable = preparation_wall_ns - acquisition_wall_ns
  ceiling   = 1.0 s + 2.0 ms per MiB of the artifact's declared data bytes
  ```

  The `1.0 s` is the fixed overhead the target always meant and the `2.0 ms/MiB` is the
  **measured** load floor — a prepared master is loaded by reading its packed object set and
  re-identifying every object, and `FinalizedObject::new` hashes, which the harness cannot
  skip without a product change. A row with no artifact keeps the plain 1.0 s. The axis is
  the **artifact's** bytes rather than the row's declared payload, because an entry-ladder
  row like `dedup-workspace-unique-500-compact-v2` declares 500 entries and no bytes while
  its master is 768 MB. **Zero of 217 rows are over, and the tightest is at 83%** — the two
  `c1.construct.*` 500 MiB rows, which are the only rows whose preparation is dominated by
  something other than a load.
- **The formula is host-specific at the top.** The accelerated SHA-256 is aarch64-only; on
  another architecture the scalar fallback runs at ~0.24 GB/s and those two rows would be
  back over the plain 1.0 s. The fallback is correct, it is just slower.
- **`prepare --lane full` is 143.5 s** (129.5 s of acquisition over 118 masters, 15.567 GB;
  an independent cold re-run measured 139.4 s and 127.9 s). **Amended by owner direction,
  2026-09-19:** the `<= 90 s` ceiling is replaced by *reported, and it pays for itself within
  two lane runs* — 129.5 s once against 98.2 s of lane preparation removed per run. Round 5
  met 75.39 s **by not doing the work** (six families had no master), and a ceiling on a
  once-per-digest acquisition creates that incentive. `runner.py prune` keeps 118 masters
  and reports what it keeps, so the cost is visible before it is paid.
- **A whole-lane quick run is not faster than a whole-lane full run.** **Amended by owner
  direction, 2026-09-19: the `<= 70 s` target is withdrawn.** It was derived from a ~106 s
  verification saving that no longer exists — lane verification is 32.4 s and only 0.8 s of
  it is skippable, because the deferred invocation exists for one family — and the rest is
  required by the frozen per-family oracles and the pinned-constant gates, where omitting it
  makes a row `INCOMPLETE` by the owner's own rule. The mode *default* the directive fixes is
  implemented: #184 section 10.3 says *"Quick is the default for iteration (`--lane smoke` and
  explicit `--case` runs); an admission run is `full` or declares itself otherwise and is
  ineligible"*, so a whole-lane run resolves to `full` and anything narrower to `sample`,
  with `--verify` winning. That is what makes iteration cheap — 0.6 s for the smoke lane,
  0.1 s for one case, against 0.06 s for `verify --reuse-pass`.
- **One instrument is deliberately unwired.** The 10 ms `RssSampler` cannot cover a phase
  under ~200 ms and a sampling thread inside the measured region perturbs what it measures,
  so it stays a tested primitive rather than a gate. The paragraph above says so.
- **A non-aarch64 cross-build could not be completed** — the target needs a C cross-compiler
  this host lacks — so the scalar fallback is verified at runtime through `Sha256::scalar()`
  rather than by a cross-build.
- **`elapsed_ns` still never gate-decides.** Owner ruling 1: the golden number reports and
  does not gate. Counters, heap, CPU, space and the per-row ceilings keep deciding.
