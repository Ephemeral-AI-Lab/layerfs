# Next-agent prompt: complete S4–S6 mutable filesystem work

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

## Assignment and stopping boundary

Implement LayerFS cluster-two **S4, S5 and S6** in the existing primary checkout
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, directly on local `main`, with product
implementation under `core/`. Continue from coherent checkpoints to each actual
milestone exit, then make its completion commit and update
[tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Do not ask for routine approval. Local checkpoint/milestone commits and tracker
updates are authorized; remote pushes, releases and deployments require separate
authorization. Keep commits local-only. Do not create another task or agent.

This document is the standalone prompt for the **next** agent. The producing
thread finished S3 and stops after committing this handoff and posting its receipt,
**before starting S4**. That owner boundary supersedes its earlier instruction to
continue through S13; the overall tracker plan remains S0–S13. The next agent
should complete the S4–S6 group and hand off the remaining work, without claiming
S7–S13 complete. Complete useful independent work before recording a hard blocker;
a failed check, checkpoint or context compaction is not a stopping condition.

## Restore these identities first

| Identity | Frozen value / scope |
| --- | --- |
| Final S3 implementation commit | `c4b49a121aec15a6eae58c04d8074fd9eb2772db` |
| Final S3 tree | `67a6b34e5b135ea5d30c48fa5d0bd977c4aa1356` |
| S3 completion receipt | [issuecomment-5994947845](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-5994947845), retained [text](checkpoints/c4b49a121.md) |
| Product/build hash receipt | [checks/s3-view/identity.json](checks/s3-view/identity.json); exact source, core lockfile and root ARM64 config hashes |
| S1 completion | `7019801f91d3a7ba9c16ef381aa4a57474feaae5`; [exit audit](S1-EXIT-AUDIT.md) |
| S2 completion | `8e2976e4e08dfda70decc99225eb4bb14c3e204a`; [exit audit](S2-EXIT-AUDIT.md) |
| S3 completion | [exit audit](S3-EXIT-AUDIT.md); prepared install checkpoint `6e84b91818bff510f4902537e413eda5094cf754`; source-window checkpoint `bdc6ed4af68c46a31a7ead52fdef633d8a82422d` |
| Design/guide baseline | `c9861bc878583822a468e78dbc0f3740eacbecbe`; design preparation did not complete implementation |
| Rust / host | Rust `1.85.1`, macOS ARM64 for actual global provider and SDK checks |
| Linux owning image | `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`, Linux ARM64; prior platform receipt records Docker 29.5.2 and kernel 6.12.76-linuxkit |
| Targets / Linux cache | Host `core/target`; Linux `core/target/cluster2-linux`; Cargo cache `core/target/cluster2-linux-cargo`, all inside this checkout |
| Actual milestone status | S1/S2/S3 COMPLETE; S0 and S4–S13 unchecked. S4 has not started |

The handoff-only commit follows the S3 identity above and changes no product
source. Its exact HEAD/tree and unchanged LOC are recorded in the appended tracker
handoff receipt. Obtain them with `git log` and `git rev-parse HEAD HEAD^{tree}`;
do not confuse the documentation wrapper identity with the tested product pin.
On resume, reconcile the actual tree and latest append-only tracker comments
before relying on this frozen snapshot.

Read before writing:

1. [Root guide](../../../../AGENTS.md) and [core guide](../../../AGENTS.md),
   [cluster-one handbook](../../../../cluster_one_handbook.md) and
   [CAS/CDC/delta handbook](../../../../cas_cdc_deltaencoding_handbook.md).
2. [Current design index](../303/README.md), all seven primary contracts:
   [mount](../303/workspace-api/mount.md), [exec](../303/workspace-api/exec.md),
   [commit](../303/workspace-api/commit.md), [unmount](../303/workspace-api/unmount.md),
   [status](../303/workspace-api/status.md), [engine](../303/daemon-sqlite.md),
   [FUSE](../303/fuse.md); also [runtime integration](../303/06-cluster-one-integration.md)
   and [implementation/validation](../303/07-implementation-validation.md).
3. [Optimization guide](../../../../docs/general/optimization-guide.md),
   this handoff, the three exit audits, [progress](PROGRESS.md), tracker body and
   latest checkpoint/completion comments. Current owner decisions and documented
   supersessions govern. Closed stages, research and historical receipts retain
   only their explicit scope.

The implementation plan owns dependencies and exits. Current public cluster-one
APIs and canonical formats govern callers; SQLite scratch does not replace them.

## Preserve the existing checkout and runtime

Before editing, inspect branch, HEAD, status, staged diff, actual active members
and tracker. Do not reset, clean, overwrite or blindly stage the checkout. The
handoff adopts only this note, its progress link and the retained S3 completion
text. Product source and manifests/locks are clean at the S3 pin.

The following unrelated state was present at the handoff snapshot and remains
unstaged/untracked after the handoff commit:

```text
 M docs/README.md
?? core/docs/issues/301/01-architecture.md
?? core/docs/issues/301/02-workspace-overlay.md
?? core/docs/issues/301/03-commit-workflow.md
?? core/docs/issues/301/04-storage-interfaces.md
?? core/docs/issues/301/05-migration-plan.md
?? core/docs/issues/301/06-tables.md
?? core/docs/issues/301/README.md
?? core/docs/issues/301/prompts/cluster1-storage-plan.md
?? core/docs/issues/301/prompts/cluster2-sandbox-plan.md
?? docs/general/sandbox-cache-design.md
?? docs/general/workspace-filesystem-view.md
?? docs/general/workspace-queue-scheduling.md
?? docs/research/extent-normalization-analysis-2026-10-02.md
?? output/imagegen/cluster-one-x-20261005/01-architecture.png
?? output/imagegen/cluster-one-x-20261005/02-streaming.png
?? output/imagegen/cluster-one-x-20261005/03-cdc-cas-delta.png
?? output/imagegen/cluster-one-x-20261005/04-validation.png
?? output/imagegen/cluster-one-x-20261005/cluster-one-x-images.zip
?? output/imagegen/cluster-one-x-20261005/generation-notes.md
```

The README diff adds three index links to the untracked sandbox/view/queue guides.
Preserve them without adopting them into product commits. The pre-handoff exact
status is also retained at `core/target/cluster2-307/state-before-handoff.txt`;
it additionally lists `checkpoints/c4b49a121.md`, which this handoff commit adopts.
No staged changes existed before preparing the handoff. Inspect again for work
from other conversations; this snapshot is not permission to overwrite it.

Runtime inventory checked on 2026-10-05 around 13:03 UTC:

- No producing-thread Cargo, Docker-run, daemon or platform-proof process remains;
  all its command sessions finished. No owned live FUSE mount remains. Host mount
  inventory had no FUSE/LayerFS match. Owned `--rm` proof containers were removed.
- Preserve these unrelated running containers: `9cf2fe345496`
  (`layerfs-experiment-305-dev`), `ce75ac504df9`
  (`layerfs-4c8cde9ee1cf49f8049298b3377d4927`), `d2433851ea59`
  (`layerfs-dbd59ea75fdc62de0a1f6d9d099609ab`), `d2550144998b`
  (`layerfs-76b116dc9d984679f5802e2ec9c0798d`). Their internal mounts/processes
  are outside this thread's ownership and were not inspected or interrupted.
- No measurement campaign or run lock was acquired by this thread. No lockfile
  existed in the inspected `core/benchmark-results/fs-bench-pro` or top-level
  `target` lock locations. No owned Cargo process holds a target lock. `lsof`
  reported a read-only FD from Apple's virtualization process PID34603 on
  `core/target/cluster2-linux/debug/.cargo-lock`, without a lock marker; preserve
  that foreign FD. Host `core/target/debug/.cargo-lock` had no reported holder.
- `core/target/cluster2-307/lock-before-sdk` and `lock-before-native-bridge`
  are saved Cargo.lock snapshots, not held locks. Incremental/compiler/registry
  lock files in the targets/cache are ordinary retained build material; do not
  remove them merely because their names contain “lock”.

Native minimum-time failure previously required aborting **owned connection91**
and cleaning the exact probe container. That cleanup finished; its normal-teardown
failure remains FAIL. Do not repeat its abort against a current connection number.
No Codex goal is active. Recheck process/lock/ownership state before using targets.

## Ready product interfaces and boundaries

[core/Cargo.toml](../../../Cargo.toml) builds eleven members: content, storage,
persistence, project, history, telemetry, overlay, Workspace, daemon,
`layerfs-api/sdk` and bridge. FUSE, API-core, sandbox, server and relocated legacy
packages remain excluded; passing core tests does not build or qualify them.
Root `crates/` is reference only. Existing excluded source stays retained/countable
until S11 replacement coverage; root retirement is S13 after S12 qualification.

| Owner | Ready behavior and source | Limits the next agent must retain |
| --- | --- | --- |
| Overlay | [Architecture](../../architecture/19-daemon-overlay.md), [schema](../../../crates/layerfs-overlay/sql/schema.sql), [types](../../../crates/layerfs-overlay/src/types.rs). One create-new mode0600 database and daemon-owned connection initialized before readiness; schema6, namespaced metadata/cells/scratch/leases/generations/tickets; indexed EXPLAIN and fixed-family statement profiles | MEMORY/OFF/EXCLUSIVE, mmap0, busy0, foreign_keys1, 4096-byte pages, FILE temp, cache suggestion -2048; disposable backing has no crash-survival guarantee or sync calls. Default page ceiling is SQLite's actual format limit, not a new Workspace quota; optional physical quotas are explicit |
| Frontier and install | [Generation/frontier code](../../../crates/layerfs-overlay/src/generation.rs), [S2 audit](S2-EXIT-AUDIT.md). Capture seals existing membership without a copy; frozen revision/base/generation, fixed EOF and selective keyset pages; known install advances floor/base and preserves later active metadata/cells/tickets | Lost reply does not undo publication. Pending publications and retained capture can be observed after lost internal results. Single retained capture currently lacks normal failed-capture resolution; live retired-generation reclamation is unfinished |
| Source custody | [Architecture](../../architecture/28-base-source-windows.md), [source contract](S3-SOURCE-FENCE.md). Exact opaque `BaseSource {route, owner, root}`, backed ownership and maintained base-reader count; acquire/observe/source reads/release; install readiness requires zero selected sources | One bounded request/provider processing window, not a descriptor or Exec lease. Existing source reads/releases survive close. Explicit release only after actual completion/fence; no Drop SQL. This is local custody, not a future global GC lease |
| Immutable base | [Workspace base](../../../crates/layerfs-workspace/src/base.rs), [architecture](../../architecture/20-workspace-base.md). `BaseView::open` checks one real root; public content root/child/inode/list/readlink/FileView ranges. Immutable ObjectId cache, retained `BaseRead` root/EOF and bounded 128KiB read windows | Cache defaults are logical admission bounds (32MiB/4096 entries), with large-object bypass; they do not prove total resident memory. Actual canonical visited-path work remains. No root import/copy/scan on repeated binding |
| Effective read semantics | [view.rs](../../../crates/layerfs-workspace/src/view.rs), [list.rs](../../../crates/layerfs-workspace/src/list.rs), [port.rs](../../../crates/layerfs-workspace/src/port.rs), [architecture](../../architecture/29-effective-base-view.md). `Workspace::bind`, `view_for_source`, `SourceView::stat/lookup/list`, `OverlayRead`; active overrides captured overrides base; aliases, whiteouts and empty-progress pages work | Three bounded input pages, at most64 visited distinct keys per window including whiteouts. Weak concurrent directory semantics; stable native cookies/handle/reference ownership remain S4/S8. `namespace_refs` is not POSIX directory nlink |
| Prepared install | [install.rs](../../../crates/layerfs-workspace/src/install.rs). `prepare_base_install` checks one canonical root outside SQL; `install_prepared_base` changes engine and binding with no fallible work after known success. Old immutable plans stay readable | `PreparedBase` plus original cause survives refusal/failure. Only known history success authorizes install. Do not infer publication outcome from root preparation or an unfenced read |
| Daemon owner | [architecture](../../architecture/21-daemon-owner.md), [commands](../../../crates/layerfs-daemon/src/commands.rs), [read port](../../../crates/layerfs-daemon/src/read_port.rs). Short typed jobs, six classes (Read/Mutation/Capture/Lifecycle/Scratch/Source), finite capture/install fences and fair namespace/class roster; queued/executing/caller-held completion credits with lifecycle reserve. Real `OwnerClient` implements `OverlayRead`; `InstallPrepared` performs paired install | Provider/content I/O stays outside SQL/binding lock. Later source acquisitions park before attempt, existing reads/releases/mutations and unrelated namespaces progress. `OwnerError::Unattempted` retains the owned command; attempted install error retains checked input. No retry/replay. Cloned bounded output after completion still needs aggregate consumer/native residency accounting |
| Host SDK | [architecture](../../architecture/22-sdk-runtime.md), [runtime](../../../crates/layerfs-api/sdk/src/runtime/mod.rs), [length port](../../../crates/layerfs-api/sdk/src/runtime/length_port.rs). Initialized independent Storage owners, stack-borrowed interleaved Saves, same-Save reads, exact accept/finish/abort custody; native verified peer plus per-operation application authority. `Sessions::length_port` serves actual owning stat lengths; demand counters are exposed | Borrowed `BoundLengths` preserves RuntimeError cause, no lifetime extension/provider reopen/self-reference. Full logical object/Save/history RPCs, fair network service, authority closure, disconnect fences and faithful import remain S9/P1/P10/P12 |
| Length facts | [Architecture](../../architecture/24-file-lengths.md). Explicit `FileLengths`, `CanonicalClient::with_lengths`, borrowed `BaseView::stat_with_lengths`; owning acknowledged Store metadata without whole payload reads | MissingLengthProvider fails explicitly; no FileView fallback. Logical transport still unfinished. A length fact does not attest the payload's integrity |
| Terminal cleanup | [Architecture](../../architecture/25-terminal-reclaim.md), [reclaim.rs](../../../crates/layerfs-overlay/src/reclaim.rs). Closed-ready index, exact last-owner gates, bounded rotating deletes; idle maintenance and after finite foreground jobs | Terminal-only initial coverage: 64 keys, 14 cells/64512 physical payload bytes, scratch<=65536 per selected window; no S6 live-generation/orphan/pressure proof inferred. First maintenance error is retained; no failed-operation replay |

Current `Publish` supplies **one inode, optional dentry and optional cell** after
trusted semantic validation. It is an engine primitive, not an implementation of
atomic link/rename/unlink or permissions. Overlay's 4096-byte cells plus512-byte
validity masks are a prototype requiring S5 representation/copy/page proof.

Canonical stored names currently require UTF-8, forbid backslash and have a
255-byte component format limit; LogicalPath has its existing4096-byte/256-component
constraints. S3's raw byte resume cursor changes no stored object grammar.
Faithful arbitrary Unix import remains P12; do not silently ignore names or claim
the existing format already supports them. Kernel owner/ctime/directory nlink,
serial allocation and complete permissions still need their actual contracts.

## First ready slice and S4–S6 exits

Start **S4**, because the S2 generation/service and S3 base/read dependencies are
complete. First reconcile the namespace/metadata invariants and owning serial
allocation with current public APIs. Add the minimal bounded **compound mutation
transaction** boundary to overlay/daemon, then implement Workspace create/link/
unlink semantics over real inherited and local parents. Include parent existence/
kind, alias/reference counts, portable metadata/time updates, failure atomicity
and concurrent selection/publication ordering. This is recommended first work,
not a newly selected payload algorithm or permission gate.

Do not implement a compound operation by independently publishing half its
effects. Revalidate the relevant mutable facts after provider I/O in a short
owner job; do not hold SQL over demand or create a resident namespace mirror.
Choose backed parent/reference/topology evidence for incremental checks; a
streaming whole-base alias walk still violates tiny-operation incrementality.
Update affected API/architecture documents and paired SQL evidence in each slice.

| Milestone | Required deliverable | Required exit evidence |
| --- | --- | --- |
| S4 — Namespace semantics | lookup/getattr/create/mkdir/symlink/link/unlink/rmdir/rename/readdir/chmod/utimens; serial ranges, binary name order, parent/reference ownership and bounded directory cursors | Atomic ordinary effects and exact errors; hard-link aliases, rename replacement/cycles/parent effects, resume under declared mutation semantics, permissions/time behavior and no resident name/handle cap. Actual query EXPLAIN plus complete-operation correlated runtime profiles justify visited-work bounds. Exercise real canonical roots and actual daemon jobs on owning hosts |
| S5 — Payload and streams | Explicit cells/tails/validity transitions, append/overwrite/inherited reads, cutoff truncate/regrow and sparse holes; R1 and owning content hole contracts resolved | Alternating-byte/dense fragmentation does not enlarge one request's work; nonzero shrink/capture/regrow never resurrects removed bytes; no WRITE base copy-up; READ gaps use bounded demand; sparse processing follows defined canonical semantics without O(logical hole length) zero streaming. Report tiny-file/page/journal/copy amplification and output/consumer custody, not only heap |
| S6 — Lifetimes and reclamation | Independent orphan state and reader/capture/operation custody; bounded repeated success/failure composition; physical reservations/headroom and weighted automatic reclaim; R4/R6/R8 resolved | Stable open-unlinked bytes across repeated log/appends and successful/failed captures/installs; last-owner proof, no growing read-depth/generation chain or payload-sized busy fold; live and idle cleanup makes bounded progress; stale deletion cannot remove newer data. Actual physical pages/debt/admission and definite/uncertain failure custody, including headroom, pass owning checks |

For S5 first derive and record the fragmentation/cutoff/hole algorithm and its
worst-case, amortized and cumulative costs. Existing cells/cutoff fields alone
do not establish semantics. For S6 build on terminal ready-reclaim and source
fences, but implement live retired-generation cleanup, independent orphans and
normal capture-failure resolution; currently none is proved. Normal failed Commit
must not gain a new generation layer or synchronously merge the whole payload.
Unknown publication cannot authorize capture release or cleanup.

Preserve both Workspace-per-tool-call and per-task orchestration: complete roots
include `.git/index`, ignored files, dependencies, symlinks, caches and outputs.
Repeated mount reuses initialized runtime/daemon and binds without scan/copy/import
or reinstalls. A Workspace can serve concurrent/sequential calls for a long time,
with repeated incremental Commits. Exec is ordinary Bash with streamed I/O, no
automatic runtime cap, special preparation, implicit Commit or unmount. Capture
includes shared published state and reply-send attempts; unpublished dirty mmap
stores are outside its frontier. No total file/edit/Workspace/Commit/flow/time
cap may be solved by raising limits or dropping data. Explicit processing windows,
physical resources and real platform/format limits must be described honestly.

## Remaining S0 corrections and cluster-one ownership

These are required constraints, not optional future polish. S1–S3 evidence covers
scoped prerequisites only; it does not close S0 or all R/P rows.

| Correction | Current obligation / routing |
| --- | --- |
| R1 | S5 fragmentation-bounded WRITE and nonzero logical cutoff, with alternating-byte and shrink/capture adversaries |
| R2 | S2's selective terminating capture cursors are ready; preserve generation predicates and integrate real request/capture ownership |
| R3 | S5/content and later Commit: remove fragmented constructor refusal; back directory/touched/validation/release state, holes and incremental alias/topology evidence |
| R4 | S6 independent orphan and bounded repeated failed/successful Commit composition, retained log descriptor, no foreground payload merge |
| R5 | Initial short fair SQL/source readiness proof ready; complete native deferred replies, runtime Store scheduling and demand transport capacity remain S8/S9 |
| R6 | S6 physical reservations, actual page accounting/shared-disk admission; no unbounded cleanup charged to a tiny write |
| R7 | Initial thirteen-role semantic admission and authenticated binding ready; contextual caller/session/history authority remains P1/S9 |
| R8 | Exact owned unattempted inputs improved; S6–S9 whole-system residency, cancellation/disconnect outcome and buffer/kernel ownership still need proofs |

Withdrawn `334fc7437` claims remain withdrawn: constant small-statement arbitrary
extent overlap, factor-two capture scans, two-version orphan bound, one-step wait,
strict read priority and foreground failed-Commit folding. No alternative discussed
in research is selected without an explicit current contract and implementation.

| Prerequisite | Status / concrete next ownership |
| --- | --- |
| P1 semantic admission/runtime authority | Owning canonical grammar/ref derivation and initial native-bound SDK scope exist; contextual closure/history/network authority remain S9 |
| P2 locked Linux content/SQLite | Actual locked ARM64 build established; native fuser capability remains blocked as documented below |
| P3 backed deferred editing | Existing EDIT_DEFERRED_LIMIT fragmented-file refusal must be removed through backed bounded inputs; S5/content prerequisite |
| P4 hole-aware canonical/read/edit/stream | Existing sparse construction processes zeros O(logical length); correct contract/implementation through owning APIs before S5 acceptance |
| P5 cheap lengths | Owning saved-file API and actual SDK/Base stat delivery complete; logical metadata transport remains S9 |
| P6 ordered directory changes | `DirectoryUpdate.changes` resident Vec remains; streamed construction prerequisite for S10, informed by S4 namespace state |
| P7 new-parent membership | Resident map and ordering_bytes refusal remain; backed bounded membership, distinct from P4 holes |
| P8 engine payload/capture/orphan/failure | Initial primitives are ready; replacement lifetime/work algorithms still S5/S6 |
| P9 qualification hosting/specification | Read measurement rules and freeze prospective specifications before new measurement; S12 real integration acceptance is later |
| P10 unknown-history resolver/fence | Exact resolver plus completion fence required; absence is not success/failure proof and Uncertain stays terminal without it |
| P11 root accessor docs | `.0` documentation corrected; no new API needed |
| P12 faithful bounded import | Native Init still refuses symlinks, retains scan state and has inherited4GiB file cap; raw-name compatibility also needs an explicit contract; S9 prerequisite |
| P13 validation/touched/zero/release | ordering_bytes-derived row/name/demand caps and resident demanded/addition/parent/touched/zero/release collections remain; touched count checked after collection is insufficient |
| P14 incremental topology | Stored non-file rebind can walk the whole base under ordering_bytes/1024 cap; preserve aliases/cycle checks with incremental backed parent/reverse-binding evidence |

Progress required content/runtime prerequisites where ready, without reviving the
server or substituting legacy paths. Complete the S4–S6 owning exits; carry later
S9/S10 integration obligations forward explicitly, without pretending S6 is full
integrated Commit/FUSE qualification.

## Reusable verification and SQL evidence

Final S3 relevant product identity has the following qualifying evidence. Reuse
unchanged rows at their exact scope; rerun affected covering checks only after a
justified source change. The handoff-only commit needs docs links/anchors, claim
accuracy and whitespace, with unchanged product checks reused.

| Check | Outcome / retained evidence |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets` | PASS576; [core-test-final.log](checks/s3-view/core-test-final.log) |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | PASS; [clippy-repaired.log](checks/s3-view/clippy-repaired.log) |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS recorded in S3 completion |
| `python3 -B core/tools/check_product_boundary.py` | PASS502 production Rust/SQL files; S3 completion |
| `python3 -B -m unittest discover -s core/tools -p 'test_*.py'` | 26 OK; S3 completion |
| Actual Linux ARM64 selected overlay/daemon/Workspace/SDK all-target checks | PASS34; [linux-test.log](checks/s3-view/linux-test.log). SDK actual global-provider cases run on macOS, compile only on Linux |
| Test-only fixture lint repairs | Linux Workspace7 PASS [receipt](checks/s3-view/linux-repaired-workspace.log), macOS SDK5 PASS [receipt](checks/s3-view/sdk-repaired.log); unchanged product coverage reused |
| Owning platform prerequisite | [Platform harness](../../../benchmark/cluster2-platform/README.md) separately builds pinned fuser0.18.0/rusqlite0.40.2; real basic mount/read/detach/join PASS. This does not build excluded product FUSE or cover its timestamp failures |

The locked Linux selection uses the recorded image, `--platform linux/arm64`,
repo bind to `/work`, working directory `/work`,
`CARGO_HOME=/work/core/target/cluster2-linux-cargo` and
`CARGO_TARGET_DIR=/work/core/target/cluster2-linux`, with
`cargo test --manifest-path core/Cargo.toml --locked --all-targets -p layerfs-overlay -p layerfs-daemon -p layerfs-workspace -p layerfs-sdk`.
Use the actual member/package names and retain root config discovery. Native
integration additionally needs `/dev/fuse`, required capabilities, real mount
readiness and fenced teardown; Docker availability alone does not prove them.

Root `.cargo/config.toml` ARM64 inputs remain
`--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2`.
Explicit RUSTFLAGS must repeat them. No third-party locked source/version/checksum/
dependency record changed in S3. Do not patch, fork, vendor, registry-edit or
substitute dependencies. Do not restore CI, retired preflight or an aggregate gate.

First-party failures are retained under [s3-view](checks/s3-view/identity.json):
mode-width/private-field compiler errors, the stale first-party composition-edge
guard rejection, and fixture Clippy failures. Repairs are documented; the guard
now allows daemon/SDK -> Workspace while rejecting reverse/domain/server edges.
Old FAIL logs remain append-only, rather than relabelled as later PASS.

| Work evidence | Actual scope / interpretation |
| --- | --- |
| [Name-window EXPLAIN/profile](checks/s3-view/name-window-profile.log) | Seeks `dentry_capture(ns,gen,parent,name>after)`; name family806 VM macOS/804 Linux beside128/1024/4096 unrelated names,64 returned, no fullscan/sort/autoindex/reprepare. Complete Linux window9 statements/921 VM includes source/parent checks; no macOS whole-operation total is invented |
| [Source DML profile](checks/s3-source/source-change-profile.log) | Live acquire/release99/123 VM macOS,94/117 Linux across all invoked families,2 changed rows each; exact indexed point source14/13 VM at unrelated-owner scales. Closed release pays additional declared ready/reclaim work |
| [Owning stat profile](checks/s3-view/owning-stat-profile.log) | Actual authenticated SDK/Store stat:131071 logical bytes,2 namespace refs, zero Store payload/pack reads; original Denied cause retained. Reuses P5 locator EXPLAIN/profile |
| [Workspace public behavior proofs](../../../crates/layerfs-workspace/tests/base.rs) | 507 keys/130 whiteouts produce377 entries with empty-page progress. Real canonical provider blocking leaves read/mutation/unrelated SQL progress; actual actor install pairs root, retains old bytes and later metadata |

These are deterministic correctness/work diagnostics, not cold latency samples,
release qualification or universal large-workload acceptance. Complexity currently
is indexed point work plus real canonical paths, and three fixed name pages with
64-key progress. Pager/journal/provider/network/kernel/output/cache aggregate
residency and physical headroom still need later evidence. Actual source/read
primitives do not prove sustained cleanup or independent descriptor custody.

## External native blocker: preserve the exact failure

Read [FUSE-TIME-BLOCKER-20261005.md](FUSE-TIME-BLOCKER-20261005.md) and its raw
build/negative/minimum/forced-cleanup logs. Published fuser0.18.0 converts signed
fractional timestamps before the LayerFS callback: `(-2,800000000)` becomes
`(-3,200000000)`; `i64::MIN` panics in the tested debug build before the callback.
The minimum case left a D-state waiter, failed normal join and required the exact
owned connection abort; final command exit137 is FAIL, not normal cleanup PASS.

Upstream's unmodified correction is
[e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7).
Registry status was rechecked on 2026-10-05: latest published version remains0.18.0.
The owner reaffirmed no third-party patch/import/source substitution, so no Git
pin exception is authorized. Wait for an allowed corrected published release,
then use its locked unmodified package and native requalification. A callback
cannot prevent this pre-callback panic; no raw-wire proxy, reduced timestamp
contract or overflow-check bypass is adopted. This is a hard S0/S8/S12 native
capability blocker, while independent S4–S6 work remains ready.

Other unfinished constraints are implementation work, not new permission gates.
Real device ENOSPC/commit-uncertainty integration is not qualified by the existing
explicit SQLite page-quota/FULL atomicity proof. Preserve exact refusal/conflict/
uncertainty custody and continue useful work; never guess resend/rollback/delete.

## Checkpoint protocol and source-size accounting

For every coherent checkpoint: update affected architecture/API docs, verify its
actual product/platform scope, calculate exact first-parent and final staged LOC,
commit, then confirm the committed tree equals the counted tree. Recompute after
any staged-source change/amend/rebase. Stage named owned paths only.

Use unchanged [tools/production_loc.py](../../../../tools/production_loc.py), SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Archive each exact tree's `core/crates` and `crates` into an owned temporary
directory and invoke that counter's `scan`; include product src and shipped SQL,
including excluded legacy source, and exclude tests/inline tests/docs/tools/
examples/harnesses/manifests/builds. Review any new shipped source classification.
Never count the dirty working tree as the commit. Receipts live under
`core/target/cluster2-307/loc/`; `s3-complete-staged.json` is the final S3 comparison.
The reference remains65417 throughout these commits.

| Commit / purpose | Core | Combined production LOC | Signed delta |
| --- | ---: | ---: | ---: |
| `7019801f9` S1 completion | 77893 ->78002 | 143310 ->143419 | +109 |
| `8e2976e4e` S2 completion | 78002 ->78190 | 143419 ->143607 | +188 |
| `6e84b9181` S3 prepared install | 78190 ->78278 | 143607 ->143695 | +88 |
| `bdc6ed4af` S3 source windows | 78278 ->78545 | 143695 ->143962 | +267 |
| `c4b49a121` S3 completion | 78545 ->79318 | 143962 ->144735 | +773 |
| This handoff-only commit | 79318 ->79318 | 144735 ->144735 | +0 |

The five implementation commits above total+1425. Since the design baseline,
core74000 ->79318 and combined139417 ->144735 total+5318. Earlier checkpoint
comparisons remain in their commit messages/tracker receipts. Relocated legacy
Workspace26835, daemon2151, SDK505 and bridge6834 remain excluded/countable;
no deletion or algorithmic shrink is claimed. Every new commit message includes
`Production LOC: <before> -> <after> (delta <signed difference>)`, plus counter,
scope and separate core/reference/combined totals. Docs-only commits report real
unchanged product totals, not fictitious zero LOC.

Finish **each** S4/S5/S6 milestone with an explicit completion commit and criterion
audit. Before beginning its next milestone, append a tracker completion comment
and update only its completed checkbox. During long milestones, append meaningful
checkpoint evidence. Use this exact template:

```text
Milestone: S<n> - <name>
State: CHECKPOINT / COMPLETE / BLOCKED
Source: <HEAD/tree and relevant source/build identities; local-only if unpushed>
Checkpoint commits: <hashes and purposes>
Delivered behavior: <concrete result>
Validation: <commands, outcomes, reused evidence and gaps>
SQLite evidence: <EXPLAIN + correlated runtime profiles, or scoped N/A>
Complexity/resources: <derived work, counts and resource/debt observations>
Production LOC: <each commit's totals/delta, scope and counter method>
Remaining work/blockers: <precise unfinished criteria>
Next ready work: <slice and satisfied dependencies>
```

Preserve append-only comments/receipts. If an update cannot be posted, save its
exact pending text locally, report the publication gap and continue independent
useful work. Do not rewrite failures or infer completion from a scaffold/fixture.

Before measurements, read [measurement policy](../../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[report template](../../../../benchmark_agent_report.md),
[core harness instructions](../../../benchmark/fs-bench-pro/AGENTS.md) and the
owning family specification before every invocation. Freeze specs and locked
release identities prospectively; one sample per case/arm, equal enforced cache
state, append-only outputs and worktree-local locks/targets. Reuse closed setup
with `--setup clone` and qualifying unaffected proofs, never a mutated sample or
warm phase credit. Do not enlarge budgets, shrink workloads or resample to pass.
Commit/capture/snapshot have one construction producer
(`LAYERFS_CONSTRUCTION_WORKERS=1`); namespace Init retains its scoped exception.
Harness budgets never become Bash runtime timeouts. Account for full physical
storage, queues, copies, caches, pager/journal/kernel/provider residency and
cumulative cleanup debt. Required proof/resource rows must pass; FAIL/NOT_RUN
does not satisfy acceptance.

Maintain a concrete next-work list. On compaction, recover from tree/commits/
tracker and continue the next ready slice without restarting completed work.
After S6 reconcile every group exit and final identity, publish its completion
receipt and produce the next handoff with exact remaining S0/S7–S13 obligations.
