# Phase 7: immutable-object architecture on PostgreSQL + S3, with a daemon SQLite overlay

> **Status:** Current planning checklist; no release candidate exists.
> Owner-requested design packet, 2026-10-03. Issue
> [#301](https://github.com/Ephemeral-AI-Lab/layerfs/issues/301). This packet
> proposes an architecture and a migration order. It claims no implementation,
> measurement, qualification or release admission.

> **Superseded 2026-10-05.** For cluster two, read the reconciled design at
> [`../303/README.md`](../303/README.md). The PostgreSQL and MinIO premise, the
> crates `layerfs-s3` and `layerfs-metadata`, decisions D2, D5, D6 and D10–D12
> and milestones M1–M5 no longer describe the product: cluster one is host-local
> SQLite (see [`cluster_one_handbook.md`](../../../../cluster_one_handbook.md)).
> The text below is unchanged and is kept as context.

## What Phase 7 is

Phase 7 is the **implementation** phase that follows two experimental phases.
Phase 5 (#287/#288/#290) is paused. Phase 6 (#293, #295, #296, #298–#300) showed
the architecture shape — daemon-owned SQLite metadata, immutable packs on MinIO,
a small global SQLite catalog — and lifted several structural limits (file size,
edit size, file count, edits per file), but it lives in a 29,356-physical-line
benchmark crate (`core/benchmark/phase6-live`, at `203879845`) and is not being
optimized further.

Phase 7 starts again from the accepted product source and builds the Phase 6
*shape* with a simpler mechanism:

- **Source base:** Phase 4.5 + Phase B, `origin/main`
  [`7edddbdb8`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/7edddbdb8e8512627aed0ed42533ef099d802384).
  Phase 5 and Phase 6 branches are evidence and lessons, not a source base.
- **Global storage:** two off-the-shelf services in local Docker containers —
  PostgreSQL for committed metadata, locators and history, and MinIO (S3 API) for
  file-content packs. LayerFS writes no service of its own.
- **Environment:** one machine; both services reached over the local network.
  Multi-machine deployment is out of scope.
- **Daemon storage:** one SQLite database holding each Workspace's overlay —
  metadata **and** payload bytes. No private backing files.
- **Workspace:** complete rewrite. C1 (`layerfs-content`) is nearly untouched;
  C2 and C5 keep their algorithms and change their storage boundary.

## The design in six sentences

1. Every committed thing — chunk, pack, file tree, filesystem root, Layer,
   Commit — is immutable and content-addressed; the only shared mutable fact is a
   Branch head.
2. Therefore construction, upload and locator registration need no coordinator:
   any daemon can do them, they are idempotent, and losing a race wastes bytes
   but cannot corrupt history.
3. Publication is one short conditional transaction on the global metadata
   store: *move this Branch head from the root I based on to the root I built*.
4. Live Workspace state is a **generation-keyed overlay** in the daemon's SQLite
   database: rows are keyed `(workspace, object, generation)`, and every mutation
   writes at the active generation.
5. Commit capture is one integer increment; rows of the frozen generation are
   never written again, so Commit reads them while commands keep running.
6. Within a generation a rewrite replaces the row in place, so a block edited a
   thousand times is stored once — twice only while a Commit is in flight.

## Documents

| Document | Covers |
| --- | --- |
| [01 — Architecture](01-architecture.md) | Principles, stores, the thirteen components and their relationship graph, ports, data paths, what is removed |
| [02 — Workspace overlay](02-workspace-overlay.md) | Daemon SQLite schema, generation rule, latest-only payload, non-pausing Commit |
| [03 — Commit workflow](03-commit-workflow.md) | Capture → construct → upload → publish → install → retire, outcomes and failures |
| [04 — Storage interfaces](04-storage-interfaces.md) | S3 object port, PostgreSQL metadata store, transfer units, ordering, integrity, MinIO objects and bucket settings, ranged reads |
| [05 — Migration plan](05-migration-plan.md) | Per-crate impact from the Phase 4.5 base, folder layout, implementation order, verification |
| [06 — Tables](06-tables.md) | The six overlay tables and eleven PostgreSQL tables |

## Sub-issues

Both start from the Phase 4.5 base and are developed in parallel, each in its own
worktree.

| Issue | Cluster | Acceptance | Planning prompt |
| --- | --- | --- | --- |
| [#302](https://github.com/Ephemeral-AI-Lab/layerfs/issues/302) | 1 — storage: content, storage, history, `layerfs-s3`, `layerfs-metadata`. Fully independent, no sandbox | Meet the earlier baseline for namespace-init speed, and for stride 10/3/1 in speed and storage compression | [prompts/cluster1-storage-plan.md](prompts/cluster1-storage-plan.md) |
| [#303](https://github.com/Ephemeral-AI-Lab/layerfs/issues/303) | 2 — sandbox: overlay, workspace, FUSE, daemon, bridge, sandbox, SDK. Independent until Commit | After integration with cluster 1, complete the seven `fs-bench-pro` families | [prompts/cluster2-sandbox-plan.md](prompts/cluster2-sandbox-plan.md) |

Each implementation plan covers architecture, file and folder structure, rollout,
and final verification and benchmark. It is written by a planning agent from the
prompt and lands in `core/docs/issues/302/` or `core/docs/issues/303/`.

## Decisions

**Accepted (owner, 2026-10-03):** D8, the component split — thirteen crates in
three groups (domain, engines, runtime), with `layerfs-s3`, `layerfs-metadata`
and `layerfs-overlay` each owning one physical store. See
[01 §3](01-architecture.md#3-components).

**Accepted (owner, 2026-10-03):** the global metadata store is PostgreSQL in a
local Docker container, not a shared SQLite file; the environment is one machine
and multi-machine deployment is out of scope. This closes the
former D2 ("who opens the global SQLite file"). See
[04 §3](04-storage-interfaces.md#3-global-metadata-store-postgresql).

**Rule amended (owner, 2026-10-03):** `core/AGENTS.md` no longer forbids WAL or
sync; journaling and synchronization are declared per store. It also records the
PostgreSQL and MinIO environment.

**Accepted (owner, 2026-10-03):** two clusters — storage (`layerfs-content`,
`layerfs-storage`, `layerfs-history`, `layerfs-s3`, `layerfs-metadata`) and
sandbox (the rest) — developed in parallel and joined only at Commit. See
[01 §3](01-architecture.md#3-components).

**Owner direction (2026-10-03), recorded in the #303 plan as two amendments:**

- A daemon keeps a registry of several Workspaces and admits several Execs per
  Workspace; each count has a configurable limit. Mutations stay one at a time
  per Workspace and construction stays on one worker.
- No capacity ceiling on accumulated changes: changed files, names, edits per
  file and bytes per edit have no application-defined count or total-size limit.
  Stored state may grow to the disk quota while resident memory stays bounded by
  windows. The 10,240-write case deferred under #276 is re-opened. Two of the
  ceilings are inside `layerfs-content` and are a requirement on cluster 1
  (contract item C9), not yet sent to #302.

**Open.** Each has a recommendation; none is frozen.

| # | Decision | Recommendation | Where |
| --- | --- | --- | --- |
| D1 | Exact source pin for "Phase 4.5" | `origin/main` `7edddbdb8` (includes Phase B #286) | [05 §1](05-migration-plan.md#1-base) |
| D3 | Overlay payload unit | Fixed block grid, block ≤ the smallest CDC chunk; exact size chosen by a count-driven diagnostic | [02 §8](02-workspace-overlay.md#8-open-parameters) |
| D4 | Overlay SQLite settings | WAL, `synchronous = OFF`, one writer plus read-only connections, explicit bounded checkpoints. The global store uses PostgreSQL's defaults | [02 §10](02-workspace-overlay.md#10-sqlite-settings) |
| D5 | S3 client | Own minimal plain-HTTP client inside `layerfs-s3`: no new dependency, and TLS is not needed for a local MinIO | [04 §7](04-storage-interfaces.md#7-client-dependencies) |
| D6 | Pack size for object storage | Keep the 256 KiB / 16 MiB limits for the first slice; revisit with request-count evidence | [04 §4](04-storage-interfaces.md#4-transfer-units) |
| D7 | Resolving an uncertain outcome | **Required** now that publication crosses a network: permit an exact identity lookup for immutable objects, prohibit anything inferential. Amends the "unknown outcome is a failed result" rule | [03 §5](03-commit-workflow.md#5-outcomes) |
| D9 | After install: keep or drop folded rows | Drop eagerly in the first slice; a local committed-content cache is a later, measured optimization | [02 §6](02-workspace-overlay.md#6-install-and-retire) |
| D10 | Shape of C2's metadata port | A few transactional units (lookup, register batch, read bodies); fixed in M0 by reading `cas/` and `sqlite/` at the base pin | [04 §3](04-storage-interfaces.md#3-global-metadata-store-postgresql) |
| D11 | PostgreSQL client | Decide in M0 after checking candidates' real dependency trees; a new dependency needs explicit approval | [04 §7](04-storage-interfaces.md#7-client-dependencies) |
| D12 | Pooled metadata packs | Seal per Commit instead of keeping one pack open for in-place appends, so every pack is immutable and named by its digest | [04 §9](04-storage-interfaces.md#9-minio-objects-and-bucket-settings) |
| D13 | Home of namespace Init/import | A cluster 1 library that composes C1, C2 and C5 for Init, import and read — the surviving part of `layerfs-server`'s service, with no Commit, conflict or edit knowledge. Needed before M5 | [05 §5](05-migration-plan.md#5-milestones) |

## Milestones

Detail and exit checks: [05 §5](05-migration-plan.md#5-milestones).

Two clusters developed in parallel. Cluster 1 is fully independent; cluster 2
is independent until its last step.

- [ ] **M0** — Contracts: resolve open decisions, freeze ports and both schemas, enforce the dependency table and the cluster rule.

Cluster 1 — storage, #302 (no sandbox, no daemon; no Commit, conflict or edits):

- [ ] **M1** — `layerfs-storage` defines its two ports and routes bodies by domain.
- [ ] **M2** — `layerfs-s3` against real MinIO.
- [ ] **M3** — `layerfs-metadata` against real PostgreSQL.
- [ ] **M4** — Storage parity: existing C2 vectors with payload on MinIO and metadata in PostgreSQL.
- [ ] **M5** — Acceptance without a sandbox: meet the earlier baseline for namespace-init speed, and for the deepseek-harness history schedules at stride 10, 3 and 1 in speed and storage compression.

Cluster 2 — sandbox, #303 (owns the join and everything after it):

- [ ] **M6** — `layerfs-overlay` built together with `layerfs-workspace` operations.
- [ ] **M7** — `layerfs-fuse` rebind and `layerfs-daemon` wiring; mount and Exec on an empty base.
- [ ] **M8** — Integration with cluster 1: committed-base reads and Commit; overlap witness.

- [ ] **M9** — Retire `layerfs-server`, the old embedded stores and the old Workspace code.
- [ ] **M10** — Final verification: all seven `fs-bench-pro` families on the integrated product.

## Nice to have

Not required for Phase 7 and not on the milestone path.

- **Ranged reads from MinIO (#300).** The first reader fetches whole packs.
  Requests cost more than bytes against a local MinIO, and the smallest useful
  range is a compression group, so this is revisited with the M8 measurement. The
  design keeps it possible at no cost now. See
  [04 §10](04-storage-interfaces.md#10-ranged-reads-300--nice-to-have).
- **Keeping folded overlay rows as a local cache of committed content (D9).**

## Not claimed

No speed, memory, scalability or durability result. Phase 6 numbers quoted in
this packet are historical observations from their own source pins and remain
cache-`INELIGIBLE` as recorded in their issues. Statement and row counts given
for the new design are **design targets to be measured**, not measurements.
