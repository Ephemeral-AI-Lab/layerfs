# C5: history metadata, exact stages and scope-wide allocation

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract. This paper selects no architecture, freezes no scope and
> decides no open ruling.

Issue: [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210). Design
parent: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
Implementation specification and its pre-publication audit:
[`proposal/commit-history/`](proposal/commit-history/).

- **Source pin:** remediation based on `35740836f84b9ef687da2f70d785c9e6b2ed2fd2`
  (reviewed product `92e56635ae4559d175fe3cd455f36f9fe6b5b498`), updated alongside
  the source in this remediation commit. The final handoff records its exact
  committed source identity. Reference root `crates/` remains a separate product.
- **Scope:** the replacement product under `core/` only.
- **Method:** source reads plus the crate's own external tests. No benchmark,
  performance or release claim is made here. Anything not established from source
  is recorded as *not implemented* rather than guessed.

## 16.1 What C5 is, and what it is not

`layerfs-history` is one new crate. It owns history identities and records,
LayerStack and Branch metadata, immutable Commits and Layers, exact-token
Workspace stages, scope-wide inode reservations, and the atomic conditional
transitions over them with bounded queries.

It owns nothing else. It opens no content object, reads no pack, starts no save
and interprets no physical field: save IDs, locators, pack IDs, delta selection
and publication remain C2's private ownership, and **no foreign key crosses
between the two databases**. Its semantic operations take typed values and return
typed outcomes; paths, sockets, clocks, process identity and live Workspace state
belong to the adapters that call it.

Dependency direction is one-way and narrow:

```text
layerfs-history ──► layerfs-content   (scalar identity types only)
                 ──► blake3           (domain-separated derivation)
                 ──► rusqlite         (optional, `native` feature)

layerfs-history ◄── layerfs-service   (composition, authorization, bootstrap)
layerfs-bridge  does not depend on layerfs-history at all
```

The `native` feature is the only provider switch. Building the crate with
`--no-default-features` compiles the portable contract and no SQLite provider; it
proves the contract is separable and claims no other backend.

## 16.2 Identity

Encodings are the reference product's, and so are the Commit and Layer
derivations, so a Commit of the same `(root, parent, base Layer)` and a Layer of
the same `(stack, parent, root)` reproduce the reference identity exactly.

| Identity | Width | Tag | Origin |
| --- | --- | --- | --- |
| `LayerStackId` | 17 | `0x31` | authority-supplied 16-byte body |
| `BranchId` | 17 | `0x11` | authority-supplied 16-byte body |
| `CommitId` | 33 | `0x12` | `blake3("layerfs/commit/v2\0" ‖ root ‖ opt(parent) ‖ base_layer)` |
| `LayerId` | 33 | `0x32` | `blake3("layerfs/layer/v2\0" ‖ stack ‖ opt(parent) ‖ root)` |
| `WorkspaceId` | 32 | — | opaque producer incarnation |
| `CatalogId` | 32 | — | `blake3("layerfs/history/catalog/v1\0" ‖ binding_key)` |
| `StageToken` | u64 | — | drawn from the catalog's own counter |

`opt(x)` is `0x00` or `0x01 ‖ x`. A stored value with the wrong width or the
wrong tag is an integrity failure, never a reinterpreted record; the schema
repeats the tag check as a `CHECK` constraint, so a foreign writer cannot store
one either.

The crate never invents an identity from a clock, a PID or `/dev/urandom`. A
Layer's source Branch and source Commit are deliberately **outside** its
identity, so two records that derive the same `LayerId` must agree on every
immutable field including that provenance; a disagreement is an integrity
failure, and the catalog compares the fields rather than overwriting the row.

Names are 1–63 ASCII bytes, lowercase letters, digits and `.`, `_`, `-`,
beginning and ending alphanumeric. The grammar is enforced in Rust and again as
SQL `CHECK` constraints.

## 16.3 Catalog schema 1

One catalog file per configured logical Store/authority binding, holding any
number of stacks. Seven application tables, `PRAGMA application_id = 1279677256`
and `PRAGMA user_version = 1`; C2's `1279677261`/`7` is a different file and is
never touched from here.

| Table | Holds | Constraints that carry meaning |
| --- | --- | --- |
| `history_meta` | singleton: catalog id, incarnation, binding key, identity format, next stage token | one row; token counter inside `1..=i64::MAX` |
| `layer_stacks` | stack id, name, scope, profile, head Layer | unique name; head belongs to the stack (deferred composite FK) |
| `layers` | Layer id, stack, parent, root, source Branch/Commit | genesis has parent and both sources absent and all three present otherwise; one genesis and one child per `(stack, parent)`; one publication per `(source Branch, source Commit)` |
| `commits` | Commit id, stack, root, parent, base Layer | base and parent ancestry in the same stack (composite FKs) |
| `branches` | Branch id, stack, name, base Layer, head Commit | unique `(stack, name)` and `(stack, Branch)`; base and head belong to the stack |
| `workspace_stages` | incarnation, exact token, stack/Branch, frozen expectations, candidate root, profile/scope, generation | one stage per incarnation; unique token; every reference inside the stack |
| `scope_allocator` | scope, high-water mark, authority | one row per scope; authority is the catalog's own identity |

`commits` carries a `layer_stack_id` column that the reference schema derives
through `base_layer_id`. It is present so that "the parent is in the same stack"
and "the base Layer is in the same stack" are composite foreign keys rather than
in-transaction queries; the composite keys make the redundant column impossible
to contradict.

Two invariants have no declarative form and are checked inside the transaction
that could break them: a Branch's head Commit must record the Branch's base
Layer, and a published Layer's root must equal its source Commit's root.

Stack and genesis reference each other, and Layers reference Branches while
Branches reference Layers. Every foreign key is therefore
`DEFERRABLE INITIALLY DEFERRED`, and `PRAGMA foreign_keys = ON` is set and read
back on open. A deferred violation surfaces at `COMMIT` and is reported as
`Integrity`.

The catalog is opened with the embedded profile C2 uses: `journal_mode = MEMORY`,
`synchronous = OFF`, `temp_store = MEMORY`, `foreign_keys = ON`, zero busy
timeout. No WAL, no `fsync`, no added crash-durability work.

Open validates the application identity, the schema version, the exact
schema definitions (tables, columns, STRICT/WITHOUT ROWID properties, indexes,
CHECKs and foreign keys) against the same schema parsed by SQLite, and the
singleton metadata row. Its stored binding must derive the configured catalog
identity and its counters must be inside their declared ranges. Reserved engine
objects use a literal `sqlite_` prefix; similarly named user objects are not hidden. An incomplete, foreign or inconsistent catalog is
refused. Nothing is migrated, repaired or promoted, and `create` never adopts an
existing catalog.

## 16.4 Records and operations

| Operation | Effect |
| --- | --- |
| `init_layerstack` | creates the genesis Layer and the stack in one transaction; no Branch, stage or Commit |
| `fork` | creates one Branch sharing the selected Layer or the selected ancestor Commit's ancestry; copies no content and no history rows |
| `read_branch` | capture one coherent Branch snapshot, release C5, then validate the immutable effective root through C1; return root/profile/scope/root serial |
| `stage_changes` | inserts one frozen stage with a freshly allocated token |
| `commit_staged` | one transaction: validate the frozen expectations, insert or verify the Commit, compare-and-swap the Branch head, delete the exact stage |
| `commit` | the service composes the same staging body and the same commit body under one admission; it is not a second implementation and not a cross-database transaction |
| `add_layer` | validate the selected source, insert the Layer, compare-and-swap the stack head; the Branch is unchanged |
| `discard_stage` | removes exactly one stage token; no content deletion, no serial refund |
| `reserve_inodes` | consumes a checked half-open range for one scope |
| `get_*`, `list_*`, `commit_history`, `layer_history` | bounded pages of typed records |

A stage is a saved candidate plus the publication context it was captured
against, and neither part is optional. An accepted stage always satisfies
`construction_base_root == expected_root` and
`intended_commit_base == expected_base`, and the profile and scope it names are
the stack's. Changing an expected token never rebases content: the construction
base must be the captured effective root.

`commit_staged` reports `UpToDate` when the candidate root equals the Branch's
current effective root and the intended base equals its base; the stage is
consumed and the head does not move, so several no-change stages can each
succeed. Otherwise it inserts the Commit and compare-and-swaps the Branch. A
Branch that moved during a successful save leaves the stage in place with its
original context and the next `commit_staged` reports `HeadMoved`.

`add_layer` checks the exact earlier publication **before** any expected-head
comparison, so asking twice is idempotent even after the stack head advanced. A
new publication first requires Commit base, Branch base and current/expected stack
head to agree; refreshing a head token does not rebase the content. Only then is a
Commit whose root equals its base Layer root `NoChanges`. Immutable/provenance
checks remain on admissible insertion and on exact-source UpToDate; a stale-base
sibling is refused before collision checking, without changing existing rows. Because one Layer
may have one child per stack, a Branch publishes once; the next publication needs
a Branch forked from the Layer it produced, or an explicit future rebase.

Forking from a historical Commit uses **that Commit's** base Layer, never the
source Branch's current base.

## 16.5 Admission, multi-writer and continuity

One short transaction at a time per catalog authority. The provider cell is taken
with a non-blocking attempt, so contention is an immediate `Busy` refusal rather
than a wait, and no transaction ever spans a caller's upload, C1 construction, C2
encoding or `save.finish`. C2 arbitration and C5 locks are never nested, and a
metadata-only command never starts a content save.

```text
request A                          request B
capture Branch snapshot            capture Branch snapshot
C1 -> C2 private save A            C1 -> C2 private save B
                                   finish B; short C5 stage/Commit transaction
finish A; short C5 stage/Commit transaction
```

Continuity is a first native support envelope, not a recovery claim. A fresh
writable catalog is created **inside the process that will own it**; that
in-process handle is the only writable authority, and successive requests and
connections share it. Read-only open and reopen are supported. Arbitrary process
restart, a restored or copied catalog, and authority loss do **not** reacquire
write authority: every mutation and every new allocation on such a handle is
refused with `ContinuityUnavailable`. No clean-exit flag, no `assume_clean` input
and no root scan establishes continuity, and no added `fsync`/WAL or silent
fresh-scope rollover is used as a workaround.

Serials live in `1..=i64::MAX`. A reservation is a half-open range starting after
the scope's high-water mark, the mark advances in the same transaction that
returns the range, and consumption is unconditional: a reservation that is never
used, a stage that is discarded and an operation that fails afterwards all leave
the mark where they found it plus their own count. The terminal value is a
checked `Capacity` refusal before publication: `start + count <= i64::MAX`. Thus
the highest exposable serial is `i64::MAX - 1`; every acknowledged reservation
has a representable exclusive end. There is no recycling, and exposed
unused serials cannot be recovered by scanning roots.

Unknown C5 statement/COMMIT outcomes suppress rusqlite's implicit rollback with
its supported `DropBehavior::Ignore`. Definite refusals attempt rollback once;
failed cleanup becomes UnknownOutcome. An uncertain provider retains one bounded
connection and refuses both reads and writes, so pending rows cannot become
observable as acknowledged state. Connection teardown releases resources; it is
not an operation rollback receipt. Known C2 finish is preserved through later C5
failure and causes no content deletion, retry or guessed stage discard.

## 16.6 Bounded pages and cursors

A page holds at most 128 records and at most 16 KiB of encoded result. The
catalog cuts a page short inside the byte budget and returns a continuation
rather than letting the codec refuse a legal query, and the caller's `limit` is
always an upper bound.

A cursor is a fixed 160-byte v2 value: range tag, catalog identity/incarnation,
three zero-padded bounded fields (subject, immutable anchor, last delivered
position), and a sixteen-byte keyed BLAKE3 MAC. The domain is
`layerfs/history/cursor/v2\0`. Names use immutable Stack/Branch IDs followed by a
checked name lookup, so every legal 1–63-byte name fits. Token pages keep their
exclusive numeric ordering. The authority supplies a nonzero 32-byte secret at
create and read-only reopen; native configuration requires
`LAYERFS_HISTORY_CURSOR_KEY`. It is not derived from public catalog data, stored
in C5, serialized, logged or generated by fallback. Missing capability refuses
open; a wrong key refuses existing continuations. Schema 1 has no key commitment,
so open alone cannot authenticate the key. The authority must retain it externally.

The authenticated anchor/position pair attests to the immutable traversal that
issued the cursor. Resume validates typed identities and ownership, then steps
from the last delivered record to its parent. It neither re-proves ancestry from
the moving live head nor repeatedly walks the immutable anchor. `start=None`
uses the cursor anchor; an explicit start must agree. Ordinary live growth does
not invalidate it; traversal can exceed 4,096 total rows through bounded pages.
There is no process-local cursor registry and old unkeyed v1 cursors are refused.

Ancestry membership is a bounded walk with a 4,096-row work ceiling. Exhausting
the ceiling is `Capacity`, never `NotFound`: an over-budget walk is unproven, and
reporting absence for it would be a false negative.

## 16.7 Service, bridge and authorization

History uses **two grouped opcodes** and its own operation profile, so the legacy
opcode numbering, payload grammar and permission bits are untouched.

| Surface | Value |
| --- | --- |
| `HistoryQuery` opcode | 6, permission bit 5 (`0x20`) |
| `HistoryCommand` opcode | 7, permission bit 6 (`0x40`) |
| Operation profile | 2 (legacy operations keep profile 1) |
| Result tag | 8 (`Response::History`), with a closed inner union |

The permission mapping is total and explicit rather than a shift of an unchecked
opcode, so an unknown opcode has no bit at all. A legacy grant mask of 31 covers
opcodes 1–5 only and therefore grants neither history opcode. Authorization is
Store-wide; no per-Branch ACL is claimed, and catalog/stack/Branch/stage
membership is validated on every suboperation.

Dispatch is exhaustive and semantic. `Operation::read_only`,
`content_mutation` and `metadata_mutation` are separate exhaustive matches, and
the old `opcode >= 3` mutation test is gone. A `HistoryQuery` is read-only, a
`InitLayerStack`, `StageChanges` and composite `Commit` write content; `Fork`,
`CommitStaged`, `AddLayer`, `DiscardStage` and `ReserveInodes` mutate metadata
only. Metadata commands never start a C2 save. HELLO/framing version stays separate from the operation
profile; an unknown profile/suboperation combination is refused before any
mutation, and there is no automatic downgrade or resend.

The daemon's generic framed relay selects history failure encoding from its
validated request profile; it has no second history parser. Legacy failures stay
exactly three bytes. History failures carry bounded typed BranchMoved,
StackMoved, StageChanged or BaseMismatch context plus a separate stage observation:
unobserved, absent, retained complete stage, or acknowledged stage with unknown
final disposition. No mutable reread or message parsing reconstructs this context.
A composite Commit preserves its already acknowledged stage if admission or
publication fails. The native frame cap is 482 bytes for history failures; legacy
decoding retains the three-byte bound. Unexpected ResultData is refused before
caller output for every operation except ReadFile.

History result codecs enforce the complete 16 KiB budget (tags, prefixes and
continuation included) before growth/allocation. Page record minimum/maximum
widths are Stack 117/179, Branch 71/166, Commit 116/149, Layer 85/168, Stage
309/342 bytes. The 32 KiB request envelope and legacy limits remain unchanged.
Candidate profile-2 compatibility changes are frozen in the
[repaired boundary](proposal/commit-history/remediation-contract-20260921.md).
BranchSnapshot appends optional root serial: GetBranch supplies a C1-validated
serial; metadata-only Fork supplies none. StackCreated appends the constructed
root and actual reserved root serial, so earlier scope reservations never cause
an assumed serial 1. Fork performs no post-publication content validation that
could mislabel a known successful metadata transition as abort.

## 16.8 Production namespace bootstrap

Initialization is production code, not the `examples/prepare_store.rs` fixture.
The service builds a bounded, pathless manifest — at most 128 entries including
the root, inside the existing 32 KiB metadata envelope — whose entries name their
parent by index, a canonical component name, a kind, portable mode/mtime and,
per kind, an already published file root or a bounded inline symlink target.

Every serial is assigned by the service from one reservation the C5 catalog
consumed first; no caller supplies a serial and no foreign scope is imported. The
scope itself is derived through C1's `scope_for_seed` from a caller-supplied seed,
so a root descriptor can be checked against the stack's scope later.

Construction is two saves, in this order:

1. one save for the prerequisite attribute trees and symlink targets, finished
   before anything reads them, and
2. one save for the filesystem tree, built from roots that are already published.

The split is deliberate: the build consumes published roots, so nothing assumes a
combined unpublished reader/sink. Earlier successful saves may become
unreferenced if a later step fails; that is the same bounded ownership the
ordinary save path has, and no cleanup is guessed.

Every declared directory receives one DirectoryUpdate, including empty children.
Child bindings are grouped by parent independently of declaration adjacency and
sorted once. This gives empty directories canonical empty pages while retaining
duplicate-name, parent-kind and acyclicity checks.

`stage_changes` initially supports the existing-inode prepared-update surface.
Before calling it, history validates every proposed inode value's semantic role
with bounded public C1 validators: a regular file's content root must open as a
file representation, a symlink's must decode as a stored target, and every kind's
metadata root must decode as an attribute tree whose typed portable fields are
valid for that kind. A stored wrong-role root is refused. This is boundary
validation, not a whole-tree walk.

## 16.9 What this does not implement

Recorded plainly, because each is a limit rather than a plan:

- **No writable service restart.** A catalog created by an earlier process cannot
  be reopened for writing; mutations fail with `ContinuityUnavailable`. A
  trustworthy external continuity handoff is a separate future provider
  capability.
- **No general host import and no arbitrary filesystem-root registration.** The
  manifest is pathless and bounded; a host tree is not scanned and a second
  import algorithm does not exist.
- **No new-inode or new-symlink operation on a live Workspace.** `stage_changes`
  accepts the existing-inode prepared-update surface with its existing
  changed-name/inode limits (128 of each). `reserve_inodes` does not silently
  enable remote create/mkdir.
- **No automatic rebase, merge, retry or conflict resolution.** C5 detects a
  stale publication, not file overlap; disjoint and overlapping same-Branch
  changes take the same expected-head check.
- **No garbage collection, content deletion, Branch deletion or serial refund.**
  `discard_stage` removes one metadata row.
- **No crash-atomic cross-store claim.** C2 finish, stage insert, Commit/Branch
  transition and Layer/stack transition are four separately acknowledged
  boundaries; a machine failure can leave the two databases inconsistent, and
  resumed reads validate the roots they use and fail explicitly if content is
  missing.
- **No Windows/WASM/cloud persistence.** Current native C1/C2 arbitration is
  Unix-specific; Linux/macOS is the initial supported profile.

FUSE/Workspace implementation, diff/conflict resolution (#164), retention policy
and stronger durability are out of scope and are not implied by anything here.
