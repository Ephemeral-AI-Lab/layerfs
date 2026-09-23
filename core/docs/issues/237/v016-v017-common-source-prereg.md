# #237: v0.1.6 versus Core 10k common-source comparison

> **Status:** Prospective research diagnostic. This comparison is not a release
> gate or a claim that the two public APIs are identical. No public arm had run
> when this protocol was committed.

## Question and fixed inputs

Measure where the authentic v0.1.6 directory Init and the current Core native
Init spend time when each imports a fresh, independent byte copy of the **same**
seed-1 source directory: 10,000 files, 100 data directories, 300,000,000 total
logical bytes **including** the 100-MB anchor. The shared prepared-master
manifest SHA-256 is
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
The closed master may be reused only for untimed preparation. Each arm gets a
new writable byte copy and a fresh Store. Pack payload remains inside SQLite
BLOBs in **both** products, and both databases must report `page_size=4096`.
Keep each product's actual format, worker topology and admission algorithm;
Core's whole-file cutoff remains 128 KiB. The native v0.1.6 small-content
cutoff is also `SMALL_LIMIT=131072` bytes; its separate `WHOLE_LIMIT=2 MiB`
is a physical whole-file-owner maximum, not that cutoff.

The reference product is the peeled `v0.1.6` release commit `44cf74848`;
the annotated tag object is `dbdf0fed`. Current root `crates/*` production
is byte-identical to that release; the isolated exact-tag build is the arm.
The Core arm is this research branch's integrated C1 direct builder plus
bounded C2 grouping, using 512-object / 4-MiB preparation waves. The
experimental seven-commit policy and external-segment Store are excluded.
Before either run, append exact source commit, instrumented product seal,
benchmark/harness seal, binary hashes, fixture-copy identity and output path
to this protocol; retain them in the receipts.

## Public boundaries and cache treatment

Reference: invoke the real
`Client::initialize_layerstack(..., LayerStackInitialization::Directory(path))`
once. Its timer starts immediately before that call and ends immediately after
its acknowledgement; fresh Store/client setup and teardown stay outside.
The benchmark-only `namespace-init-diagnostic-core-fixture` route reports the
external fixture profile truthfully and emits aggregate diagnostic counters.
It signals **READY** after Store/client setup, then waits for **GO**. At READY,
the host wrapper hashes and invalidates the independent source copy, checks
whole-input payload residency without faulting it, and sends GO only if all
pages are nonresident. The Rust timer starts immediately after GO. No source
payload access is allowed between READY and that timer.

Core: invoke the real daemon `ImportNativeDirectory` request through its
`StackCreated` acknowledgement once via the H3 research driver,
`--independent-source-copy --fixed-operation-identity`, without `--verify`.
The H3 driver hashes/invalidates the independent copy and rechecks every
source payload page immediately before the public timer. Both arms require
**0/27,503 resident source payload pages** at the final check and record the
check-to-timer gap. The methods and exact backend versions must be recorded.
Directory and inode metadata residency cannot currently be qualified on this
host; both rows remain exploratory/`INELIGIBLE` for a fully cold admission
claim. No previous arm's source pages or Store can serve the timed operation.
No OS-specific cache command or external pack segment is a product change.

Use one release-profile public run per arm, reference first, Core second,
with fresh output paths, no best-of/retry and no verification inside the
performance call. Keep a failed, incomplete, or cache-ineligible receipt.
Verification/readback may run once afterward on each retained Store, outside
the public time. The old 0.578245-s/518.8-MB/s historical row stays archival:
its source cache was uncontrolled and its source bytes differed from this
common fixture.

## Aligned metrics and interpretation

Report a table with one column per arm and a comparability note for each row:

| Domain | Required observations |
| --- | --- |
| Public operation | Monotonic call wall, 300,000,000 B / wall in decimal MB/s, complete command wall and status. |
| Source and construction | Scan/frontier time, file-open/read call and byte counts where available, worker construction and blocked-send sums, source residency and device-read evidence. |
| Handoff and admission | Object/slab/message counts, producer and consumer wait, admission/owner wall, new/reused objects, pack create/append or full-rewrite counts and bytes. |
| SQLite | Exact BEGIN/COMMIT counts and wall, statement counts by kind, locator rows per INSERT, content lookup/collision calls, page/cache/spill policy and actual-owner readings where available. |
| Namespace/publication | Reference final root/LayerStack construction and Core C1 namespace/prerequisite/tree Saves plus C5 publication; name stages that do not map one-to-one. |
| Resources and output | Service/process user/system CPU, sampled RSS with coverage, SQLite content/History file and total DB apparent/allocated bytes, pack capacity/used/slack, independent semantic readback status. |

Stage timers may overlap. Worker sums, blocked sends and nested SQL/pack
timers must **not** be added to the public wall or treated as disjoint unless
their scopes prove it. Product object IDs and filesystem roots may differ
because canonical formats and public routes differ; each arm's semantic
readback is against the **same source manifest**, not an equality requirement
between the two root IDs. A CLI/client call and a daemon request are different
surfaces; transport, setup and teardown differences remain visible rather
than being silently normalized away. The comparison can identify bottlenecks
and generate a prospective treatment; it cannot alone certify a version
speedup under a fully cold namespace contract.

After both closed Stores exist, run read-only, version-specific
`EXPLAIN QUERY PLAN` and selected bytecode `EXPLAIN` for actual hot object,
pack, publication and multi-row locator statements. Pin each SQL text,
schema/index inventory, SQLite library version, Store hash and table counts.
Plans diagnose search shape; opcode counts and estimates are **not** measured
execution times. Compare old single Store footprint with Core's content Store
**plus its separate History catalog**, while also reporting content pack
geometry separately. SQL/plan findings belong in a companion report with
source links and raw outputs.

## Frozen arm identities before the first public run

The reference arm is the **unmodified product** from peeled tag commit
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, with benchmark-only
READY/GO and aggregate trace code in clean isolated worktree
`/Users/yifanxu/.codex/worktrees/issue237-v016-comparison/layerfs` at
`b0730c70a567f5019ba0435d73bfa9f6a04b4004`. Its `crates/*` tree SHA is
`dcc4fb6fd01115dcbf91ba02df414e91eb5733be`, equal to the release tag.
The release `fs-benchmark-pro` executable SHA-256 is
`604bc5c7b3577fdc78acc531353600d28077bb44d668fee90efe28d51e88bffe`.
The one-run wrapper SHA-256 is
`5789a47ef189a465aede5b06b9f20c5f4c685fd32dece2bed4f317c07ab2f604`;
its Rust benchmark main/infra SHA-256 are
`0cea9fff3615ff1c66b5ebafc189a336ad6f2d3c0c0babb8f42b32bc55f9dfbc`
and `4a1145887569dcf26285b12fb8c812605bbcb178587fcfe46e828dfa1d11f9fd`.
The fixed benchmark seed is
`9a5998a338a9fc0f35a2c333cad16d550944149f2c8edc8b1d5f897da335edaa`
and diagnostic nonce `9a5998a338a9fc0f`. Its fresh, absent output is
`benchmark-results/issue237-v016-v017-head2head-a` inside that worktree;
the Store is under its `store/` child. The exact wrapper invocation is:

```sh
python3 benchmark/fs-bench-pro/issue237_v016_reference.py \
  --master /Users/yifanxu/.codex/worktrees/2776/layerfs/benchmark-results/fs-bench-pro/prepared/namespace-10000-9f0c701648528472 \
  --cold-driver /Users/yifanxu/.codex/worktrees/2776/layerfs/docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --binary target/release/fs-benchmark-pro \
  --out benchmark-results/issue237-v016-v017-head2head-a
```

The Core arm is the integrated research tree at source commit
`7f2124ba07e0c2b77a614f3429d454b2fdd57b08` in isolated worktree
`/Users/yifanxu/.codex/worktrees/issue237-v017-micro/layerfs`. Its only
dirty paths are three temporary aggregate Service diagnostics:
`operation/history.rs`, `history_bootstrap.rs`, and `import_native.rs`.
The instrumented product seal is
`d9b17e338d61a33a8f907d07e2758b7ac83c9e246212dbef200b59f5a0472d74`,
the harness seal
`6d9a3e2eb2a1eea1f3f0df15949d6f3bab103657a824b0a04d34482eff8`,
and the archived instrumentation diff SHA-256
`5e3f2cd7560b53d9b84feef572e0596ede40eb17f76b218d39ccbb6091cbe434`.
The release Service/daemon binary SHA-256 values are
`c4e22ff40f8f363b4fa305be323ddc10f22ed164d5b1e36f3ed0dab4b3e6d131`
and `1eb2973d41dd79cdb1d6663c0577fb760373da945ed3c519f2cc255225058a55`.
Its fresh, absent output is
`benchmark-results/fs-bench-pro/issue237-v017-head2head-a` inside that
worktree. The exact public diagnostic invocation is:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --case namespace-10000 --independent-source-copy \
  --fixed-operation-identity \
  --out benchmark-results/fs-bench-pro/issue237-v017-head2head-a
```

The H3 driver SHA-256 is
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`;
both arms use its independent byte copier and the same cold backend (SHA-256
prefix `fef5391e`, full hash in raw receipts). The Core worktree's prepared
master path is an untimed symlink to the closed root master; H3 dereferences
it into a distinct writable byte copy before timing. Neither arm reads the
master in its public operation. Both output paths were absent at this
protocol amendment. Reference SQL statement-kind counters are traced exactly;
Core reports exact Save COMMIT/object-INSERT/presence/pack counts, while its
successful BEGIN count and two History-catalog writes are **source-derived**.
Other Core SQL statement-kind counts are `NOT_MEASURED`, not estimated from
EXPLAIN. Worker sums and stage children remain overlapping where noted.
