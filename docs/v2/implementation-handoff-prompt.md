# LayerFS V2 implementation handoff

Use this document when continuing or reviewing LayerFS V2 in this repository.
The consolidated architecture is `spec.md`; the exact public surface is
`sdk-cli-operation-families.md`. The two refinement documents record the design
proof that led to the consolidated contract and do not preserve the behavior
they superseded.

## Required reading

Read completely, in this order:

1. `docs/v2/spec.md`
2. `docs/v2/pull_refinement.md`
3. `docs/v2/layerstack_branch_name_refinement.md`
4. `docs/v2/sdk-cli-operation-families.md`

Then inspect the current worktree before editing:

```text
git status --short
git diff --stat
git diff
git log -1 --oneline --decorate
rg --files
Cargo.toml
Cargo.lock
every affected Cargo.toml
all callers of every changed public symbol
focused tests and benchmark/evidence tooling
```

The tree may contain user and prior-agent work. Preserve unrelated changes.
Never reset, revert, stage, commit, push, delete, or absorb those changes to
simplify a V2 patch.

The tracked TUI crate and installer are deliberately removed. Do not restore or
replace them. OverlayFS remains deferred; use the real FUSE path.

## Frozen architecture audit

A correct implementation has exactly:

```text
LayerStackStore  6 tables / 25 columns / user_version 3
BranchStore      9 tables / 33 columns / user_version 3
Workspace        no database
```

One context has one parent endpoint, one BranchStore, one immutable parent
route, one Monitor, and one worker per active Workspace UUID.

Confirm the code contains immutable named facts and separate receiver-local
scopes:

```text
LayerStackFact(id, name)
BranchFact(id, layer_stack_id, name, origin)
LayerStackScope(through_layer_id, serving_mode)
BranchScope(Local | Remote(through_commit_id, serving_mode))
```

Confirm names are validated and immutable, LayerStack names are authority-wide
unique, Branch names are LayerStack-scoped unique, and no Project entity exists.

## Pull audit

Layer Pull through `Ln` must import every Layer fact `L1..Ln`. Replica must
complete the union of every selected root, including objects that disappear
from later roots.

Explicit Branch Pull through `Cn` must preserve authority identity/name and
import complete visible inherited ancestry, all origin facts, and every
required Layer prefix. Replica must make every selected Commit and Layer root
offline-complete.

Verify exact transition results:

```text
Created
Advanced
ModeChanged
UpToDate
AlreadyContained
HeadMoved
```

An older request never moves a boundary or changes mode. Replica→Reference
retains physical objects/receipts. Reference→Replica completes prior local
history plus any remote suffix before changing policy.

Incremental Pull must use stop-exclusive endpoints. C3→C6 remotely enumerates
only C4..C6. Facts/objects/receipts publish before the final scope; interruption
must not expose a partial placement.

## Reference and Replica audit

Reference is local-first with exact-missing authenticated parent fallback.
Replica is local-only, retains no parent route, and reports a missing promised
object as `Integrity`. Mixed-root reads preserve each root's policy.

Changing Replica to Reference changes serving policy only. No hidden deletion,
GC, copy, or per-scope refcount is allowed.

## Fork and Push audit

Fork requires a new name and a `LocalForkSource`. It accepts no endpoint or
placement, performs no network call or object copy, and always creates a fresh
BranchId.

Full visible history and locally owned lane are separate traversals over the
same stored history. Pull, Diff, and Fork membership use full history. Push
uses only the owned lane and stops before the immutable fork boundary or
already acknowledged authority head.

Push must never announce pulled ancestry. It validates origin, ownership, name,
base/head, Commit membership, and complete roots before the final authority
transaction. Final publication is a small exact CAS. An empty inherited Fork
does not create an authority row.

## Workspace audit

Remote Branches are readable and mountable but Commit/Push return typed
`ReadOnlyBranch`. A local Fork is required for edits.

One shared writable lease per Branch and exact head/base CAS prevent lost
updates. Real FUSE and materialization use the same root-keyed reader.

Reconciliation must keep three choices distinct:

```text
Branch
Layer
WorkingTree
```

Only a later mutation intersecting an affected path invalidates a choice.
Unresolved Commit is refused. No conflict state enters either database.

## Performance and transaction audit

Require independently measured missing-only equations for objects and every
fact kind. One operation-scoped spillable Seen domain must deduplicate the
union of all required roots. Histories and memberships are paged; large history
uses bounded temporary storage, not an unbounded Vec.

No SQLite writer transaction may contain network I/O, history enumeration,
object-closure traversal, deterministic identity hashing, or unbounded
materialization. Query continuation SQL must seek through an index rather than
scan from the beginning.

## SDK, CLI, and Monitor audit

Confirm the exact grammar in `sdk-cli-operation-families.md`:

- named initialization;
- Pull uses `--through` and exactly one serving flag;
- explicit Branch Pull exists;
- Fork requires `--name` and forbids serving flags;
- Query exposes authority and receiver LayerStacks/Layers/Branches/Commits;
- completion shows qualified names but substitutes typed IDs;
- JSON schema version 3 emits structured scope/name/boundary fields;
- no deleted alias parses.

Operation receipts name creation names, IDs, through boundaries, modes, and
outcomes. Passive Monitor snapshot performs zero Store SQL. Explicit analysis
separates physical placement from serving policy and reports exact CAS/transfer
equations when denominators are present.

## Required verification loop

After each semantic change, run the smallest failing test and its direct
dependents. Diagnose root cause before widening the gate. At terminal run:

```text
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Also run focused proof for:

- exact schema/name/wire/full-fact collision behavior;
- >512 Layer and Commit histories;
- older/equal/newer/incomparable boundaries;
- both serving-mode transitions;
- interruption and visibility-last;
- offline historical Replica and missing-object Integrity;
- incremental Pull and Push suffix-only receipts;
- zero-copy local Fork;
- multi-LayerStack isolation and same Branch name across LayerStacks;
- query page uniqueness/continuations/plans;
- structured JSON and named completion;
- Branch/Layer/WorkingTree resolution and path-scoped invalidation;
- shared Workspace lease, materialization, live FUSE, and Docker when available;
- passive Monitor zero-SQL and exact dedup/placement accounting.

Finally rerun the current FUSE benchmark. Record the commit, full dirty-source
seal, UTC/local timestamp, host/runtime configuration, commands, raw artifact
paths, medians, findings, and comparison provenance in a commit-named file
under `docs/experiment_report/2026-08-30/`. Never substitute an earlier LayerFS
measurement for a current-code run.

A compile-only, unit-only, design-review, plausible-prose, or partial migration
is not terminal. Any failed gate, structural violation, missing proof, or
benchmark regression requires correction and another focused verification
cycle. Stop only for a genuine external capability blocker after exhausting
safe in-repository alternatives, and report the exact external action required.
