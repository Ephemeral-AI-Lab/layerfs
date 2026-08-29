# Handoff prompt — implement LayerFS Phase One to terminal pass

Use the following as the initial prompt for the Phase One implementation agent.
This is an execution wrapper, not a second architecture specification. When it
conflicts with a binding document, the binding document wins.

---

You are the sole implementation owner for LayerFS Phase One in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
```

Your objective is to cold-implement and verify the complete LayerFS backend,
real FUSE paths, portable materialization, monitoring, thin SDK, standalone CLI,
and frozen frontend contract. Produce the simplest coherent final architecture
with the least justified production code, tables, columns, public types,
methods, arguments, transactions, round trips, and duplicated responsibilities.
Do not optimize for the smallest patch against the current code.

Continue until Phase One reaches a genuine terminal pass. `REVISE`, `No-Go`, a
failed test, conflict, benchmark regression, or structural violation requires
replanning and correction; it is not a stopping condition. Stop only for a
genuine external blocker after exhausting safe in-repository alternatives and
all independent work, and report the exact external action required.

## 1. Read before editing

Read these files completely in this authority order:

1. `docs/cli-tui/00-overview.md`
2. `docs/cli-tui/01-topology-and-source-tree.md`
3. `docs/cli-tui/02-cli-sdk-contract.md`
4. `docs/cli-tui/04-monitoring-dedup-performance.md`
5. `docs/cli-tui/05-implementation-plan.md`
6. `docs/cli-tui/03-tui-design.md` only to understand the frozen frontend
   contract that Phase Two will consume; do not implement its UI

Treat the exact source-tree manifest in `01-topology-and-source-tree.md`, with
`layerfs-tui` excluded, and stages B0–B10 in `05-implementation-plan.md` as
binding. Reconcile or explicitly supersede conflicting older application-facing
documents before allowing two architectures to remain normative.

Inspect the current repository before changes:

```text
git status --short
git diff --stat
git diff
rg --files
Cargo.toml and every affected Cargo.toml
all callers of each public type/function before moving or deleting it
current focused tests and benchmark tooling
```

The worktree may already contain user or previous-agent changes. Preserve all
unrelated changes. Never reset, revert, delete, overwrite, or absorb them merely
to simplify your patch.

## 2. Frozen mental model

LayerFS is the coordinator, not a fifth Store or durable history:

```text
LayerFS context host
    |-- one Store connection graph
    |-- one Monitor collector
    `-- one Workspace worker per active UUID

Workspace final delta -> Branch Commit -> optional Stack -> Layer
```

Responsibilities:

```text
LayerStore
    authoritative LayerHistory and accepted Layers

StackStore
    optional intermediate StackHistory and Stacks
    not a LayerStore cache

BranchStore
    Branch heads, immutable Commits and locally created objects

Workspace
    one ephemeral COW transaction pinned to one Branch head
    no database and no durable tool-operation history
```

Routes:

```text
direct:  Workspace -> BranchStore -> LayerStore
stacked: Workspace -> BranchStore -> StackStore -> LayerStore
```

Navigation/ownership points from Layer toward Workspace. Publication points from
Workspace toward Layer. Every BranchStore has one immutable parent route and is
never silently reparented.

Layered reads are zero-copy across Store boundaries:

```text
Workspace COW
    -> BranchStore
        -> optional StackStore
            -> LayerStore
```

Fallback occurs only on an exact missing-object result, never on corruption or
another local error. Required copies in two or three physical Store databases
are placement, not failed deduplication.

The verb grammar is fixed:

```text
Commit = final Workspace delta -> Branch result
Merge  = Branch -> Branch integration
Push   = transfer missing immutable data toward authority
Pull   = transfer selected immutable state toward work
Add    = conflict-check and accept one Stack or Layer
End    = cleanup only
```

Push never performs Add or Merge. Pull never performs hidden Merge. End never
constructs a delta or commits.

## 3. Phase boundary

Implement every Phase One package and tool except `layerfs-tui`:

```text
layerfs-content
layerfs-storage
layerfs-layer-store
layerfs-stack-store
layerfs-branch-store
layerfs-workspace
layerfs-fuse
layerfs-materialization
layerfs-monitor
layerfs-sdk
layerfs-cli
layerfs-eval
```

Do not create, scaffold, compile, test, or depend on `layerfs-tui`, Ratatui, or
Crossterm. Phase One ends with a complete headless product and a non-Ratatui
frontend-contract fixture. Do not start Phase Two.

APFS immutable-base/clone acceleration, GC, rollback policy, automatic retry,
and elaborate crash/network recovery are deferred. Do not scaffold them.

## 4. Final architecture, not compatibility

Destructively converge on the exact final tree and ownership. Delete obsolete
packages, files, façades, aliases, duplicate algorithms, and compatibility
wrappers once current in-repository callers have moved and focused parity is
proved. Do not preserve old shapes merely because they compile today.

Forbidden outcomes include:

```text
two Content/Storage architectures
Direct and Stacked product façades beside the semantic SDK
ambiguous Store::open
raw FullStorage/SQL/object-transfer application APIs
layerfs-sync, layerfs-transfer, layerfs-server or Project packages
second Workspace/Core/Overlay package
Workspace, Monitor, Docker or FUSE implementation inside SDK
Store or SQLite access inside FUSE proxy/container
Workspace/output/monitor/retry/recovery tables in any Store
god files or implementation-bearing lib.rs/mod.rs/main.rs
common.rs, utils.rs, manager.rs, product.rs or repository.rs catch-alls
handwritten production files over 1,500 lines, excluding sql.rs and schema.rs
TUI-only backend APIs
```

Prefer deletion, standard-library/platform behavior, existing dependencies, and
one shared owner. Add an abstraction only when at least two real implementations
or consumers require it. Fix root causes in the shared owner rather than adding
guards to every caller.

## 5. Smallest public contract

Implement exactly:

```text
6 typed Store constructors
12 application-facing semantic Store operations
3 explicit Workspace lifecycle operations
3 Workspace execution operations
4 Workspace read operations
2 Monitor public queries
one concrete CliSession/frontend seam
the compact CLI grammar in 02-cli-sdk-contract.md
```

Do not add aliases or convenience methods beyond the frozen contract. Derive
history IDs and expected heads internally. Keep exact CAS in lower Store
primitives without exposing expected-head arguments to ordinary users.

The Workspace lifecycle names and semantics are binding:

```rust
create_workspace_session(request)
commit_workspace_session(session_id)
end_workspace_session(session_id, Clean | Discard)
```

No `begin`, `finalize`, implicit capture, or Commit-on-End/unmount/exit alias.

## 6. Final-delta Commit is the center

Workspace history is final-state history, not tool-operation history.

`commit_workspace_session` must:

```text
stop new execution/write admission
boundedly drain already-entered short callbacks
return WorkspaceBusy for active execution/shell/long writer without waiting
reopen admission and preserve state on Busy
quiesce and freeze the mutation generation
compare final view with the pinned base root
collapse intermediate/cancelled writes, create/delete and rename/undo
emit one canonical-path-ordered final delta
stream changed final files through the one CDC/canonical path
reuse unchanged base subtrees
deterministically rebuild touched tree spines
stage only missing candidate objects
create at most one Commit
exact-CAS the Branch head once
success -> read-only
HeadMoved/error -> preserve the final delta and inspectable Workspace
```

Equivalent final filesystems against the same pinned base, including canonical
metadata and hard-link graph, must produce the same root and reachable
ObjectIds regardless of shell/tool/FUSE operation sequence. Tool calls, command
logs, FUSE requests, timing receipts, and stdout/stderr never enter Commit
identity or a Store schema.

Initially rechunk each changed final file deterministically from its beginning.
Add incremental chunk rejoin only after proving exact identity equivalence and a
measured need.

## 7. Persistent CLI host and Workspace workers

Separate CLI processes must reconnect to active Workspaces. Implement one small
context-scoped host inside `layerfs-cli`, not a new server package:

```text
CLI processes / future TUI
        -> owner-only bounded local typed socket
        -> one context host
              |-- Store handles opened once
              |-- Monitor collector
              `-- Workspace worker w1..wN
```

Require an atomic context lock, `0700` runtime directory, owner-only socket,
peer-UID verification where supported, spawn/READY handshake, deterministic
duplicate-starter behavior, and validated stale PID/socket cleanup while holding
the lock. Cleanup never fabricates recovery or Commit. Refuse disconnect of a
Store with active dependents.

Host, worker, CLI, FUSE helper, mount, or container exit never implies Commit.

## 8. Store, transfer and efficiency invariants

Schemas remain exact:

```text
BranchStore: 3 tables / 9 columns
StackStore:  8 tables / 24 columns
LayerStore:  8 tables / 24 columns
StackStore Full DDL == LayerStore Full DDL
```

Missing-only transfer equations apply independently to objects and every typed
fact kind:

```text
announced = preexisting ⊎ missing
sent      = missing
missing   = inserted ⊎ raced_existing
```

Use indexed fixed-page membership, known-root pruning, child-before-parent
admission, and visible heads last. Never rechunk/re-encode/remint a stored
canonical object during transfer.

Frozen limits:

```text
membership page                              512 same-kind IDs
missing bitmap                               fixed 64 bytes
object batch                                 <=128 rows / normally <=4 MiB
large singleton                              <=16 MiB
fact batch                                   <=128 rows / <=64 KiB
transfer object buffers                      <34 MiB
three-way + candidate/deferred memory         <=8 MiB before spill
one active Store operation                    <42 MiB + frozen SQLite overhead
Workspace final-delta memory                  default cap 8 MiB
live output tail per execution                default cap 1 MiB
```

Enforce the `J`/`F` transaction equations in the implementation plan with SQL
trace tests. Network, CDC, hashing, signature verification, and merge never run
inside a SQLite write transaction. Queued Store callers allocate their working
set only after admission.

Passive monitoring adds zero Store queries/turns/hashes/CDC/transactions.
Explicit Monitor snapshot/analyze may use the typed read-only observation
capability, outside the writer gate and separately timed. Unsupported remote
observation reports unavailable; never estimate.

## 9. FUSE is a priority

Implement one FUSE semantic path used by direct and stacked routes. FUSE owns
only its port, kernel callbacks, inode/handle mapping, mount lifecycle, bounded
session protocol, and thin proxy. It never owns DB access, topology, Branch
Commit, Docker execution, or implicit publication.

Complete and verify:

```text
host-FUSE direct/stacked functional matrix on a capable runner
Docker thin-FUSE direct/stacked functional and authoritative performance matrix
portable materialization functional/control path
multiple Workspaces per Branch
multiple isolated UUID mounts in one trusted FUSE-ready container
container receives no DB path/file/credential
capability isolation and host-side retained output
```

Do not perform per-syscall remote mutation, Push, Add, or whole-database copy.

## 10. Execute stages B0–B10

Follow `05-implementation-plan.md` in dependency order:

```text
B0  freeze contracts and cold-reset packages
B1  Content/Storage foundation and deterministic final-state identity
B2  minimal schemas, Create/Connect, topology and context-host ownership
B3  Store operations, transfer, CAS/CDC deduplication
B4  Workspace COW and final-delta Commit
B5  context host integration, host FUSE and materialization
B6  Docker thin-FUSE
B7  Monitor
B8  thin semantic SDK
B9  complete standalone/reusable CLI
B10 terminal correctness, structure, FUSE, dedup and performance closure
```

After every semantic change or responsibility move:

```text
run the smallest focused test
    -> fix root cause until it passes
    -> run directly dependent tests
    -> continue to the next responsibility
```

Do not repeatedly rerun unchanged passing suites. Run the full workspace only
at meaningful integration closures and the final source freeze.

If you delegate, assign disjoint file ownership, tell every worker that others
are editing the same worktree, and require them not to revert unrelated changes.
Synthesize and verify their claims yourself.

## 11. Evidence and performance

Retain current-source raw evidence for every applicable focused and terminal
gate. Passing prose and historical output are not evidence.

Use the unchanged benchmark:

```text
containers/layerfs-fuse/fs-bench.sh
SHA-256 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef
containers/layerfs-fuse/verify_fs_bench.py
```

Run the four frozen Docker thin-FUSE populations and settings from B10. Save
new evidence under:

```text
poc/evidence/phase-one-backend-<UTC>/
```

Preserve raw JSON/stdout/stderr, verification receipt, checksums, mount and
container inspection, source HEAD plus dirty-tree hash, image/executable
identity, environment, topology/DB locations, semantic roots, Store/Workspace/
Monitor receipts, query plans, inventories, resource measurements, and cleanup
proof. Report p95 only with at least 20 samples. Treat isolated sub-percent
timing differences as noise; never dismiss duplicate bytes, extra round trips,
full scans, whole-copy fallback, unbounded memory, or violated hard gates.

## 12. Terminal proof

At final source freeze run and retain raw output for:

```text
cargo fmt --all -- --check
cargo test -p layerfs-content
cargo test -p layerfs-storage
cargo test -p layerfs-layer-store
cargo test -p layerfs-stack-store
cargo test -p layerfs-branch-store
cargo test -p layerfs-workspace
cargo test -p layerfs-fuse
cargo test -p layerfs-materialization
cargo test -p layerfs-monitor
cargo test -p layerfs-sdk
cargo test -p layerfs-cli
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Also prove from the frozen source:

```text
exact non-TUI source tree and dependency bans
exact 3/9 and 8/24 schemas
six constructors, twelve semantic methods and three Workspace lifecycle calls
direct and stacked headless end-to-end lifecycles
same final state from different operation histories -> same root/ObjectIds
multi-writer HeadMoved preserves Workspace delta
host and Docker real-FUSE gates
ten-install 90% / 10 -> 1 / 10x dedup fixture with required placements separate
bounded transaction, round-trip, memory, output and retention evidence
internally consistent per-process timing fragments correlated by OperationId
non-Ratatui client consumes plans, completion, events, paging and interruption
authoritative fs-bench verifier reports PASS_OPTIMIZED
```

Do not claim terminal pass while any applicable item is missing or unverified.

## 13. Progress and final report

Keep progress reports concise and evidence-led:

```text
current B-stage
what changed and its owner
focused gate and raw result
current blocker/defect, if any
next dependency-ordered action
```

At terminal pass, report:

1. final architecture and destructive changes;
2. exact public API/CLI inventory;
3. schema, source-tree, dependency and LOC proof;
4. direct/stacked Workspace/FUSE correctness evidence;
5. CAS/CDC/dedup/transaction/round-trip/memory evidence;
6. Monitor/storage/timing evidence;
7. authoritative benchmark artifact paths and checksums;
8. complete format/test/Clippy commands and raw results;
9. any explicitly deferred non-Phase-One work.

Do not begin TUI implementation. Hand the frozen CLI frontend contract and
Phase One evidence to the Phase Two owner only after every terminal gate passes.

---
