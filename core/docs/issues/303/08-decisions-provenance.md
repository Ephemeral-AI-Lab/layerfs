# 08 — Decisions, provenance and open questions

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. Claim labels are defined in the
> [entry point](README.md#claim-labels).

Review revision 2026-10-05: supersedes the algorithms and bounds of design
`334fc743751b9a181e670d0601a24fb3169208f9` where identified below. Product
source remains pinned to `f96d97651`; no implementation or new measurement
accompanies this revision. Required corrections and proof obligations are
tracked in [README](README.md#required-corrections-before-implementation).

Owner update 2026-10-05: one local overlay SQLite database per daemon, initialized
once before readiness; Workspace rows are namespaced within it. Bash Exec has
no automatic runtime timeout. This supersedes the per-Workspace-file proposal;
shared writer/pager/failure accounting and fair admission apply below.

Owner direction 2026-10-07 (serverless direct Store): every Linux daemon opens
the global SQLite Store directly from a volume the daemons share; there is no
host or server adapter in the data path. Project Init stays on the host and
ends with one sealed Store file installed once into that volume; afterwards the
host is control-only (mount, Exec, Commit, status, unmount). No retry, busy
handler or readiness wait is added. Both Store profiles are supported and
development verification stays Disposable. A mounted SQLite volume is the raw
shape of the serverless design; another database may later sit behind the same
ports. This reverses the two 2026-10-03 rows of §2 marked below, revises K2 and
K24, and retires the host-mediated runtime. The simplified contract is
[06](06-cluster-one-integration.md); K28–K33 and O-18–O-21 record it. Nothing
here is implemented or measured by this revision.

## 1. Provenance

Every source this set was reconciled from, where it lives, and its state on
2026-10-05. Uncommitted and unpushed sources are context, not decisions:
committed instructions and owner direction govern.

| Source | Location | Git state | Identity |
| --- | --- | --- | --- |
| Repository rules | `AGENTS.md`, `core/AGENTS.md` | tracked on `main` | `f96d97651` |
| Cluster one handbooks | `cluster_one_handbook.md`, `cas_cdc_deltaencoding_handbook.md` | tracked on `main` | pinned to product `8cbeadef0`; one drift found ([06 §6](06-cluster-one-integration.md#9-prerequisites) P11) |
| Benchmark rules and report layout | `docs/general/benchmark_rules.md`, `benchmark_agent_report.md` | tracked on `main` | `f96d97651` |
| Product source | `core/crates/` | tracked on `main` | `f96d97651` |
| Issue #303, body and 4 comments | GitHub | open; last updated 2026-10-03T18:42:12Z | — |
| Issue #305, body and 15 comments | GitHub | open; last updated 2026-10-04T01:02:39Z | — |
| Issue #306, body and 4 comments | GitHub | open; last updated 2026-10-04T06:41:39Z | — |
| Issues #301, #304 | GitHub | open; no comments | — |
| Issue #302 | GitHub | closed; searched by keyword only | — |
| #301 packet, 7 documents and 2 prompts | `core/docs/issues/301/` in the primary checkout | **untracked** | SHA-256 prefixes: README `59e838a2`, 01 `626fa619`, 02 `b1753b9a`, 03 `0f896445`, 04 `8a6f1bbe`, 05 `9b6451c3`, 06 `a91645ce`, as read before the supersession notices of §8 were added |
| #303 implementation plan | `core/docs/issues/303/IMPLEMENTATION-PLAN.md` in worktree `/Users/yifanxu/.codex/worktrees/phase7-cluster2-sandbox/layerfs` | **untracked**; that worktree's HEAD is `7edddbdb8` | SHA-256 prefix `8b40dac6`; 1,196 lines; not changed by this work |
| #304 study, 6 documents | `core/docs/issues/304/` in the same worktree | **untracked** | SHA-256 prefixes: README `a651a565`, commit `2172f987`, exec `181ea15b`, fuse `dcd95304`, mount `258ea74e`, tough-situations `652e9be3`; not changed |
| #305 reports, contracts and receipts, 65 files | `core/docs/issues/305/` on branch `codex/phase7-experiment-305` | committed locally, **unpushed** | branch head `1451b68a7` |
| #306 plan, reports and receipts, 8 files | `core/docs/issues/306/` on the same branch | committed locally, unpushed | `1451b68a7` |
| #305 experiment source | `core/experiment/real-tree/src/` on the same branch | committed locally, unpushed | `1451b68a7` |

Three reviews fed this set, each read-only: a limitation and design audit, a
FUSE and mutation-performance review, and a SQLite and cluster one integration
review. Where they disagreed, §6 records the resolution.

## 2. Superseded assumptions

| Assumption in the prepared documents | Status | What supersedes it |
| --- | --- | --- |
| Global storage is PostgreSQL plus MinIO | **superseded** | Owner direction on #302, 2026-10-03; the provider is host-local SQLite in `layerfs-persistence` |
| Crates `layerfs-s3` and `layerfs-metadata` | **superseded** | Removed (`da0fa3a12`); `layerfs-persistence` and `layerfs-project` exist instead |
| "Any daemon can construct, upload and register; immutability removes the coordinator" | **superseded 2026-10-03; that supersession reversed 2026-10-07** | Every daemon opens the shared Store and constructs, saves and publishes in-process (K28). Coordination is the database: one short write transaction at a time and conditional history transitions, not a coordinator process |
| "Only `layerfs-daemon` wires concrete engines" | **superseded 2026-10-03; that supersession reversed 2026-10-07** | The daemon opens the Store on Linux (K28, K29); the host wires Persistence only for Init |
| `layerfs-bridge` shrinks to control only | **replaced 2026-10-03; restored 2026-10-07** | The bridge carries control only; object reads, Save and history calls leave the wire (K31) |
| One host-local Store with one writable owner; opens only on macOS | **superseded 2026-10-07** | One Store file on a volume shared by the daemons, several writer processes, no host in the data path (K28–K30) |
| The engine cannot be opened in the sandbox | **superseded 2026-10-07** | Persistence opens on Linux; the Store path is reachable by the daemon and hidden from Bash (K28, K32) |
| Publication is one conditional transaction; the stage table is removed | **superseded** | `stage_changes` then `commit_staged`; one stage row per Workspace; a conflict keeps the stage |
| Uncertain upload settled by `HEAD` on a pack digest | **superseded** | No object store; no pack-by-digest endpoint |
| "Bytes before references" as a caller obligation across two services | **superseded** | One SQLite publication transaction inside cluster one |
| The host starts PostgreSQL and MinIO containers; the daemon holds their credentials | **deleted** | No such services |
| Families 1–2 are covered by cluster one's milestone M5 and reused by identity keyed on the removed crates | **superseded** | #302 closed without that milestone; reuse follows root `AGENTS.md` §3.4 |
| Base `7edddbdb8`; LOC base "core 70,279" | **replaced** | `main` `f96d97651`; core 74,000 |
| The old crates build in the workspace and can be renamed with an alias | **replaced** | They are excluded and have no lock entries |
| A daemon dies with "frozen persisted", then lookup and install | **superseded** by the plan's own finding and by K3 | A Workspace does not survive its daemon (O-3) |

## 3. Decisions of this design

Each is [proposed design]. Revised/withdrawn entries supersede 334fc7437.
Unrevised historical rationale is retained as provenance, not evidence that
R1–R8 bounds are implemented. "Reverses" names a prepared item marked decided.

| # | Decision | Evidence that decided it | Reverses |
| --- | --- | --- | --- |
| K1 | **Owner update:** one overlay SQLite database/connection per daemon, initialized once before readiness | Fast logical Workspace open; Workspace-prefixed indexed metadata/payload/scratch; fair shared writer | Per-Workspace-file isolation proposal withdrawn; DB/pager/failure domain shared |
| K2 | **Revised 2026-10-07:** Content construction, logical base reads, Save and history publication all run in the daemon over a directly opened Store (K28) | Owner direction; Workspace ports are already backend-neutral | The 2026-10-05 split "host semantic admission/storage/history"; the hosting rule O-1 for the global Store |
| K3 | MEMORY/OFF for disposable overlay, conditional on crash policy | No WAL checkpoint; journal/dirty-page and residency costs still require bounds | WAL/recovery alternative remains O-3 |
| K4 | One short SQL transaction per mutating request; reads are unframed statements under the mutex | The owner's stated path; a failed batch commit would lose acknowledged operations | Nothing for writes. Review C's proposal to batch several requests per transaction is not adopted |
| K5 | **Revised:** bounded payload update representation with no base-payload copy-up | Immutable extent boundaries failed the fragmentation counterexample; [02 §5](02-base-overlay.md#5-payload-replacement-required) requires a replacement algorithm | Original byte-exact extent algorithm withdrawn |
| K6 | Payload rows carry a stream number, not an inode and generation | Truncate to zero, relabel and unlinked-file retention then never rewrite payload | — |
| K7 | **Revised:** lookup-leading primary keys plus generation-selective cursors/indexes | Two versions per key did not bound disjoint active keys or scan EOF | Original no-secondary-index/factor-of-two argument withdrawn |
| K8 | **Replacement required:** short failed-capture resolution plus bounded consolidation | Foreground payload merge pauses the hot inode; bounded retention/progress must be derived | Synchronous failed-Commit fold withdrawn |
| K9 | **Replacement required:** orphan owner independent of namespace capture | Successive pins accumulated O(Commit count) versions for one descriptor | Capture pin-until-close rule withdrawn |
| K10 | Five tables; custody and allocators in memory | The database dies with the daemon | Six tables with `handle` and `xattr` (#301 packet 06); agrees with the plan's four plus `reclaim` |
| K11 | The owner-promoted mount profile, made correct by the coherence invariant; the per-WRITE invalidation is removed | [05 §3–§4](05-fuse-assessment.md#3-target-mount-profile) | "The mount keeps direct I/O" (#303 planning prompt), already reversed by the owner on #305 |
| K12 | Stat identity is an invariant; `ctime = mtime` | E18 request counts; cluster one stores no ctime | — |
| K13 | **Revised:** fair runnable admission, deferred inode/resource waiters | Two blocked request threads stalled unrelated files; no contention EBUSY remains a requirement | Blocking Condvar admission withdrawn |
| K14 | A second Commit request is refused at once with a typed result | One stage per Workspace; the two-version bound | Retained from the prepared design |
| K15 | Save finish, stage and transition are three bridge calls; a conflict discards the stage by exact token | The current history API | "One publication call with five outcomes" (plan contract C5) |
| K16 | `Uncertain` defines no resolution | `core/AGENTS.md` and the handbook grant none | The plan assumed its own rule text (Q1) was in force |
| K17 | **Revised:** embedded cluster-one runtime with starvation-free per-Workspace/service-class queues and demand transport capacity | Strict reads-first starved Saves; whole-Save checkouts exhausted upstream capacity | Reads-before-writes and serial whole-Commit fallback withdrawn |
| K18 | **Revised:** bounded maintenance, reserved headroom, actual allocation accounting | Pre-reply quota loops and inferred garbage pages did not establish bounds | Mutation-triggered reclamation and repeated preflight cleanup withdrawn |
| K19 | Unsupported format semantics refused at mutation; R3 processing limits removed through integration | Format refusals do not authorize artificial edit/Commit caps | — |
| K20 | Pinned SDK views are not designed here | They change the version bound; no owner answer exists | — (O-10) |
| K21 | Ordinary Bash Exec with no automatic runtime timeout, explicit lifecycle and bounded streaming output | Owner direction 2026-10-05; [01 §9](01-architecture.md#9-exec-is-an-ordinary-shell-process) | Dormant sh/30 s/8 KiB/status-polling wrapper is not the target contract |
| K22 | Terminal unmount includes logical close and automatic cleanup; no required public close | Owner direction 2026-10-05; [04 §10](04-concurrency-commit.md#10-second-commit-and-terminal-unmount) | Separate detach-only unmount and close lifecycle withdrawn |
| K23 | Complete filesystem at one-call minimum granularity; long-lived multi-call Workspaces with incremental Commits supported | Owner clarification: lifetime independent of call/task; ignored files/dependencies/caches/output ready; Commit advances base and preserves later changes | Source-only/filter/reinstall projections and mandatory one-call teardown rejected |
| K24 | **Revised 2026-10-07:** `layerfs-server` stays retired and no host runtime replaces it; cluster-one libraries are embedded by the daemon, and by the host only for Init | Owner direction: no host/server adapter in the data path | "Embed cluster-one runtime through public adapters" on the host; the SDK runtime, client and Bridge data codec built under #307 are retired (K31) |
| K25 | Capture retains existing rows; later active mutations and operation scratch create separate required state | Discussion clarification; [Commit §5.1](workspace-api/commit.md#51-existing-captured-rows-new-active-rows-and-scratch) | No bulk overlay snapshot copy or premature captured deletion |
| K26 | Automatic bounded SQL row deletion after logical retirement, including idle periods | [engine §6.1](daemon-sqlite.md#61-automatic-batched-sql-deletion) | No manual cleanup, intentional TTL, shared-table DROP or implied file shrink; throughput still unqualified |
| K27 | Both per-tool-call and per-task modes; per-tool-call expected commonly; Exec duration independent of either | Owner clarification: commands may be short or long-lived; multi-call reuse and incremental Commit supported | Presumed short Exec, one-call ownership limit and automatic lifecycle by command class rejected |
| K28 | **Owner direction 2026-10-07:** every Linux daemon opens the global SQLite Store file directly from a volume the daemons share and performs reads, Save and history in-process | "Daemon directly communicates to db rather than a host/server adapter then db"; a mounted SQLite volume is the raw shape of the serverless design and another database may later sit behind the same Storage/History ports | One host-local Store with one writable owner; macOS-only open; host-mediated object/Save/history transport |
| K29 | Persistence opens on Linux for both profiles; development verification stays Disposable | Owner: "support both profiles, but for testing purpose, focus on disposable". The profile definitions for a file shared by several processes are proposed in [06 §3](06-cluster-one-integration.md#3-the-shared-store) and need the owner's ruling O-21 | macOS gates and the macOS allocation owner in the daemon path |
| K30 | No retry: one write transaction is attempted once; a contended Store returns one exact before-effect `Busy` refusal and the caller's state is retained | Owner: "keep it simple, we need no retry". No busy handler, timeout, readiness wait or writer gate is added | The side review's proposed bounded wait before a write |
| K31 | The host is control-only after install: mount, Exec, Commit, status, unmount. SDK `client/` and `runtime/`, the Bridge data codec and daemon `upstream/` are retired; R1 application assembly, R3 restart custody and R4 remote Save are not built | Owner direction; they exist only to carry the data path across a process boundary that no longer exists | #307 S9 rows R1, R3, R4 as scoped on 2026-10-07 |
| K32 | The Store volume is reachable by the daemon and not by Bash run inside a Workspace | Owner direction; mechanism in [06 §6](06-cluster-one-integration.md#6-store-visibility) | — |
| K33 | Project Init stays on the host and ends with one sealed Store file; install copies it once into the shared volume. No collector runs under several writers | Owner direction; a sealed file has no sidecar and is safe to copy | Host-held writable Store after Init |

## 4. Disposition of prepared items

Status: **retain**, **replace**, **delete**, **superseded**, **unresolved**.
Sources: "#303" the issue; "plan" the untracked implementation plan; "packet"
the #301 documents; "#304" the study.

### 4.1 Model and schema

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| A daemon-owned SQLite overlay holding names, inodes and payload; no private backing files | #303 goal | **retain** | Owner requirement |
| Generation-keyed rows; short capture/install | packet 02 §2–§4 | **retain with R2/R4 ownership corrections** | [02 §4](02-base-overlay.md#4-generations-and-visibility) |
| Within a generation only latest visible bytes remain | packet README | **retain semantics; replace algorithm** | R1 bounds fragmentation and binary tail growth |
| Parent-serial plus name directory rows; a directory rename writes two rows | packet 02 §5 | **retain** | Depends on `resolve_child` and `list_inode`, which exist |
| `inherit_len` | packet 02 §5 | **retain**, renamed `lower_len` | — |
| No opaque marker; a recreated directory gets a new serial | packet 02 §5 | **retain** | — |
| A removal always writes a whiteout | plan §0.5 | **replace** | A name born and removed in one generation leaves no row (`below`) |
| One database per daemon, `ws`-prefixed keys | planning prompt and owner clarification | **retain / restored** | K1 owner update; one startup connection/schema, fair shared writer |
| Fixed block grid, 4 KiB blocks on 8 KiB pages | plan D3 | **replace** | K5; by file-format arithmetic that pair fits one block per leaf |
| Extents "kept as the alternative" | packet 02 §8 | **retain**, promoted | K5 |
| Six tables; `handle` and `xattr` tables | packet 06 | **replace** | K10 |
| Shrink deletes rows before reply | packet 02 §5 | **replace** for nonzero shrink; zero stream swap retained | R1 logical cutoff |
| Single retire floor | plan §0.3 | **replace** | K9 |
| A failed Commit adds a layer | packet 02 §6; plan §0.3 | **replace; algorithm required** | R4 must bound composition without foreground payload merge |
| Drop folded rows after install (D9) | plan | **retain** | The kernel page cache softens the re-read |
| Storage quota as a page budget | packet 02 §8 | **revise** | max_page_count is daemon-wide; logical per-Workspace admission and global physical reserves |
| WAL; writer plus readers; explicit `PASSIVE` checkpoints; `mmap_size = 0`; bundled SQLite | packet 02 §10 | **replace** except bundled and `mmap_size` | K3 |
| "WAL so a killed daemon leaves a consistent database" | packet 02 §10 | **unresolved** | O-3 |

### 4.2 Paths, concurrency and Commit

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| Commands are never paused for a Commit; no transaction spans construction, transport or Exec | planning prompt | **retain** | Owner requirement |
| One pending Commit per Workspace; one construction worker | planning prompt | **retain** | Owner requirement; root `AGENTS.md` §3.8 |
| One mutation at a time per Workspace; one transaction per request; reply after commit | plan §0.4; #304 | **retain** | K4. The cost objection came from the prototype's framing of reads, which is removed |
| Every Workspace shares the overlay writer/pager | plan and owner clarification | **retain with fair bounded scheduling** | Shared physical/database failure and admission accounting |
| Copy-up pre-read outside the transaction, re-checked inside | plan §0.4 | **delete** | No write reads the base |
| Kernel invalidation kept, one per mutation | plan §0.5 | **delete** | [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |
| Serial range reserved once and used locally | plan F12 | **retain**, with background refill | — |
| Several Workspaces and Execs with configured limits, refused not queued beyond the limit | plan amendment 1 | **retain** | Recorded only in untracked files; consistent with the owner's requirements here |
| No capacity ceiling on accumulated changes | plan amendment 2 | **retain** | Same |
| Kept "windows": 128 handles, 32 views, 4 GiB per file | plan Q12 | **replace** (handles), **unresolved** (views, O-10); file cap removal already required | — |
| Commit phases admit, capture, construct, publish, install, retire | packet 03 §2 | **retain**, with stage and transition | [04 §5](04-concurrency-commit.md#5-save-stage-transition-install-retire) |
| Four outcomes | packet 03 §5 | **replace** | [04 §6](04-concurrency-commit.md#6-outcomes) distinguishes where an answer was lost |
| Resolve Uncertain by `CommitId` lookup (D7) | plan | **unresolved** | O-4. The stage token gives a stricter exact read |
| Overlap witness at the port level | plan §3.1 | **retain** | [07 §5.2](07-implementation-validation.md#52-acceptance-and-adversarial-proofs) |
| End-to-end gate by pausing the MinIO container | plan §3.1 | **delete** | — |
| View model with pinned SDK leases | plan §0.3 | **unresolved** | O-10 |

### 4.3 Components, integration and rollout

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| Crate split: `layerfs-overlay` knows SQL only; `layerfs-workspace` knows no SQL; `layerfs-fuse` knows no storage | planning prompt | **retain** | [01 §3](01-architecture.md#3-ownership-execution-location-database-and-transport) |
| No `LowerFilesystem` trait; the base is read with `layerfs-content` over `AuthenticatedObjects` | plan §0.2 | **retain** | The provider behind the trait is now a bridge client |
| An empty base is a real cluster one root | plan | **retain** | — |
| Contract C1 object reads, C3 serial reservation | plan §6 | **retain**, restated | [06 §2](06-cluster-one-integration.md#2-cluster-one-apis-the-daemon-calls) |
| Contract C2 history over PostgreSQL; C7 composition in `engines.rs`; C8 receipt reuse keyed on removed crates | plan §6 | **superseded** | — |
| Contract C4 Save session with Refused/Uncertain classes and a resolver | plan §6 | **replace** | `begin_save`, `accept`, `finish`, `take_failure`; no resolver exists |
| Contract C5 one publication call; C6 a conclusive `NotPublished` | plan §6 | **replace** / **unresolved** | K15; O-4 |
| Contract C9 cluster one stops refusing on accumulated change | plan amendment | **unresolved** | Never sent; #302 closed ([06 §6](06-cluster-one-integration.md#9-prerequisites) P3, P7) |
| layerfs-server retired | Owner clarification / #303 done-when | **retain** | No revival/rename; host application embeds current cluster-one libraries and bounded runtime adapters |
| Bridge payload and history contracts deleted | #303 comment 3 | **replace** | Construction contracts deleted; history contracts kept |
| Rename the old crate to `-legacy`, delete it at the end | plan S1 | **retain** | As a relocation of a dormant crate |
| Rollout S0–S14 | plan §2 | **replace** | [07 §3](07-implementation-validation.md#3-slices) |
| Trust boundary: non-root command identity, cleared environment | plan §0.8 | **retain** | There is no boundary to carry over; it is new work |
| Size estimate 43,165 → 15,450–20,950 | #303 comment 3 | **replace** | [07 §4.3](07-implementation-validation.md#43-estimated-future-size) |
| Acceptance: the seven families on the integrated product | #303 | **retain** | Owner requirement |
| Deliverables of the #304 study (complexity, statements, transactions) | #304 | **replace** | Covered by [03](03-mutation-hot-path.md) as targets; the counts come from slice S7 |
| Ideas: scratch paths outside the Workspace | #304 tough-situations | **delete** | The owner requires a full Workspace |
| Ideas: kernel caching with invalidation, negative entries, readdirplus, `fsync` as a no-op | #304 | see [05 §7](05-fuse-assessment.md#7-optimization-disposition) | — |

## 5. Contradictions found and how each is resolved

| # | Contradiction | Resolution |
| --- | --- | --- |
| 1 | "Maximum edit count per file" names three mechanisms: a removed `MAX_EDITS_PER_FILE = 4,096` and a live `MAX_PIECES_PER_FILE = 8,193` in the root reference engine, and an emergent budget cap in Phase 4.5 | All three are in the inventory ([02 §10](02-base-overlay.md#10-limitation-inventory)); none has a successor |
| 2 | "Checkpoint on every change" has no literal match in Phase 4.5 source | Two mechanisms are identified: the per-mutation page-file revision on `main`, and the per-write WAL check with inline `PASSIVE` checkpoint in the #305 prototype. Both are removed |
| 3 | Documents say PostgreSQL and MinIO; `main` has host-local SQLite | §2 |
| 4 | "Bridge control only" against a Store that only the host can open | K2 |
| 5 | The committed hosting rule forbids Docker-owned SQLite; the owner requires a daemon-owned overlay | **Not resolvable here.** O-1 |
| 6 | One transaction per mutation against "extremely fast" | K4 retains acknowledgement atomicity; R1/R5/R6 and sustained counts must establish cost. Prototype counts do not prove throughput |
| 7 | Six tables or four; custody persisted or in memory | K10 |
| 8 | Does a Workspace survive a daemon restart? | Designed for "no" (K3) with a stated alternative; O-3 |
| 9 | Earlier core agent rules named PostgreSQL/MinIO and blanket MEMORY/OFF despite the current global Store profiles | Resolved 2026-10-05: refreshed root/core rules distinguish global Durable/Disposable SQLite, daemon overlay proposal and unsupported backends; no product change |
| 10 | Uncertain-outcome lookup assumed by the plan; not granted by any committed rule | K16; O-4 |
| 11 | The two owner amendments exist only in untracked files | Treated as owner requirements because the task brief states them; they should be recorded on #303 |
| 12 | "Remove artificial limits" against kept windows (4 GiB, 32 views, 128 handles) | Handle/file-cap removal required; SDK views remain O-10 |
| 13 | Today's daemon contradicts multi-Workspace, multi-Exec and activity during Commit | K13 |
| 14 | "Phase 7 keeps the trust boundary" against a base that has none | New work in S8; O-8 |
| 15 | Rollout mechanics assume buildable legacy crates | [07 §2](07-implementation-validation.md#2-build-structure) |
| 16 | Families 1–2 "covered by cluster one M5" | [07 §5.4](07-implementation-validation.md#54-qualification) |
| 17 | Background compaction proposed in the packet, foreground fold at 334fc7437 | R4: foreground payload fold withdrawn; bounded replacement algorithm required |

## 6. Where the reviews disagreed

| Topic | Positions | Resolution |
| --- | --- | --- |
| Several FUSE requests per SQL transaction | The integration review proposed batching with savepoints. The owner's brief specifies a short transaction per mutation | K4: one transaction per mutating request. Batching would let a failed commit lose acknowledged operations |
| Key order | Generation-leading access was originally rejected | Review correction: retain lookup-leading primary keys and add generation-selective access; disjoint active keys disprove the factor-of-two argument |
| Payload unit | Cells with base gap fill were rejected; immutable extents then failed overlap bounds | R1 considers validity-masked cells without base gap fill or bounded normalization; algorithm not yet selected |
| Truncate | Nonzero shrink originally deleted before replying | R1: atomic logical cutoff and background reclaim; regrow/capture ownership proof required |
| Placement | The audit and the FUSE review left it open; the integration review recommended option C | K2, with option A as the stated fallback and O-2 for the owner |
| Negative-entry caching | The FUSE review marked it "adopt with invalidation" | Not in the first slice; first candidate after it. It was measured only with permissions off |
| `MAX_AFFECTED = 128` | The plan called it a limit; the audit showed it is a scan page size | The audit is right; not listed as a limit |

## 7. Questions only the owner can answer

Only product/policy choices require owner input. The no-cap, bounded-memory,
smooth-mutation and concurrent-progress requirements are already given. No
implementation or qualification is claimed by resolving a question. Historical
IDs are kept below so earlier references remain traceable.

| # | Question | Recommendation | Blocks |
| --- | --- | --- | --- |
| O-1 | **Resolved again 2026-10-07:** the global Store, encoding and history are Linux-owned too; every daemon opens the shared Store directly (K28). Host Init remains on macOS | The 2026-10-05 routing kept the global Store on the host. The [hosting scope](../../../../docs/general/benchmark_rules.md#hosting-scope-for-cluster-one-and-cluster-two) text needs the same alignment before a new measurement is admitted; old frozen families retain their topology | Integrated implementation/registration/proof still required; no new measurement admission |
| O-2 | **Superseded 2026-10-07:** placement is construction, Save and history in the daemon (K2 revised) | Option C and its host semantic admission are withdrawn with the wire | — |
| O-3 | Must a Workspace survive a daemon process crash? (no / yes) | No. "Yes" selects the write-ahead alternative and a restart protocol not designed here | S1 |
| O-4 | May exact uncertain-history resolution be added, with completion fencing and coherent authorized reads, no resend/delete on a guess? | Specify policy; two unfenced reads are insufficient | Terminal Uncertain remains until permitted and implemented |
| O-5 | **Resolved 2026-10-07:** both profiles are supported; development verification runs Disposable | The shared-file definition of Disposable is O-21 | — |
| O-6 | After a conflict, what does the product offer: reopen on the new head, commit to a fork, or leave it to the caller? | Leave it to the caller in the first release | — |
| O-7 | May the overlay start writeback and drop clean pages on its own files to bound guest page cache, given that root `AGENTS.md` §4 forbids sync calls on Workspace backing? | — | Target T8 |
| O-8 | Which uid:gid do commands run as, and is it one identity per daemon or one per Workspace? Under a shared identity a command of one Workspace can open another's mount | One per Workspace | S8 |
| O-9 | Is reporting `ctime = mtime` accepted? | Yes | S4 |
| O-10 | Are pinned read-only SDK views kept? Each one is an extra live generation | Defer them | — |
| O-11 | **Resolved by requirement:** remove inherited 4 GiB Workspace file cap | Engineering work; retain platform/resource limits | No additional owner gate |
| O-12 | **Engineering assignment:** allocate P1/P3–P7/P12 follow-up ownership | Required integration scope, not a request to weaken requirements | No additional owner gate |
| O-13 | **Engineering defaults:** start with 2 Workspaces/4 Execs as proposed, expose explicit resource settings | Defaults must pass sustained progress/resource proofs; no qualification claimed | No additional owner gate |
| O-14 | **Resolved:** unmount includes logical close and cleanup; successful unmount discards uncommitted local state, no implicit Commit | Normal Busy/Uncertain preserves state; explicit force handles cancellation/unknown custody | No separate public close |
| O-15 | **Resolved by owner:** a Workspace may serve many sequential/concurrent calls over a long lifetime and Commit incrementally | Same mount and current live view; no automatic teardown on call exit or Commit; lifetime independent of task | Qualify both fast fresh mounts and persistent cache/ownership/reclaim behavior |
| O-16 | For registered selections whose subject is a removed mechanism, is `NOT_RUN — mechanism removed`, shown beside a prospectively registered successor, the accepted disposition? | Yes | S12 |
| O-17 | **Engineering linkage, extended 2026-10-07:** bundled SQLite on Linux for the overlay and now for Persistence too; existing dependency and locked build | Host Init keeps the system SQLite; the schema identity is checked at open. Do not patch dependencies | No additional owner gate |
| O-18 | After install the Store is inside the VM and the host cannot open it. How does the host fork a Branch or read history? | Two more control verbs served by a daemon (fork, history read); nothing reopens the file on the host | Host SDK surface |
| O-19 | Is a second host import into an installed volume supported? | No in the first slice: one sealed file per Project, and install refuses an existing target. A later import needs Init on Linux | Install |
| O-20 | Does control stay on the authenticated native channel (Bridge about 540 lines, keeps `snow`) or move to container stdio (Bridge 0, one `docker exec` per call)? | Keep the native channel: Exec streams output and a mount is long-lived | Bridge remainder |
| O-21 | What is Disposable for a Store shared by several processes? Today it is a memory journal with rollback locking: a daemon killed mid-commit tears the file for every daemon, and readers and the writer block each other | WAL with `synchronous=OFF`: a killed process cannot tear the file and readers never block; only a kernel or VM crash loses the Store. It is a profile-identity change and old Disposable Stores are refused, not converted | Persistence on Linux; every multi-daemon proof |
| O-22 | May the macOS preallocation and extent-release owner (273 lines plus its call sites) be deleted? It assumes one writer, sizes from the main-file length, and its sealed output is copied and discarded | Yes. Strict-allocation selections become `NOT_RUN — mechanism removed` (O-16); Init speed needs one new measurement and the eight historical failures stay as recorded | Persistence cut |
| O-23 | A `Busy` between stage and publish leaves a stage row. Combine stage and publish into one history transaction (a `HistoryCatalog` addition of about 20 lines), or keep two calls and specify the leftover-stage discard? | Combine: one write transaction fewer per Commit and no partial state | Commit path |
| O-24 | Which unprivileged user runs Bash? Hiding the Store requires that it is not the daemon's user (extends O-8) | One per Workspace, as O-8 | Exec confinement, S8 |

**Earlier questions on #303.** None of Q1–Q13 has a recorded answer.

| Earlier | Now |
| --- | --- |
| Q1 | O-4 |
| Q2 | O-1 |
| Q3 | O-3 |
| Q4 | O-7 |
| Q5 | O-10 |
| Q6 | O-16 |
| Q7 | O-17 |
| Q8 (the daemon binary refuses to start between two slices) | Moot: the crates are dormant today |
| Q9, Q11 | O-8 |
| Q10 | O-13 |
| Q12 | O-10, O-11; the handle limit is removed |
| Q13 (send C9 to cluster one) | O-12 |

## 8. Stale prepared documents

- The seven documents of the #301 packet in the primary checkout each received
  a supersession notice under their status banner, pointing here. Their bodies
  are unchanged. They are untracked files and are not part of this branch's
  commits.
- The #303 implementation plan and the #304 study live untracked in another
  worktree and were **not** modified. This set supersedes the plan; the study's
  open deliverables are covered as noted in §4.3.
- The #305 and #306 reports and receipts are evidence and were not touched.

## 9. Review corrections and remaining proof obligations

Three subagents reviewed the owner's seven readiness/load-bearing questions before
writing the primary operation/engine documents. Findings are consolidated in
[README](README.md#pre-write-review-of-the-owners-seven-questions). One-time full
root preparation and demand-mounted per-call readiness differ; ignored files and
symlinks cannot be restored during Exec or omitted from Commit. Phase-4.5 structure
and transport caps must disappear through actual streaming/backing API work.

The new source-qualified docs distinguish current host-local library composition
from future distributed providers and do not restore layerfs-server. Immutable
canonical IDs improve reuse/replication safety; role/reference/authority checks,
mutable history CAS, durability and retention remain load-bearing obligations.

The independent review used design `334fc743751b9a181e670d0601a24fb3169208f9`,
tree `c453cfbf622aaeaace0788fee553811568e7f9df`; product tree remained
`05c00c5d62889ae316bec9ea09dba16e93ba888e`, identical to `f96d97651`.
Root/core rules, both handbooks, all nine documents, complete #303/#305/#306
bodies/comments, relevant product APIs and local experiment evidence were read.
No workload/build or command in the original deepseek-harness checkout was run.

Confirmed design contradictions: immutable extent-boundary fragmentation cost,
active-key capture scan growth, foreground shrink/fold stalls, accumulating orphan
pins, blocking two-worker dispatch, strict-read starvation and pressure loops.
Confirmed source constraints: deferred construction refusal, directory Vec,
new-parent map, sparse zero input, symlink-refusing native Init, global quarantine.
Owner clarification restores one local SQLite per daemon for fast bootstrap,
with Workspace-prefixed state and one shared fair writer/pager. Concurrent SQL
writers and per-Workspace database corruption isolation are not claimed. Exec
has no automatic runtime timeout; the legacy 30-second cap must be removed.

Required proofs: replacement composition/normalization, actual page accounting,
Save lifetime/interleaving, kernel cache ordering, aggregate residency and exact
operation fencing. R1–R8 track these without claiming a completed implementation.

The handbook `.object()` error is corrected to `.0` in this documentation revision.
Reports/receipts and owner-promoted A2 candidate retain their original identities
and limitations. Historical estimates are not updated as if they were measurements.

## 10. Not verified

- Nothing was compiled, run or measured for this set.
- That `layerfs-content` and a bundled SQLite build for
  `aarch64-unknown-linux-musl`.
- Replacement page/index/journal bounds, reservation arithmetic and actual
  reclamation capacity under sustained writes; old illustrative counts withdrawn.
- The replacement BLOB schema/counts. Fixed-length positional writes exist in
  pinned rusqlite; resizing requires SQL/binding and binary tail proofs.
- That two `Storage` values over one provider can run interleaved Saves on one
  thread as the comment on `begin_save` says.
- How `update_filesystem` treats a removed name the base does not bind, and
  whether removing a base inode needs more than unbinding its last name.
- The storage admission code behind prerequisite P1.
- Kernel behaviour quoted from #305 (GETATTR after directory changes, sticky
  `ENOSYS`) is the experiment's counted observation on one kernel, not read
  from kernel source.
- Compact receipt JSON files were not cross-checked cell by cell; numbers come
  from the reports and the issue text.
- Issue #302 was searched by keyword, not read end to end.
- The emergent 8,192 / 10,240 edit threshold is quoted from a Phase B document,
  not reproduced.
