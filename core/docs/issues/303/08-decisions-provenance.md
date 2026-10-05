# 08 — Decisions, provenance and open questions

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. Claim labels are defined in the
> [entry point](README.md#claim-labels).

## 1. Provenance

Every source this set was reconciled from, where it lives, and its state on
2026-10-05. Uncommitted and unpushed sources are context, not decisions:
committed instructions and owner direction govern.

| Source | Location | Git state | Identity |
| --- | --- | --- | --- |
| Repository rules | `AGENTS.md`, `core/AGENTS.md` | tracked on `main` | `f96d97651` |
| Cluster one handbooks | `cluster_one_handbook.md`, `cas_cdc_deltaencoding_handbook.md` | tracked on `main` | pinned to product `8cbeadef0`; one drift found ([06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two) P11) |
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
| "Any daemon can construct, upload and register; immutability removes the coordinator" | **superseded** | One host-local Store with one writable owner; opens only on macOS |
| "Only `layerfs-daemon` wires concrete engines" | **superseded** | The engine cannot be opened in the sandbox |
| `layerfs-bridge` shrinks to control only | **replaced** | It carries object reads, the Save session and history calls ([06 §3](06-cluster-one-integration.md#3-bridge-operations)) |
| Publication is one conditional transaction; the stage table is removed | **superseded** | `stage_changes` then `commit_staged`; one stage row per Workspace; a conflict keeps the stage |
| Uncertain upload settled by `HEAD` on a pack digest | **superseded** | No object store; no pack-by-digest endpoint |
| "Bytes before references" as a caller obligation across two services | **superseded** | One SQLite publication transaction inside cluster one |
| The host starts PostgreSQL and MinIO containers; the daemon holds their credentials | **deleted** | No such services |
| Families 1–2 are covered by cluster one's milestone M5 and reused by identity keyed on the removed crates | **superseded** | #302 closed without that milestone; reuse follows root `AGENTS.md` §3.4 |
| Base `7edddbdb8`; LOC base "core 70,279" | **replaced** | `main` `f96d97651`; core 74,000 |
| The old crates build in the workspace and can be renamed with an alias | **replaced** | They are excluded and have no lock entries |
| A daemon dies with "frozen persisted", then lookup and install | **superseded** by the plan's own finding and by K3 | A Workspace does not survive its daemon (O-3) |

## 3. Decisions of this design

Each is [proposed design]. "Reverses" names a prepared item marked decided.

| # | Decision | Evidence that decided it | Reverses |
| --- | --- | --- | --- |
| K1 | One overlay database file per Workspace | Independent writers, O(1) close, `max_page_count` as the quota, failure confined to one Workspace ([01 §7](01-architecture.md#7-decisions-that-shape-the-architecture)) | "One database per daemon, shared tables keyed by Workspace" (#303 planning prompt) |
| K2 | Content construction and logical base reads in the daemon; storage and history on the host | `layerfs-persistence` opens only on macOS; `apply_edits` needs a replayable source; a cache keyed by `ObjectId` survives base changes | "Construction is local and the engines are wired in the daemon"; also conflicts with the hosting rule (O-1) |
| K3 | `MEMORY` journal, `EXCLUSIVE` locking, `synchronous = OFF`, one connection | No write-ahead log means no checkpoint and one payload write; root `AGENTS.md` §4 forbids sync on Workspace backing; a Workspace is lost with its daemon today | WAL with a writer and reader connections and explicit `PASSIVE` checkpoints (#301 packet 02 §10; plan D4) |
| K4 | One short SQL transaction per mutating request; reads are unframed statements under the mutex | The owner's stated path; a failed batch commit would lose acknowledged operations | Nothing for writes. Review C's proposal to batch several requests per transaction is not adopted |
| K5 | Payload as non-overlapping extents of at most 128 KiB, overwritten in place; no copy-up | The base is behind a bridge call, so a write must not read it | Fixed block grid of at most 8 KiB with copy-up (plan D3) |
| K6 | Payload rows carry a stream number, not an inode and generation | Truncate to zero, relabel and unlinked-file retention then never rewrite payload | — |
| K7 | Keys `(ino, gen)` and `(parent, name, gen)` | The newest row is one seek whatever the generation count | Review C proposed generation-leading keys; see §6 |
| K8 | At most two live generations; a failed Commit folds the captured rows into the active generation before returning | Hard version bound; constant read depth | "A refused Commit clears `frozen_gen` and nothing else; no compaction is built" (plan §0.3) |
| K9 | An open unlinked file retains only its own rows | One deleted-but-open file must not hold back all retirement | The single retire floor (plan §0.3) |
| K10 | Five tables; custody and allocators in memory | The database dies with the daemon | Six tables with `handle` and `xattr` (#301 packet 06); agrees with the plan's four plus `reclaim` |
| K11 | The owner-promoted mount profile, made correct by the coherence invariant; the per-WRITE invalidation is removed | [05 §3–§4](05-fuse-assessment.md#3-target-mount-profile) | "The mount keeps direct I/O" (#303 planning prompt), already reversed by the owner on #305 |
| K12 | Stat identity is an invariant; `ctime = mtime` | E18 request counts; cluster one stores no ctime | — |
| K13 | Mutations wait on the Workspace mutex; no `EBUSY`; no daemon-wide slot | Owner requirements R4–R7 | The product's refusal-based coherence |
| K14 | A second Commit request is refused at once with a typed result | One stage per Workspace; the two-version bound | Retained from the prepared design |
| K15 | Save finish, stage and transition are three bridge calls; a conflict discards the stage by exact token | The current history API | "One publication call with five outcomes" (plan contract C5) |
| K16 | `Uncertain` defines no resolution | `core/AGENTS.md` and the handbook grant none | The plan assumed its own rule text (Q1) was in force |
| K17 | One store owner thread on the host; reads before writes | `try_lock` returns `Busy`; `Storage` is not `Sync` | Contract C1's "one `Send + Sync` value" |
| K18 | Maintenance in bounded steps; a per-Workspace pressure rule; reclamation before `ENOSPC` | [03 §7](03-mutation-hot-path.md#7-maintenance) | Inline checkpointing in the #305 prototype |
| K19 | What cluster one cannot express is refused at the mutation | A Commit must not fail for a reason the command could have been told | — |
| K20 | Pinned SDK views are not designed here | They change the version bound; no owner answer exists | — (O-10) |

## 4. Disposition of prepared items

Status: **retain**, **replace**, **delete**, **superseded**, **unresolved**.
Sources: "#303" the issue; "plan" the untracked implementation plan; "packet"
the #301 documents; "#304" the study.

### 4.1 Model and schema

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| A daemon-owned SQLite overlay holding names, inodes and payload; no private backing files | #303 goal | **retain** | Owner requirement |
| Generation-keyed rows; capture is one statement; install is one statement | packet 02 §2–§4 | **retain** | [02 §4](02-base-overlay.md#4-generations-and-visibility) |
| Within a generation a rewrite replaces in place | packet README | **retain** | In-place BLOB write |
| Parent-serial plus name directory rows; a directory rename writes two rows | packet 02 §5 | **retain** | Depends on `resolve_child` and `list_inode`, which exist |
| `inherit_len` | packet 02 §5 | **retain**, renamed `lower_len` | — |
| No opaque marker; a recreated directory gets a new serial | packet 02 §5 | **retain** | — |
| A removal always writes a whiteout | plan §0.5 | **replace** | A name born and removed in one generation leaves no row (`below`) |
| One database per daemon, `ws`-prefixed keys | planning prompt | **replace** | K1 |
| Fixed block grid, 4 KiB blocks on 8 KiB pages | plan D3 | **replace** | K5; by file-format arithmetic that pair fits one block per leaf |
| Extents "kept as the alternative" | packet 02 §8 | **retain**, promoted | K5 |
| Six tables; `handle` and `xattr` tables | packet 06 | **replace** | K10 |
| Shrink deletes the active generation's rows beyond the new size | packet 02 §5 | **retain** for a non-zero size; **replace** for zero | K6 |
| Single retire floor | plan §0.3 | **replace** | K9 |
| A failed Commit adds a layer | packet 02 §6; plan §0.3 | **replace** | K8 |
| Drop folded rows after install (D9) | plan | **retain** | The kernel page cache softens the re-read |
| Storage quota as a page budget | packet 02 §8 | **retain** | `max_page_count` per file |
| WAL; writer plus readers; explicit `PASSIVE` checkpoints; `mmap_size = 0`; bundled SQLite | packet 02 §10 | **replace** except bundled and `mmap_size` | K3 |
| "WAL so a killed daemon leaves a consistent database" | packet 02 §10 | **unresolved** | O-3 |

### 4.2 Paths, concurrency and Commit

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| Commands are never paused for a Commit; no transaction spans construction, transport or Exec | planning prompt | **retain** | Owner requirement |
| One pending Commit per Workspace; one construction worker | planning prompt | **retain** | Owner requirement; root `AGENTS.md` §3.8 |
| One mutation at a time per Workspace; one transaction per request; reply after commit | plan §0.4; #304 | **retain** | K4. The cost objection came from the prototype's framing of reads, which is removed |
| "Every Workspace shares the one overlay writer" | plan | **replace** | K1 |
| Copy-up pre-read outside the transaction, re-checked inside | plan §0.4 | **delete** | No write reads the base |
| Kernel invalidation kept, one per mutation | plan §0.5 | **delete** | [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |
| Serial range reserved once and used locally | plan F12 | **retain**, with background refill | — |
| Several Workspaces and Execs with configured limits, refused not queued beyond the limit | plan amendment 1 | **retain** | Recorded only in untracked files; consistent with the owner's requirements here |
| No capacity ceiling on accumulated changes | plan amendment 2 | **retain** | Same |
| Kept "windows": 128 handles, 32 views, 4 GiB per file | plan Q12 | **replace** (handles), **unresolved** (views, O-10; file size, O-11) | — |
| Commit phases admit, capture, construct, publish, install, retire | packet 03 §2 | **retain**, with stage and transition | [04 §5](04-concurrency-commit.md#5-save-stage-transition-install-retire) |
| Four outcomes | packet 03 §5 | **replace** | [04 §6](04-concurrency-commit.md#6-outcomes) distinguishes where an answer was lost |
| Resolve Uncertain by `CommitId` lookup (D7) | plan | **unresolved** | O-4. The stage token gives a stricter exact read |
| Overlap witness at the port level | plan §3.1 | **retain** | [07 §5.2](07-implementation-validation.md#52-the-overlap-witness) |
| End-to-end gate by pausing the MinIO container | plan §3.1 | **delete** | — |
| View model with pinned SDK leases | plan §0.3 | **unresolved** | O-10 |

### 4.3 Components, integration and rollout

| Item | Source | Status | Note |
| --- | --- | --- | --- |
| Crate split: `layerfs-overlay` knows SQL only; `layerfs-workspace` knows no SQL; `layerfs-fuse` knows no storage | planning prompt | **retain** | [01 §3](01-architecture.md#3-ownership-execution-location-database-and-transport) |
| No `LowerFilesystem` trait; the base is read with `layerfs-content` over `AuthenticatedObjects` | plan §0.2 | **retain** | The provider behind the trait is now a bridge client |
| An empty base is a real cluster one root | plan | **retain** | — |
| Contract C1 object reads, C3 serial reservation | plan §6 | **retain**, restated | [06 §2](06-cluster-one-integration.md#2-cluster-one-apis-cluster-two-calls) |
| Contract C2 history over PostgreSQL; C7 composition in `engines.rs`; C8 receipt reuse keyed on removed crates | plan §6 | **superseded** | — |
| Contract C4 Save session with Refused/Uncertain classes and a resolver | plan §6 | **replace** | `begin_save`, `accept`, `finish`, `take_failure`; no resolver exists |
| Contract C5 one publication call; C6 a conclusive `NotPublished` | plan §6 | **replace** / **unresolved** | K15; O-4 |
| Contract C9 cluster one stops refusing on accumulated change | plan amendment | **unresolved** | Never sent; #302 closed ([06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two) P3, P7) |
| `layerfs-server` retired | #303 "done when" | **replace** | Rewritten as the store host |
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
| 6 | One transaction per mutation against "extremely fast" | K4. The 1,259,910 transactions of B/E10 were mostly framed reads; the prototype also `stat`ed its log after every write |
| 7 | Six tables or four; custody persisted or in memory | K10 |
| 8 | Does a Workspace survive a daemon restart? | Designed for "no" (K3) with a stated alternative; O-3 |
| 9 | `core/AGENTS.md:156-161` still names PostgreSQL and MinIO and the handbook says otherwise | Reported; the rule text is the owner's to correct |
| 10 | Uncertain-outcome lookup assumed by the plan; not granted by any committed rule | K16; O-4 |
| 11 | The two owner amendments exist only in untracked files | Treated as owner requirements because the task brief states them; they should be recorded on #303 |
| 12 | "Remove artificial limits" against kept windows (4 GiB, 32 views, 128 handles) | Handles removed; O-10, O-11 for the rest |
| 13 | Today's daemon contradicts multi-Workspace, multi-Exec and activity during Commit | K13 |
| 14 | "Phase 7 keeps the trust boundary" against a base that has none | New work in S8; O-8 |
| 15 | Rollout mechanics assume buildable legacy crates | [07 §2](07-implementation-validation.md#2-build-structure) |
| 16 | Families 1–2 "covered by cluster one M5" | [07 §5.4](07-implementation-validation.md#54-qualification) |
| 17 | Background compaction proposed in the packet, "not built" in the plan | K8: fold after failure, in the foreground of the failing Commit |

## 6. Where the reviews disagreed

| Topic | Positions | Resolution |
| --- | --- | --- |
| Several FUSE requests per SQL transaction | The integration review proposed batching with savepoints. The owner's brief specifies a short transaction per mutation | K4: one transaction per mutating request. Batching would let a failed commit lose acknowledged operations |
| Key order | The integration review proposed generation-leading keys so a generation is one key range | K7: `(ino, gen)`. With two live generations the scan cost differs by at most a factor of two, and the newest-row lookup is one seek instead of two |
| Payload unit | The integration review proposed one extent per 128 KiB cell with gap fill from below | K5: several extents, never a gap fill, because gap fill is a base read on the mutation path |
| Truncate | The integration review proposed range tombstones with sequence stamps on every block row | K6: a stream swap for truncate to zero; a bounded synchronous delete for a non-zero shrink |
| Placement | The audit and the FUSE review left it open; the integration review recommended option C | K2, with option A as the stated fallback and O-2 for the owner |
| Negative-entry caching | The FUSE review marked it "adopt with invalidation" | Not in the first slice; first candidate after it. It was measured only with permissions off |
| `MAX_AFFECTED = 128` | The plan called it a limit; the audit showed it is a scan page size | The audit is right; not listed as a limit |

## 7. Questions only the owner can answer

Each can be answered in one line. "Blocks" names the slice that cannot start
without the answer.

| # | Question | Recommendation | Blocks |
| --- | --- | --- | --- |
| O-1 | Amend the permanent hosting rule (`docs/general/benchmark_rules.md:14-20`) so that (a) the daemon-owned overlay SQLite and (b) canonical construction by `layerfs-content` may run in the sandbox container, with the Store, encoding and history on the host? (both / (a) only / no) | Both. "(a) only" selects placement option A | S12; with "(a) only", S0 |
| O-2 | Placement: option C (construct in the daemon; the host validates and stores) or option A (the host constructs from raw changes)? | C, conditional on prerequisites P1 and P2 | S0 |
| O-3 | Must a Workspace survive a daemon process crash? (no / yes) | No. "Yes" selects the write-ahead alternative and a restart protocol not designed here | S1 |
| O-4 | May an unknown `commit_staged` outcome be settled by exact reads (`stage(workspace)`, then `commit(id)`), with nothing resent and nothing deleted? If so, with what rule text in `core/AGENTS.md`? | Yes | S10 handles `Uncertain` as terminal without it |
| O-5 | Does the integrated global Store run Durable or Disposable? | — (it decides whether parallel read handles are possible on the host) | S9 tuning only |
| O-6 | After a conflict, what does the product offer: reopen on the new head, commit to a fork, or leave it to the caller? | Leave it to the caller in the first release | — |
| O-7 | May the overlay start writeback and drop clean pages on its own files to bound guest page cache, given that root `AGENTS.md` §4 forbids sync calls on Workspace backing? | — | Target T8 |
| O-8 | Which uid:gid do commands run as, and is it one identity per daemon or one per Workspace? Under a shared identity a command of one Workspace can open another's mount | One per Workspace | S8 |
| O-9 | Is reporting `ctime = mtime` accepted? | Yes | S4 |
| O-10 | Are pinned read-only SDK views kept? Each one is an extra live generation | Defer them | — |
| O-11 | Is the 4 GiB per-file contract removed on the Workspace path? | Yes | S5 |
| O-12 | Who owns the cluster one prerequisites P1 and P3–P7, now that #302 is closed? | Open a cluster one follow-up issue | S0 |
| O-13 | Default values for `max_workspaces` and `max_execs`? | 2 and 4, the values Stage C of #305 planned | S8 |
| O-14 | Does a close with uncommitted changes discard or refuse? | Refuse unless forced | S8 |
| O-15 | May a mount outlive one tool call? | — (it decides how much the kernel caches and lifecycle shortcuts are worth) | — |
| O-16 | For registered selections whose subject is a removed mechanism, is `NOT_RUN — mechanism removed`, shown beside a prospectively registered successor, the accepted disposition? | Yes | S12 |
| O-17 | Approve `rusqlite`'s `bundled` feature for the Linux daemon only? | Yes | S1 |

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

## 9. Not verified

- Nothing was compiled, run or measured for this set.
- That `layerfs-content` and a bundled SQLite build for
  `aarch64-unknown-linux-musl`.
- Every SQLite page, overflow and journal statement: file-format arithmetic and
  documented behaviour, to be confirmed by the count diagnostic of slice S7.
- `rusqlite` 0.40.2's positional BLOB API as used for in-place writes.
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
