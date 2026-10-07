# Focused pre-S8 resource growth and retention

> **Status:** Prospective implementation/diagnostic plan at `1fd566b74`.
> Owner2026-10-07 authorized the proposed workload-growth and repeated-operation
> checks with “proceed”. Numerical latency/RSS acceptance remains deferred.

The question is whether growing namespaces, streamed content and repeated
operations introduce growing resident ownership or prevent release. Exact
continuous peaks, every SQLite-internal copy and exclusive device attribution
are outside this focused selection. Unsupported observations remain explicit.

## Deepest-file plan

- Reuse the sealed100000-name F11 preparation, explicit128KiB clone helper,
  direct Store/Owner/BoundWorkspace ports, Content streaming constructor and
  fixed-window full-byte oracle. Add external `resource_prepare.rs` for matching
 1000/10000-name sealed fixtures through the real SDK Init/seal path. Preparation
  is separate, bounded by100s, and never counted as a Commit measurement.
- Add external `resource_growth.rs` and focused `resource_growth/{allocation,
  operations,observe}.rs`. Its normal product calls are unmodified. An external
  System allocator wrapper records actual live Rust requested bytes and counts;
  it excludes C allocations, allocator slack, stacks, mappings and kernel cache.
  It changes no product allocator/API and adds no dependency. Record instrumentation
  overhead; report sampled process/cgroup memory separately. No lifetime peak is
  substituted for a phase peak.
- Reuse the old Store mutation/construction helper through parameterized external
  functions for a unique first-byte change. Keep existing callers equivalent.
  Every repeated Commit represents the explicit local mutation; S10 normalization
  is not introduced. A separate Save case streams Content through a direct Store
  producer and verifies all bytes through its immutable read ports, without
  claiming that a standalone file Save is a Workspace Commit.
- Extend existing registration and observer code with a separate growth registry,
  exact argument/phase inventories and supported resource sampling. Existing
  registries and historical receipts remain immutable. Add independent retained-row
  validation and focused tamper/omission tests. No generic preflight runner.
- Record a source-derived inventory of fixed/read/construction/producer/credit
  owners and release lifetimes, including SQLite allowances and unavailable
  native-allocation details. Configured limits never substitute for observed RSS.

## Frozen selected cases and execution

Each case runs once per platform, in table order, with a fresh writable byte-copy
clone and fresh daemon process. No baseline comparison, cache conditioning or
speed claim. Cache starts at normal startup and evolves naturally; cache filling,
allocator retention and OS backing cache must be reported rather than hidden.

| Case | Namespace files | Operation |
| --- | ---: | --- |
| growth-n1000 |1000|One matching byte mutation and Store-half Commit, verification and terminal release|
| growth-n10000 |10000|Same one-byte operation|
| growth-n100000 |100000|Same one-byte operation|
| growth-save4m |1000|One4MiB streamed Save, full byte oracle and producer release|
| growth-save32m |1000|One32MiB streamed Save, same fixed windows|
| growth-save256m |1000|One256MiB streamed Save, same fixed windows|
| growth-repeat64 |100000|64 distinct one-byte Commits in one bound Workspace/daemon; preserve every cycle including initial cache/allocator growth; terminal cleanup|

Release binaries are built with locked Rust1.85.1/repository ARM64 inputs and
construction_workers=1 before any execution. New diagnostics use the existing15s
complete command budget,100s outer stop and a<10s independent retained-evidence
verifier. No timeout/workload reduction after a failed selection. Store files
are native temporary files, never `/work`; the original sealed preparation is
read as bytes only. Disposable only. Serialize every build/test/diagnostic.

Hard functional/count checks: correct roots/bytes and distinct publication,
one attempt, Save reservation/publication/history arithmetic, fixed4 read handles,
cache<=8MiB, credits/outstanding0 after released operations, no receipt overrun,
automatic terminal namespace cleanup and successful owner exit. Preserve every
original outcome before evaluating observations. Growth judgments use the actual
live-owner inventory and increasing cases; no fabricated RSS tolerance or
post-hoc numerical PASS. Any unexplained accumulation gets a source diagnosis
and remains unresolved until corrected/proven. Global history/file allocation
is expected to persist and is reported separately from live memory/local debt.

No new product change is planned. If a product defect is found, record its own
deepest-file correction/proof before rerunning affected work. Local commits use
the pinned production counter with all migration subtotals; protected notes,
root reference, prior receipts and the four protected containers stay untouched.

Iteration03 retains an external-example Clippy failure: explicitly dropping a
borrow-only tracking adapter has no destructor effect. Its last use already ends
the borrow; remove that redundant call without changing the product route.
Source inventory04 predates this one-line cleanup; the final build inventory
supersedes it. No diagnostic selection ran before the correction.

## Source-derived owner inventory

The variables are fixed read handlesR=4, simultaneous admitted producersP (one
in these cases), active read demandsA, and the configured daemon job creditQ.
None is the total file byte countB, namespace populationN or prior Commit countH.
This is an ownership/complexity bound, not a fabricated whole-process byte limit.

| Owner and source | Bound and lifetime |
| --- | --- |
| Shared CanonicalCache (`workspace/src/base/cache.rs`) |8MiB logical allowance, canonical bytes+256 per retained object; two BTree maps; pressure evicts. Actual allocator overhead is observed separately. |
| Each Storage handle (`storage/src/store/handle.rs`, `read/fetch.rs`) |704KiB candidate index;8192 signature rows when loaded; locator, absent, descriptor, value-group and missing-ordinal windows bounded by4096 entries. FixedR readers; each independent producer drops after its operation. |
| Each Save (`storage/src/save/state.rs`, `store/policy.rs`) |16MiB encode arena +1MiB decode arena; two2MiB encoded caches,512KiB decoded groups and512KiB pooled values; pending<=512 objects and4194303 canonical bytes; bounded pack/group construction, metadata candidate index<=131072 entries. No whole-file/whole-namespace Vec. |
| Codec (`storage/src/encoding/codec.rs`) |Static Zstd contexts live inside Rust-owned aligned arenas; no growing codec context allocation. Arena bytes are included by the external Rust allocator counter. |
| Each read wave (`storage/src/read/objects.rs`) |1MiB decode arena, fixed encoded/decoded/pool caches;<=4096 IDs and32MiB canonical output. Multiplicity follows active bounded demands, not total object population. |
| Metadata candidate tree (`storage/src/encoding/pool/index.rs`) |131072 entries, logical charge24B/entry=3145728B on this64-bit build; BTree nodes/slack additional and allocator-observed. It resets at capacity; this charge is not an RSS limit. |
| Owner (`daemon/src/overlay/{owner,queue}.rs`) |8MiB job credit with64KiB lifecycle reserve;16 namespaces,16+2 jobs each; fixed lane queues and separately counted scheduler allocation. Original completions release their credits. |
| SQLite connections |One Store writer+R read sessions, each cache_size=-2048; one overlay owner with2048KiB pager suggestion. These are allowances, not hard malloc/RSS bounds. WAL, rollback/dirty pages, prepared statements and C allocations retain separate scope. |
| Global history/OS file cache |Published Store data/history persists by design. OS file cache and dirty writeback may grow with touched backing and are reported as file/kernel memory, never hidden by a bounded Rust heap claim. No GC or forced cache eviction is introduced. |

For a fixedP/A/R/Q, explicit Rust payload/index windows remain bounded independently
ofB/N/H, with canonical tree depth separately paid. The allocator observations
cover all live Rust requested bytes including external driver state; they exclude
malloc slack, stacks and SQLite's C allocations. Process RSS and Linux cgroup
anon/file/kernel observations check those additional dimensions diagnostically.
Only the source-backed windows can receive structural PASS; cross-case readings
and cycle trajectories must be reported before any empirical growth conclusion.
No sum of overlapping counters is a physical resident bound.

## Build/preparation checkpoint

Host growth05 and Linux07 release executables build. The host-only preparation
helper08 matches the retained wide fixture's file mode0640 and fixed mtime before
any selection.09/10 prepare and seal1000/10000 files, remove their native sources
and retain491520B/3166208B Stores.12 pins these and the unchanged100000-name
preparation. These are fixture setup receipts, not new Init measurements.
16 pins final sources and the distinct growth/preparation build scopes.
Tooling tests02 pass24 bodies; final daemon all-target Clippy11, formatting13,
706-file boundary14 and46 guard tests15 pass. The old b'X' external helper calls
remain equivalent wrappers around the new tagged test-input functions.
Actual growth selections are NOT_RUN until registered at the committed identity.
Linux diagnostic containers will be retained stopped until independent validation;
failed/unknown copies remain available rather than being implicitly removed.
