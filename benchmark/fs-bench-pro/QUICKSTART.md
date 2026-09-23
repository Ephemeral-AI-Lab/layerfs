# LayerFS benchmark quick start

Use the family scripts or the shared runner. macOS owns the SDK/coordinator,
SQLite, canonical publication and spool. Linux Docker owns the daemon, real FUSE
and workloads. No Docker-owned SQLite, data-sharing mounts or fallback topology.
See [benchmark rules](../../docs/general/benchmark_rules.md) and
[the checkpoint contract](../../docs/roadmap/0.1/0.1.3/checkpoint-74-75.md).

## Build once per relevant change

From the repository root, with Docker Desktop running:

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --build-host
export LAYERFS_BENCH_IMAGE="$(python3 benchmark/fs-bench-pro/shared/runner.py --build-image)"
```

Reuse matching builds and protected prepared inputs. Source/product/image seals
are checked. Host builds default to one writable Cargo target at
`benchmark-results/host-store/builds/incremental-1.85.1-release`; Cargo decides
which dependencies need recompilation. Independent, read-only executable copies
and their producing identities live in `binary-archive/<sha256>/`. Publishing a
new executable replaces its inode, so an old hard link cannot alter a control.
Host builds default to at most eight jobs (also bounded by logical CPU count);
`CARGO_BUILD_JOBS=1..8` selects a lower or explicit limit. The actual selection is
recorded and included in native/dependency compatibility seals. Docker keeps its
separate two-job policy.
Each identity records build mode, Cargo command wall, compilation seal and the
packages actually recompiled. `LAYERFS_BUILD_ISOLATION=sealed` explicitly selects
the older isolated-target diagnostic path; it is not the development default.
A warm no-op is not evidence of fast recompilation: optimized benchmark codegen
can still exceed the preferred 10-second development budget (`BUILD_SLOW`).

Linux Cargo builds reuse one locked cache per pinned toolchain/architecture,
with Cargo's profile subdirectories. Source seals name executables, not another
dependency tree. Linux workload compilation has its own Docker layer containing workload and
included family Rust sources. Host-only family Python/shell files are excluded
from the Docker context; source labels still describe the full host harness.
Reusing this layer does not skip workload self-checks or executable archiving.
Host-only Python/shell changes need a new host identity, but can reuse an image
whose compilation seal still matches. The runner resolves mutable image tags
freshly, reuses inspection only by immutable image ID, creates each container
from that ID, and validates its actual image, mounts and resource limits.
Readiness and capability capture share one exec; both command-window cgroup
snapshots retain their original boundaries.
See the [#105 build-loop report](../../docs/roadmap/0.1/0.1.5/issue105/results.md)
for exact measurement boundaries, cache states, and remaining limits. The
measurement lock is **per worktree**: two runs in one worktree still never
overlap, and builds, performance and verification in *different* worktrees run
freely without excluding each other (owner direction, 2026-09-21 —
[build and measurement isolation](../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)).
A build may now overlap a timed phase in another worktree, so a row records the
competing work it observed instead of assuming a quiet host; see that document
before promoting any number.
The standard container has 2 CPUs, 2 GiB RAM, no swap and 256 PIDs. Host CPU and
memory remain separate resource scopes.

Keep the shared cache and the newest two legacy sealed targets. Preview and
apply only the runner-owned build selection under its measurement lock:

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --prune-builds 2
python3 benchmark/fs-bench-pro/shared/runner.py --prune-builds 2 --apply
```

The receipt lists candidates, actual removals and bytes. Unknown or symlinked
Cargo targets fail closed. This never selects fixtures, prepared inputs,
sample Stores, binary/image archives or source. Keep only the executable
snapshots referenced by the active comparison in its declared retention record;
archive retirement is a separate explicit selection. Do not repeat #117's
completed evidence/worktree cleanup or run `cargo clean` between iterations.

## Select one case

```bash
target/release/fs-benchmark-pro infra-list git_tool_workflow
bash benchmark/fs-bench-pro/families/git_tool_workflow/perf.sh \
  --case git-tool-100-mixed-v4 --seed 1 --setup clone \
  --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode \
  --product-timeout 300 --timeout 310 --setup-timeout 600
```

Fast performance means one complete sample, not less workload. The checkpoint
uses 300 seconds for the complete product workload and 310 seconds outer wall.
The normal selected runner defaults remain 120/130 seconds. Collection mode
reports historical latency targets separately from execution success.

Use `--repetition 1` for SDK edit families and `--seed 1` for other families.
Initialization uses `--setup fresh`; post-initialization cases normally use
`--setup clone`. Clone means a closed, validated, independent writable byte copy,
not an APFS clone and not a cold-OS-cache claim. Each run gets a fresh live owner.

Preparation is automatic on a cache miss. Run a family's `setup.sh` only when
explicit preparation is useful; do not repeat setup before every sample. Keep
master hashes/isolation, fixture versions and cleanup checks. Never reuse mutated
samples, clear protected caches routinely, or move cold product work into setup.
Use `--output` with a fresh path; raw receipts must not be overwritten.

## Namespace content: pseudorandom on/off

### Explicit Workspace sequences over namespace inputs

The shared runner can use an existing complete namespace input to initialize a
fresh Store, perform K public SDK edits and one Commit per tool workspace, and
record edit, Commit, their sum, and the full workspace chain separately:

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --family init_namespace \
  --case namespace-100000 --sequence 100 --sequence-commits 1 --seed 1 \
  --image "$LAYERFS_BENCH_IMAGE" --perf-fast --collection-mode --output NEW_OUTPUT
```

This is the separately identified `workspace-sequence-v1` operation; bootstrap
Init is setup and supplies no cold-Init acceptance result. `--sequence-reopen`
reopens before each workspace; `--sequence-active-cache` explicitly reads up to
100 targets through FUSE first. Plain Init without `--sequence` keeps its fixed
cold gate. Sequence verification uses `verify-selected.py` with the same flags
and exact source/input/image identities. It checks all changed bytes/lengths,
a declared unchanged-file sample, and initial/selected historical roots after
reopen; it does not claim exhaustive unchanged-namespace verification.

Verification keeps the ordinary 45-second work / 59-second complete limit for
sequences with at most 1000 total SDK edit calls. An explicitly selected sequence
with `--sequence COUNT` times `--sequence-commits COMMITS` greater than 1000 uses
the fixed `workspace-sequence-scaling-v1` policy: 600 seconds of work and a
614-second complete limit, including receipt publication. Selection authentication
still has 45 seconds, all deadlines start at invocation, and work reserves four
seconds for cleanup. The receipt binds the exact policy and sequence parameters.
These limits do not change ordinary families, historical access's 15-second
contract, or performance targets; performance `--timeout` / `--product-timeout`
flags do not set verification deadlines. Large proofs remain explicit selections,
and an over-budget or truncated sequence proof cannot pass.

For attribution only, a valid `LAYERFS_EDIT_DIAGNOSTIC_NONCE` produces bounded
per-edit diagnostic records and excludes the run from performance distributions.
Clear it for plain qualification and large-count cases. Default-budget spill
coverage remains pending until the deferred capacity repair makes it reachable.
Immutable input acquisition never automatically evicts other qualified inputs;
retire fixtures/prepared masters only through an explicit owner action.

Namespace Init defaults to the original pseudorandom content. Use an explicit
case with `--pseudorandom` or `--no-pseudorandom`:

```bash
bash benchmark/fs-bench-pro/families/init_namespace/perf.sh \
  --case namespace-100-compact-v3 --seed 1 --image "$LAYERFS_BENCH_IMAGE" \
  --perf-fast --pseudorandom
bash benchmark/fs-bench-pro/families/init_namespace/perf.sh \
  --case namespace-100-compact-v3 --seed 1 --image "$LAYERFS_BENCH_IMAGE" \
  --perf-fast --no-pseudorandom
```

Off produces deterministic structured text with the same paths, per-file sizes,
counts, metadata and total bytes. It is synthetic compressible text, not a real
repository. It resolves to a separate `<base-case>-text-v1` case with its own
fixture digest/cache/proof identity. The same option works for all four namespace
tiers. Text cases are opt-in and do not expand the default mandatory registry.
For verification, use that resolved case ID (or the base ID with
`--no-pseudorandom`) and the exact source/input/image identities from its performance
receipt. Do not mix modes or use text timings to satisfy the original random-data
cold gate. No product compression or size-based storage dispatch policy changes.

See [the content-toggle contract](../../docs/roadmap/0.1/0.1.5/issue111/content-toggle-contract.md).

### Cold-only namespace-100000 qualification

`init_namespace / namespace-100000` has a fixed **2.7-second cold-source**
qualification gate, including in `--collection-mode`. There is no warm-policy
option. Prepared fixture reuse still avoids regeneration; before each timed
sample the runner validates the immutable input, invalidates source data pages
on macOS and checks whole-input residency. Acquisition time is separate from
Init. This concerns OS source-data pages, not device caches or cold metadata.

If acquisition is unsupported, incomplete, stale or leaves resident pages, the
operation's raw timing remains in `perf.jsonl` with `status=INELIGIBLE` and a
`cold_acquisition` explanation. It is excluded from qualification medians and
pass counts, and the command returns nonzero. A cache label or high read count
alone cannot qualify. Declared repetitions continue without timing-based retries.
Nonce diagnostics and old receipts without the new evidence are ineligible.

Cold qualification v2 also requires verified fixture metadata: file mode `0640`,
directory mode `0750`, and mtime `1700000000000000000` ns for every entry,
including the payload root. The guard checks the complete directory inventory,
rejects extra empty directories and symlinks, and rechecks metadata during
acquisition. It never repairs the fixture. A byte-identical copy with changed
directory timestamps remains ineligible.

Receipts now include `cold_acquisition.metadata_validation`. Missing, partial or
incorrect metadata evidence cannot qualify, even with zero resident pages or a
fast timing. Historical v1 receipts remain unchanged under their original
protocol; they cannot be reused as v2 qualification evidence. See the
[metadata guard contract](../../docs/roadmap/0.1/0.1.5/issue111/cold-metadata-contract.md).

The campaign loader rechecks raw evidence instead of trusting saved PASS
summaries. Its paired cold report refuses mismatched fixture, seed, harness,
workload, environment, acquisition method or arm order. Independent verification
remains separate; a performance gate pass is not release admission. See the
[cold qualification contract](../../docs/roadmap/0.1/0.1.5/issue111/cold-qualification-contract.md).

## Verify separately

The current owner-directed loop is one focused code check, one target case, one
sample per registered case/arm, and seed `1` (or repetition `1` where required).
There is no n3 screen, averaging, or rerun to stabilize a passing number. The old
#118 n3 rule is historical and does not apply to new measurements; preserve its
receipts unchanged. A new sample is needed only for an affected case after a
relevant code/identity change, and it is run once at the final identity. Keep
unaffected successful cells and matching `--reuse-pass` proofs; do not repeat a
whole checkpoint after each edit.

During implementation, run the smallest test that exercises the changed path
immediately. Use a selected case rather than a family or full-lane campaign.
After the final relevant edit, run the affected independent proof once and the
required workspace checks once. Record whole command/build/setup/proof/cleanup
wall beside the declared product timer. Expensive boundaries and endurance runs
remain explicit selections.

Use the family's `verify.sh` with the exact case, seed/repetition, source, input,
image and setup identity from the performance receipt. SDK proofs also bind the
performance `row_id` using `--performance-rows`. Verification has a 45-second
work allowance and 59-second hard deadline. Prepare compatible inputs separately
when necessary; report preparation time outside verification.

The normal verifier uses the family's practical coverage: bounded SDK regions,
selected paths/ranges, or selected historical snapshots. Git keeps its full
semantic head/tree/parent and reopened-custody checks. Every receipt must state
actual coverage and omissions; sampled PASS is not exhaustive qualification.
The 600-second sustained proof is a separately accounted optional long test.
Existing bounded parallel-read/write and repeated-publication proofs cover the
routine sustained-operation smoke; do not relabel either as 600 seconds.

## Full checkpoint — one shared campaign

After repairs, freeze source and the registry. This single campaign serves both
#74 (passing suite) and #75 (performance tables):

```bash
python3 benchmark/fs-bench-pro/issue54_collect.py --checkpoint \
  --image "$LAYERFS_BENCH_IMAGE" \
  --output benchmark-results/host-store/campaigns/checkpoint-final
```

The collector reuses the shared runner, lists admitted families once, collects
one sample per active case and runs routine proofs. Obsolete capped-edit entries
outside the host runner are not additional active cases. Keep all failures and
requalify affected results after a fix; do not repeat successful cells for nicer
numbers. During implementation use selected cases, not repeated full campaigns.

Publish Markdown, JSON and CSV grouped by family and exact test ID. Report the
original timer, phases, setup/proof/cleanup wall, resources and coverage. Compare
only matching workload profiles/topologies/timers. Mixed-v3/v4 and history-v2
replaced older workloads; fewer files are not evidence of a product speedup.
Previous campaign reports retain their original source and coverage limitations.

### Bounded historical access (#101)

`historical_access` opens explicitly supplied retained history through public
SDK/FUSE. Build host and image separately with the commands above. It never
constructs history or builds prerequisites during a selected test.

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --family historical_access --list
python3 benchmark/fs-bench-pro/shared/runner.py --family historical_access \
  --case ha-small-head-v2 --store /absolute/path/to/sealed/store.sqlite \
  --image "$LAYERFS_BENCH_IMAGE" --output /absolute/path/to/new/performance
python3 benchmark/fs-bench-pro/shared/runner.py --family historical_access \
  --case ha-small-head-v2 --store /absolute/path/to/sealed/store.sqlite \
  --image "$LAYERFS_BENCH_IMAGE" --mode verification \
  --performance /absolute/path/to/new/performance/result.json \
  --output /absolute/path/to/new/verification
```

The v2 manifest pins the existing closed157-state schema9 Store and original
checkpoints1/57/65/157. Any different Store fails compatibility validation; it is
not silently rebuilt. See `families/historical_access/fixture.json` and the
[contract](../../docs/roadmap/0.1/0.1.5/issue101/historical-access-v2.md).
Each selected invocation has one15second deadline including preparation and
teardown. Verification has its own15second watchdog. `--all` explicitly runs
all eleven performance cases serially with separate envelopes; verification binds
one selected performance receipt. These are diagnostic qualifications, not
release admission or a paired product speedup campaign. #102 owns that campaign.

### Optional repository history (#102)

`--family repository_history --list` lists the three optional profiles.
Execution requires explicit `--profile stride-1`, `stride-3` or `stride-10`
(157,53,17 retained states). It delegates to the existing sealed DeepSeek importer:

```bash
python3 benchmark/fs-bench-pro/shared/runner.py --family repository_history \
  --profile stride-10 --image "$LAYERFS_BENCH_IMAGE" --output /absolute/new/history-run
python3 benchmark/fs-bench-pro/shared/runner.py --family repository_history \
  --profile stride-10 --image "$LAYERFS_BENCH_IMAGE" --storage-verify-run /absolute/new/history-run
```

The verifier uses that measured Store and its original state oracles. These long
profiles never run by default and are reported NOT_RUN_OPTIONAL unless selected.
Stride10 includes checkpoint157 and is17states; it is not the ten-state spread.
Git/control comparisons must use the identical selection; no Git17 result is
implied by registration. See the prospective #102 campaign contract.

### Promoted uncompacted campaign (#104)

Use `issue102_collect.py --family FAMILY` with the frozen
[issue104 declaration](../../docs/roadmap/0.1/0.1.5/issue104/mandatory-campaign.json).
The collector validates the complete registry before executing one family and its
proofs; `--resume` preserves terminal attempts. Retained failed-proof or
allocation-only recollection requires an explicit applicability document. Do not
invoke the legacy monolithic checkpoint command for this campaign.

The [terminal report](../../docs/roadmap/0.1/0.1.5/issue104/results.md) records all
18 families, 209 performance selections and 237 proofs, exact commands and source
checkpoints. The 500-transition unrelated-history case misses its 15-second target;
#104 remains open. The promoted schema10 product stays uncompacted, with the
separately bound uncompacted access fixture and independent copies. No optional
repository history or 600-second endurance proof was selected.
