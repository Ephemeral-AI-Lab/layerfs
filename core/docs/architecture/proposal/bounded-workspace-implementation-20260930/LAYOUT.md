# Consolidated target repository layout

> **Status: Research; informative and not a product contract.**
> Proposed implementation layout, 2026-09-30. Baseline:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. These are target paths, not
> directories/files already implemented or instructions to create empty modules.

Read the [implementation specification](README.md), [Workspace](WORKSPACE.md),
[Server](SERVER.md), [concurrency](CONCURRENCY.md), [acceptance](ACCEPTANCE.md)
and [scenario/scaling map](ISSUE276-AND-SCALING.md). This file consolidates
tentative packet paths into one target layout. Responsibilities and interfaces
remain unchanged. In particular, the tentative flat daemon files are grouped
under workspaces, commands, control and transport below.

## 1. Package boundaries

Keep the existing Core crates. Add no universal manager/resource/runtime crate,
plugin registry or parallel replacement workspace. Root crates remain reference
code and do not become dependencies of Core. Existing unrelated source remains
in its component; the tree below highlights the changed responsibility groups.

```text
core/
  crates/
    layerfs-api/
      core/                   public API contracts and typed handles
      sdk/                    public lifecycle and ordinary exec wrappers
    layerfs-sandbox/          runtime provisioning and SDK control ownership
    layerfs-daemon/           mounted Workspace and command process assembly
    layerfs-fuse/             ordinary syscall projection and kernel session
    layerfs-workspace/        private live state, generations and Commit custody
    layerfs-bridge/           logical wire contracts and authenticated native IO
    layerfs-server/           authorized C1/C2/C5 composition and resource domains
    layerfs-content/          canonical semantics, reads, construction and proofs
    layerfs-storage/          physical CAS, codecs, packs, SQLite and scratch IO
    layerfs-history/          existing history/allocation/expected-head semantics
    layerfs-telemetry/        bounded operation observations and completeness
  benchmark/                 existing external benchmark framework
  tools/                     development checks; never product implementation
  docs/                      architecture/specification/evidence
```

## 2. Workspace: selected authority and streaming

```text
layerfs-workspace/src/
  lib.rs
  runtime/
    host.rs                   shared byte admission and private backing host
    state.rs                  fixed current selection and lifecycle
    leases.rs                 Workspace/operation ownership
    references.rs             issued handles/views and bounded caches
  filesystem/                 existing public filesystem semantics
  backing/
    active/
      format.rs               private record/schema checks
      page.rs                 authenticated private page framing
      nodes/
        parse.rs              borrowed bounded node decoding
        cursor.rs             monotone selected-tree traversal
        builder.rs            bounded ordered page output
        split_join.rs         persistent structural edits
      files/
        sources.rs            exact terminal/parent span authority
        intervals.rs          coverage, cuts and coalescing
        mutation.rs           per-inode candidate preparation
        reader.rs             selected range reads
        hot.rs                bounded selected-shape optimization
      catalog/
        inodes.rs             inode versions, dirty facts and saved results
        names.rs              bindings, parents, aliases and orphans
        publication.rs        short CURRENT catalog transaction
      generations/
        context.rs            immutable full selected views
        capture.rs            G1 cut and new live G2
        resolver.rs           immediate-parent/result/exception binding
        pins.rs               immutable selected ownership
      payloads/
        pack.rs               existing small payload packing
        location.rs           stable identity/versioned physical location
        owners.rs             owned segments and bounded capabilities
      custody/
        ledger.rs             exact paged physical owners
        edges.rs              root/child reference transitions
        quota.rs              allocation/reservation transfer
        cleanup.rs            paged retirement and exact refunds
        compact.rs            bounded relocation
  commit/
    operation.rs              one composite attempt orchestration
    prepare.rs                captured input preparation
    files.rs                  FileSet source/coverage/unit progression
    namespace.rs              header/binding/inode cursor encoding
    results.rs                provisional rows and UnitCompletion seals
    ready.rs                  protected outcome/install capacity
    install.rs                preserve CURRENT G2 and select new baseline
    completion.rs             known/Unknown/local/cleanup continuation
```

Paged mechanics live in nodes; byte/version semantics in files; selected state
in catalog/generations; ownership in custody. Commit consumes these capabilities
and does not implement another tree, pager or process runtime. Workspace host
does not own daemon mount/command registry or child process policy.

## 3. Daemon and Sandbox: live entries and command lifecycle

```text
layerfs-daemon/src/
  lib.rs
  main.rs
  run.rs                      capability wiring and process assembly
  workspaces/
    registry.rs               exact keyed entry/count leases
    lifecycle.rs              attach/mount/closing/retained transitions
  control/
    auth.rs                   exact selector/rights
    dispatch.rs               owned operation tickets
  commands/
    state.rs                  command identity/domain/terminal state
    admission.rs              process/FD/byte ownership
    supervisor.rs             shared event-driven progression
    output.rs                 bounded capture and excess drain
    cancel.rs                 exact cancel/reap/drain custody
  transport/
    pool.rs                   exclusive service channel leases
  platform/
    process_domain.rs         narrow runtime capability contract
    linux/
      readiness.rs            readiness/child/timer events
      domain.rs               exact delegated domain authority
      bootstrap.rs            trusted setup then arbitrary shell exec

layerfs-sandbox/src/
  channel_pool.rs             instance-bound SDK control channels
  control_operation.rs        owned call/wait/cancel
  execution_profile.rs        runtime/delegation/actor capabilities
  docker.rs                  existing selected runtime provisioning
  owner.rs                   public provisioning/control coordination

layerfs-api/core/src/
  workspace.rs               public Workspace API contracts
  exec_handle.rs             exact command/cancellation handle
layerfs-api/sdk/src/
  workspace.rs               ordinary public lifecycle wrapper
  workspace_exec.rs          generic exec/start/wait/cancel implementation
  exec_handle.rs             SDK handle ownership
```

The commands folder supervises arbitrary opaque commands; it contains no
handlers named after cp, pip, Python, models or database workloads. Registry
lookup yields a lease and ends before slow work. Linux modules implement
runtime capabilities; no platform code enters canonical content algorithms.

## 4. Bridge: contracts separate from transport mechanisms

```text
layerfs-bridge/src/
  contract/
    prepared/
      types.rs               declared totals and checked row roles
      read.rs                incremental binding parser
      write.rs               bounded cursor encoder
    file_set/
      types.rs               member/unit/selection contracts
      read.rs                exact streamed member input
      result.rs              bounded result rows and unit terminal seal
    exec_lifetime.rs         UntilOwnedExit/control/outcome grammar
    view_stream.rs           selected read/list data totals and trailer
    observation.rs           bounded completeness envelope
  adapters/native/
    client.rs                finite/public caller wrapper
    server.rs                owned request/reply wrapper
    io_state.rs              partial prefix/record cursors
    crypto_record.rs         authenticate/seal/nonce ownership
    operation.rs             channel state transitions
    reactor.rs               readiness and byte-credit dispatch
    liveness.rs              heartbeat/stall state
    result_stream.rs         typed result-data progression
    protocol/
      file_set.rs            operation framing
      ...                    retained existing protocol codecs
```

contract describes meaning, ordering and bounds. adapters/native handles owned
encrypted IO. Neither decides canonical tree partitions, spawns a command or
opens Store tables. Existing wire compatibility stays explicit and isolated.

## 5. Server and C1: orchestration separate from logical algorithms

```text
layerfs-server/src/
  service/
    construction/
      admission.rs           typed C2/C5 phase and byte admission
      file_set.rs            generic sequential C1/C2 assembler
      file_input.rs          direct/replay source orchestration
      prepared.rs            sealed namespace input and proof phases
      results.rs             paged results and known/Unknown delivery
      verification.rs        bounded live namespace validity leases
      namespace_import.rs    explicit snapshot/new-scope remap
      finish.rs              exact object/workflow completion
  host/
    memory_domain.rs         owned native memory/IO capability

layerfs-content/src/
  object/                    existing canonical identity/framing
  file/
    edit/
      apply.rs               one split/join construction sequence
      tree.rs                existing logical tree operations
      draft.rs               typed unfinished-page/reference state
      state.rs               bounded cache/indexed-state access
      finish.rs              children-first exact finalization
      policy.rs              explicit v1/v2 construction policy
    ...                      existing codecs/read/build functions
  filesystem/
    profile.rs               checked namespace profile dispatch
    rows/
      source.rs              header/binding/value cursors
      memory.rs              fixed resident arena adapter
      spool.rs               bounded sealed slot/record access
    proof/
      subjects.rs            identities/types/allocator preconditions
      bindings.rs            exact before/after effects
      graph.rs               exact colors/stack/final graph
      state.rs               typed proof state and narrow IO ports
      certified.rs           nonforgeable VerifiedNamespace
    parents/
      codec.rs               parent leaf/branch grammar
      build.rs               initial forward/reverse correspondence
      update.rs              incremental final-parent proof
      read.rs                authenticated parent lookup/cursor
    references/
      effects.rs             compact reference events
      join.rs                counts/values/content-root joins
      runs.rs                bounded size-tiered ordering
      release.rs             exact released-descendant closure
    ...                      existing directory/inode/attribute codecs
```

Server binds real resources and providers. C1 operates through authenticated
objects, finalized consumers and narrow state/run capabilities. C1 opens no
SQLite database/file, knows no mount or command, and imports no daemon/Workspace.

## 6. C2, C5 and telemetry

```text
layerfs-storage/
  src/
    cas/
      admission.rs           object/wave/index allocation leases
      batch.rs               bounded producer/consumer overlap
      lifecycle.rs           existing private Save visibility/finish
    construction_state/
      session.rs             metadata-only scratch lifecycle
      index.rs               bounded indexed metadata pages
      runs.rs                physical sequential-run adapter
      profile.rs             verified scratch/cache policy
    sqlite/
      memory.rs              global engine guard and readback
      ...                    existing query/blob/transaction provider
    migration/
      authority.rs           quiesced exact-Store maintenance
      locators.rs            bounded copy/equality proof
      transition.rs          table selection and paged retirement
      status.rs              exact known/Unknown phase custody
    encoding/                existing codec; unchanged compression policy
    pack/                    existing placement/framing
  sql/
    construction_scratch.sql required metadata runtime schema
    ...                      explicit Store schema versions

layerfs-history/
  src/                       existing opaque profile/scope/history semantics
  sql/schema-v1.sql          unchanged C5 catalog initially

layerfs-telemetry/src/
  operation/
    identity.rs              bounded operation/source binding
    aggregate.rs             fixed phase/work totals
    completion.rs            explicit required-source completeness
  output/
    retention.rs             bounded queue/terminal custody and loss
    ...                      existing codecs/output ownership
```

Scratch contains construction metadata only, never a second payload/CAS copy.
C2 keeps physical representation and SQLite. C5 remains the history/allocator
owner; new namespace import invokes its existing initialization contract after
certification instead of adding a competing history service.

## 7. FUSE: semantic projection and kernel adapter

```text
layerfs-fuse/src/
  lib.rs
  projection/
    identity.rs              RuntimeActor/issued-handle checks
    files.rs                 normal file operations
    names.rs                 namespace operations
    listing.rs               paged directory/cookie behavior
    attributes.rs            modes/mtime/errno/kernel facts
    quota.rs                 truthful shared admission snapshot
  linux/
    wire/
      requests.rs            checked kernel UAPI decoding
      replies.rs             bounded single-use reply encoding
    requests/
      leases.rs              live kernel request ownership
      interrupt.rs           exact cancellation/publication race
    session.rs               reader/readiness/dispatch
    mount.rs                 actor access/detach authority
```

The projection invokes Workspace semantics. Kernel session owns device framing
and interruption; it does not contain range trees or Commit construction. The
selected target is detailed in ACCEPTANCE; legacy provider compatibility is
explicit, never a required-capability fallback.

## 8. Tests, documentation and replacement discipline

Each crate keeps public integration tests in tests/, helpers/fixtures under
tests/, runnable examples under examples/ and benchmarks under benches/ or the
existing core/benchmark harness. No test code or alternate benchmark algorithm
enters src/. Root scenarios.md continues to route workload expectations.

Add lib.rs/mod.rs only as declaration/reexport/delegation entries: <=200 physical
lines. Every other shipped implementation file, including SQL, stays <=999.
Small cohesive files remain together; split at a real responsibility boundary,
not per struct, arbitrary suffix or aesthetic preference.

Replacing a component is not adding active_v3 next to active_v2 forever. Retire
superseded whole-population/reconciliation/host-gate paths after their explicitly
supported owner/format obligations close. Isolate required legacy codecs/readers
at the format boundary. Never silently fall back to the old execution algorithm.
Existing algorithms moved/split between these paths have zero net LOC impact.
New capabilities require their own additions/deletions accounting from the
[integrated forecast](README.md#integrated-implementation-loc-forecast).
