# SQLite service/runtime architecture: local, host + Docker, and future cloud

> **Status:** Research; informative and not a product contract.

Investigation dated 2026-09-20, requested after the service/runtime/bridge design.
Owner direction: cloud/serverless/remote placement is a future ultimate goal,
durability will be enabled later, and SQLite is preferred. This commissions a
future path; it does not enable durability, WAL, automatic retries, migrations or
cloud providers in today's product. Pair 3 still precedes pair 1 and pair 2.

Read [#157](https://github.com/Ephemeral-AI-Lab/layerfs/issues/157),
[#158](https://github.com/Ephemeral-AI-Lab/layerfs/issues/158) and
[#173](https://github.com/Ephemeral-AI-Lab/layerfs/issues/173), their comments,
existing reports, selected load-bearing sources in all three pinned repositories,
current LayerFS source, and current official SQLite/Cloudflare documentation.
No product changes, builds, provider deployments, tests or measurements were run.
No issue was changed and this report does not complete the broader Radish study.

## Conclusion

Keep **service + runtime + bridge**. Add a second explicit portability boundary
inside C2: **storage operations versus the concrete SQLite provider**. A portable
bridge does not make native `rusqlite` code portable to managed SQLite.

The most direct cloud route is to move the operation owner close to SQLite:

```text
 LOCAL NATIVE
 runtime -- direct / local bridge --> service [C1 + C2 + native SQLite]

 HOST + DOCKER
 Linux runtime -- host bridge --> macOS/Linux service [C1 + C2 + SQLite]

 CLOUD: NATIVE FIRST
 local/container runtime -- network bridge --> native service + local SQLite
                                               durable volume / later replication

 CLOUD: MANAGED SQLITE ACTOR, LATER
 local/container/isolate runtime -- WebSocket/HTTP bridge --> named service owner
                                                              |
                                             portable C1/C2 operation bodies
                                                              |
                                                 managed SQLite provider
```

These are target arrangements, not supported-deployment claims. Local native SDK
or logical-operation access does not automatically provide a macOS filesystem
mount. Linux FUSE remains a projection; an isolate can use a virtual filesystem
API, while arbitrary native executables need a compatible OS execution surface.
Cloudflare Computer demonstrates those distinct surfaces rather than requiring
FUSE in its Durable Object.

Prefer a hosted owner to remotely paging one database file or translating each
SQL query into RPC. SQLite's ordinary WAL mode requires participating processes
on one host; it is not a shared-network-filesystem solution. The older
`cloud-sqlite-vfs.md` network-file/WAL discussion must not be treated as proof of
such support. [SQLite WAL documentation](https://sqlite.org/wal.html)

## What the three projects actually teach

| Study and source pin | Source-backed lesson | Apply to LayerFS | Qualification |
| --- | --- | --- | --- |
| Drive9, `cff6d294b452b54a964de2217c137af303602be9` | Authoritative server, scoped clients and separate runtime staging; pending bytes carry their actual base revision and staging-generation identity | Keep storage credentials on the service; bind frozen input to its real base and runtime generation; distinguish backend outage from dead mount | Its server uses MySQL/TiDB-compatible databases plus S3, not SQLite. Borrow ownership, fencing and supervision rather than its database stack or distributed SQL assumptions. |
| Cloudflare Computer, `64c462b083860cad29b374b9e4cda1e6a680f902` | One filesystem implementation accepts a small SQL/transaction provider; authoritative DO SQLite and disposable container-local SQLite; runtime dials upstream; metadata/bytes/cursors are separate | Put transaction composition next to storage, separate projection lifecycle from durable authority, retain transport-independent operation input and exact generation/cursor accounting | Its final-state replication protocol and mutable history-less filesystem are not LayerFS semantics. The pinned container path does not hibernate and unsynced container writes can be lost. Do not import automatic retry/reconciliation. |
| Radish, `98768e8124ca24ec3f76211daa45290392f6eefa` | Named owner per database, bounded byte protocol over WebSockets, session reconstruction, SQLite metadata with optional R2 tier | Route by logical Store identity; keep bridge framing independent of socket APIs; distinguish transient session cache from authoritative state; transfer immutable bytes before publishing metadata | Not production ready; its tests use a local `bun:sqlite` adapter rather than proving workerd behavior. Per-name authorization and SQLite/R2 atomicity remain application responsibilities. |

Evidence: [Drive9 report](../drive9/architecture_overview.md),
[Computer report](../cloudflare-computer/architecture_overview.md),
[Computer placement study](../cloudflare-computer/sqlite-placement-and-container-transport.md).
Direct source checks: Drive9
[`commit_queue.go`](https://github.com/mem9-ai/drive9/blob/cff6d294b452b54a964de2217c137af303602be9/pkg/fuse/commit_queue.go),
Computer [`storage.ts`](https://github.com/cloudflare/computer/blob/64c462b083860cad29b374b9e4cda1e6a680f902/packages/dofs/src/storage.ts)
and [`backend.ts`](https://github.com/cloudflare/computer/blob/64c462b083860cad29b374b9e4cda1e6a680f902/packages/computer/src/backend.ts),
Radish [`worker.ts`](https://github.com/Dhravya/radish/blob/98768e8124ca24ec3f76211daa45290392f6eefa/src/worker.ts),
[`do.ts`](https://github.com/Dhravya/radish/blob/98768e8124ca24ec3f76211daa45290392f6eefa/src/do.ts),
[`session.ts`](https://github.com/Dhravya/radish/blob/98768e8124ca24ec3f76211daa45290392f6eefa/src/session.ts),
[`tier/engine.ts`](https://github.com/Dhravya/radish/blob/98768e8124ca24ec3f76211daa45290392f6eefa/src/tier/engine.ts)
and [`sqlite-adapter.ts`](https://github.com/Dhravya/radish/blob/98768e8124ca24ec3f76211daa45290392f6eefa/test/sqlite-adapter.ts).

Drive9's append-log WAL work is principally about user SQLite databases running
inside a network-backed filesystem. It does not require LayerFS's own Store to
be placed on that filesystem, nor does it supply a ready C2 durability adapter.
Computer's state-based synchronization can inform bounded final-change payloads
and missing-content negotiation without replacing LayerFS's immutable roots,
conditional publication or single-attempt failure semantics.

## Two portability boundaries

```text
 layerfs_runtime
      |
      | Bridge contract: logical operations, identities, bounds, outcomes
      v
 layerfs_service
      authorization / Store routing / operation lifecycle
      |
      +--> C1 canonical construction, edits and reads
      +--> C2 reuse, physical encoding, packing and publication
                    |
                    | Persistence contract: bounded grouped I/O,
                    | atomic mutation, visibility and completion
                    v
          native SQLite provider OR managed SQLite provider
```

These are internal responsibilities; do not create one crate per box or a
speculative backend registry. The native provider stays concrete initially.
When the managed-provider experiment starts, extract the minimum real boundary
needed by those two implementations. Keep per-object/pack/header SQL work local
to the operation owner rather than turning every provider call into a network hop.

## Current LayerFS gaps that a bridge alone cannot solve

Paths in this section are repository-relative. The source manifest records the
working bytes inspected; other Stage 6 work is concurrent, so these are not
claims about its final artifact.

| Boundary | Current evidence | Future requirement |
| --- | --- | --- |
| Native database binding | `rusqlite`/`Connection` appears in 18 C2 source files, including CAS lifecycle, dependency reads, delta selection/read, pooling and public errors | Separate concrete engine handles/errors from physical algorithms and expose bounded storage operations. This is real C2 refactoring, not swapping a connection string. |
| Connection/persistence policy | `sqlite/connection.rs` opens a `Path`, enforces MEMORY/OFF and native pragmas; `sqlite/write.rs` issues `BEGIN IMMEDIATE` | Provider-specific transaction and durability implementations behind explicitly validated capability/profile requirements. Never silently weaken required atomicity. |
| Publication ownership | Save state, connection and cleanup baseline live in an operation; unpublished output is gated by a watermark | Re-establish ownership across transactions and process replacement; durable save/publication status when that feature is enabled. A single-threaded event runtime alone is not whole-operation exclusion. |
| Format/parameter envelope | `LOOKUP_PAGE_IDS = 128` plus query overhead; singleton pack ceiling is 16 MiB + 4 KiB | Derive query batches from provider parameters including non-ID bindings; choose a physical representation that fits the provider, preserving canonical identity. Refuse unsupported profiles until implemented. |
| Pack I/O | `sqlite/write.rs::append_pack` replaces the whole pack BLOB; `lookup.rs::pack_bytes` reads the entire BLOB | Measure provider cost; consider immutable bounded group/segment rows plus a small descriptor before remote placement. Singleton splitting requires its own format/readback design. |
| Schema checks | `sqlite/schema.rs` requires exact tables and rejects unexpected non-SQLite tables | Separate application-owned schema validation from managed-provider metadata; explicitly migrate/import versions. Do not disable validation globally. |
| Native/Wasm execution | C2 uses `zstd-sys`; C1's concrete ordering backing uses ordinary files/seek | Qualify codec/hash determinism, dependencies, allocation/CPU and caller-supplied scratch/replay capabilities on the actual target. Managed SQLite is a host API, not a `rusqlite::Connection`. |
| Cold start | Current service objects/indexes/caches are process-owned; reopen loads persisted state | Make caches disposable, measure rebuild costs and bound startup memory/work. No successful operation may depend on an unpersisted process pointer or session object surviving. |
| History composition | C2's public `finish` does not accept an outer history transaction; extra tables are refused | Pair 2 must define saved-root versus staged/published history atomicity, with safe unreferenced objects or an explicit shared transaction mechanism. |

Do not carry earlier codec-workspace or schema numbers forward without rereading
the final Stage 6 source; those files changed during the investigation. No claim
about current cloud CPU or memory fitness follows from local benchmark results.

## Managed SQLite: independently checked constraints

Official provider documentation checked on 2026-09-20 establishes concrete
constraints, not a qualification of LayerFS:

- Cloudflare's SQL API provides `sql.exec`; transactions use platform methods
  rather than SQL `BEGIN`/`SAVEPOINT`. Synchronous transaction callbacks and
  lossless integer/binary mapping need explicit adaptation. Cursors crossing
  an `await` do not provide snapshot guarantees.
  [SQLite storage API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- Current limits include **100 bound parameters**, **2 MB row/BLOB/string size**,
  **100 KB SQL text**, and **10 GB per SQLite-backed object on Paid**.
  [Durable Object limits](https://developers.cloudflare.com/durable-objects/platform/limits/)
- Managed DO SQLite uses WAL and provider-controlled replicated durability.
  Durability enabled later can remove today's profile exclusion, but does not
  resolve SQL/API/format/lifecycle mismatches.
  [Cloudflare's SQLite replication explanation](https://blog.cloudflare.com/d1-read-replication-beta/)
- Hibernation discards in-memory state; constructors run again. Persistent
  identity and accepted state must be reconstructible.
  [Lifecycle](https://developers.cloudflare.com/durable-objects/concepts/durable-object-lifecycle/)
- Requests can interleave around non-storage asynchronous work; input/output
  gates do not make an arbitrary multi-await workflow one transaction.
  [Durable Object rules](https://developers.cloudflare.com/durable-objects/best-practices/rules-of-durable-objects/)
- WebSocket hibernation is a distinct endpoint API. A socket connection alone
  does not make the service hibernatable.
  [WebSocket guidance](https://developers.cloudflare.com/durable-objects/best-practices/websockets/)
- Workers run Rust through WebAssembly; the existing native binary is not simply
  deployed unchanged. Codec and scratch capability tests remain unrun.
  [Rust support](https://developers.cloudflare.com/workers/languages/rust/),
  [Wasm support](https://developers.cloudflare.com/workers/runtime-apis/webassembly/)
- Workers currently do not expose ordinary inbound TCP through their TCP socket
  API. Preserve a WebSocket/HTTP delivery option in the bridge contract.
  [TCP sockets](https://developers.cloudflare.com/workers/runtime-apis/tcp-sockets/)
- Worker isolate memory includes both JavaScript and Wasm allocations and is
  currently capped at 128 MB. Count all input/output, codec, cache and scratch
  memory together.
  [Workers limits](https://developers.cloudflare.com/workers/platform/limits/)

Source-versus-platform caution: Computer's pinned wrapper emits SQL SAVEPOINTs
for nested transactions, whereas the current documented managed SQL API forbids
such statements. The abstraction pattern is useful; the exact nested path needs
an actual runtime check. Likewise Radish's local SQLite adapter is not evidence
that every production platform restriction was exercised. No subject's passing
local tests become LayerFS provider acceptance.

## What to preserve in pair 3 now

1. **Logical identity, not process identity.** Address authorized Store/service
   identities, explicit base roots and operation generations; keep file paths,
   file descriptors and native connections inside the local adapter. A future
   named actor is naturally one Store ownership unit, not automatically one
   Workspace or one LayerStack. Choose the boundary to match isolation/dedup.
2. **Transport-independent requests.** Direct, native stream and future
   WebSocket/HTTP adapters carry the same operation meanings. Support bounded
   chunk delivery and clean termination without requiring a forever-open TCP
   session or a native listener on the service. Let the runtime initiate the
   connection where network reachability requires it.
3. **Immutable base and input-generation checks.** Keep the base that generated
   the bytes distinct from any later expected publication head. Do not pair old
   bytes with a newly refreshed head, and do not let cleanup delete a later
   runtime generation's data. Drive9's staging fences are a concrete checklist.
4. **Separate authority from runtime cache.** Published data belongs to the
   service. Runtime overlays can remain transient under their declared policy;
   never describe accepted-but-unsaved writes as recoverable from the service.
   Reconstructible projections and explicit loss are different from guaranteed
   durability of every write.
5. **Honest outcomes.** Accepted input, saved objects, published history and
   durable acknowledgement are distinct. Add a durability capability/meaning
   when implemented; do not equate `finish` or socket delivery with future
   power-loss survival. Keep today's no-resend rule.
6. **Bounds as an explicit supported profile.** Specify frame/in-flight/response
   bounds, request input stability, scratch/replay needs and provider constraints.
   Do not hardcode the native engine's generous limits into the public protocol.
7. **Authoritative tenant routing.** The service validates the caller and Store
   permission. A caller-selected Store name is not itself authorization. Keep
   dedup and visibility within the chosen trust domain by default.

These are contract requirements and small composition decisions, not an instruction
to implement multiple transports, a generalized SQL framework, leases or cloud
failover now. Pair 3 still ships one real selected deployment and test client.

## Durability later: more than enabling WAL

Specify the acknowledged failure guarantee first: process crash, host power loss,
or replicated storage failure domain. SQLite WAL by itself does not promise every
acknowledged write survives power loss; synchronization settings and storage
behavior matter. [SQLite synchronization documentation](https://sqlite.org/pragma.html#pragma_synchronous)

The future implementation needs durable save/publication ownership, atomic
metadata/outcome transitions, reopen inspection/recovery, consistent backup and
restore, schema migration/export, and independent crash tests. Preserve successful
versions and never infer from a lost response that mutation did not occur. A
persisted operation identifier/outcome can support authoritative inspection;
automatic retry, resume or replay is a separate policy change not granted by the
decision to enable durability. No consensus subsystem is needed for a single
qualified native owner; a managed platform's documented ownership can satisfy
that role when its full contract fits.

Cloudflare output gates can provide the provider's durable-response boundary;
they do not make LayerFS root publication, remote payload upload and history
updates a single transaction. Local native and managed implementations need
separate qualification of the same externally stated guarantee.

## SQLite first; object storage only when justified

Keep metadata and authority in SQLite. A native durable SQLite service can be a
complete initial cloud deployment; S3/R2 and per-content replication are optional
later capacity/durability work, not prerequisites for pair 3.

If large immutable groups/payloads later move to object storage, use an explicit
publication order:

```text
 create immutable body
       -> complete/verify upload under its identity
       -> atomic SQLite reference + required publication metadata
       -> acknowledge the promised completion level
```

An upload succeeded but metadata failed can leave an orphan. Metadata must never
publish a body whose required upload was not established. Conditional versions,
reachability and retention govern later cleanup; no distributed transaction
between SQLite and R2 is assumed. Radish's upload-then-revalidate/demote ordering
illustrates the issue, not a complete LayerFS implementation. Physical delta-base
dependencies also need retention; deleting an unreferenced-looking blob without
that graph is unsafe.

## Remote database only is a separate, harder destination

A remote SQLite-compatible endpoint may expose statements/batches rather than a
native connection, transaction callback or whole-save ownership. For example,
D1 offers prepared statements and transactional batch execution; that alone is
not a substitute for arbitrary local read-compute-write control flow.
[D1 database API](https://developers.cloudflare.com/d1/worker-api/d1-database/)

If this placement is required later, qualify grouped reads, conditional atomic
publication, error/outcome taxonomy and writer authority across the complete
save; count serialization and all network round trips. Prefer moving those
operations next to SQLite over reimplementing a remote Rust Connection. Keeping
SQLite does not require keeping one identical file format or one native API on
every provider, but any format/profile difference needs explicit compatibility
and migration rather than a silent switch.

## Delivery plan and qualification

| Phase | Build | Proof required |
| --- | --- | --- |
| Now: pair 3 | Native service + selected bridge; document the future boundaries above | Same real handler via direct calls and actual host/Linux link; bounds, authorization, error/unknown-outcome checks |
| Next: pair 1, then pair 2 | Runtime/projection, then history/publication | Real filesystem behavior, stable generation fences, root versus logical Commit outcomes; keep core replacement proof separate |
| Later: durability | One supported durable SQLite profile and recovery/backup contract | Process/host-failure and lost-ack checks, reopen invariants, restore and migration evidence |
| First cloud delivery | Native service colocated with durable SQLite, clients connect outward | Real remote auth/routing, ownership, cold start/restart, latency and whole-path resources |
| Managed actor experiment | Actual SQLite provider adapter plus portable/Wasm core path | Native versus managed canonical parity, provider limits, transaction behavior, restart/hibernation, allocation/CPU; execute in actual runtime, not only a local fake |
| Later scaling | Store ownership shards and optional immutable payload tier | Isolation/dedup policy, no cross-shard atomicity assumptions, upload/publication failure matrix, dependency-safe retention |

Design proof matrix: native direct, native service/local IPC, macOS owner/Linux
runtime, native remote service, managed SQLite actor, and remote-database-only.
The first four can share the native C2 implementation. Managed actor and remote
database cells require additional provider work; label unimplemented cells
NOT_RUN, not portable merely because the protocol is shared. Tests should include
large accepted payloads, parameter-boundary batches, exact 64-bit identifiers,
unknown outcomes, cold reconstruction and malformed input. Timing/memory claims
remain subject to the repository's measurement rules and must not overlap other
owners' runs.

## Evidence and limitations

Captured issues: [157](issue-157.json), [158](issue-158.json), [173](issue-173.json).
[Source manifest](source-manifest.json) records studied commits and inspected
LayerFS file hashes. External repository snapshots match the issue pins. Study
status is not inferred from issue OPEN/CLOSED alone: Drive9 and Computer have
completion comments; Radish has a detailed source map but no completed in-repo
overview at the time of this investigation.

All recommendations above are analysis, not adopted provider support or changes
to the present no-durability/no-retry profile. Local link/whitespace validation
is the only new executable check; no future capability is claimed tested.
