# R1e-bootstrap exclusive native SQLite guard

> **Status: Independent implementation input; exits unrun.**
> Parent43c977c00e342d207757cd8bf5f6e2cd15041650,2026-10-01.
> C1-only R1d-run-seek remains the current primary checkpoint. This independent
> C2/native-startup slice uses non-overlapping ownership and coordinated gates.
> No strict memory/progress/physical/provider qualification follows from a plan.

The smallest complete delivery is the real native Server binary's exclusive
early SQLite bootstrap before host::run opens Store/history or starts workers.
It addresses SC-07/08 global engine authority; the existing R1e shape/protected/
physical and aggregate factory-lifetime problems remain separate gates.
[R1E-ENGINE-DESIGN](R1E-ENGINE-DESIGN.md) contains the supporting source audit.

## Exact API and authority

`engine/mod.rs` only declares/reexports; every unsafe signature/block/raw pointer
stays in Storage's audited `engine/ffi.rs`. Safe guard/profile/observation types
remain focused implementation files. Root owns the exact Core boundary allowlist
and external self-tests. Storage retains deny unsafe and deny unsafe-op-in-unsafe;
C1/telemetry/Server library retain forbid. The only separately audited Server
call is `src/bin/layerfs-server.rs` at genuine early main.

The frozen entry is unsafe `bootstrap_exclusive() ->
Result<&'static EngineGuard, &'static EngineBootstrapFailure>`. Its caller proves
exclusive process/library SQLite/global configuration authority and no concurrent
initialization/config/shutdown throughout bootstrap. A prior initialized provider
is defined eligibility refusal, not undefined behavior; no reset/shutdown/retry
is allowed. A OnceLock serializes only the owned invocation and caches one actual
success or original failure; it cannot prove absence of foreign initialization.

Order is MEMSTATUS config1 -> require acknowledgement -> explicit initialize ->
query prior hard limit -> set33554432 -> exact readback -> bounded actual native
tracking probe. Even hard_heap_limit(-1) initializes this provider, so no initial
query may precede config. Probe request8192 is fixed; actual msize/current-byte/
allocation-count deltas and free return are checked. One RAII free attempt runs
on every owned error path. No fake allocator, altered library or bundled fallback.

EngineGuard is unforgeable, has no public constructor/Clone, and gives profile,
bootstrap observation, validate and quarantine observation. The established
witness is private to the FFI boundary. Native main holds its static guard through
host::run. Exact provider version/source, previous limit, tracking before/during/
after and actual allocation size are owned observations. Current/lifetime
high-water bytes/counts are distinct; lifetime totals never become phase peaks.

`validate() -> StorageResult<EngineObservation>` checks the same33554432 hard
limit and native counters. A changed limit/status failure quarantines the guard
terminally without repair. Failure records stage, original typed cause, whether
bootstrap initialized/installed the limit and whether the probe-free attempt ran.
Taking/reporting a failure never reattempts global configuration or adopts an
already initialized provider.

## Owning checks and limits

Fresh real processes must establish supported config/tracking/free, deliberate
already-initialized/query-before-config refusal, actual32MiB enforcement/NOMEM,
exact known/Unknown provider cleanup and real early native binary use. Reuse
unaffected checks; root runs scoped locked Cargo only after coherent freeze, with
sequential worktree target ownership and repository-root ARMv8 flags. External
guard self-tests already pass11 after exact allowlist/denial cases; product/proof
exits are unrun until the append-only log records them.

Heap enforcement does not establish healthy largest SQL operations, protected
C5 progress, first-party176MiB allocation, physical256MiB Server/RSS/cache or
aggregate factory lifetime/drain. StrictServerMemory bit remains disabled. No
benchmark family/runner/campaign or #288 issue update is authorized by this slice.
Larger guard/profile/capacity or provider changes cannot turn a failure into PASS.

Ownership: r0_oracles engine module/Storage lib declaration/external engine
tests; root normative guard/self-tests, Core AGENTS, actual binary caller,
architecture/docs/acceptance/LOC/commit/publication. Other workers preserve those
edits; C1 reference-run files remain r1_catalog-owned. Root integrates actual
ready slices as separate source-pinned checkpoints without unstaged work in LOC.


## Observed Darwin capability boundary and custody refinement

The original frozen six-primary gate kept2 refusal PASS and4 eligible FAIL at
actual observed hardlimit0. Raw evidence02 remains unchanged. One labelled
fixed8192 native cause probe04 establishes tracked bytes0→8192→0/count0→1→0,
msize8192/free once on the same Apple3.51.0 source; it does not establish the
required33554432 limit. First64 function bytes do not prove a stub or macro.
The exact required readback is unavailable on this selected provider. No new
EngineGuard can be issued, no Store/scratch/native probe body is reached, and
native main refuses before host configuration or Store/history effects. Direct
legacy logical library composition remains its old separately scoped profile.

The custody refinement records hard_limit_set_attempted before the one setter;
hard_limit_installed becomes true only after exact readback. Nonexact required
readback is explicit UnsupportedPolicy with original stage/actual value cached.
This changes no limit/route/provider, and cannot relabel old FAILs. A distinct
fresh-process Darwin unavailable-provider proof and actual native early refusal
are selected; supported native/SQL/Store/scratch/Linux bodies remain separate
unrun gates. Silent Linux image acquisition was cancelled as unavailable;
no Linux/physical/benchmark proof ran. This checkpoint is capability-limited,
with strict/concurrent enablement disabled and independent C1 work continuing.
