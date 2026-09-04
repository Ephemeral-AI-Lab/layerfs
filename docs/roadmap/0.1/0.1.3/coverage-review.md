# v0.1.3 whole-Workspace coverage review

> **Status:** Archived planning review from 2026-09-04; retained for rationale.
> The [current family index](README.md), one current specification per family,
> and [testing rules](testing-rules.md) supersede this review's counts, scope,
> and one-Commit restriction. No measurements or release claims are recorded here.

## Recommendation

Keep eight timed families, with 56 new timed cases rather than 48, and add a
separate proof inventory of 15 named recipes. The extra cases target whole
Workspaces. Remove four link-creation timing rows, add eight Workspace-locality
rows and four whole-tree content-read rows, and replace weak existing curves
without retaining their old versions as additional cases.

The earlier eight family files are pre-review drafts. The current parent README and canonical family files supersede this
review for planned scope, caps, and case disposition.
Before implementation, update each selected family's complete definitions,
IDs, fixture/oracle hashes, metrics, budgets, and issue together as required by
[benchmark rules](../../../general/benchmark_rules.md). Unregistered draft
IDs are not frozen evidence. Released IDs and results remain immutable.

## Why whole-Workspace coverage is necessary

The current candidate planner abandons the localized path for directory
namespace changes in
[`changes.rs`](../../../../crates/layerfs-workspace/src/changes.rs),
`try_build_localized_candidate` (around lines 348–374). The fallback calls
`base_manifest` and `final_manifest` (around lines 49–50 and 621–674), which
can enumerate the complete tree. A small mutation in a small fixture cannot
reveal work proportional to a large untouched Workspace.

FUSE directory pagination is also an independent risk:
[`filesystem.rs`](../../../../crates/layerfs-fuse/src/filesystem.rs)
constructs a readdir result before applying the offset, and
[`cow_tree.rs`](../../../../crates/layerfs-workspace/src/cow_tree.rs)
builds the entry list. Wide-directory enumeration needs explicit coverage.
These are source-grounded risks, not measured performance failures.

The old mixed tier 1 performs only a read. Larger prefixes include append then
truncate and create then move then unlink. Those operations can cancel in the
final snapshot, so final-state verification alone cannot prove their behavior.
The replacement uses complete, dependent agent episodes and checks intermediate
observations in verification mode.

## Size envelope

Pending any unit correction, preserve the existing binary fixture units:

```text
MAX_WORKLOAD_FILE_BYTES = 500 * 1024 * 1024 = 524,288,000
MAX_WORKLOAD_TOTAL_BYTES_EXCLUSIVE = 1024 * 1024 * 1024 = 1,073,741,824
```

These caps apply to workload filesystem contents at every initial,
intermediate, and final state, not merely the imported base. Include generated
outputs, temporary replacement files, Git objects/index/locks, sparse logical
lengths, and files later deleted. Conservatively count each hard-link pathname's
logical length. CAS deduplication never reduces the logical-byte count.

The size envelope describes the tested filesystem's logical contents. Physical
`store.sqlite`, spools, binaries, logs, input caches, and verifier artifacts
have separately reported disk/resource budgets; they are not hidden workload
files. A literal 500 MiB limit on the physical Store file would already reject
released Store-footprint controls. Do not describe this envelope as a limit on
total test-process disk usage.

For every recipe, precompute a conservative transient byte bound and reject
oversized fixture/schedule plans before execution. Verification must check
operation-boundary lengths and byte sums, including the point when a temporary
replacement coexists with its target; final census alone is insufficient.
Stream expected bytes and digests instead of building an extra full expected
copy inside the workload. The qualifier must also bound external fixture and
verification disk use separately.

### Inherited growth cases

Five frozen 500 MiB input rows exceed the file cap after editing:

| Operations | Original result bytes | Capped replacement input bytes | Replacement result bytes |
| --- | ---: | ---: | ---: |
| insert, append, prepend, zero-extend by 4 KiB | 524,292,096 | 524,283,904 | 524,288,000 |
| replace 2 KiB with 4 KiB | 524,290,048 | 524,285,952 | 524,288,000 |

Use five versioned capped-result replacements for future capped runs, with new
scenario/fixture/plan identities and a versioned family manifest. Prepare the
shorter deterministic prefixes outside timing; do not add a truncate to the
measured edit. Name their actual input and final sizes. The original five rows
remain historical and are explicitly excluded from this capped campaign; all
other 51 SDK rows retain their existing definitions. A capped replacement is
not directly pooled with the old oversized result. A paired claim requires both
source arms to run the same new definition.

Do not claim the inherited complete 32-row length-changing family was rerun
unchanged: it has a cap-driven versioned replacement. Released namespace and
Store controls retain their declared logical sizes and separate resource scope.

## Shared whole-Workspace fixture

Reuse four-tier selection, deterministic generation, prepared-input custody,
independent writable samples, and the existing public SDK/FUSE lifecycle.
Add a bounded tree fixture profile using the existing byte generator:

```text
one shard = 200 regular files = exactly 1 MiB
128 files * 1 KiB + 64 files * 8 KiB + 8 files * 48 KiB = 1 MiB

shards       1       10       100       500
files      200    2,000    20,000   100,000
payload  1 MiB  10 MiB   100 MiB   500 MiB
```

Derive file bytes independently from framed shard/path seeds; do not duplicate
one shard's payload to manufacture reuse. Four tiers use nested shard prefixes.
Use a frozen placement with a wide directory containing enough entries to
force multiple FUSE readdir pages, a spine of 128 short components, and regular
sibling directories. Directory shape is fixed within this profile, not another
Cartesian matrix. Respect the existing 256-component/4 KiB path limits.

Metadata scan, full content read, background-size locality, and distributed
SDK edits share these fixtures when identity and preparation compatibility
match. Creation/import timing still performs real creation/import; it cannot
consume a post-operation cache. Fixture hashes and per-tier manifests must be
qualified before implementation is admitted.

## Proposed timed inventory

Every curve has exactly four tiers: 1/10/100/500 of its declared unit. A timed
case uses one final Commit attempt; full verification, fault injection, and
reopen are separate modes. Three samples per new timed case gives 168 new
performance rows per candidate campaign. Repetitions are not size tiers.

| Family | Curves and exact scaling question | Cases | Disposition |
| --- | --- | ---: | --- |
| Payload create/read | Create one 1/10/100/500 MiB file; issue 1/10/100/500 random 4 KiB reads against one fixed 500 MiB file. | 8 | Keep. Random-read coverage makes no whole-file throughput claim. |
| Namespace dedup locality | Import 1/10/100/500 independently written 1 MiB files: one base plus localized variants. | 4 | Keep; shrink the draft's 5 MB/file fixture. Maximum logical content is 500 MiB. |
| Tiny-file operations | Create, `lstat`, and unlink 1/10/100/500 selected paths in a fixed substantial background tree. | 12 | Keep all three diagnostics; mixed workflows do not isolate lookup cost. |
| Directory construction and Workspace reads | Construct 1/10/100/500 irregular chains; scan metadata for the complete 1/10/100/500-shard Workspace; stream every file's contents in that same whole-tree curve. | 12 | Keep construct; replace selected-subtree traversal; add four content-read cases. Metadata and content scans are distinct curves. |
| Git workflow | 1/10/100/500 changed paths in a fixed mostly unchanged repository; status/diff/add/check/commit/status, then LayerFS Commit. | 4 | Strengthen background and authentic editor replacement; retain ordinary Git operations. |
| Subtree mutation | Relocate one populated subtree and delete another; scale each affected subtree to 200 × N files against a fixed untouched background. | 4 | Replace small independent rename/create/delete cells. Attribute move and delete phases separately; Commit covers both. |
| Workspace change locality | One fixed small structural change against N-shard Workspace size; N singular SDK edits in distinct files across a fixed 500-shard Workspace. | 8 | Add. Separate total-tree size from dirty-file count. |
| Agent work episodes | 1/10/100/500 complete dependent work episodes across an existing Workspace, then one Commit. | 4 | Replace raw-operation prefixes; tier 1 exercises the complete episode. |
| **Total** | **14 curves × four tiers** | **56** | **Eight timed families** |

Inherited payload anchors, namespace controls, Store controls, and SDK edit
cases are accounted for separately and only once. Do not inflate the new-case
count with existing controls, samples, source arms, or proof cohorts.

### Concrete workload boundaries

- **Tiny files:** use a fixed 100-shard/100 MiB background. Keep the existing
  diverse 0–8 KiB size cycle. Target paths are a separate bounded set; metadata
  lookup never hides creation or payload reading inside its timer.
- **Workspace scans:** enumerate every path exactly once and retain explicit
  readdir page/entry counts. Metadata scan reads no payload; content scan streams
  all payload. Both end `UpToDate`, providing clean-Commit coverage without
  another no-op performance family. Hashes are computed in verification only;
  performance records cheap byte/count receipts.
- **Constant-change locality:** move exactly one small prepared file between
  two fixed sibling directories, independent of N. Measure workload and Commit
  separately; verify every untouched path and track metadata/payload/object work.
  The test must not scale the changed frontier together with background size.
- **Distributed SDK edits:** at each N, overwrite 4 KiB in N distinct eligible
  files spread across directories in the fixed 500-shard tree. Invoke N singular
  `Client::edit_workspace_file_range` calls, not an invented multi-file batch.
  The real public batch API is same-file only. No Exec remains active during
  owner edits. Report aggregate edit and one Commit latency separately.
- **Subtree mutation:** use a fixed 100,000 × 2,500-byte untouched background;
  each of the two affected trees has 200 × N × 1 KiB payload. At N=500 the
  initial total is 454,800,000 bytes. Move tree A to another parent and delete
  tree B. Basic create and file-rename semantics remain covered by tiny files,
  directory construction, Git, and episode verification.
- **Git:** cap the prepared tracked background at 32 MiB, plus bounded changed
  targets. Reserve and verify a total `.git`/worktree/temp envelope below
  256 MiB across setup and workload, without relying on compression. Fix Git
  version/config and background maintenance. Some modifications must use a real
  bounded editor save (write temporary file, sync, rename over target).
- **Agent episode:** read source, edit a small file, observe it through an alias,
  move its containing directory, read through the new name, atomically replace
  another file, and retain a durable output. Include an append/truncate or
  temporary-create/remove pair only with intermediate verification. Use new
  small targets per episode within a 64 MiB background and a precomputed
  <128 MiB peak; all episode operations precede the single Commit. Do not make
  each episode own another 500 MiB file or rely only on cancelling mutations.

The 500-unit episode curve is one composite workload execution. Repeated
public Exec calls are a separate resource/lifecycle proof below, not another
performance curve.

## Proof inventory: 15 named recipes

Failure recipes use one moderate whole-Workspace fixture and deterministic
barriers/fault points. Do not multiply them by four sizes unless a boundary
cannot otherwise be reached. Reuse and extend existing focused checks rather
than creating duplicate one-file tests.

| Group | Recipe | Required independent evidence |
| --- | --- | --- |
| Dedup (2) | Mechanisms; preexisting-content reuse | Keep the two existing recipes, exact transcripts and separate seed/cohort results. |
| Links (2) | Hard-link alias lifecycle; symlink lookup semantics | Hard-link writes observed through aliases, unlink one alias without losing the other, exact link classes/counts after reopen; relative/dangling/cyclic symlink behavior with stable supported errors and exact targets. |
| Metadata (1) | Chmod/mtime/xattr cohorts | Combine setup into one proof, retain three independently attributed outcomes. Xattr may accept documented unsupported behavior, not fabricated success. |
| Session (2) | Dirty-but-net-zero; 500 sequential Exec calls | Restore bytes and every affected file/directory mtime before expecting `UpToDate`; repeated Exec uses bounded output, releases readers, observes resource bounds and final cleanup, and commits one surviving output. |
| Reliability (8) | The eight recipes below | Whole-Workspace atomicity, failure containment, integrity, and cleanup; not host-crash durability. |

Reliability recipes have **13 isolated fault/concurrency subcases**:

| Recipe | Subcases | Required behavior |
| --- | ---: | --- |
| `workspace-publication-failure-retry` | 3 | Fail candidate construction, a later admission batch after earlier object admission, and final publication. Head/visible snapshot unchanged; dirty Workspace intact; retry publishes exactly once, then `UpToDate`. Unreachable previously admitted objects may remain. |
| `workspace-published-presentation-failure` | 1 | Publication succeeds but real-FUSE presentation refresh fails. Report `Created` with presentation failure; recover the projection and prove exact committed state without a duplicate Commit. |
| `workspace-dirty-end-discard` | 1 | Publish A, continue editing to B in the same Workspace; clean End rejects dirty B; Discard removes B and releases resources; orderly reopen yields A. |
| `workspace-commit-busy` | 2 | Hold a writable FUSE handle, then separately a managed execution behind a barrier. Commit rejects busy work without losing edits; release/finish, then publish exactly. |
| `workspace-write-sync-failure` | 2 | Inject short spool append and bounded NoSpace. Preserve the prescribed failed-operation state and earlier successful work; propagate deferred error to the declared barrier/Commit path. Never fill the host disk. |
| `workspace-dirty-runtime-disconnect` | 1 | Keep Store owner alive; interrupt the dirty workload/daemon route. No accidental publication; explicit error, Discard, exact prior snapshot and reusable lease/mount path. |
| `workspace-descendant-integrity` | 2 | In separate disposable Store copies, corrupt then remove a referenced non-root payload object. Reads fail closed; never substitute zeros or silently accept altered bytes. |
| `workspace-parallel-tools` | 1 | Four workers use disjoint subtrees plus a deterministic closed-file handoff. All workers finish before Commit; independent full-tree oracle and cleanup pass. |

A shared 32–64 MiB reliability tree with hundreds or thousands of paths is
sufficient unless a specific production batch/spill boundary requires more
entries. Prove the relevant boundary was actually crossed with counters; do
not assume fixture size proves it. Fault injection belongs only to verification
and must reuse legitimate test seams, not contaminate product timing or bypass
production integrity checks.

Existing native tests already cover small invalid-edit cases, same-owner lease
exclusion, root corruption, Store publication compare-and-swap, and managed
attach failure. Keep those regressions without cloning them into new four-tier
families. Extend the missing integrated path instead:

- [Workspace file-edit tests](../../../../crates/layerfs-workspace/tests/file_edit.rs)
  cover small failure/discard cases; extend failure atomicity across dirty paths
  and admission batches.
- [Reconciliation tests](../../../../crates/layerfs-workspace/tests/reconciliation.rs)
  and [lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs)
  cover publication/presentation distinctions; add the ordinary real-FUSE path.
- [FUSE proxy tests](../../../../crates/layerfs-fuse/tests/proxy.rs) and
  [file I/O](../../../../crates/layerfs-workspace/src/file_io.rs) cover deferred
  errors and short appends; prove composition with a live Workspace.
- [Live Docker test](../../../../crates/layerfs-sdk/tests/live_docker.rs)
  covers attachment/disconnect cleanup; extend dirty prior state and long use.
- [Store tests](../../../../crates/layerfs-layerstack-store/tests/v4.rs)
  cover root integrity; add referenced-descendant failure propagation.

## What is removed or consolidated

- Remove four standalone link-creation timing cases; keep stronger alias and
  symlink proofs plus link interactions inside whole-Workspace episodes.
- Replace four selected-subtree traversal rows with whole-Workspace metadata
  scans; no additional standalone clean no-op timing family.
- Replace four tiny independent namespace-mutation mixtures with populated
  subtree relocation/deletion; no duplicate create/delete micro-curves.
- Replace four raw-operation mixed prefixes with four complete-episode cases.
- Combine three metadata proof lifecycles into one recipe with three outcomes.
- Add no new single-file overwrite/append/prepend/truncate families: inherited
  SDK coverage already owns those operations.
- Keep tiny `lstat`: its lookup-only latency is distinguishable from directory
  enumeration and payload reads. Per-class counters keep composed cases useful
  without replacing the isolated diagnostic.

## Oracles and metrics

Each whole-Workspace verifier compares an independently constructed manifest
of every path: type, size, streamed byte digest, mode, normalized timestamp,
symlink target, and hard-link equivalence class. Include untouched sentinels;
a digest of changed files alone is insufficient. A candidate-produced root is
not its own independent correctness oracle.

Record workload and Commit separately; record full lifecycle, verifier, and
external supervision separately. Require counts of affected/visited paths,
public SDK calls and Execs, FUSE requests/bytes, scanned bytes, candidates,
inserted/reused objects, transaction/batch maxima, RSS/cgroup, spool and Store
growth, and cleanup. Missing observability is unknown evidence, never zero.

Whole-tree scans may scale with all entries/bytes. Constant-change locality
must reveal unexpected dependence on untouched state. Exact unchanged subtree
identities and work counters support interpretation; set performance ceilings
from the untouched baseline before candidate optimization. Do not invent fixed
microsecond limits or weaken a gate after observing candidate failure.

Verification may probe intermediate states and inject faults. Performance must
not run manifests, digest computation, reopen, or fault injection. A verifier
failure blocks admission even when performance data was otherwise valid.

## Durability decision and scope

The released [storage contract](../../../versioned/0.1.0/storage-format.md)
uses `journal_mode=MEMORY` and `synchronous=OFF`, explicitly excluding process
crash, OS crash, power-loss, and recovery durability. Clean reopen and atomic
live-process rollback do not establish those guarantees.

This proposal can establish orderly persistence, live-process publication
atomicity, integrity checking, and dirty-runtime failure containment. It cannot
justify calling acknowledged state crash-safe. If agent-host crash survival is
required for the intended load-bearing product, treat durability as an explicit
product/storage decision before that claim: define the acknowledgement and
recovery contract, then qualify isolated interruptions before/during/after
publication and acknowledged-state recovery. Physical power-loss claims need
stronger evidence than killing one child process. Do not silently change the
released compatibility profile under a benchmark task.

Timed families keep one Branch and one final Commit. Reliability proofs may
retry, check `UpToDate`, continue after a successful Commit, or run several
workers inside one Workspace. Those state transitions are essential correctness
coverage; they do not introduce v0.1.4 history-depth or Branch-fan-out curves.

The general SDK-edit rule and ordinary-tool workflows currently need a narrow
clarification before implementation: SDK edit claims keep their strict SDK-only
entrypoint, while separately registered POSIX/FUSE tool workflows exercise and
report their actual filesystem route. This review does not silently weaken the
SDK-only benchmark rule or replace a tool's writes with SDK calls.
