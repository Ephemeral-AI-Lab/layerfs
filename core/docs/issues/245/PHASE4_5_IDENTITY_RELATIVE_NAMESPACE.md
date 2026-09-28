# Phase 4.5 proposal: identity-relative namespace

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Research baseline: `885b0e105` on `codex/issue258-phase4`, whose [Phase 4
record](PHASE4_INHERITED_RENAME.md) describes implemented behavior. This is a
proposal tracked by [#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264),
a sub-issue of [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258),
and linked from review branch [PR #263](https://github.com/Ephemeral-AI-Lab/layerfs/pull/263),
which remains stacked on unmerged PR #260. No Phase 4.5 source or performance
result exists at this baseline.

## 1. Decision and scope

Make the mounted namespace a graph of stable inode serials and individual
parent/name bindings. A directory move changes two bindings and the live
moved-directory parent link. It does not materialize, validate, or rewrite
the absolute paths of descendants. Mounted lookup, listing, read and mutation
remain rooted in ordinary `WorkspaceApi::exec` and FUSE parent-inode/name
operations. The inherited subtree remains in the immutable canonical base;
neither the move nor ancestor retention copies up its child records.
[libfuse's low-level rename
contract](https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html)
already supplies parent inodes plus component names, matching the proposed
mounted boundary.

The Phase 4.5 mounted-route limit should be charged memory, backing capacity,
deadline, or an actual platform/encoding boundary, rather than an arbitrary
maximum *total* path length, depth or descendant count. This does not remove
per-name grammar, finite integer representation, the size of one page or
transport frame, or a caller's one-syscall pathname limit. Pages and frames
must split or stream so their size does not cap a logical tree or file. This
proposal inventories adjacent span, page and tree-height ceilings without
silently claiming to finish #248/#256 or C1 Commit optimization.

There is a smaller #258-only alternative: replace `Node.path` with a charged
dynamic byte vector, remove the aggregate path/depth refusal, use identity-
keyed reads, and keep rewriting only the `P` resident paths on rename. It
would avoid the `N` descendant scan with fewer caller changes, but
rename would still pay `O(P·L)` path-byte work and each resident node would
retain its full path. Phase 4.5 chooses the identity-relative design because
the requested target includes fast private metadata and resource-proportional
resident space, not only removal of the #258 scan. The smaller alternative is
the minimum implementation if the scope is narrowed later; it is not
represented as already implemented.

## 2. Before and target architecture

```text
CURRENT, PR #263                           TARGET, Phase 4.5

FUSE rename(parent,name,                    FUSE rename(parent,name,
            newparent,newname)                         newparent,newname)
          |                                            |
  build old/new absolute paths                  resolve two effective bindings
          |                                            |
  growth: list N effective descendants          check replacement and walk d
  for 4096-byte/256-component bound             retained ancestors for a cycle
          |                                            |
  rewrite P resident full paths                 COW-update one/two parent deltas
          |                                     and update live moved-node parent
          +---------- publish one root ----------------+
```

```text
Resident Node today                         Resident directory Node target
  serial                                    serial
  parent serial                             parent serial + one charged name
  path: [u8; 4096]                         attached-to-live-root state
  path_len                                  lookup/handle refs; Handle pins View

The private D/N/I COW key space remains the same; its parent-directory
bindings already persist the move. No P(parent-edge) page is added for the
mounted route. Full paths, when an interface asks for one, are derived or
streamed from components rather than retained on every resident Node.
```

The current [Node](../../../crates/layerfs-workspace/src/runtime/state.rs)
stores `[u8; 4096]` plus `path_len`; [rename path
preflight](../../../crates/layerfs-workspace/src/filesystem/rename_paths.rs)
scans inherited descendants when the destination prefix grows, and rewrites
resident paths before publication. `NODE_LIMIT = 256` is the **allocation
chunk**, not a resident-node count cap; [state
growth](../../../crates/layerfs-workspace/src/runtime/state.rs) charges more
chunks. The separate `steps > NODE_LIMIT` ancestor check in
[rename](../../../crates/layerfs-workspace/src/filesystem/rename.rs) is a real
fixed-depth refusal. The handle table's 128-entry limit is another real
admission ceiling, independent of node growth.

The target keeps Phase 4's stable serial and canonical-origin reads. Every
resident, live-bound directory must retain its **complete ancestor chain** to
the root. `cache_lookup` already knows the direct parent; the current `collect`
must change from retaining only directly pinned nodes to retaining their
charged ancestor closure. A rename walks the destination's live chain using
the existing serial-to-node index, checks for the moved serial or a repeated
ancestor, then updates the moved directory's parent link in the same guarded
publication as the private root. If the source directory is not resident it
cannot have a resident **directory** descendant whose live ancestry is
needed for a mutation; a held regular file may still exist by serial and
does not supply a unique parent edge. Persisted parent
bindings remain the existing directory deltas; a later lookup rebuilds live
links from them and the canonical base. This mounted design needs **no new
canonical parent index or C1 root format**.

Files may have several hard-link names, so no single-parent rule applies to
them. C1's separate alias validation still protects unique-bound directories
and symlinks. Detached/unlinked held inodes remain readable by serial and
their pinned view, but a detached directory must not act as a live mutation
parent via an obsolete link. Readdir's `..` behavior across a live move and a
frozen handle needs an explicit test and contract; it cannot be inferred from
the current cached path.

## 3. Why private-backing metadata remains local

```text
                            live resident directory graph
                      serial -> (parent serial, one name)
                      retain charged ancestors of each pin
                                      |
                                      | current view and revision
                                      v
frozen root G1 ----------------------+---- existing charged 4 KiB COW pages
                                     |       D(generation, serial): dirty frontier
live candidate root G2 -------------+       N(dir serial): origin + entries/tombstones
                                             I(inode serial): file/symlink overlay
                                      |
                                      v
                            owned backing segments + page epochs

Commit(G1): ordered dirty cursor -> one changed-row stream -> C1 successor
Mutation(G2): edit existing parent directory keys -> seal -> atomic root swap
```

The target writes **no new private parent-edge key**. The two parent directory
deltas already persist the changed binding, and later component lookups
reconstruct live parent links. The current [4 KiB page
grammar](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs)
and [keyed COW
tree](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/keyed/update.rs)
path-copy a constant number of changed keys. The [dirty
cursor](../../../crates/layerfs-workspace/src/commit/lower.rs) reads reached
private leaves in order. G1 and G2 share unchanged pages; inherited
descendants add no private records merely because their ancestor moved. The
new cost is the charged memory for retained ancestor Nodes, paid by the live
observations that require them.

For a rename touching `c` keyed records (a constant number of parent,
name/tombstone and optional origin records), a private tree of height `h`
reads and writes only reached paths: target `O(c·h)` metadata-page work and
new pages, with the actual 4 KiB pages charged to private backing. The live
root swap is constant-sized; G1 retains its referenced old pages. A name
lookup reads the local directory delta and, on inheritance, the canonical
directory's indexed child. The design removes `N` descendant metadata reads
from rename; it does **not** claim a measured latency, constant total disk
use across frozen generations, or a path-local C1 Commit.

The current page tree still has `LEVEL_LIMIT = 7` and
`PageRef.slot <= 65536` fixed refusals; its `Directory` record also uses a
`u16` local count and `u32` byte count. Those are separate resource-ceiling
audits, not solved by identity-relative rename. A 4 KiB page can remain a
bounded work unit if logical structures split and grow, while arbitrary
height/slot refusals need removal, versioning or a proof that a real
resource/encoding bound is reached first.

## 4. Complexity contract, not a performance result

Let `N` be effective inherited descendants of the moved directory, `P` live
resident nodes including retained ancestors, `d` actual ancestor depth, `L`
full-path bytes under today's contract, `h` private keyed-tree height, `K`
changed records, `B` all canonical bindings, and `S_i` each effective subtree
C1 currently validates for a rebound directory. These are variables, not new
configured ceilings. An existing `node_index` lookup costs `O(log P)` in the
current ordered map; private page costs below assume a constant number of
edited keys and include split/merge work along their reached paths.

| Operation | Current Phase 4 | Phase 4.5 target and required proof |
| --- | --- | --- |
| Equal/shorter-prefix directory rename | Constant-count COW metadata edits plus resident path checks and rewrites: `O(P·L + P·d + h)` local byte/page work, with indexed reads in addition; no `N` scan. The ancestor loop searches resident nodes per step and refuses after 256 steps. | With a proved attached-edge invariant, existing constant-count COW edits plus a `d`-edge walk through `node_index`: target `O(h + d·log P)` local/page work, no `P·L` rewrite or `N` traversal. If edges are checked against the effective view instead, add `d` indexed private/canonical reads and possible service round trips. |
| Longer/deeper-prefix directory rename | A successful fully checked move inspects all `N` effective descendants for path safety; an invalid path may fail early. Add resident rewrites and COW edits. One path frame per depth; no descendant copy-up. | Same target as the equal-prefix move. Prove all descendants remain reachable through component lookups, including an aggregate path beyond 4096 bytes. |
| One mounted child lookup/list page | Private directory delta and, if inherited, canonical identity-keyed lookup/list, but a `child_path` check still materializes aggregate bytes. | Same indexed lookup without aggregate-path construction; listing all `N` entries still costs at least `Ω(N)` returned work. Retain the observed parent's charged ancestor chain. |
| Resident and private space | Each resident `Node` includes `[u8; 4096]`, at least `4096·P` path bytes; rename adds a charged depth/path scan stack on growth. Private edits add `O(h)` pages per changed key. | Resident names and unique ancestor Nodes are charged to actual live observations; no full-path array or `O(N)` rename state. Private edits remain `O(h)` pages per changed key. A frozen root retains the pages it actually references until release. |
| Dirty private lowering | Persistent cursors visit reached leaves, not all canonical bindings. | Preserve the same private stream; no new parent-edge row is necessary for mounted rename. |
| Complete C1 Commit | For stored-directory rebinds requiring the full checks, parent-alias validation scans the base and cycle validation walks relevant moved-effective subtrees: at least proportional to `B` and those `S_i`, apart from changed-row work. Commits without such candidates may skip a scan. | **Unchanged until a separate, sound C1 validation/index proof lands.** Phase 4.5 must not advertise path-local Commit from faster private metadata alone. |

These are source-derived targets and lower bounds, not latency numbers. A
correctness proof must show that every live mutation parent has an effective
ancestor chain, not merely cached serial pointers; orphaned held nodes cannot
authorize a mutation. The
existing equal-length 3-versus-67-descendant diagnostic only counts upstream
Store calls and Workspace private pages; it does not measure C1 internal page
visits, cache state or this proposed path. A new performance case must be
registered prospectively under the benchmark rules.

## 5. CAS, CDC and delta encoding: what moves and what does not

A canonical directory leaf maps a name to a serial; an inode separately holds
its `content_root` and `metadata_root`. A pure directory move changes one or
two parent directories' bindings, their inode-table values and the filesystem root.
It does not change inherited file bytes or their content roots. The current
[Workspace save](../../../crates/layerfs-workspace/src/commit/save.rs) skips a
directory-only dirty record as a file-content save, while [C1
update](../../../crates/layerfs-content/src/filesystem/update.rs) rebuilds the
changed directory roots and inode table. Existing CAS file and chunk object IDs
remain shareable across old and new heads. A canonical page gets a different
ID when its bytes change; identical bytes retain the same ID. Moving a binding
does not invoke CDC or re-chunk descendants. A later temp-file replacement
does invoke the ordinary file-content path for the replacement's bytes.

The [CDC profile](../../../crates/layerfs-content/src/file/cdc/gear.rs) owns
file chunk boundaries, and [C2's physical PREFIX delta
choice](../../../crates/layerfs-storage/src/encoding/delta/select.rs) is
limited to payload roles. Namespace pages are ordinary or pooled metadata,
not file chunks. The **chosen mounted design** changes no C1 root format, CAS
role, CDC profile or C2 delta selector. Existing file object IDs and old
canonical heads remain readable; explicit Commit may emit changed directory
pages, pooled inode leaves and a new filesystem root, while a pure directory
move does not rebuild file payload or chunk objects. Verify old/new file
content roots, chunk IDs and zero file-content saves rather than assuming a
speed or byte-saving result.

A **later C1 path-local Commit** could use an authenticated canonical reverse
index. That is a separate format and cost decision: [root
v1](../../../crates/layerfs-content/src/filesystem/root.rs) names only its
inode table; a new index root or sidecar needs exact root identity and
old-head compatibility. [History pins the
profile](../../../crates/layerfs-history/src/sqlite/staging.rs) per LayerStack;
new CAS references must satisfy [C2 dependency
checks](../../../crates/layerfs-storage/src/cas/dependencies.rs). A new object
role would also affect [C1's role
grammar](../../../crates/layerfs-content/src/object/output.rs) and [C2's SQL
schema](../../../crates/layerfs-storage/src/sqlite/schema.rs). The [server
import namespace](../../../crates/layerfs-server/src/service/save/import/namespace.rs)
already knows parent identities and could charge index construction there,
but none of that work is required for mounted Phase 4.5. The current
source-import scanner uses full `PathBuf` host paths; lifting aggregate path
ceilings for *import* also requires separate component-relative host
traversal.

## 6. Costs, failure modes and resource boundaries

| Cost or risk | Required treatment |
| --- | --- |
| Ancestor retention | `State::collect` must mark the complete ancestor closure of live pinned directory nodes. Its work may depend on `P` and `d` during forget/collection; do not move that sweep into rename or omit its charged memory. Root is permanent. |
| Detached held directory | `rmdir` or replacement can leave a readable held directory with an obsolete parent pointer. Mark it non-navigable for live mutations, or validate each ancestor edge against the selected effective view. A pointer chain alone is not a cycle proof. |
| Concurrent forget | A forget may evict nodes without changing namespace revision. Pin the candidate's required chain or recheck its presence, attachment and revision under the final state lock before publishing. |
| Long mounted path | Remove aggregate-path joins and validation from lookup/list/create/remove/rename/read/readdir/readlink. Use serial plus one validated name. Do not merely delete the preflight scan while a later caller still returns `Capacity`. |
| Stale identity or symlink | Stale file reads and `serial_original` still issue path `Inspect::Attributes`; switch to existing `InodeAttributes`. C1 has `readlink_inode`, but Bridge/Service need an additive identity-readlink query for canonical symlinks. |
| Page, handle and count ceilings | Private level 7, slot 65,536, `Directory` `u16` count, 128 handles, C1 level 31 and other format ceilings remain distinct work. Phase 4.5 removes the aggregate mounted-path and fixed ancestor-step refusals; it does not label all product structures resource-only. |
| Commit and versioning | Preserve one sealed private root publication, G1/G2 frozen views, known/unknown outcome custody and old heads. C1's full-base and rebound-subtree validation scans remain until separately repaired. |

After Phase 4.5, mounted component traversal can reach a logical descendant whose assembled
path exceeds 4096 bytes. A single oversized pathname argument may still be
rejected by the OS; the oracle must navigate in short relative steps. Direct
C1 `LogicalPath` and path-based Bridge Inspect retain their own 4096/256
per-request bounds until a separately specified component or streaming API
replaces them. The server's host source-import `PathBuf` route likewise needs
separate work before **all** LayerFS paths have only resource limits. Fixed
page and wire-frame sizes can remain when the logical data is split or
streamed; fixed totals must have a representability proof or be removed.
[Linux pathname(7)](https://man7.org/linux/man-pages/man7/filename.7.html)
distinguishes one pathname argument from traversal in smaller `openat` steps.

## 7. Proposed file ownership and LOC forecast

The source plan is a planning estimate from `885b0e105`, not an allocation or
a commit count. The current production baseline is **124,196 combined**:
Core **58,779**, legacy reference **65,417**. For each future commit, use
`tools/production_loc.py` on the exact first parent and staged tree; tests,
docs and tools contribute zero. All Core production files stay below 1000
physical lines and `lib.rs`/`mod.rs` below 200.

```text
Legend: + new production file, ~ edit existing file, - remove, = source kept

core/crates/
├── layerfs-workspace/
│   ├── src/
│   │   ├── runtime/
│   │   │   ├── state.rs                    ~ Node fields, charge, collection
│   │   │   ├── ancestry.rs                 + closure, attached state, cycle walk
│   │   │   └── mod.rs                      ~ thin declaration only
│   │   └── filesystem/
│   │       ├── namespace.rs                ~ component-name grammar and lookup
│   │       ├── namespace_view.rs           ~ effective serial/name view
│   │       ├── create.rs, remove.rs        ~ component mutation callers
│   │       ├── directory.rs                ~ readdir and frozen handle `..`
│   │       ├── original.rs, read.rs        ~ stale serial-based reads
│   │       ├── symlink.rs                  ~ identity-based readlink
│   │       ├── rename_preflight.rs         + attachment/replacement/cycle check
│   │       ├── rename.rs                   ~ COW publication, moved Node edge
│   │       ├── rename_paths.rs             - path scan/rewrite removed
│   │       └── mod.rs                      ~ thin declaration only
│   └── tests/
│       └── namespace.rs                    ~ focused public Workspace behavior
├── layerfs-bridge/
│   ├── src/
│   │   ├── contract/request.rs             ~ InodeReadlink Inspect variant
│   │   └── adapters/native/
│   │       ├── protocol/metadata.rs        ~ checked wire codec
│   │       └── client.rs                   ~ checked response validation
│   └── tests/attributes.rs                 ~ external identity-readlink cases
├── layerfs-server/src/service/read/content.rs
│                                        ~ dispatch identity readlink
├── layerfs-content/src/filesystem/read.rs = readlink_inode already exists
├── layerfs-fuse/src/adapter.rs           = parent/name route already exists
└── layerfs-api/sdk/
    ├── src/workspace.rs                  = generic Exec API already exists
    └── tests/
        ├── inherited_workspace.rs       ~ deep move and count oracle
        └── agent_route.rs               ~ mounted Exec and explicit Commit
```

| Responsibility | Proposed files; current physical lines | Change |
| --- | --- | --- |
| Resident identity and ancestry | `workspace/src/runtime/state.rs` (700), planned new `runtime/ancestry.rs` | Replace fixed path array with charged one-component identity link; maintain ancestor closure, attached state, and `node_index` walk. Split by responsibility before `state.rs` nears 999 lines. |
| Component namespace operations | `workspace/src/filesystem/namespace.rs` (267), `namespace_view.rs` (408), `create.rs` (759), `remove.rs` (438), `directory.rs` (158) | Separate name validation from aggregate `child_path`; remove path arguments, preserve paged child lookup/list and live/frozen handle rules. |
| Rename | `workspace/src/filesystem/rename.rs` (942), new `rename_preflight.rs`, retire `rename_paths.rs` (113) | Move ancestry/cycle/replacement preparation out of the near-limit file; remove descendant scan and cached-path rewrite; update only the moved resident directory's parent link under final lock. `filesystem/mod.rs` (17) gets a thin declaration only. |
| Identity reads | `workspace/src/filesystem/original.rs` (71), `read.rs` (264), `symlink.rs` (160) | Stale read/original use serial attributes; symlink target uses identity readlink. |
| Identity-readlink wire | `bridge/src/contract/request.rs` (698), native `protocol/metadata.rs` (794), native `client.rs` (567), `server/src/service/read/content.rs` (186) | Add and validate one identity Inspect variant; C1 `FilesystemRead::readlink_inode` already exists. Keep wire messages bounded. |
| External proof | `sdk/tests/inherited_workspace.rs`, `sdk/tests/agent_route.rs`, focused Bridge/C1 tests | Public mounted Exec, deep component traversal, handles, old heads, explicit Commit and page/Store/private-byte counts. Tests do not count as production LOC. |

The FUSE adapter (977 physical lines) already supplies parent serial and
component name; SDK `WorkspaceApi::exec` (190) needs no workload-specific
change. The estimated **production LOC delta** for the mounted identity-
relative design is **−150 to +300**, with low confidence until implementation
and exact counting. The narrower charged-dynamic-full-path alternative is
estimated **−50 to +250** and retains `O(P·L)` rename work. A later canonical
parent index plus path-local C1 Commit proof is a separate provisional **+700
to +1,600** LOC expansion and a root/role compatibility task; near-ceiling
`content/src/filesystem/validate.rs` (997) would have to split by alias and
cycle responsibility. These ranges are planning forecasts, not measured
source-size comparisons or performance claims.

## 8. Staged proof gates

1. **4.5A, resident closure.** Prove root retention, complete charged
   ancestors for every live directory, and safe collection after forget.
   A detached held directory remains readable by serial/view but cannot be
   a live mutation parent. Test a pinned child after forgetting its parent,
   then nested move/cycle refusal.
2. **4.5B, identity-only mounted path.** Convert every named caller above,
   including stale file reads, readdir and symlink readlink. Run one ordinary
   `WorkspaceApi::exec` moving a base-resident directory with an uncached
   descendant that acquires an aggregate path beyond 4096 bytes; reach it
   component by component and verify identity, modes and bytes. Then replace
   a temp file and perform one explicit Commit. Check old/new heads and
   G1/G2, held child and directory handles, `..`, nested move/back, invalid
   replacement and descendant cycle before publication.
3. **4.5C, resource and work accounting.** Compare growing-prefix moves
   with 3 and 67 or more inherited descendants. Report upstream calls,
   private page reads/writes, private bytes, resident ancestor count, charged
   memory, clean close and C1 page visits separately. Prove no descendant
   read/copy-up and no `P`-wide work inside rename. Force a small declared
   memory/backing budget and verify atomic prepublication refusal. Do not
   substitute this for a C1 Commit complexity proof.

The public checks use locked release binaries and test commands shorter than
30 seconds; rerun only a failing or affected focused selection. A
performance case, if taken, is registered prospectively under the repository
benchmark and cache-state rules. The full-size public benchmark is not a
Phase 4 closure prerequisite by the owner direction. No debug binary or
workload-specific shell shortcut is admissible.

## 9. Open rulings and scope boundary

- Choose the exact `attached` state and frozen-directory-handle `..`
  semantics. Alternatively validate each parent/name edge in the candidate
  effective view; that costs `O(d)` indexed reads, still independent of `N`.
- Specify whether direct whole-path Inspect remains a bounded convenience
  while mounted component traversal accepts long aggregate paths, or gets
  its own component iterator. The latter broadens the product contract.
- Audit private level/slot, handle, count, span and C1 tree-height ceilings
  separately. Lift arbitrary logical refusals with charged growth or prove
  they are unreachable before format/address-space exhaustion. Neither this
  mounted slice nor a fixed 4 KiB page size proves that all ceilings are gone.
- Optimize C1 full-base alias and moved-subtree cycle validation only with a
  separate sound index and candidate-graph proof. A private rename gain is
  not a path-local Commit claim.
