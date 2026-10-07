# S8 mechanisms and evidence ledger

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-08 on local `main` at `32d969776` (product source pin
> `f0797c646`). Classifies what is implemented, what is proved and at which
> scope, what S8 must build, and which optimizations are only candidates. It
> selects nothing by itself: the [S8 specification](S8-SPECIFICATION-20261008.md)
> owns every decision. No build, test, mount or measurement was run.

A specification, a source count, a prototype number or a component cache hit
cannot certify mounted speed. Nothing below promises a win over native ext4 or
over a passthrough adapter. Candidates are ranked by the strength of their
evidence, their cost and their correctness risk, and each names the count that
would falsify it before any timing is taken.

Review correction after `77cf51686`, 2026-10-08: follow the revised
[specification](S8-SPECIFICATION-20261008.md) and
[correction ledger](checks/s8-spec-review-fixes-20261008/02-correction-ledger.md).
Original reviews and receipts remain unchanged. These are corrected proposals
and prospective oracles, not new implementation or runtime evidence.

Owner supersession 2026-10-08, R0 at `1a6bb53ef`: SDK organization is
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or the external
executor owns commands, streams, exit status and explicit cancellation. The
filesystem daemon has no Exec supervisor, launcher mode, per-Exec cgroups,
command registration or custom Exec wire. Filesystem admission/capture and
complete drain require their own exact owners. The
[R0 reconciliation and withdrawal ledger](checks/r0-owner-reconciliation-20261008/03-owner-and-proof-ledger.md)
owns the prospective disposition; historical source pins, receipts and verdicts
stay unchanged. The [R0–R9 rollout](ROLLOUT-LEDGER-20261008.md) is the current
implementation assignment, superseding narrower old checkpoint dispatches.

## 1. Labels

| Label | Meaning |
| --- | --- |
| IMPLEMENTED | Read in active `core/crates/*/src` at the pin |
| PROVED (scope) | A retained receipt exists; the scope in brackets is all it covers |
| MANDATORY | Required by an owner requirement or an existing contract; S8 is incomplete without it |
| CANDIDATE | Optional. Not an S8 gate. Enters only through its own gate row |
| S10 | Depends on live namespace normalization |
| REJECTED | Not pursued, with the reason |

Evidence strength for candidates: **source** (the cost is visible in active
source and countable), **count** (a retained request or statement count on a
different system), **weak** (one confounded observation), **none**.

## 2. Implemented and proved foundations

| Foundation | Status | Evidence | What it does not establish |
| --- | --- | --- | --- |
| One Store per daemon: one writer, fixed readers (default four), one 8 MiB immutable cache, alive across mounts and zero-mounted intervals | IMPLEMENTED | [open.rs](../../../crates/layerfs-daemon/src/store/open.rs), [bootstrap.rs](../../../crates/layerfs-daemon/src/bootstrap.rs) | Fair reader admission; reader health handling |
| Bounded root bind: fresh cache 10/11/12 object batches for 1/1024/100000 files; warm repeat zero object demands with the history snapshot and local Open still paid | PROVED (logical bind, functional counts) | [F13](PRE-S8-F13-20261007.md), [F15](PRE-S8-F15-20261007.md), [completion map](PRE-S8-COMPLETION-20261007.md) | Kernel attach cost; a mounted warm call |
| One overlay database per daemon, Workspace-prefixed, one fair SQL owner, bounded jobs, automatic bounded reclamation | IMPLEMENTED; PROVED (32 finite arrivals × 6 classes × 4 Workspaces) | [F10 receipts](PRE-S8-COMPLETION-20261007.md), [queue.rs](../../../crates/layerfs-daemon/src/overlay/queue.rs) | Fairness under sustained unequal job weights; debt-coupled admission |
| Atomic namespace and byte mutations as one owner job with fact rounds; bounded 128 KiB windows; publication tickets released by reply attempt | IMPLEMENTED | [driver.rs](../../../crates/layerfs-workspace/src/mutation/driver.rs), [types.rs](../../../crates/layerfs-workspace/src/operations/types.rs) | Their cost through a kernel mount |
| Independent open, read, captured-reader and operation custody; orphan domain independent of capture depth | IMPLEMENTED (S6) | [S6 lifetime contract](S6-LIFETIME-CONTRACT.md), [S6 audit](S6-EXIT-AUDIT.md) | The native FORGET/RELEASE mapping; group retirement at detach |
| Authenticated control: mount binding, Store-half Commit, status, logical unmount, fork, history; exact lost-reply custody; status with zero Store SQL | PROVED (control, no Exec, no kernel) | [F13](PRE-S8-F13-20261007.md), [native control](../../architecture/68-native-workspace-control.md) | Native readiness. `Bound` is not `Ready` |
| Store half of Commit with counted reservations and overwrite-only publication | PROVED (direct Content producer) | [F8 composition](PRE-S8-F8-COMPOSITION-20261007.md), [overwrite decision](BRANCH-OVERWRITE-DECISION-20261007.md), [reservation decision](SAVE-RESERVATION-DECISION-20261007.md) | A live normalizer (S10) |
| Complete installed roots: symlinks, hard links, 100000 names, 500000000-byte dense and 1000000019-byte sparse oracles | PROVED (direct ports, within the size waiver) | [F11](PRE-S8-F11-20261007.md) | The historical full fixture; files above 4 GiB (`NOT_RUN — waived by owner`) |
| Source-bounded working ownership and exact terminal release over namespace growth, 4/32/256 MiB Saves and 64 Commits | PROVED (14 selections, diagnostic memory only) | [resource growth](PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md) | Continuous peaks; exclusive physical attribution; numerical acceptance (owner-deferred) |
| Stable inode serial as node identity; serials never recycled | IMPLEMENTED | [ports.rs](../../../crates/layerfs-daemon/src/store/ports.rs), [serials.rs](../../../crates/layerfs-workspace/src/workspace/serials.rs) | Identity across a live install (S10) |
| Signed-timestamp decode correction in the pinned fuser | PROVED (Docker; five PASS) with one retained mounted platform FAIL | [patch record](FUSER-REGISTRY-PATCH-20261006.md), [Docker ruling](LINUX-TIMESTAMP-DOCKER-20261006.md) | Any S8 product path: the crate is `patch.unused` in the active lockfile |
| macOS seal releases unused extents at sole-owner close | PROVED (host, stride 1 only) | [seal record](SEAL-ALLOCATION-STRIDE1-20261007.md) | Daemon allocation or any S8 speed. A live daemon never seals |

Costs visible in active source that the foundations leave in place. These are
facts about the pin, not measured penalties.

| Cost | Where |
| --- | --- |
| A cache hit clones the object bytes under the cache mutex; an insert allocates, copies and frees evicted entries under it, for a whole batch in one hold | [cache.rs](../../../crates/layerfs-workspace/src/base/cache.rs), [client.rs](../../../crates/layerfs-workspace/src/base/client.rs) |
| Every base operation re-reads and decodes the root object and its tree path from canonical bytes | `BaseView::reader()` in [view.rs](../../../crates/layerfs-workspace/src/base/view.rs) |
| Every file-length demand reaches a Store reader; lengths are not cached above Storage | [ports.rs](../../../crates/layerfs-daemon/src/store/ports.rs) |
| A reader is chosen by a blind counter and then a blocking mutex; a quarantined reader stays in rotation | [open.rs](../../../crates/layerfs-daemon/src/store/open.rs) |
| Identical concurrent misses each acquire | [client.rs](../../../crates/layerfs-workspace/src/base/client.rs) |
| `SourceView::lookup`/`stat` compose one answer from up to five independent owner jobs | [view.rs](../../../crates/layerfs-workspace/src/workspace/view.rs) |
| Every port blocks its caller in `Pending::wait` | [completion.rs](../../../crates/layerfs-daemon/src/service/completion.rs) |
| A mutation is followed by a second owner job that releases its publication ticket | `ReplyAttempted` in [commands.rs](../../../crates/layerfs-daemon/src/overlay/commands.rs) |
| An unreleased lease, request or base-source row keeps a closed namespace `Held`; no reclaim phase deletes those tables | [close.rs](../../../crates/layerfs-overlay/src/lifetime/close.rs), [reclaim.rs](../../../crates/layerfs-overlay/src/maintenance/reclaim.rs) |
| A demand whose hits and fetched results together exceed the 32 MiB window fails after the fetch, and nothing fetched is retained, so an identical later demand pays again | [client.rs](../../../crates/layerfs-workspace/src/base/client.rs) |
| The Persistence session answers in-process contention with `Busy` | `try_lock` in [transaction.rs](../../../crates/layerfs-persistence/src/backend/sqlite/transaction.rs) |

## 3. Mandatory S8 mechanisms

Each row is contract. "Spec" names the owning section; "Proof" names rows of the
[proof plan](S8-PROOF-PLAN-20261008.md).

| ID | Mechanism | Required by | Spec | Proof |
| --- | --- | --- | --- | --- |
| M-1 | Native attach with exact readiness, first-party mount and unmount, negotiation receipt | [mount contract](../303/workspace-api/mount.md), I-3 | [§4](S8-SPECIFICATION-20261008.md#4-control-operations-and-acknowledgement-points), [§8.1](S8-SPECIFICATION-20261008.md#81-profile) | FP-1, FP-2, H-3 |
| M-2 | Registry extension: native state, gauges, `Attach`/`Locate`/`ForceUnmount`, status fields | [#314](https://github.com/Ephemeral-AI-Lab/layerfs/issues/314) priority 1; dispatch §6 | [§5.1](S8-SPECIFICATION-20261008.md#51-workspace-and-mount) | FP-24, FP-25-Routes |
| M-3 | Owned deferred replies, R handoff credits plus N fixed receive slots, callback-entry capacity exception, completion notifiers and fair service | R5, K13 and I-8 as corrected | [§6](S8-SPECIFICATION-20261008.md#6-native-request-service-and-scheduling) | FP-8, FP-34, H-10 |
| M-4 | Consistent compound answer jobs with positive-entry custody acquisition; separately counted processing/fact work | S6 ownership and D-4 | [§6.4](S8-SPECIFICATION-20261008.md#64-request-shapes-and-their-owner-jobs) | H-4, FP-12, FP-31 |
| M-5 | Fair bounded cold-demand admission; idle and healthy reader selection; per-request failure scope; snapshot through a reader | R5; dispatch §6 | [§7.2](S8-SPECIFICATION-20261008.md#72-mandatory-changes) | FP-27, H-11 |
| M-6 | Kernel profile and coherence rules without notifications | [fuse.md §2, §4](../303/fuse.md#4-cache-coherence-and-lifetime-transitions) | [§8](S8-SPECIFICATION-20261008.md#8-kernel-profile-and-coherence) | FP-9 to FP-18 |
| M-7 | Kernel-origin mapped WRITE acceptance with handle-liveness check only | [fuse.md §5](../303/fuse.md#5-optimization-disposition) | [§8.3](S8-SPECIFICATION-20261008.md#83-coherence-matrix) | FP-15 |
| M-8 | Indexed native lookup counts and independent open/processing custody; checked bounded FORGET units and exact removed-inode semantics | S6 contract and corrected D-6; resident windows bounded | [§5.3](S8-SPECIFICATION-20261008.md#53-lookup-references-and-open-handles) | H-7, H-8, FP-28, FP-29, FP-31 |
| M-9 | WITHDRAWN prospective daemon Exec mechanism; runtime-owned successor M-9-Runtime below | Latest owner direction; original source pin retained | [§5.4](S8-SPECIFICATION-20261008.md#54-runtime-process-and-stream-ownership) | Old FP-5/6/7/19/30 withdrawn |
| M-9-Runtime | Actual Sandbox ordinary execution/streams/status, caller cancellation and access setup; no daemon command owner | K35; SDK Project/Workspace/Sandbox shape; I-11/12/13 | [§10](S8-SPECIFICATION-20261008.md#10-ordinary-runtime-execution-access-and-confinement) | FP-5/6/7/30-Runtime, FP-19-FS |
| M-10 | Confinement of Bash from both databases and from the connection; propagation contract | K32, [06 §6](../303/06-cluster-one-integration.md#6-store-visibility) | [§10.2](S8-SPECIFICATION-20261008.md#102-what-confinement-is-and-is-not), [§10.3](S8-SPECIFICATION-20261008.md#103-mount-propagation-contract) | FP-5-Runtime, FP-22-FS |
| M-11 | Reversible normal probe; forced active-control refusal; separate abort/plain-detach effects; full daemon-work drain before logical native-owner revocation and bounded retirement | Unmount contract, K22 and review corrections R3–R5 | [§11](S8-SPECIFICATION-20261008.md#11-terminal-unmount-drain-and-reclamation) | FP-20, FP-21, FP-23-FS, FP-32, FP-33, H-8, H-15, H-19 |
| M-12 | Debt and maintenance state in status; debt-coupled mount admission | [unmount U9](../303/workspace-api/unmount.md#7-per-tool-call-and-concurrent-workloads), [mount M7](../303/workspace-api/mount.md#7-workloads-and-future-proofs) | [§11.4](S8-SPECIFICATION-20261008.md#114-reclamation-and-debt) | FP-26, H-16 |

M-4 is a prospective count hypothesis, not a universal read-only shortcut.
Pure observations can use one answer job; positive LOOKUP must also acquire
indexed custody, and deferred plans may need processing leases. H-4 records
those writes and fact rounds. FORGET does real indexed ownership work and
retirement has O(K) total backing work; zero-SQL/constant-total claims from the
original proposal are withdrawn. No historical receipt changes verdict.

## 4. Ranked candidates

None of these is selected. Each enters S8 only when its gate is met, and none is
a pre-S8 gate. The dimension names are: `W` live Workspaces, `Q` admitted
requests, `I` returned identities, `B` returned bytes, `D` tree depth, `E` cache
entries, `F` concurrent flights.

### 4.1 Rank 1 — shared ownership of cached bytes

| Field | Content |
| --- | --- |
| Source | [#313](https://github.com/Ephemeral-AI-Lab/layerfs/issues/313) item 3; [cache.rs](../../../crates/layerfs-workspace/src/base/cache.rs), [client.rs](../../../crates/layerfs-workspace/src/base/client.rs) |
| Hypothesis | Bytes copied, allocated or freed while the cache mutex is held go from `B` per hit and per inserted batch to zero (H-13). Copy count per hit and per miss is unchanged at one, because the cluster-one read trait returns owned bytes: this is not zero-copy |
| State, key, lifetime | Entries hold `Arc<[u8]>` instead of `Vec<u8>`; key stays `ObjectId`; lifetime until eviction or last borrower. Public `CanonicalCache::{new, diagnostics}` and the owned-byte read API are unchanged |
| Atomicity | Hit: one critical section bumps the reference, touches recency and counts the hit; the copy happens outside. Insert: the shared buffer is built outside; one critical section re-checks presence, moves evicted entries to a local list, inserts and updates charge and eviction counters; the list is dropped outside |
| Work | Per hit `O(log E)` under the lock plus `B` outside; worst case unchanged; cumulative lock hold drops by total bytes served |
| New memory domain | Bytes still borrowed after eviction. They stay charged to a `lent` gauge (specification D-15) |
| Failure paths | Oversized or duplicate insert is a no-op, never an error. Lock poison is the only refusal; an already authenticated demand must not fail because the final insert found the lock poisoned |
| Benefit | Shorter critical sections when several Workspaces hit concurrently. Size unknown |
| Interference | None on correctness; one more gauge |
| Risk | Low |
| Evidence | source |
| Gate | A recorded baseline of lock-held copy bytes and cache wait under several concurrent Workspaces (H-13) shows the hold is material |

### 4.2 Rank 2 — small immutable fact cache

| Field | Content |
| --- | --- |
| Source | `BaseView::reader()` and the per-stat length demand (section 2); dispatch §7; [#314](https://github.com/Ephemeral-AI-Lab/layerfs/issues/314) priority 2 ("a complete additional immutable-attribute cache is separate work") |
| Hypothesis | A warm LOOKUP or GETATTR of an unchanged base entry performs zero object-cache clones and zero length demands, instead of about `D` clones and one reader query (H-5) |
| State, key, lifetime | Three fixed-size record kinds in one bounded allowance: `(directory content root, name) → child serial`; `(inode table root, serial) → inode value and metadata`; `file content root → length`. Keys are exact canonical identities, never a path, branch or serial alone. Entries are immutable, so there is no invalidation; lifetime is eviction or daemon exit |
| Work | Hit `O(1)` expected; miss falls through to today's path and inserts. Memory is the declared allowance, charged separately from the object cache |
| What it never caches | Any overlay row, link count, size or name of a Workspace. Mutable results stay Workspace- and version-scoped in SQL. No mirror |
| Benefit | Removes repeated decode and the reader round trip from the most frequent request kinds |
| Interference | Second memory domain; duplicates information already present in cached canonical bytes |
| Risk | Low for correctness (immutable keys); medium for memory accounting |
| Evidence | source |
| Gate | Recorded per-request object-cache hits and length demands on a walk and a stat-heavy workload (H-5, H-17) |

### 4.3 Rank 3 — background queue depth

| Field | Content |
| --- | --- |
| Source | Kernel behaviour at `v6.12` (specification §8.2) |
| Hypothesis | With `max_background` `n`, up to `n` background requests of one mount are in userspace at once; with 1, cold cached reads of one Workspace are served one at a time regardless of loops (H-9) |
| Change | One `KernelConfig` value; possibly more loops with it |
| Work and memory | More simultaneously owned READ replies per mount, each at most one window; `R` still bounds them |
| Risk | Low for correctness: I-9 holds for any depth. Resource proof required |
| Evidence | source (kernel), none measured |
| Gate | FP-9 confirms the serialization on the mounted kernel. The owner has agreed to one prospectively registered single-mechanism arm (P-6 ruling) |

### 4.4 Rank 4 — coalescing identical concurrent misses

| Field | Content |
| --- | --- |
| Source | [client.rs](../../../crates/layerfs-workspace/src/base/client.rs); listed as an optional investigation in #314 |
| Hypothesis | `n` requests missing one identity cause one acquisition instead of `n` (H-11 duplicate count) |
| Placement | At `Store` level, below `StorePorts`, where the exact `PortError` exists. Key: `ObjectId` within the one validated Store |
| Shape | A caller's batch splits into identities already in flight (subscribe) and the remainder (one new batch flight). Never one Store call per identity; never a delay to gather more |
| Bounds | Flights `F ≤ read_handles`; waiters per flight bounded; on overflow the demand proceeds uncoalesced. Transient bytes are at worst `F × 32 MiB`: the adapter cannot see lengths before the call, so no tighter bound is claimed |
| Ownership | The Store owns the flight. A cancelled subscriber or an unmounting Workspace drops only its subscription |
| Failure | Specification D-15: same original failure to every current subscriber; nothing cached; no re-acquisition. Fate is shared with the leader's whole batch because Storage batches are all-or-nothing |
| Benefit | Removes duplicate cold decodes that today can occupy every reader with the same work |
| Interference | Waiters are tied to the leader's batch duration |
| Risk | Medium (provenance, cancellation) |
| Evidence | source |
| Gate | A concurrent-cold proof records nonzero duplicate acquisitions after M-5 is in place |

### 4.5 Rank 5 — skipping the owner job for untouched keys

| Field | Content |
| --- | --- |
| Source | M-4 leaves one owner round trip per read-class request; #305 recorded a cross-thread wake of about 40 µs per FUSE round trip in that VM as a diagnostic, not a product number |
| Hypothesis | For a key the Workspace has never touched, read-class requests perform zero owner jobs |
| State | A fixed-size per-mount filter of touched serials and touched parent directories with no false negatives, inserted by the mutation's owner-side completion before its reply is attempted; never cleared except by proof of emptiness |
| Risk | High: it is per-Workspace in-memory state derived from mutable rows. I-6 must hold against every interleaving of insert, reply attempt and install. #314 selects no mutable mirror without a demonstrated requirement |
| Evidence | weak |
| Gate | After M-4, recorded owner-hop cost dominates a read-mostly fresh-mount workload, and an ordering proof for I-6 is written first |

### 4.6 Lower-ranked candidates

| Candidate | Hypothesis | Cost and risk | Evidence | Gate or disposition |
| --- | --- | --- | --- | --- |
| Coalesced publication-ticket release | Owner jobs per mutation approach one as bounded batches of tickets are released together | Engine command change; capture's wait on the frontier must stay bounded by one release job; no timer | source | Recorded ticket-release share of owner jobs under a write-heavy workload |
| FLUSH answered `ENOSYS` | One request fewer per close | Sticky and mount-wide; nothing may be deferred to close. A `close()` still waits for mapped writes in the kernel | count (E17: 10,000 of 60,002 requests on the passthrough) | Permission-preserving single-mechanism arm |
| Cached negative entries | Repeated misses stop reaching the daemon | A name created by a kernel request must invalidate through the reply path only; never revalidated without the parent lock | count (E12 lookups 604,146 → 460,360, measured with permissions off) | Coherence proof first, then a permission-preserving arm |
| Adaptive READDIRPLUS | The LOOKUP after each listed entry disappears | Each returned entry is a lookup reference; the kernel instantiates an inode per entry; CREATE/READDIRPLUS cannot carry separate lifetimes | weak, mixed sign (−6.3%, −4.5%, +7.9%) | Never always-plus; adaptive only, on the product |
| CACHE_SYMLINKS, PARALLEL_DIROPS, cached directory listings | Fewer READLINK; parallel LOOKUP per directory; no repeated enumeration | PARALLEL_DIROPS changes the locking FP-12 depends on | none to weak | One mechanism per arm, each with its proof |
| Requests above 128 KiB | Fewer sequential READ/WRITE | Larger owned buffers per request; one request stays one transaction | none | Recorded bytes per request on large sequential cases |
| Role-segmented or scan-resistant cache admission | A payload scan no longer evicts navigation pages | Needs the object role at insert; splits a small allowance; mis-sizing hurts sequential reads | source (structural only) | H-11 interference row shows a hot Workspace re-paying navigation after a peer's scan |
| Cache lock sharding | Less contention | Breaks exact global LRU and the single charge counter | none; rank 1 already removes copies from the lock | After rank 1, only if wait remains |
| Deficit-weighted owner lanes | Service share follows job weight | Changes the proven scheduler | none; F10 covers finite arrivals | H-12 fails |
| Whole-file CopyFileRange reuse | A copy of an unchanged file transfers no payload | Independent destination inode, stable authorized source, later-write proof | none | After the mutation checkpoint |
| #313 item 2: partial-cell mutation CPU and copies | Less per-byte mask work on small writes | Touches payload semantics | source | It is on the authentic S8 WRITE route: record cells, partial cells and copied bytes under tiny and dispersed writes first |
| #313 items 1, 4, 5: Save transactions, pack search, record batching | Fewer Store transactions and linear searches | Commit path | source | Commit route; evaluate with S10. Not an S8 item |

Rejected and staying rejected: kernel writeback for acknowledged writes;
mutable-file kernel passthrough that bypasses capture; removing
`default_permissions`; always-plus enumeration; fixed CPU affinity as product
policy; a private per-Workspace immutable cache; a host data path; a second SQL
scheduler; a batching timer anywhere; any SQLite optimization claimed from wall
time without the exact `EXPLAIN QUERY PLAN` and correlated runtime statement,
VM-step, row, page, copy and wait counts.

## 5. S10 dependencies

| Capability | Why S10 |
| --- | --- |
| Commit of a mounted Workspace's namespace and its survival in a fresh mount | Needs the live normalizer; a directly constructed Content root is not it |
| Known install under a live mount without cache invalidation | Needs the identity proof across install, including directory link counts |
| A second-mount `git status` fast path | Needs a committed refreshed index as well as stable identity |
| Related roots produced by Commit | Roots produced by host Init are available to S8; roots produced by Commit are not |
| Repeated incremental Commit on one mount | Engine ownership exists; the mounted proof needs the normalizer |
| #313 items 1, 4 and 5 | Their route is Commit |

## 6. Historical dispositions carried forward

These are preserved exactly; the proof plan's
[eligibility section](S8-PROOF-PLAN-20261008.md#2-historical-evidence-and-eligibility)
holds the per-row matrix and the retained
[evidence review](checks/s8-specification-20261008/04-review-evidence-proof.md)
holds every number with its source.

| Evidence | Disposition |
| --- | --- |
| #305 A2 | ext4 passthrough with no canonical Store or Commit. Its cached profile is a prospective candidate. A2O and A2P were not promoted; the permission-removing arms are not product candidates. It supplies request-shape context and a passthrough floor, not a product result |
| #305 Stage B, E10/F | The command exited 0; the required mounted survival verifier reached 10 s. The cell is `FAILED` and survival is unestablished. It ran on the zero-lifetime direct-I/O profile, not the promoted one. It cannot be an arm or a speed control |
| #305 Stage C | `NOT_RUN` |
| #305 timing before `cf-fsbench-v2` | Exec values are upper bounds high by up to about 51 ms, unmount by about 16 ms; receipts were not rewritten |
| #306 | Every numeric cell is `INELIGIBLE`; not a matched ranking. Backend, architecture (including emulated x86-64), cache state, time caps and skipped content proofs stay attached to each row, with each original `FAILED`, `EXCEEDED`, `INCOMPLETE` and `NOT_RUN` |
| Full fixture | 130,045 entries and 3,475,776,149 regular-file bytes including `.git`, dependencies and symlinks; reduced base 35,024 entries; dependency replay 95,021 entries and 2,126,509,110 bytes; manifest `98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`. The pre-S8 Init and Q1 fixtures are different workloads |
| Host Init on WAL | The speed comparisons that failed stay `FAIL`; the WAL result is the accepted baseline for that exact scope |
| Strict allocation | `NOT_RUN — mechanism removed` |
| Seal, stride 1 | The original allocation `FAIL` stays; the corrected selection passes at its own identity; stride 10 and 3 and Init were not rerun for it |
| Files above 4 GiB | `NOT_RUN — waived by owner` |
| Durable | `NOT_RUN — disabled by owner until explicit reauthorization` |
| E04 | Closed on its original host-mediated topology; never rerun and not reopened |
| F14 | Exact continuous peaks and unsupported attribution `INCOMPLETE`; numerical acceptance `OWNER_DEFERRED` |

One discrepancy inside a historical report is recorded and not corrected: the
stateless-arm report's headline says nineteen `OK` and five `SLOW`, while its
own table and raw receipts give twenty and four.

## 7. Review findings

Three read-only reviews informed this ledger. Their reports are retained
verbatim and every finding has an accepted, rejected or deferred disposition
with its original reason in the [finding ledger](checks/s8-specification-20261008/05-finding-ledger.md).
The later [six-finding correction](checks/s8-spec-review-fixes-20261008/02-correction-ledger.md)
supersedes counter-only lookup custody, Quiescent-as-release, receiver-only drain,
the terminal normal-probe fence, the MNT_FORCE sequence and implicit pre-read
credit control. The original ledger/reviewer reports are retained, not rewritten.
