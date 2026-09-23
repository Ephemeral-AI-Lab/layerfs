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
Core's whole-file cutoff remains 128 KiB. Record any different reference
cutoff rather than silently modifying v0.1.6.

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
