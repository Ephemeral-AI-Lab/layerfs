# LayerFS V2 pull/name refinement terminal report

Status: **PASS**

Source identity:

```text
base commit  b498b7253bb94ec4e24840a15b825fe00334c7dd
source seal  0738ea50570bd7b33bb7d389fe29d4e011bda9e589791bcf17710d5626afba24
```

The source seal covers the exact Cargo manifests, lockfile, `crates/**`, and
`tools/**` used by the final tests and benchmark. The dirty worktree is
intentional; no commit or push was performed.

## Reconciled architecture

V2 now has exactly two durable databases:

```text
LayerStackStore  authority for named LayerStacks, Layers, pushed named
                 Branches, Commits, and complete canonical objects

BranchStore      exact pulled LayerStack prefixes, local/remote named Branch
                 histories, Commits, local/replicated objects, complete-root
                 receipts, and receiver-local serving scopes
```

Workspace remains ephemeral and has no database. One Client binds one authority
endpoint, one BranchStore, one immutable parent route, one Monitor, and one
worker per active Workspace UUID.

The cold schema is `user_version=3`:

```text
LayerStackStore  6 tables / 25 columns
BranchStore      9 tables / 33 columns
```

Immutable named facts are separate from mutable heads and receiver-local scope:

```text
LayerStackFact(id, name)
BranchFact(id, layer_stack_id, name, immutable origin)
LayerStackScope(through_layer_id, serving_mode)
BranchScope(Local | Remote(through_commit_id, serving_mode))
```

LayerStack names are authority-wide unique. Branch names are unique within
immutable LayerStackId ownership. Names do not affect IDs, canonical objects,
CDC, or filesystem identity.

## Pull, Fork, and Push result

Layer Pull imports the complete Layer prefix through the requested boundary.
Replica completes the missing union of every selected Layer root.

Explicit Branch Pull preserves authority BranchId/name/ownership and imports
complete visible inherited ancestry, origin facts, and every required Layer
prefix through the exact Commit. Replica makes every selected historical Commit
and Layer root offline-complete.

Pull outcomes are exact:

```text
Created
Advanced
ModeChanged
UpToDate
AlreadyContained
HeadMoved
```

Older boundaries neither move the scope backward nor change its mode.
Reference→Replica completes prior local history plus the remote suffix.
Replica→Reference changes policy without deleting objects or receipts.

Fork is named and local-only. It has no endpoint or placement, performs no
hidden Pull or object copy, and always creates a fresh BranchId.

Full inherited history and the locally owned lane are separate traversals over
one stored history. Pull, Diff, and Fork membership use full history. Push uses
only the locally authored suffix and never retransmits pulled ancestry.

Facts, objects, and complete-root receipts publish before the final scope or
authority CAS. Network calls, history enumeration, closure walks, hashing, and
unbounded materialization are outside SQLite write transactions.

## SDK and CLI

The exact SDK and CLI families are frozen in:

```text
docs/v2/sdk-cli-operation-families.md
```

Key SDK signatures are:

```rust
initialize_layerstack(name, source) -> InitializeLayerStackResult
pull_layer(through_layer_id, placement) -> PullLayerResult
pull_branch(branch_id, through_commit_id, placement) -> PullBranchResult
fork_branch(name, LocalForkSource) -> BranchId
push_branch(branch_id) -> PushResult
add_layer(branch_id) -> AddLayerResult
diff(DiffRequest) -> OperationHandle
query(Query) -> QueryPage
```

CLI initialization and Fork require names. Pull requires `--through` and
exactly one of `--reference` or `--replica`. Fork forbids placement. Completion
shows qualified names while substituting exact IDs. Query JSON schema version 3
emits structured name/scope/mode/through fields and parses as JSON Lines.

## Algorithm audit

PASS:

- deterministic Layer/Commit identity is recomputed before admission;
- full immutable fact equality, not ID-only membership, is enforced;
- complete Layer/Commit histories page beyond 512 without truncation;
- exact boundaries remain pinned after authority advancement;
- C3→C6 incremental history enumerates exactly C4..C6 remotely;
- full-history membership and owned-lane Push use distinct stop conditions;
- scope and authority publication are visibility-last and exact-CAS;
- Branch, Layer, and WorkingTree conflict choices are distinct;
- affected-path fingerprints cover content, type, metadata, directory subtree,
  and alias/hard-link state;
- unrelated mutations preserve a resolution; affected/ancestor/descendant
  mutations invalidate it;
- one writable Branch lease is shared across Clients.

## Deduplication audit

PASS:

- one spillable Seen domain covers every root in a Pull or Push operation;
- duplicate roots and shared descendants are traversed once;
- facts and objects use independently bounded missing-only equations;
- known complete roots are pruned without weakening authentication;
- canonical objects are never rechunked, reminted, or copied per scope;
- local Fork creates zero transfer receipts and copies zero objects;
- Push receipt proof announces one Commit for a one-Commit local suffix and no
  inherited Commit facts;
- two named LayerStacks in one authority reuse the same canonical CAS objects.

The Monitor fixture reports `12,610` local candidate bytes, `1,261` inserted,
and `11,349` reused: 90% saved and 10.0 logical-to-new-physical for that
deliberate repeated-admission population. This is a focused audit fixture, not
a production workload estimate.

## Placement audit

PASS:

- each visible LayerStack prefix has one through boundary and serving mode;
- each visible remote Branch history has one through boundary and serving mode;
- local Branch scope is writable and carries no remote serving mode;
- unscoped interrupted facts are hidden from record/query pages;
- Replica→Reference retains physical coverage while changing serving policy;
- no per-scope object ownership/refcount/closure tables exist.

In the two-Store Monitor fixture, Reference initially has physical CAS equal to
the cross-Store union and placement factor 1.0. Completing the same closure as
a Replica makes physical CAS exactly twice the union and placement factor 2.0,
which is the expected one-authority-plus-one-replica placement—not duplicate
storage within either CAS.

## Reference versus Replica audit

PASS:

```text
Reference  local-first; exact missing object may fall back to the immutable
           parent route; returned bytes are authenticated

Replica    local-only; retains/calls no parent route; a missing promised object
           is Integrity
```

Mixed-root reconciliation preserves each root's policy. A complete reader
retains no parent. Offline Replica proof covers historical inherited Commits and
required Layer roots, not only the selected head.

Pulled remote Branches are read-only: query, Diff, mount, and read work; Commit
and Push return typed `ReadOnlyBranch`. A named local Fork is required for
writes.

## Workspace and filesystem proof

PASS:

- real container FUSE mounted Reference and Replica simultaneously;
- reads, writes, hard links, symlinks, Commit, and End passed on both;
- explicit materialization passed metadata, hard-link, symlink, and content
  checks;
- deferred proxy mutation errors surface at the next synchronization boundary;
- normal kernel page cache and complete-directory cache remain active;
- final container cleanup found no FUSE mount, SQLite file, OOM, or OOM-kill.

## Verification gates

Final captured gates:

```text
cargo fmt --all -- --check                                      PASS
cargo test --workspace --all-features                          PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings
                                                                  PASS
git diff --check                                                PASS
```

The deleted-model search is empty for TUI dependencies, old ForkSource,
placement-bearing Branch facts, Project types/tables, and unbounded
`branch_lane()`.

Production Rust source is 32,750 physical lines after excluding inline test
modules and `tests/**`. Every non-SQL/schema production file is at most 1,500
physical lines. `layerfs-storage/src/sql.rs` is the documented SQL/schema
exception at 2,143 lines.

## Final FUSE benchmark

The exact current source was rebuilt for native Linux arm64 and ran the frozen
12-scenario matrix with three samples plus one warmup and randomized targets:

```text
Reference  2,911,947,750 ns  2.62x aggregate vs Cloudflare  12/12 wins
Replica    2,964,575,169 ns  2.57x aggregate vs Cloudflare  12/12 wins
```

Both verifier receipts are `PASS_OPTIMIZED`. The detailed report is:

```text
docs/experiment_report/2026-08-30/
  fs-bench-b498b7253bb94ec4e24840a15b825fe00334c7dd.md
```

## Evidence paths

```text
bench/reference.json
bench/reference.verification.json
bench/replica.json
bench/replica.verification.json
bench/comparison.json
bench/cargo-test.txt
source/source-SHA256SUMS
source/source-verify.txt
verification/02-workspace-tests.txt
verification/03-clippy.txt
verification/cli-query.jsonl
verification/production-loc.txt
verification/loc-ceiling.txt
verification/deleted-model-search.txt
live/container-inspect-before.json
live/container-inspect-after.json
live/runtime-after.txt
```

## Destructive removals and explicit deferrals

The cold replacement retains the already-applied removal of the old
`layerfs-layer-store`, `layerfs-stack-store`, `layerfs-tui`, historical merge
modules, TUI installer, point-Pull behavior, placement-bearing Fork, and
compatibility aliases. None were restored.

Explicitly deferred:

- object garbage collection;
- LayerStack/Branch rename or aliases;
- OverlayFS projection;
- TUI product work.

No external blocker remains.
