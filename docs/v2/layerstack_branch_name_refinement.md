# LayerFS V2 LayerStack and Branch name refinement

Status: **adopted into the binding `docs/v2/spec.md`**.

This document adds durable human names to LayerStacks and Branches without
adding a Project entity, Project table, ProjectId, Project Store, alias system,
or rename protocol. The consolidated record, schema, SDK, CLI, query, transfer,
and frontend clauses now live in `docs/v2/spec.md`; this document retains the
detailed naming and scoping proof.

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative.

## 1. Final invariant

The product may call a named LayerStack a project, but the durable model
remains only LayerStack:

```text
project = (LayerStackId, LayerStackName)
```

A named Branch belongs permanently to exactly one LayerStack:

```text
branch = (BranchId, LayerStackId, BranchName)
```

IDs are authoritative machine identity. Names are immutable human labels:

```text
LayerStackId    identifies one LayerStack globally
LayerStackName  identifies it for humans within one LayerStackStore

BranchId        identifies one Branch globally
BranchName      identifies it for humans within one LayerStack
```

No correctness decision, CAS, object identity, Commit identity, Layer identity,
transfer membership, or Workspace binding may substitute a name for its typed
ID.

## 2. No Project architecture

V2 continues to have no:

```text
ProjectId
ProjectRecord
projects table
ProjectStore
Project package
Project SDK family
Project transfer protocol
Project object ownership
Project-specific Workspace implementation
```

Product presentation may say:

```text
Project: api-server
```

The exact durable fact remains:

```text
LayerStack {
    id: SA,
    name: api-server,
    head: L18,
}
```

A saved context may remember a preferred `LayerStackId`, but this is frontend
configuration, not a third durable Store entity. One physical LayerStackStore
may contain many named LayerStacks.

## 3. Shared name type

LayerStack and Branch use one shared validated type because both are CLI,
frontend, transfer, SQL, and display trust boundaries:

```rust
pub struct EntityName(String);
```

The accepted language is:

```regex
^[a-z0-9](?:[a-z0-9._-]{0,61}[a-z0-9])?$
```

Therefore a name:

- contains 1 through 63 ASCII characters;
- starts and ends with a lowercase ASCII letter or digit;
- may contain lowercase letters, digits, `.`, `_`, or `-` internally;
- contains no whitespace, slash, backslash, control byte, terminal escape,
  Unicode normalization ambiguity, or platform path separator;
- is compared byte-for-byte after validation; no locale or case folding is
  involved.

Examples:

```text
valid
    api-server
    web-client
    main
    search-rollout-17
    eval_v2
    release.2026

invalid
    ""
    Main
    feature/foo
    ../escape
    name with spaces
    -leading
    trailing-
```

`EntityName` validates once at construction. Store admission MUST independently
enforce the same invariant against malformed wire or SQL input. CLI,
completion, and frontend code MUST reuse the domain type rather than maintain
slightly different validators.

Names are not paths. Even though the accepted alphabet is path-safe, no Store,
socket, runtime directory, mount path, database path, or container path may be
derived from a name without its owning component's existing path validation.

## 4. LayerStack names

Every LayerStack has one immutable name selected during initialization:

```rust
pub struct LayerStackRecord {
    pub id: LayerStackId,
    pub name: EntityName,
    pub head_layer_id: LayerId,
}
```

LayerStack names are unique within one physical LayerStackStore:

```text
LayerStackStore authority.sqlite
    api-server     -> SA
    web-client     -> SB
    evaluation     -> SC
```

The same name may exist in a different LayerStackStore because a different
Store is a different authority:

```text
authority-a.sqlite / api-server
authority-b.sqlite / api-server
```

The name is not included in `LayerStackId`, `LayerId`, or canonical object
digests. An ID collision with a different name is nevertheless an immutable
fact collision and returns `Integrity`.

LayerStack initialization requires the name at the trust boundary. It creates
the named LayerStack and its genesis Layer atomically:

```rust
initialize_layerstack(
    name: EntityName,
    source: LayerStackInitialization,
) -> InitializeLayerStackResult

struct InitializeLayerStackResult {
    layer_stack_id: LayerStackId,
    genesis_layer_id: LayerId,
}
```

The result contains both IDs so the caller never has to derive or rediscover
the newly created project identity.

## 5. Branch names and permanent LayerStack ownership

Every Branch has one immutable name and one immutable owning LayerStackId:

```rust
pub struct BranchRecord {
    pub id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub name: EntityName,
    pub base_layer_id: LayerId,
    pub head_commit_id: Option<CommitId>,
    pub forked_from_layer_id: Option<LayerId>,
    pub forked_from_branch_id: Option<BranchId>,
    pub forked_from_commit_id: Option<CommitId>,
}
```

Branch names are unique within their LayerStack, not globally within the
Store:

```text
api-server/main       allowed
web-client/main       allowed

api-server/main       first Branch
api-server/main       second Branch -> NameConflict
```

Storing `layer_stack_id` directly on Branch is justified. Although it can be
derived through `base_layer_id`, the direct immutable column:

- makes project-scoped name uniqueness enforceable by SQLite;
- makes Branch listing by LayerStack indexed and bounded;
- prevents a Branch from moving to another LayerStack during base movement;
- avoids repeated joins in CLI/frontend Branch trees and transfer validation;
- gives Pull, Fork, Push, Add, Workspace, and Diff one exact ownership field.

Every base Layer assigned to a Branch MUST have the same `layer_stack_id` as
the Branch. Commit, reconciliation, Push, Pull, and Add MUST reject a mismatch
as `Integrity`; they never rewrite Branch ownership.

## 6. Creation and immutable names

Every local Branch Fork requires a new name and generates a new BranchId:

```rust
fork_branch(
    name: EntityName,
    source: LocalForkSource,
) -> BranchId
```

Layer Fork derives the owning LayerStackId from the exact source Layer:

```text
fork --name main from Layer SA/L12

result
    BranchId: B1
    LayerStackId: SA
    BranchName: main
    origin: SA/L12
```

Branch Fork derives and verifies the owning LayerStackId from the exact source
Branch and Commit:

```text
fork --name search-rollout from SA/main/C17

result
    BranchId: B2
    LayerStackId: SA
    BranchName: search-rollout
    origin: B1/C17
```

Fork MUST reject a requested name already used by another Branch in the same
LayerStack. It MUST NOT append a number, overwrite the existing Branch, reuse
its BranchId, or silently choose another name.

V2 adds no rename operation. Names are immutable after initialization or Fork.
There is no alias, redirect, rename history, local display override, or
authority rename synchronization. A new rollout name requires a new Fork.

## 7. Pull and names

Pull preserves authority names exactly. Reference and Replica acquire the same
named logical facts.

### 7.1 LayerStack Pull

Pull through a Layer imports the exact authority LayerStack name with the
complete Layer prefix:

```text
authority
    LayerStack SA name api-server
    L1 <- ... <- L12

pull SA through L12 as Reference or Replica

receiver
    LayerStack SA name api-server
    L1 <- ... <- L12
```

The receiver supplies no replacement name. An existing local LayerStackId with
a different name is `Integrity`. A different LayerStackId already using
`api-server` in the same receiver is `NameConflict`; Pull never renames either
scope.

### 7.2 Branch Pull

Branch Pull imports the exact authority Branch name and owning LayerStackId:

```text
authority
    api-server/main
    BranchId B1
    through C17

pull B1 through C17 as Replica

receiver remote placement
    api-server/main
    BranchId B1
    through C17
    mode Replica
    read-only
```

The receiver supplies no local Branch name during Pull. A BranchId collision
with a different name or LayerStackId is `Integrity`. A different BranchId
already using the same name in that LayerStack is `NameConflict`.

Pulled inherited Branch-origin facts retain their authority names. Pull MUST
not flatten names, synthesize placeholder Branches, or qualify and store a
second composite name.

### 7.3 Mode and boundary changes

Reference-to-Replica, Replica-to-Reference, and through-boundary advances do
not change LayerStack or Branch names. Placement is receiver-local policy;
name is immutable semantic fact.

## 8. Push and name conflicts

Push transfers a locally owned Branch's exact name and LayerStackId with its
immutable Branch fact. Authority admission validates:

```text
same BranchId, different name or LayerStackId
    -> Integrity

different BranchId, same (LayerStackId, BranchName)
    -> NameConflict

same BranchId and immutable facts, expected head
    -> normal Create, Advance, or UpToDate path
```

`NameConflict` is not `HeadMoved`:

```text
HeadMoved
    same Branch identity
    unexpected mutable Commit head

NameConflict
    different Branch identities
    same human name inside one LayerStack
```

Two BranchStores may independently create `api-server/experiment`. The first
successful Push claims that authority name. The other Push returns the exact
existing and incoming BranchIds and performs no rename, Fork, merge, retry, or
partial Branch visibility.

Add derives the target LayerStack from `branch.layer_stack_id` and verifies the
base Layer belongs to it. A Branch name has no effect on Layer identity or Add
idempotence.

## 9. Schema requirements

Exact combined DDL must be frozen when this refinement and Pull placement are
reconciled into `docs/v2/spec.md`. The minimum semantic columns and constraints
are binding.

LayerStack rows in LayerStackStore and receiver-side LayerStack placement facts
contain:

```sql
layer_stack_id BLOB PRIMARY KEY,
name           TEXT NOT NULL,
head_layer_id  BLOB NOT NULL,
UNIQUE (name)
```

Branch rows contain at least:

```sql
branch_id       BLOB PRIMARY KEY,
layer_stack_id  BLOB NOT NULL,
name            TEXT NOT NULL,
base_layer_id   BLOB NOT NULL,
head_commit_id  BLOB,
-- immutable origin columns
UNIQUE (layer_stack_id, name)
```

DDL MUST enforce the cheap portion of `EntityName` validation:

```sql
CHECK (length(name) BETWEEN 1 AND 63),
CHECK (name = lower(name)),
CHECK (name NOT GLOB '*[^a-z0-9._-]*'),
CHECK (substr(name, 1, 1) GLOB '[a-z0-9]'),
CHECK (substr(name, -1, 1) GLOB '[a-z0-9]')
```

The shared Rust validator remains authoritative for byte-length, encoding, and
wire input. SQL constraints provide defense in depth, not a second name
language.

The schema SHOULD enforce Branch/base Layer ownership with a composite unique
key and foreign key where the final admission order permits it:

```sql
UNIQUE (layer_stack_id, layer_id)

FOREIGN KEY (layer_stack_id, base_layer_id)
    REFERENCES layers(layer_stack_id, layer_id)
```

No names table, project table, alias table, normalized-name table, branch-name
history, or duplicate composite `project/branch` string is allowed.

## 10. Identity and transfer encoding

Names and Branch `layer_stack_id` are immutable fact fields. Exact fact
comparison, signing bytes, wire encoding, transfer membership, collision
checking, and query snapshots include them.

They do not alter these identifiers:

```text
LayerStackId    generated independently of name
BranchId        generated independently of name
LayerId         derived without LayerStackName
CommitId        derived without BranchName
ObjectId        derived only from canonical object bytes
```

This distinction is binding:

```text
name not in ID digest
    does not make name optional or mutable

immutable fact field
    means same ID with different name is Integrity
```

Fact batching and missing-only transfer apply to the enlarged exact facts. A
name never causes canonical filesystem content to be rechunked, reminted,
copied, or transferred twice.

## 11. SDK surface

The minimum changed requests are:

```rust
initialize_layerstack(
    name: EntityName,
    source: LayerStackInitialization,
) -> InitializeLayerStackResult

fork_branch(
    name: EntityName,
    source: LocalForkSource,
) -> BranchId
```

Pull remains ID-based and imports names:

```rust
pull_layer(
    through_layer_id: LayerId,
    placement: RemotePlacement,
) -> PullLayerResult

pull_branch(
    branch_id: BranchId,
    through_commit_id: CommitId,
    placement: RemotePlacement,
) -> PullBranchResult
```

Workspace, Diff, Push, Add, and End remain ID-based. No duplicate `*_by_name`
semantic methods are added. The one paged query seam returns names beside typed
IDs so CLI completion and frontends may resolve user input before planning.

The public error model adds one typed conflict:

```rust
NameConflict {
    layer_stack_id: LayerStackId,
    name: EntityName,
    existing_id: NamedEntityId,
    incoming_id: NamedEntityId,
}
```

The exact error representation may use separate LayerStack and Branch variants
if that avoids an otherwise single-use `NamedEntityId` abstraction. It must
remain typed and include both conflicting IDs.

## 12. CLI grammar

LayerStack initialization requires `--name`:

```text
layerfs layerstack init --name <name> --empty
layerfs layerstack init --name <name> <directory>
```

Every local Branch Fork requires `--name`:

```text
layerfs branch fork --name <name> --layer <layer-id>

layerfs branch fork --name <name> \
    --branch <branch-id> --commit <commit-id>
```

Pull accepts no replacement name:

```text
layerfs layerstack pull --through <layer-id> --reference|--replica

layerfs branch pull <branch-id> --through <commit-id> \
    --reference|--replica
```

Semantic execution continues to carry typed IDs. CLI completion, query output,
plans, receipts, and errors display names with short IDs:

```text
Project: api-server (SA...91)
Branch:  search-rollout (B2...72)
Through: C17
```

Qualified presentation may use:

```text
api-server/main
api-server/search-rollout
web-client/main
```

The CLI MUST NOT store that composite string or pass it as semantic identity.
If name-based input is accepted, parsing resolves it through the paged query
seam to exact IDs before `plan` and reports ambiguity or absence. Scripts may
always use IDs. No alias command or parallel name-based SDK operation is added.

## 13. Workspace, Diff, FUSE, and materialization

A Workspace remains anchored to an exact Commit, or to an initial Layer before
the target Branch's first Commit, and publishes to one exact BranchId. Names
are presentation only:

```text
Workspace W9
    project:       api-server (SA)
    target Branch: search-rollout (B2)
    anchor:        C17
```

A name change cannot move a Workspace because V2 has no rename and the worker
stores typed IDs. Diff, FUSE, materialization, Monitor receipts, and output
correlation likewise carry IDs and decorate their read models with names.

No mount path or container path is inferred from a LayerStack or Branch name.
The caller continues to provide and validate explicit Workspace placement.

## 14. Frontend contract

The frontend lists named LayerStacks as projects without introducing a Project
backend type:

```text
Projects
|- api-server        SA...91  Replica through L18
|- web-client        SB...20  Reference through L11
`- evaluation        SC...44  Replica through L7
```

Within one selected LayerStack, it lists scoped Branch names:

```text
api-server
|- main              B1...18
|- search-rollout    B2...72
`- storage-test      B3...09
```

Frontend selection state stores `LayerStackId`, `BranchId`, `CommitId`,
`LayerId`, and `WorkspaceId`, never names or row indexes. A refresh remaps rows
by typed ID. Names are escaped as ordinary text and never interpreted as ANSI,
markup, a path, or a command.

## 15. Multi-project topology

One physical Store pair may serve many named LayerStacks:

```text
one context
|- LayerStackStore authority.sqlite
|  |- SA api-server
|  |- SB web-client
|  `- SC evaluation
|
`- BranchStore branches.sqlite
   |- SA/main
   |- SA/search-rollout
   |- SB/main
   `- SC/baseline
```

All LayerStacks in that BranchStore share the same immutable parent
LayerStackStore StoreId. Canonical objects deduplicate across named projects
inside one physical database. The names are organization, not isolation.

Projects requiring separate authority, permissions, lifecycle, backup, or
deletion use separate Store pairs and contexts:

```text
context api
    api-authority.sqlite
    api-branches.sqlite

context web
    web-authority.sqlite
    web-branches.sqlite
```

A BranchStore never mixes LayerStacks from different LayerStackStore IDs.

## 16. Required focused proof

The refinement is not implemented until current-source tests prove:

1. valid boundary names of lengths 1 and 63 are accepted and every invalid
   character, case, length, leading, and trailing form is rejected by the
   shared type and Store admission;
2. two LayerStacks with different names coexist in one LayerStackStore;
3. duplicate LayerStack names with different IDs return typed `NameConflict`;
4. two Branches named `main` coexist in different LayerStacks;
5. duplicate Branch names inside one LayerStack return typed `NameConflict`;
6. Layer Fork derives the exact LayerStackId and stores the requested name;
7. Branch Fork preserves the source LayerStackId, creates a fresh BranchId,
   and stores the requested name;
8. Commit, reconciliation, Pull, Push, or Add cannot move a Branch to another
   LayerStack;
9. Reference and Replica LayerStack Pull preserve the exact authority name;
10. Reference and Replica Branch Pull preserve exact Branch and LayerStack
    names and create read-only remote placements;
11. same ID with a different immutable name or LayerStackId is `Integrity`;
12. concurrent Push of different BranchIds with the same project-scoped name
    admits exactly one and returns `NameConflict` for the other;
13. names do not change LayerStackId, BranchId, LayerId, CommitId, ObjectId, CDC
    bytes, or canonical filesystem identity;
14. missing-only fact transfer includes names without retransmitting canonical
    objects;
15. one physical Store pair supports at least two named LayerStacks, Branches
    named `main` in both, concurrent Workspaces, FUSE/materialization, Commit,
    Push, and Add without cross-LayerStack routing;
16. CLI parsing requires names for init/Fork, Pull rejects replacement names,
    completion displays qualified names, and plans execute only resolved IDs;
17. non-Ratatui frontend fixtures preserve selection by ID when newly inserted
    named rows change page ordering;
18. schema inventory, uniqueness indexes, foreign-key checks, and query plans
    match the reconciled exact DDL.

## 17. Required reconciliation

The implementation owner must reconcile, in dependency order:

1. `docs/v2/spec.md` terminology, records, IDs, schemas, operations, SDK, CLI,
   query model, source tree, stages, and terminal gates;
2. `docs/v2/pull_refinement.md` named facts and placement display;
3. `docs/v2/implementation-handoff-prompt.md`;
4. `docs/v2/sdk-cli-operation-families.md`;
5. shared `EntityName`, storage records, fact equality/signing/wire encoding,
   exact DDL, indexes, admission, and errors;
6. LayerStack initialization results and CLI output;
7. local Fork requests and project-scoped Branch uniqueness;
8. Pull name preservation and remote read-only placement;
9. Push `NameConflict`, immutable ownership, and Add routing;
10. queries, completion, plans, receipts, Monitor, SDK tests, CLI tests,
    frontend fixtures, FUSE/materialization coverage, and terminal evidence.

Production code that adds a Project table/type, uses a name as semantic
identity, permits two equal Branch names in one LayerStack, changes a name
during Pull/Push, or omits `layer_stack_id` from Branch ownership is a
structural violation after this refinement.
