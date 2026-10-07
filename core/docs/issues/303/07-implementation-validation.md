# 07 — Implementation and validation plan

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. This is a plan: no code was written and no build, test or
> benchmark was run. "Measured" below means counted by the repository's LOC
> counter on that commit. Every "after" size is an estimate and is labelled as
> one. Claim labels are defined in the [entry point](README.md#claim-labels).

Execution progress is tracked in
[implementation issue #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Use the [handoff prompt](09-implementation-handoff.md) for continuous iteration,
checkpoint commits and milestone evidence updates. This plan owns dependencies
and exit criteria; tracker preparation does not satisfy them.

Review revision 2026-10-05: supersedes the algorithms and bounds of design
`334fc743751b9a181e670d0601a24fb3169208f9` where identified below. Product
source remains pinned to `f96d97651`; no implementation or new measurement
accompanies this revision. Required corrections and proof obligations are
tracked in [README](README.md#required-corrections-before-implementation).

Implementation sequencing revision 2026-10-05: includes the mandatory optimization
EXPLAIN/profile gate, P13/P14, cached mmap/reply-frontier prerequisites, persistent
multi-call incremental Commit, and post-qualification root-v0.1.6 retirement.
Product source and retained numeric evidence are unchanged.

Layout/size planning revision 2026-10-05: specifies the default SDK runtime-adapter
placement and a new responsibility-based LOC range in §4.3–4.4. The range includes
the expanded ownership, profiling, FUSE and runtime scope; it is an estimate, not
implementation evidence or a code-size acceptance gate. A fresh read-only count
confirmed the unchanged production baseline below.

Owner update 2026-10-05: one local overlay SQLite database per daemon, initialized
once before readiness; Workspace rows are namespaced within it. Bash Exec has
no automatic runtime timeout. This supersedes the per-Workspace-file proposal;
shared writer/pager/failure accounting and fair admission apply below.

## 1. Starting point

[implemented and source-verified]

- `main` at `f96d97651be5299f153ccde2bc8d921dd58807ad`.
- The cluster two crates (`layerfs-workspace`, `-fuse`, `-daemon`, `-bridge`,
  `-sandbox`, both `layerfs-api` packages) and `layerfs-server` are in `exclude`
  in `core/Cargo.toml:19-28`. They are byte-identical to `7edddbdb8`, have no lock
  entries, and are not built or tested by the prescribed
  `--manifest-path core/Cargo.toml` commands.
- `layerfs-server` and the SDK import `layerfs_storage::Store` and
  `layerfs_history::sqlite`, which cluster one no longer exports. This comes
  from reading imports and exports; nothing was compiled.
- Production LOC, `python3 -B tools/production_loc.py --detail` (counter
  SHA-256 `c0fe7f36…624adb`): core 74,000 in 449 files; reference 65,417;
  combined 139,417.

| Crate | Production LOC (measured) |
| --- | ---: |
| `layerfs-workspace` | 26,835 |
| `layerfs-bridge` | 6,834 |
| `layerfs-server` | 3,996 |
| `layerfs-daemon` | 2,151 |
| `layerfs-fuse` | 1,447 |
| `layerfs-sandbox` | 1,106 |
| `layerfs-api` (two packages) | 796 |
| **Cluster two scope** | **43,165** |

## 2. Build structure

[proposed design; current implementation sequence]

Both workstreams implement in `core/`. Root `crates/` is the v0.1.6 reference
until cluster two completes; its retirement is S13, after integrated qualification.
No new core component depends on legacy source or revives `layerfs-server`.

- Add rewritten/new members only when they have real product implementation and
  build coverage. Directory presence and active-cluster-one tests do not qualify
  excluded Workspace/FUSE/daemon/API/bridge/sandbox source.
- `layerfs-overlay` owns SQLite rows/indexes/payload/operation records/ownership and real
  profiling. Workspace owns filesystem semantics and stable composition, without
  SQL, kernel protocol or physical-pack knowledge.
- Portable content, overlay and semantic Workspace code must build on the owning
  hosts. Confirm locked Linux content/bundled SQLite and host feature isolation.
  Reuse existing published dependencies, preserve ARM64 build inputs and report
  an unsupported capability; do not patch/fork or choose a fallback.
- Host application composition embeds current Handles/Storage/HistoryCatalog
  and runtime-owned object/Save/history adapters. The default placement is
  `layerfs-api/sdk/src/runtime/`, embedding the current libraries rather than
  reviving a server/coordinator package. Global persistence is the supported
  host-local Store; the daemon overlay is separate. Confirm public constructor
  and Save lifetimes in S0; proposed paths do not prove a sound implementation.
- Replace dormant Workspace code at its final path. A temporary excluded
  `layerfs-workspace-legacy` relocation may preserve migration/reference access,
  but is not a dependency or lasting second implementation. Confirm source
  classification/LOC from exact staged trees when moving or retiring it.
- Retain useful FUSE semantic checks, stable identity and mounted test contracts.
  Rewrite service, cache/mmap compatibility and reference disposal around the new
  engine; do not rename legacy APIs to hide incompatible contracts.

```text
S0: source/API/build/lifetime decisions + adversarial cost contracts
 |
 +--> engine lane: S1 -> S2 -> S4 -> S5 -> S6 -> S7 -> S8 --+
 |                         ^                              |
 +--> base/runtime lane: S3 + S9 -------------------------+--> S10 Commit
 |                                                        |       |
 +--> cluster-one lane: backed construction/validation, ---+       v
      incremental topology, holes, faithful import             S11 cleanup
                                                                  |
                                                               S12 qualify
                                                                  |
                                                               S13 retire
```

The lanes describe dependencies, not automatic delegation or extra construction
workers. Runtime/cluster-one integration can advance alongside the engine.
Actual Save lifetime/interleaving, stable ownership and correct large-input
contracts must be proved before integrated Commit is accepted. Algorithms whose
bounds are still unresolved are implementation gates, not permission to raise
limits or defer their cost to an unbounded background job.

### 2.1 Checkout and execution environment

[owner execution direction, 2026-10-05; environment availability checked]

Implement in the existing primary repository checkout on **local `main`**, under
`core/`. No separate implementation branch/worktree is planned. This session
fast-forwarded local main from `f96d97651` to the existing documentation commit
`334fc7437` and switched the checkout without changing product source or losing
uncommitted documentation. This is local branch preparation, not a push or a new
implementation commit. Retained evidence/source pins above keep their original
identities.

| Layer | Development / execution environment | Ownership |
| --- | --- | --- |
| Authoring and host checks | Existing macOS ARM64 checkout; installed Rust 1.85.1; locked core manifest | Cluster-one provider, SDK runtime composition, portable library checks and product-boundary tooling |
| Sandbox integration | Running Linux ARM64 Docker backend; checked client/server version 29.5.2 | Actual daemon, SQLite overlay, Linux FUSE mounts and ordinary Bash execution |
| Host runtime | macOS process embedding current Handles/Storage/HistoryCatalog | Selected global Store profile; authenticated object/Save/history adapters, initialized once |
| Sandbox runtime | Linux sandbox with a daemon initialized before Workspace readiness | One overlay database per daemon; multiple namespaced Workspaces and independent process/stream custody |
| Build outputs | Worktree-owned `core/target/`; distinct owned Linux target such as `core/target/cluster2-linux/` | Reuse matching build seals; no target/cache borrowed from another worktree |

Docker availability does not prove mounted FUSE readiness or the locked Linux
toolchain. S0 still establishes the real image/build configuration, `/dev/fuse`
access, required capabilities, bundled SQLite support and native mount/teardown
behavior. Actual target-dependent checks run on the owning platform; a macOS
test pass cannot cover excluded Linux adapters. The root `.cargo/config.toml`
ARMv8 AEAD inputs must reach Linux builds too; an explicit RUSTFLAGS/image build
must preserve them. Do not patch dependencies or select a fallback to hide an
unsupported required capability.

Reuse initialized host runtime and sandbox daemon across Workspace opens.
Workspace-per-tool-call does not imply a new container, provider or database per
call. Each logical open binds the complete immutable base and namespaced local
state; native FUSE attach cost remains part of the actual operation. A persistent
Workspace can serve multiple calls and incremental Commits in the same environment.

Local main is the single integration line. Coordinate file ownership for any
explicitly authorized parallel work and account for combined changes before each
commit. Preserve unrelated work; source moves, dependency changes and retirement
remain explicit. Freeze/commit the required source and build identities before
qualification measurements, obey the per-worktree measurement lock, and record
any build interference under the existing measurement contract. Working on main
does not authorize a remote push or change any proof/cache/budget rule.

## 3. Slices

[proposed design] Each slice delivers real behavior, its focused correctness
proof and source/work evidence. Follow [core verification](../../../AGENTS.md#checks-and-completion)
and the [optimization guide](../../../../docs/general/optimization-guide.md);
use exact per-commit production LOC and architecture updates. No runtime
benchmark or new code was produced to revise this plan.

SQL EXPLAIN and correlated runtime database profiling start in S1 and accompany
each changed hot path. S7 consolidates evidence; it is not the first point at which
the engine is instrumented. Complexity analysis rejects quadratic/amplified work
at operation, command and repeated-Workspace-lifetime scales.

| Slice | Deliverable | Dependencies | Exit evidence |
| --- | --- | --- | --- |
| **S0 Contracts and build risks** | Derive R1–R8 algorithms/ownership/service bounds; assign P1–P14, including backed validation/touched/release state and incremental topology. Confirm Linux content/SQLite/transport capabilities and actual Save lifetimes; select concrete payload/pressure and command-identity semantics | Current handbooks/design/source | No withdrawn extent/fold/pin rule used. Adversarial work/custody contracts and real public API/build paths established; unresolved algorithms identified explicitly |
| **S1 Shared overlay engine** | One daemon database initialized once; schema/version/settings, namespaced metadata/payload/operation records/ownership, prepared SQL, indexed access, real EXPLAIN/profile observations and atomic errors | S0 | Settings read back; binary payload and namespace routing correct; plan/runtime profile for first queries; no per-Workspace DB or custom mutable tree/graph engine |
| **S2 Generations and fair service** | Incarnation routing, short shared-owner jobs, active/captured membership and fixed EOF, publication/reply-attempt frontier, install/retire; bounded queue credits and lifecycle progress | S1 | Tiny capture during growing active namespace visits only its domain; no bulk copy; unrelated service progresses; failure retains exact state |
| **S3 Immutable base access** | Current content APIs for root/child/inode/list/readlink/range; exact immutable cache keys, EOF/attributes, base-overlay merge and retained read roots | S0; usable S1 interfaces | Real content-built roots behave correctly, including symlinks/hard links and retained roots across install; no mount scan/copy or special empty-base branch |
| **S4 Namespace semantics** | lookup/getattr/create/mkdir/symlink/link/unlink/rmdir/rename/readdir/chmod/utimens; serial ranges, byte-name ordering, parent/reference ownership and bounded cursors | S2, S3 | Atomic ordinary effects, correct aliases/rename/resume and permissions; SQL plans/profiles support visited-work bounds; no resident name/handle cap |
| **S5 Payload and stream semantics** | Selected bounded cells/tails/validity, append/overwrite, inherited reads, cutoff truncate/regrow and hole handling; explicit representation transitions | S4; R1 and content hole contracts | Dense fragmentation does not enlarge one request's work; no base WRITE copy-up; READ gap demands bounded; tiny-file/page/journal/copy amplification reported |
| **S6 Lifetimes and reclamation** | Independent orphan custody, bounded success/failure composition, reservations/headroom, exact reader/capture/operation owners and weighted automatic reclaim | S5; R4/R6/R8 | Repeated log/orphan/Commit failures do not grow read depth or require payload-sized busy folding. Reclaim progresses while live/idle; stale deletion cannot remove new data |
| **S7 Engine cost gate** | Consolidate per-operation EXPLAIN/profile, request/statement/VM/row/page/byte/copy and queue/debt evidence; select physical layout through declared tradeoffs | S1–S6 | Derived worst-case/amortized/cumulative work demonstrated or rejected. No quadratic mechanism, hidden scan, input-sized resident set or raised cap accepted from a small timing win |
| **S8 FUSE, daemon and explicit APIs** | Native mount, deferred owned replies/fair dispatch, coherent promoted cached profile, kernel-origin mmap/time updates, cheap individual FORGET, event-driven lifecycle; registry plus mount/exec/commit/status/unmount contracts and ordinary Bash streams | S3, S6, S7; runtime reads may use S9 | Mounted alias/truncate/mmap/capture-frontier tests; blocked requests leave unrelated work runnable. Short/long Exec and multi-call same mount work; actual buffer/thread/teardown costs visible |
| **S9 Runtime adapters and complete roots** | Embed existing host libraries; bounded authenticated object/policy/serial/Save/history calls; pending-Save visibility, fair demand/finish service and exact disconnect fences. Fix faithful bounded initial import, symlinks/large files, runtime-compatible complete execution state | S0; P1/P2/P5/P12; parallel to S1–S8 | Real Store reads and two interleaved Saves work through actual APIs; no whole-Save connection checkout or guessed outcome. Full root includes ignored/dependency/cache/output data without per-call restoration |
| **S10 Incremental Commit** | Normalize captured final state; bounded file/namespace construction and operation records; Save finish, stage, conditional transition, known install and exact refused/conflicted/uncertain custody | S6, S8, S9; P3/P4/P6/P7/P13/P14 corrected | Published root equals captured state; later active changes remain. Repeated same-mount Commit, same-Branch conflict and phase failures preserve identity/bytes/ownership without whole-base alias walks |
| **S11 Integration cleanup** | Remove superseded core Workspace backing, host-construction/duplicate Init routes and retired server integration after replacement coverage. Update SDK/sandbox/bridge wiring and active members | S10 | One authentic core path; no legacy aliases/fallback/parallel implementation; source removals/relocations classified. Root v0.1.6 reference retained for S12 comparisons |
| **S12 Integrated qualification** | Prospectively selected full-root correctness/resource and seven-family reporting on actual integrated source; fresh/retained Workspaces, short/long Execs, concurrency and sustained cleanup | S11; actual implementation and frozen specifications | Required proof/resource rows pass; every selection retains its actual outcome. EXPLAIN/profile and bounded-work evidence accompany performance claims; historical failures remain unchanged |
| **S13 Legacy-root retirement** | After cluster-two completion/qualification, remove root `crates/` and obsolete manifest/build/test wiring; retain required shared root config, immutable receipts and reproducible Git/baseline identities | S12; verified migration closure | Core builds/checks and current dependencies are independent of the removed tree. Per-commit legacy/core/combined LOC recorded; no reference deletion described as measured algorithmic speedup |

Bring-up may use test providers under tests over real content-built roots; that
does not qualify real Store/runtime or complete execution readiness. The
cluster-one follow-up lane covers P3/P4/P5/P6/P7/P12/P13/P14 through existing
public contracts or explicit first-party contract changes, preserving canonical
compatibility. SQLite-backed mutable operation records does not replace canonical trees.

The first implementation tranche is S0/S1 with the minimal S2/S3 interfaces:
one initialized indexed/profiled engine, correct generations and one real-root
read/mutation/capture path. Progress it alongside the highest-risk cluster-one
and Save-lifetime proofs. Do not begin with cache/worker tuning or a full legacy
crate port.

## 4. Source removal, relocation and estimates

### 4.1 Removal (measured at `f96d97651`)

| Removed | LOC | Slice | Kind |
| --- | ---: | --- | --- |
| `layerfs-workspace`, all 101 files | 26,835 | S11 | Replacement of a mechanism. `backing/` (13,866 per the #303 ledger) is deleted outright. `filesystem/` and `runtime/` implement behaviour that is kept and rewritten, so their removal is not a simplification claim |
| Bridge files of host-side construction: `contract/prepared_stream.rs` 387, `contract/metadata.rs` 23, `contract/source.rs` 29, `adapters/native/protocol/metadata.rs` 945, `adapters/native/protocol/prepared.rs` 48 | 1,432 | S11 | Deletion, to be confirmed file by file when the slice opens them |
| Daemon `transport.rs` 88, `headless.rs` 133 | 221 | S11 | Replaced by the upstream pool |
| `layerfs-server`, all 31 files | 3,996 | S11 | Retired target, not rewritten/restored. Existing directory is excluded reference at the pin. Duplicate Init/import behavior already belongs in layerfs-project; runtime adapters reuse current public libraries |

Under placement option C the bridge's history contracts
(`contract/history.rs` 349, `protocol/history_failure.rs` 163) are **kept**:
history calls cross the bridge. The prepared plan deleted them.

### 4.2 Relocation

This is a possible temporary reference-preservation step, not a mandatory
runtime package or an authoritative count for a future staged tree.

| Move | LOC | Delta |
| --- | ---: | ---: |
| `layerfs-workspace` → `layerfs-workspace-legacy` (S1) | 26,835 | 0 |

No other relocation is planned. Init already lives in `layerfs-project`.

### 4.3 Estimated future size

> **Planning estimate, revised 2026-10-05. Not measured future source and not a
> commitment or size gate.** R1–R8 and sound Save/service lifetimes still need
> concrete algorithms. Preserve required behavior and validation even if the
> resulting implementation exceeds this range; revise the estimate explicitly.

The current working-tree production count was confirmed with
`python3 -B tools/production_loc.py --json`, using counter SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
No production source differs from the pin in §1. These are source-size counts,
not benchmarks. SQL schema/statements and shipped runtime adapters count as
production; tests, docs, examples and tools do not.

| Responsibility / placement | Now (counted production LOC) | After (planning LOC) | Basis |
| --- | ---: | ---: | --- |
| `layerfs-overlay` | — | 3,000–4,500 | Schema, typed indexed metadata/payload/operation records operations, generation ownership, physical pressure/reclaim and real DB profiling; SQLite supplies the mutable indexing/page engine |
| `layerfs-workspace` | 26,835 | 6,000–8,000 | Existing filesystem operations are 4,909 LOC; retain their semantics, reuse public cluster-one construction/read APIs, add stable capture/composition and exact Commit outcomes; no private page/tree/pack engine |
| `layerfs-fuse` | 1,447 | 1,500–2,200 | Adapter and mount custody plus owned deferred jobs, coherence/send ordering, cached mmap compatibility and request telemetry |
| `layerfs-daemon` | 2,151 | 1,500–2,500 | Registry/incarnation routing, fair short overlay jobs, upstream service and ordinary Bash process/stream lifecycle; no host construction path |
| `layerfs-bridge` | 6,834 | 4,500–5,500 | Keep authenticated native transport, typed history/control/outcomes; remove prepared-stream construction, add bounded object/Save contracts and multiplexing |
| SDK host runtime adapters, inside `layerfs-api/sdk/src/runtime/` | Included in the old server scope, not a separate current subtotal | 1,200–2,000 | Compose current Handles/Storage/HistoryCatalog; own authenticated object/Save/history handlers, semantic admission and fair provider/session service; no new coordinator crate |
| `layerfs-server` | 3,996 | 0 (target retirement) | No implementation restored here; required runtime adapter work is counted explicitly above |
| `layerfs-sandbox` | 1,106 | 1,000–1,300 | Container/native attachment and teardown custody; no per-call content preparation |
| API contracts + SDK forwarding, excluding the runtime row above | 796 | 800–1,000 | Five Workspace operations plus existing Project/Sandbox composition; ordinary Exec and exact typed outcomes |
| **Cluster two replacement scope** | **43,165** | **19,500–27,000** | Sum of the non-overlapping responsibility estimates, including runtime adapters and shipped SQL |

The working estimate is therefore **about 20–27 thousand production LOC**, with
roughly 24 thousand as a planning reference. Against 43,165 this would be
16,165–23,665 fewer lines, or about 37–55%; the reduction is projected, not
achieved. This is not an instruction to fit code by suppressing errors, merging
responsibilities, removing validation or compressing lines. FUSE/daemon/transport
can grow where correctness and service ownership require it.

The private `backing/` subtotal was independently recomputed through the same
counter functions: **13,866 LOC in 47 files**. The other old Workspace subtotals
are filesystem 4,909, Commit 3,996, runtime 2,485, overlay 993 and source-root
types/declarations 586. Their combined 26,835 is replaced as a whole; only the
private backing mechanism disappears outright. Retained filesystem behavior is
not counted as deleted merely because its implementation moves or is rewritten.

The current cluster-one product subtotal is **30,835**. Adding the replacement
estimate gives **50,335–57,835 core production LOC before the still-unestimated
cluster-one prerequisite changes**. P3/P4/P6/P7/P12/P13/P14 and other actual
handbook changes must be counted in their owning crates, not hidden outside the
estimate or claimed to cost zero. Runtime handlers count at their real path even
if final S0 placement changes.

Root `crates/` contributes **65,417** separate reference LOC today. Its S13
retirement is migration removal, not another 65,417-line algorithmic improvement;
it happens only after integrated qualification. Current core plus reference is
139,417. Temporary relocation/duplication remains counted until actually removed.
Earlier pre-review ranges in design Git `334fc7437` remain withdrawn as budgets;
this revision replaces that planning table, not any historical measurement receipt.
Recompute every implementation commit from exact parent/staged/committed trees.

### 4.4 File ownership

[proposed design] The crate boundaries are intentional; individual file names
are proposals. Create files only when a slice needs real implementation. The
payload modules name responsibilities without selecting an unproved R1/R4
algorithm. Each crate has external `tests/`; examples, fixtures, harnesses and
diagnostics remain outside product `src/`.

```text
core/
  Cargo.toml, Cargo.lock
  crates/
    layerfs-content/                          cluster-one canonical content APIs
    layerfs-storage/                          cluster-one immutable Save/read
    layerfs-history/                          cluster-one stage/conditional head
    layerfs-persistence/                      cluster-one provider ownership
    layerfs-project/                          faithful initial root acquisition
    layerfs-telemetry/                        shared product observations

    layerfs-overlay/                          only owner of mutable SQL/payload
      sql/schema.sql                         rows, indexes, operation records, ownership
      src/lib.rs                             declarations/reexports
      src/db.rs, profile.rs, error.rs         engine setup and typed errors
      src/workspace.rs, generation.rs        namespace rows and atomic frontier
      src/inode.rs, directory_entry.rs                 typed metadata and name operations
      src/payload/                           bounded byte/validity/read operations
      src/ownership.rs, operation_record.rs            pins, leases and operation records
      src/scan.rs, reclaim.rs                 keyset walks, debt and physical space
      src/metrics.rs                         SQL/BLOB/transaction runtime profiling
      tests/

    layerfs-workspace/                        filesystem semantics; no SQL/framing
      src/lib.rs, types.rs, error.rs          declarations and public results
      src/ports.rs                           actual object/Save/history boundary
      src/workspace.rs, view.rs               open/retire and base-overlay view
      src/base/client.rs, cache.rs, view.rs   authenticated immutable base access
      src/ops/                               lookup/open/read/write/resize/attrs
                                             create/link/remove/rename/readdir
      src/ownership.rs                       open, lookup, orphan and read leases
      src/commit/slot.rs, capture.rs          admission and stable capture
      src/commit/file.rs, namespace.rs        public content APIs + backed inputs
      src/commit/publish.rs, resolve.rs       exact Save/stage/Commit disposition
      src/commit/install.rs, outcome.rs       safe base advance, retained active rows
      tests/

    layerfs-fuse/                             kernel adapter and native custody
      src/lib.rs, adapter.rs, replies.rs      checked callbacks and owned replies
      src/mount.rs, jobs.rs                   attach/detach and deferred job bridge
      src/coherence.rs, metrics.rs           cache/send ordering and request costs
      tests/

    layerfs-daemon/                           service ownership and orchestration
      src/main.rs, lib.rs, run.rs, config.rs  readiness and process composition
      src/registry.rs, lifecycle.rs           Workspace/Exec identity and custody
      src/overlay_owner.rs                   bounded fair short engine jobs
      src/upstream.rs                        object/Save/history client sessions
      src/control.rs, control_commit.rs      mount/exec/commit/unmount/status routing
      src/execution.rs                       ordinary Bash and streamed process I/O
      tests/

    layerfs-bridge/                           wire contracts, not FS construction
      src/lib.rs
      src/contract/                          object, Save, history, control, outcome
      src/adapters/native/                   framing/auth/multiplex/backpressure
      tests/

    layerfs-sandbox/                          container and daemon attachment
      src/                                   existing focused owner/session modules
      tests/

    layerfs-api/
      core/src/                              public operation types/contracts
      core/tests/
      sdk/src/                               Project/Sandbox/Workspace forwarding
      sdk/src/runtime/                       embedded host composition, no server
        owner.rs                             Handles/Storage/HistoryCatalog setup
        objects.rs, save.rs, history.rs       authenticated typed request handlers
        admission.rs                         fair provider jobs and resource shares
      sdk/tests/

  docs/                                      current architecture and contracts
  benchmark/                                 external integrated workloads
  tools/                                     development checks

No layerfs-server package in the finished product.
No private backing/page/tree/pack implementation under Workspace.
Root crates/ remains reference only until S13 qualification and retirement.
```

The four main mutable-workspace responsibilities are overlay storage, Workspace
semantics, FUSE adaptation and daemon service ownership. Keep one authoritative
representation of each fact: SQL rows own persisted mutable state; Workspace
decides filesystem/Commit semantics; daemon routes/schedules active calls; FUSE
maps kernel requests and owns native replies. Caches and maintained observations
do not become a second resident namespace or persistent custom index.

The daemon owns the overlay instance and its fair executor; the overlay crate
owns SQL connections/transactions. In the current single-owner candidate this
means one connection initialized at daemon startup, not a connection/database per
Workspace. Do not duplicate a general scheduler inside every crate. Short SQL
jobs can interleave multiple Execs/Commits; an entire Exec or Commit never occupies
a database-wide lock. The selected SQLite profile and Save lifetimes still require
their S0–S2 proofs.

`layerfs-workspace/src/ports.rs` owns actual process/runtime port traits. Reuse
existing content backing contracts or add a needed independently usable boundary
through its owning API; do not create an interface per algorithm. Reuse
cluster-one canonical trees, CDC, CAS and delta code rather than copying their
implementation into Workspace or the SDK. Native framing/authentication stays in
bridge; the SDK runtime binds it to current public libraries without recreating
the retired construction/coordinator service.

Use ordinary focused Rust files and shipped SQL. Every production file is at
most 999 physical lines; `lib.rs`/`mod.rs` are declaration/delegation-only and at
most 200. These ceilings are distinct from the production-LOC estimates above.
Required payload/job modules split by responsibility before either ceiling.

### 4.5 Physical state layout

[proposed design] Code layout and runtime storage are different. Application-
selected paths host the global cluster-one Store; the daemon has one separate
disposable overlay database. This is ownership shape, not fixed filenames or an
implemented recovery/profile promise.

```text
host-selected global Store location/
  existing cluster-one provider data          immutable objects + mutable history

sandbox daemon state/
  overlay.sqlite                             all local Workspace namespaces
                                             metadata + physical payload + operation records
                                             ownership + automatic reclaim debt
  SQLite-managed sidecars                    only as required by selected profile

sandbox mount locations/
  workspace-A/                               FUSE view of base root + A rows
  workspace-B/                               FUSE view of base root + B rows
```

There is no database, private payload pack or copied base tree per Workspace or
Commit. Payload bytes are physically held in the daemon database under the
selected bounded representation; a mount path is a filesystem view. Terminal
unmount retires that namespace and automatically schedules eligible row removal;
shared tables/database and other Workspaces stay live. SQL deletion makes
cells/pages reusable and does not promise database-file shrink. Immutable global
objects/history are outside local teardown. Aggregate processing windows limit
resident/queued work, not total Workspace files, bytes, edits or Bash duration.

## 5. Validation

### 5.1 Correctness precedes any timing

The seven [primary documents](README.md#primary-design-documents) own workload
coverage. Their case IDs are design coverage, not registered benchmark selections.
Review complete-root readiness and R1–R8 before implementation. Both per-tool-call
and per-task modes are required, with per-tool-call the expected common case.
Mount -> ordinary Exec -> explicit affected-state Commit -> terminal unmount is
the smallest-lifetime example. A persistent Workspace can serve multiple calls
and incremental Commits before its final unmount. Exec duration is independent
of mode; cover short and long-lived commands in both. Initial acquisition includes every
supported entry, including ignored caches/output and all symlinks; if a required
entry cannot be represented, readiness fails explicitly rather than omitting it.

Report each phase and the complete call, including output drain and automatic
cleanup debt; a fast Exec alone is not load-bearing admission. Status is optional,
bounded observation, never a mandatory progress/Commit checkpoint. No benchmark,
measurement profile, cache contract or proof budget is silently changed here.

[proposed design]

| Requirement | Test |
| --- | --- |
| Accepted-write semantics | A successful write is readable by another Exec; definite failure before publication changes no partial data/metadata; a lost reply after local publication preserves the actual changed state |
| Exact capture | Rows of a request are never split across generations: a writer loop and a capture loop, then a row audit |
| No edit-count limit | 10,240 and 100,000 writes to one file, appended, dispersed and repeated; Commit of each |
| No unrelated whole-file/Workspace work | Derived point/window work and indexed depth explicit; no old-fragment, whole-base or quadratic amplification hidden behind a small statement/window count |
| Repeated edits across Commit | The first interleaving of [04 §9](04-concurrency-commit.md#9-required-interleavings), asserted on stored rows |
| Open-unlinked lifetime | One descriptor across many successful/failed Commits; bound versions/read depth and prove content |
| Truncate | Logical cutoff, regrow zeros; capture/install/retirement before old cleanup resumes |
| Rename of a base directory | No descendant row; children resolve; Commit publishes the move |
| Enumeration | Entries present throughout appear exactly once under concurrent rename and unlink; resume at a partially consumed reply |
| Two Workspaces | Shared writer fairness, namespace isolation and global database failure; logical quota and physical headroom separately; continuous reads plus simultaneous Saves; same-Branch conflict with exact stage disposition |
| Persistent multi-call Workspace | Sequential/overlapping calls, repeated incremental Commits and later active writes; same-mount caches/identity, bounded orphan/version ownership and reclamation during live activity; no automatic teardown |
| Duration independent of mode | Short and long-lived Execs in both per-tool-call and per-task orchestration; process/stream/request custody and backpressure remain valid without command classification or default timeout |
| Capture row lifetimes | Existing G rows retained without bulk copy; only affected G+1 state and operation records created; install never deletes active writes |
| Automatic cleanup while idle | After Commit/install and terminal unmount, issue no further API calls; observe fair batched SQL deletion/debt completion without manual trigger or TTL; retained-owner release gates respected |
| SQL space reuse | Rows removed, cell/page/freelist behavior correctly accounted; database file need not shrink and shared tables/other Workspaces remain live |
| Terminal unmount | Detaches/fences, invalidates incarnation and owns automatic cleanup; no separate close/implicit Commit; pre-effect Busy/Uncertain refusal preserves usability, later failure retains exact stopping/native custody |
| Ordinary shell Exec | Same command through Exec and another authorized shell on the mount has identical filesystem semantics; Bash syntax, no automatic runtime timeout, long-running process and output streaming, no implicit capture/Commit/path routing/status observer |
| Stat identity | Identical `ino`, `size`, `mtime`, `ctime`, `mode` across two mounts and across an install |
| Coherence | One test per row of [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |
| Mutation-time refusals | Each row of the refusal table in [02 §6](02-base-overlay.md#6-names-and-inodes) |
| Outcomes | Each row of [04 §6](04-concurrency-commit.md#6-outcomes), driven through a test implementation of the ports under `tests/` |

Tests use the public API of the production library. No test-only branch, hook
or feature is added under `src/`.

### 5.2 Acceptance and adversarial proofs

[owner bootstrap/Exec requirements]

Prove daemon startup initializes exactly one overlay database/schema and subsequent
Workspace opens do no additional database open/schema setup or base scan. Report
base binding, logical open and actual FUSE mount separately; no bootstrap time is
claimed from removing file creation alone. Terminal unmount detaches/fences the namespace
without an inline full-row delete; background cleanup cannot starve another mount.
Audit metadata/payload/operation records queries for Workspace prefixes and intended indexes.

Exec has no automatic shell runtime timeout or the old 30 s wire/SDK cap. Test
explicit cancellation and streamed output separately from storage RPC deadlines.
Harness stop budgets remain measurement rules, not product shell semantics.

[proposed validation; no current product acceptance]

| Dimension | Full repository | Continuous file log | Concurrent Workspaces |
| --- | --- | --- | --- |
| Correctness | Faithful base and persisted full-tree proof required | Append/rotation/alias/mmap proof required | Authorization and outcome fencing required |
| Mutation latency | R1/R5/R6 changes required | R1/R4/R6 changes required | R5/R6 changes required |
| Sustained throughput | Integrated evidence absent | Pager/tail/journal costs unmeasured | Shared Store/device service unmeasured |
| Commit progress | R2/R3 changes required | Tail route reasonable; failure/orphan R4 required | R5 scheduling/transport required |
| Fairness | Deferred workers required | Same-inode progress required | Per-Workspace service shares required |
| Processing memory | R3 plus aggregate windows required | Residency/journal bounds required | Queue/session/cache totals required |
| Versions/disk pressure | Reservation and actual debt accounting required | No growing orphan pin chain; rotation debt bounded | Shared physical headroom and outcome-aware teardown required |

The full fixture remains 130,045 entries and 3,475,776,149 regular-file bytes:
103,108 files, 16,867 directories, 10,070 symlinks. Copied source HEAD
`639ed015397290b3745d163aafe02ffee4aa3f84`, manifest SHA-256
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
Retained preparation evidence is on `codex/phase7-experiment-305` at
`1451b68a720bbe2175a103dd9b35693ad05e2be1`,
`core/docs/issues/305/PREPARATION-REPORT.md`. These are historical manifest
observations, not a new scan. Full replay is 95,021 entries / 2,126,509,110 bytes.
Preserve .git, dependencies, symlinks, caches, output and hard-link aliases; no
small fixture substitutes for final full affected-state proof.

Native Init currently refuses symlinks (P12). Establish a bounded faithful full
base and compatible Linux toolchain. Historical pure-JavaScript builds/copy-link
replay do not prove real installer lifecycle scripts/concurrency or native tools.

Before timing, use deterministic public-port/mounted interleavings:

1. Block Save acceptance after capture of S1. Overwrite/truncate/rename/create/
   unlink while blocked; prove published state equals S1 and live state includes
   later writes. A second Commit publishes them.
2. Alternating-byte fragments -> full-window overwrite: bound BLOB/SQL/page/journal
   work. Nonzero shrink -> capture/install/retire -> resumed cleanup preserves zeros.
3. Capture one key while full dependency replay grows A: fixed EOF, generation
   selective visits and replayed membership. Fragmented/wide/sparse Commit completes
   with fixed processing windows; no inflated resource cap substitutes.
4. Retain one unlinked descriptor across many Commits; bound versions/depth. Inject
   late definite failure with large C/A log streams; prove latest attributes/bytes
   without payload-sized busy pause. Logging means file append; stdout alone bypasses it.
5. Two inode waiters plus unrelated request; continuous reads plus two Saves; all
   configured Save sessions plus independent demand read. Prove finite progress.
6. Rotation/delete near quota: actual freed pages, reserved headroom, queue delays,
   debt and other-Workspace service. Test physical-disk pressure separately.
7. Malformed roles/references, oversized input, cross-session/history authority;
   unknown Save/stage/transition/discard and forced terminal unmount during every queued wait.
8. Mounted coherence cases in 05 §9, full git status across mounts/.git index,
   compatible build/output mutation, copy/link replay and full Commit survival.

Fault coordination lives under tests/ and uses public ports/behavior; no test-only
production hooks. A labelled count diagnostic measures causes, not repeated gate
samples. Every proof's scope and limitations are prospectively declared.

### 5.3 Count diagnostics

Follow the [optimization guide](../../../../docs/general/optimization-guide.md).
SQLite debugging requires paired EXPLAIN and correlated runtime DB profiles,
including real VM steps, statement/transaction/BLOB observations and scope.
A plan or command wall alone is insufficient; unavailable required profiling
leaves the claim unqualified. Derived work must reject quadratic scaling across
operations and repeated call/Commit lifetimes. These checks begin with S1.

[proposed design] Count-driven instruments, reproducible across windows,
labelled as diagnostics and never as samples of a benchmark arm (root
`AGENTS.md` §3.1): statements, transactions and pages written per operation
class, including BLOB opens/writes, validity work, journal peaks and index splits;
visited/returned captured-active-retired rows per replay and worker occupancy;
base calls per operation class; bridge calls and bytes per Commit;
objects emitted, reused and inserted per Commit (`WriteOutcome`); lock-wait
maximum per phase; garbage pages over time.

### 5.4 Qualification

[proposed design, under the owner's recorded acceptance]

**Before any run.** Read `benchmark_agent_report.md` and
`docs/general/benchmark_rules.md`. The following must exist first:

1. The [current hosting scope](../../../../docs/general/benchmark_rules.md#hosting-scope-for-cluster-one-and-cluster-two)
   is applied to a real integrated implementation. O-1 routing is resolved;
   the policy update alone supplies no implementation or measurement admission.
2. A committed specification per family that freezes case identities, order,
   limits, the cache contract and the verifier before sampling.
3. A declared cache contract that names every cache of
   [02 §8](02-base-overlay.md#8-caches-owner-key-visibility-invalidation):
   `BaseCache`, the attribute cache, the overlay's page cache, the guest page
   cache and the host Store's caches. A warm cache is setup reuse, never a cold
   claim; an undeclared state is `INCOMPLETE` or `INELIGIBLE`.
4. Release binaries with pinned identities; one sample per case per arm; a
   fresh output path; one construction worker.

**Acceptance.** The owner's acceptance for this cluster is the seven
`fs-bench-pro` families on the integrated product (#303). A family is complete
as a report when, at one frozen integrated source, every registered selection has a
terminal status recorded in the applicable table of
`benchmark_agent_report.md`: a functional `PASS` with its command budget,
independent verifier and cleanup, or a non-`PASS` status reported as plainly.
Terminal dispositions complete the report, not product acceptance: required
correctness/resource rows must pass; FAIL/NOT_RUN remain failures/omissions.
Numeric latency stays `INELIGIBLE` unless the cache contract is declared and
enforced equally.

| Family | Disposition in this plan |
| --- | --- |
| 1 `init_namespace`, 2 `history_retention` | Cluster one families. Reused from qualifying receipts by identity if the product, compilation and binary scope they cover is unchanged; otherwise only the affected checkpoint is run (root `AGENTS.md` §3.4). The retained Durable Init rows are `FAIL` on the relative speed ceiling and stay so |
| 3 `workspace_write` | All nine cells of the write matrix. The Repeated cells are the registered home of target T3. The 10,240-write case deferred under #276 is re-opened as a prospectively registered case |
| 4 `workspace_commit`, 5 `workspace_namespace`, 6 `workspace_mutations`, 7 `workspace_shell_package` | One row per registered case. A selection whose subject is a removed mechanism (private-backing custody, funds, page credit) is `NOT_RUN — mechanism removed`, shown beside its prospectively registered successor; that disposition needs the owner's confirmation, as the prepared plan already asked |

No historical receipt is relabelled: the Phase B dirty-discard `FAIL` and the
recorded deferrals stay as recorded.

**What #305 and #306 do and do not contribute.** They are diagnostics on a
passthrough and a throwaway prototype
([05 §1](05-fuse-assessment.md#1-what-was-measured-and-what-was-not)). They
motivate the mount profile and the request-count targets. They are not arms of
any qualification of this design and no row of theirs is reused as a sample.

## 6. Rules for every commit

- The production LOC line, from the first parent and the staged tree, with the
  same counter: `Production LOC: <before> -> <after> (delta <signed>)`, with
  reference and core subtotals.
- An architecture document under `core/docs/architecture/` is updated in the
  same commit as any change to a boundary, format, algorithm or named bound.
- No new dependency where an existing crate provides the capability; no
  patched, vendored or forked dependency; locked builds.
- The exact checks that ran, the ones that did not, and why. There is no CI and
  no aggregate gate to cite.
