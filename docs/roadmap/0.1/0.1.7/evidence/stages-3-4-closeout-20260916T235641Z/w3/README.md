# W3 evidence — checkpoint D's decoded frontier (G3, G4, G5, G6)

## What changed

`core/crates/layerfs-content/src/file/edit/tree.rs` held its frontier as
`BTreeMap<ObjectId, Vec<u8>>` of **encoded** drafts: `hold_node` encoded and
hashed every draft at creation, `commit_node` decoded it again and re-encoded it
into a `FinalizedObject`, nothing was ever released, and the deferred charge grew
with the edit count. The frontier is now:

* `Draft::Page(FinalizedObject)` — a page the canonical builder emitted. Its
  bytes, identity and references are final when it is emitted, so holding it is a
  move and publishing it is a move: no decode, encode or hash is repeated for it,
  and its bytes are decoded only when a split or join needs its boundary.
* `Draft::Node(ExtentNode)` — a node the operation built from decoded parts, held
  **decoded** under an operation-local key (`DRAFT_KEY_TAG` + counter) and encoded
  and hashed exactly once, in `commit_node`, after the commit walk proves it final.

`child_summaries` recovers each child's summary from the parent's descriptors,
which carry the child's local key until the child is committed; `commit_node`
patches the descriptor with the child's real identity before encoding the parent,
so children are published before parents and a parent's bytes are produced once.
A node that a later `split`/`concat` supersedes is released
(`EditObjects::release`), and `apply_edits` releases the replaced range it drops
(`tree::discard`). `EditCounters::nodes_created` now counts nodes **published**
(one per distinct emitted mapping object), and `peak_deferred_bytes` is the peak
live charge, charged from the decoded entries a draft actually occupies.

`ConstructedFile` (`file/content.rs`) gained `counters: EditCounters` so the real
`apply_edits` path reports its frontier work; complete construction and whole-file
results report zeros because they hold no unfinished mapping node.

## Finality argument (per emission rule)

**R1 — only nodes the final mapping reaches are published.** `commit_node`
descends from the summary the operation ended with. A draft that no descriptor
chain reaches is never visited, so no speculative object is ever emitted and no
prune pass exists or is needed. A draft that a later `split`/`concat` replaced is
not merely unreachable, it is released and never encoded at all.

**R2 — children before parents.** For `Draft::Node` the branch commits every child
first, takes each child's real identity, writes it into the descriptor, and only
then encodes the parent. For `Draft::Page` the branch reads its descriptors once
and commits every child before publishing the page itself; a leaf page needs no
decode. The consumer therefore never sees a parent before its child - asserted by
`support::TrackingConsumer::assert_children_precede_parents` from
`edit_bounds` (W4.6).

**R3 — one encode and one hash per node, at finality.** A `Draft::Node` is encoded
and hashed in `commit_node` and nowhere else. A `Draft::Page` was encoded and
hashed once, by the builder that emitted it; a page's bytes cannot change after
emission, so that single hash is the node's identity and the frontier never
repeats it. The reviewed code paid one encode and one hash at `hold_node`, one
decode before it, and one decode plus one encode at `commit_node`.

**Why no later admitted edit changes an emitted node.** All edits of one operation
are in a single `EditStream`, applied in order inside one `replace_chunked` call;
`EditObjects::finish` - the only commit path - runs after the loop. Nothing in
the frontier publishes incrementally except payloads, and a payload's bytes are
final when it is created.

**Why neither side of a join can change an emitted node.** `split` and `concat`
never mutate a node in place: they build new nodes from decoded parts, release the
inputs they supersede, and return summaries that reference only live nodes.
A node created by `split` is superseded by the joins that immediately follow it in
the same edit, and the joins' result is what the next edit's `split` consumes.
`concat_optional` returns an input unchanged when the other side is absent, and
that input stays live in both cases.

**A stored subtree costs nothing.** A summary whose identity is not in the draft
map is a published subtree: `commit_node` returns its identity unchanged and its
bytes are never read. That is the copy-on-write reuse guarantee.

## Commands, exits and raw output

`w3-verify.log` (append-only; one clippy failure is retained and its re-run
follows it):

| Command | Exit | Result | Wall |
| --- | ---: | --- | ---: |
| `cargo +1.85.1 test ... -p layerfs-content --test edit_reference` | 0 | 2 passed (nine sealed cases) | 56.6 s |
| `... --test edit_localized` | 0 | 5 passed | 5.1 s |
| `... --test edit_bounds` | 0 | 8 passed | 17.0 s |
| `... --test edit_batch` | 0 | 7 passed | 1.0 s |
| `... --test edit_model` | 0 | 6 passed | 1.0 s |
| `cargo +1.85.1 test --workspace --locked --no-fail-fast` | 0 | **43 targets, 251 tests, 0 failed** | 119.9 s |
| `cargo +1.85.1 clippy --workspace --all-targets -- -D warnings` | 101 then 0 | an `unnecessary_cast` in the new test, fixed; re-run exit 0 | 1.7 s / 0.7 s |
| `cargo +1.85.1 fmt --all --check` | 0 | clean | 0.4 s |
| `python3 core/tools/check_product_boundary.py` | 0 | PASS | 0.1 s |
| `git diff --check` | 0 | clean | 0.1 s |
| production LOC pair | 0 | core 10938 → 11053 (delta +115) | - |

Wall times are for whole commands. `edit_reference` (56.6 s) and the workspace
suite (119.9 s) exceed the 15 s *performance-selection* budget and are declared
here as verification runs; no performance row is derived from them. No command
was re-run for a better number.

## The oracles (W3 proof + W4.1 + W4.6)

`edit_bounds.rs::the_retained_frontier_does_not_grow_with_the_edit_count`
replaces the vacuous `the_retained_frontier_does_not_grow_with_the_file` (which
applied `Edit::delete(0, 0)`, short-circuited to the base root and asserted on the
*base's* extent count). It applies 1/2/4/8/16 real overwrites to one fixed-shape
chunked file through `apply_edits` and asserts, per case:

* `payloads_created == edits` — the case really produced work;
* `nodes_created ==` the number of `ExtentLeaf`/`ExtentBranch` objects the
  consumer received — the W3 counter oracle;
* `extent_count(result) > extent_count(base)` — the mapping really changed;
* `assert_children_precede_parents()` — W4.6's child-first assertion for edits;
* the peak frontier stays inside a constant bound and **does not double when the
  edit count doubles**.

Measured peaks (bytes), with the release rule in place: `[2208, 2288, 2448, 2768,
3408]` for 1/2/4/8/16 edits — the growth is the final page's own decoded size
(two extents per overwrite), not the edit count.

## Control runs (`w3-fails-without-fix.log`)

* **Control A — release removed** (`EditObjects::release` becomes a no-op):
  peaks `[6308, 12856, 26672, 57184, 129728]`, the oracle fails with
  "doubling the edit count doubled the frontier".
* **Control B — `nodes_created` counted where drafts are created** (the reviewed
  semantics): the oracle fails with
  "assertion `left == right` failed: nodes_created must equal the emitted mapping
  objects".

## Identities

| | |
| --- | --- |
| Source | commit `42f9364c8` (first parent) plus the W3 change |
| Toolchain | `cargo 1.85.1 (d73d2caf9 2024-12-31)`, `rustc 1.85.1 (4eb2518d3 2025-03-15)` |
| Profile | `dev` (debug) test profile, `--locked` |
| Worker count | `LAYERFS_CONSTRUCTION_WORKERS=1` |
| Fixtures | `file_with_extents(40)` — a real chunked construction bisected to exactly 40 extents (≈650 KB) — plus `noise(len)` overwrite payloads of 300…315 bytes |
| Cache state | not a measurement: no timed phase, no sample count, no cache claim |
| Production LOC | core `10938 -> 11053` (delta +115); C1 4385 → 4500, C2 unchanged, telemetry unchanged |

## What this artifact does not prove

* It is not a measurement: no speed, storage or memory figure is claimed. The
  encode/decode/hash counts are **source-derived**, not instrumented - G11/W7 is
  where phase-local heap instrumentation arrives.
* A builder-emitted page is still encoded and hashed once by the canonical
  builder itself, which is the shared complete-construction path and its contract
  (it emits finalized canonical objects). The tree no longer repeats that work.
* `edit_reference` matching the sealed reference cases is a correctness oracle,
  not evidence about the reference's own performance.
