# Stage One — Implementation Map and Complexity Audit

Authority: [12-stage1-performance-completion.md](12-stage1-performance-completion.md)
Rule: minimal product changes; reuse current codecs, readers, mutation code,
publication, drivers, and evaluator executable.

## 1. Resulting source tree

`+` means add; `*` means edit; unmarked files remain unchanged.

```text
crates/
├── layerfs-core/
│   └── src/
│       ├── content/rope.rs                    * shared authenticated reads; range diff
│       ├── namespace.rs                       * persistent directory diff
│       ├── inode.rs                           * persistent inode-table diff
│       ├── metadata.rs                        * shared authenticated decode
│       ├── content/extent.rs                    unchanged canonical model
│       ├── content/extent_codec.rs              unchanged canonical bytes
│       └── namespace_codec.rs                   unchanged canonical bytes
├── layerfs-engine/
│   └── src/
│       ├── lib.rs                             * borrowed/batch reads; complete counters
│       ├── scratch.rs                         * scratch counters
│       ├── integrity.rs                       * one auth/decode accounting path
│       ├── publication.rs                     * avoid duplicate closure work by mode
│       ├── refs.rs                            * current/ref helpers and counters
│       └── generation.rs                      * single clean reopen/admission
├── layerfs-vfs/
│   ├── src/
│   │   ├── resolver.rs                        + path resolution + direct read route
│   │   ├── refresh.rs                         + A→B Merkle plan/apply/verify
│   │   ├── lib.rs                             * APIs and operation counters
│   │   ├── workspace.rs                       * live state/checkpoint/authority
│   │   ├── managed_edit.rs                    * direct logical vs native edit split
│   │   ├── materialize.rs                     * return authority during construction
│   │   ├── capture.rs                         * retain/reuse topology; external stays full
│   │   └── driver.rs                          * observed native work snapshot
│   └── tests/
│       └── stage1_routes.rs                   + platform-neutral route tests
├── layerfs-os/
│   ├── src/apple/
│   │   ├── store.rs                           * accept explicit integrity mode
│   │   ├── workspace.rs                       * remove proven redundant APFS work
│   │   ├── metadata.rs                        * one preflight; exact final read-back
│   │   ├── apfs.rs                            * only clone/replace/sync helpers extracted
│   │   └── ffi.rs                               unchanged unless a missing syscall is proved
│   └── tests/
│       └── apple_stage1.rs                    + Apple sync/metadata/refresh checks
├── layerfs-sdk/
│   ├── src/lib.rs                             * thin Stage One surface
│   └── tests/
│       ├── workflow.rs                        * retained lifecycle expectations
│       └── stage1_routes.rs                   + public route tests
└── ...

tools/layerfs-eval/
├── Cargo.toml                                 * existing workspace `blake3` only if absent
└── src/
    ├── main.rs                                * one stage1 command: prepare/run modes
    ├── stage1_fixture.rs                      + generators, seals, clone reset, oracle
    ├── stage1.rs                              + both schedules and one row writer
    └── apple_poc.rs                             preserve compact correctness smoke
```

Do not create `store.rs`, `compaction.rs`, a benchmark framework, a second
schema, an async runtime, a policy registry, or a provider hierarchy merely to
match an older proposed tree.

## 2. Minimal dependency shape

```text
layerfs-core
    ^
    |
layerfs-engine
    ^
    |
layerfs-vfs --------> layerfs-core
    ^
    |
layerfs-os ----------> layerfs-engine
    ^
    |
layerfs-sdk
    ^
    |
layerfs-eval
```

Rules:

```text
core/engine/vfs: no Apple types, libc, paths-as-authority, or cfg(product test)
os: Apple syscalls and unsafe only here
sdk: thin ownership/lifecycle wrapper, no SQLite or codec logic
eval: fixture/oracle/schedule only, never product semantics
```

## 3. Gap-by-gap implementation

### I1 — One fetched row, one authentication, one role decode

Current:

```text
Engine row authentication
  -> core authenticated_get rehash
  -> validate_identity decode
  -> role callback decode again
```

Target:

```text
ObjectRead::with_authenticated_canonical(id, callback)
ObjectRead::get_authenticated_batch(ids <= 64, callback)

borrow SQLite row
  -> authenticate canonical bytes once
  -> role-decode once in callback
  -> release row
```

Files:

```text
core: content/rope.rs, namespace.rs, inode.rs, metadata.rs
engine: lib.rs, integrity.rs
```

Reuse the existing borrowed identity-validation primitive in
`object/codec.rs`; edit that file only if implementation proves a missing
primitive.

Hard equation:

```text
fetched_rows
  == fetched_row_authentication_passes
  == fetched_row_role_decode_passes

new_object_authentication_passes       // separate admission class
incumbent_authentication_passes        // separate fetched incumbent class
payload_batch_maximum <= 64
```

No raw unauthenticated `Vec<u8>` product traversal remains. In-memory test
stores authenticate before callback too.

### I2 — Complete counters

Current `EngineCounters.statements` omits manual SQL in integrity, refs,
scratch, StoreId, and generation selection. Rope “created” counters count
emission attempts, not unique CAS inserts.

Add observed fields:

```text
fetched_rows/fetched_row_authentication_passes/fetched_row_role_decode_passes
new_object_authentication_passes/incumbent_authentication_passes
payload_batch_queries/references/maximum
objects_created_delta/objects_reused_delta
put_lookup_statements/put_insert_statements/created_rows/reused_rows
chunks_emitted/rope_nodes_emitted
namespace_nodes_emitted/inode_nodes_emitted
scratch_tables/statements/rows/high_water_bytes
root_diff_nodes/changed_paths/full_fallback_files
native_read/write/patch/suffix bytes
temp/clone/sync/rename/replace/metadata calls
authority_full_scans/workspace_reuses/rematerializations
```

Structural constants are labeled `enforced`, not reported as observed zeroes.
Unavailable timers are the string `Unavailable`.

Counter repair also exposes, rather than hides:

```text
Verified root verification pass 1: publication-closure drain
Verified root verification pass 2: namespace graph validation
```

Consolidate the two full traversals only when one authenticated visitor proves
both invariants. Until then, report both complete passes.

Scratch is derived, non-authoritative work. Prove that classification with
crash/cleanup tests, then remove unnecessary `DELETE/FULL` queue durability
and SELECT+UPDATE churn while retaining exact ownership and restart cleanup.

Compaction repair:

```text
prepare source SELECT and candidate INSERT once, outside the object loop
deduplicate validation of identical retained roots
report sum(closure(root)) unless one genuinely shared validator exists
```

Object insertion reports SELECT-miss and INSERT separately. Optimize that path
only if the 100 MiB write row identifies it as a material owner.

### I3 — Direct reads

Reuse `layerfs_core::content::rope::{read_range,read_all}`.

```text
SDK root/path/range
  -> shared resolver: namespace descent + inode lookup
  -> FileState root
  -> core rope read
  -> caller Write sink
```

Complexity:

```text
path:  O(sum_i(log D_i + log I))
range: O(log E + C_R + R)
space: O(tree height + <=1 MiB stream buffer)
native work: 0
```

### I4 — Direct write and logical edit

Reuse Publication, resolver, `rope::replace`, inode update, and streamed file
builder. Do not materialize APFS.

```text
replace_range(expected_ref_state, path, P, delete, input)
  -> resolve
  -> CDC only supplied input
  -> persistent rope splice
  -> persistent inode-table path-copy
  -> expected-head Publication
```

Complexity:

```text
CDC bytes                         <= replacement bytes B
unaffected suffix payload reads  = 0
unaffected suffix payload writes = 0
directory nodes for content edit = 0
time                              O(B + log E + sum_i(log D_i + log I))
memory                            O(height + <=1 MiB)
```

`replace_file` streams `Theta(F)` input. It never accepts or creates a 100 MiB
buffer.

Mode separation:

```text
Trusted live edit
  O(B + log E + sum_i(log D_i + log I))

Verified edit without authenticated transition receipt
  Trusted work + Theta(visible root closure)

Verified-after-Trusted reopen
  unique retained marking + required per-root/context validation
```

The direct edit API is common product code; the integrity policy changes
authority validation, not bytes, roots, write shape, or durability.

### I5 — Integrity-mode admission and reopen

Current Apple store opening forces `Verified`; generation opening performs a
preliminary Verified open and then the requested open.

Target:

```text
LayerFs::open             -> Verified
open_with_integrity       -> exact requested Store-lifetime mode
clean generation admit    -> one selected generation open
recovery work             -> only when residue exists
parent directory sync     -> only after actual cleanup
current_head              -> accepted ref state, never the stale open-time copy
```

Never allow Trusted history to become Verified authority. A later Verified
open still performs the required scrub.

### I6 — Repeatable managed checkpoint

Current successful capture recursively deletes native state. Replace the
ambiguous `Option + dirty + committed` combination with one explicit enum:

```text
Live(ref_state, authority)
  -> edit -> Dirty(base_ref_state, descriptors, authority)
  -> checkpoint requested-visible -> Live(new_ref_state, rotated_authority)
  -> conflict -> ExternalDirtyConflict
  -> ambiguous -> Indeterminate
  -> discard -> Closed
```

Checkpoint order:

```text
freeze edit admission
sync touched native state
replay exact descriptors through sole Publication
freshly reconcile ambiguous outcome
rotate expected head/root only for requested-visible
clear old spool
retain native workspace/topology
```

Proof populations:

```text
2 edits    basic rotation
10 edits   mixed edit shapes
100 sequential edit -> durable checkpoint -> retained-Live rotations
           one initial materialization; zero rematerializations
           descriptor/spool reset after every acknowledged/reconciled checkpoint
           bounded Q/FD/temp
```

### I7 — Cold materialization constant-factor repair

Keep required work:

```text
name collision preflight
one final content+metadata file sync
atomic rename
parent directory sync
exact supported metadata read-back
no-follow/substitution protection
```

Remove only proven redundancy:

```text
build live hard-link/topology authority during construction
  -> no second complete canonical/native authority pass

set temp content + metadata
  -> one final temp sync, not two

ordinary file with all metadata finalized before final temp sync
  -> remove duplicate temp sync and post-rename entry sync

restrictive flags or hard-link metadata finalized after rename
  -> retain one sync after the final metadata mutation
  -> or prove flags can safely be finalized before rename

same-directory rename
  -> sync directory once

clean recovery
  -> no parent sync when nothing was removed

metadata
  -> one fail-before-mutation preflight; retain final exact verify
```

Every removed sync needs one focused fault/reopen test. Do not optimize away
the last required file or directory durability edge.

Current asymptotic class remains explicit:

```text
Theta(file output bytes)
+ O(paths log inode_count)
+ namespace/metadata tree visits
+ native-name preflight
```

### I8 — Managed exact no-op

Authority tuple:

```text
StoreId
current immutable root
native root stable identity
workspace generation
managed mutation serial
topology authority
```

When the tuple is intact and target equals current root:

```text
payload reads/writes = 0
CDC bytes            = 0
native reads/writes  = 0
SQLite transactions  = 0
publication COMMITs  = 0
```

An external or reopened path does not inherit this authority. It uses exact
full verification/capture and remains linear.

### I9 — Changed-root A→B refresh

```text
caller first moves/aligns the named ref to exact target RefState
Live(A) -> Refreshing(A,B)
inode-table root diff
  -> changed InodeIds
  -> changed directory roots: entry diff
  -> changed file roots: rope diff
  -> map changed inode to retained native parent edges
  -> classify route
  -> apply private changes
  -> exact construction verification
  -> sync changed directories
  -> complete proof: Live(B,target_ref_state)
```

Failure state:

```text
pre-native-visibility failure       -> Live(A)
any possibly visible partial apply -> IncompleteDerived
IncompleteDerived                  -> discard/rebuild only
                                       no edit/checkpoint/no-op authority
```

`refresh` requires the selected named ref to equal its exact target
`RefState`. It does not create or move a ref implicitly. This prevents a
physically refreshed `Live(B)` workspace from retaining publication authority
for `main=A`.

Routes:

| Change | Native route | Canonical complexity |
|---|---|---|
| Equal root | `ExactNoop` | `O(1)` admitted authority |
| Locally derived same-length single-link file | clone/patch or same-file patch | shared-identity changed spines + bytes |
| Locally derived different-length file | `FullFallback` for changed file | local logical diff; physical `Theta(file)` |
| Arbitrary/unrelated roots | compare both trees | worst-case `Theta(nodes(A)+nodes(B))` |
| Hard-linked regular file | in-place shared-inode patch, or rebuild/relink complete alias group | never clone/replace one alias |
| File create/delete | atomic create/remove + parent sync | path-local namespace/inode diff |
| Directory create/delete | bottom-up construction/removal + exact emptiness/topology proof | changed subtree/path work |
| Same/cross-parent rename | atomic rename + each distinct changed parent synced once | changed directory spines |
| Directory-subtree rename | parent-edge update; no descendant path-string rewrite | changed parent spines |
| Symlink create/replace/delete | atomic link construction/replacement + parent sync | changed entry/inode work |
| Metadata-only update | exact metadata apply/read-back + required sync | content root reused |
| Hard-link add/remove/split/merge | update/rebuild complete alias authority | full group exactness |
| Root-directory metadata | apply/read-back/sync root directory | metadata/inode work |
| Unsupported metadata/special kind | fail closed | no partial authority |

For every route, the apply plan binds old/new construction visibility, exact
sync owner, failure disposition, and topology-authority update.

Live topology is bounded and disk-backed where necessary:

```text
InodeId -> parent InodeId + basename edges
native hard-link key -> InodeId
disk-backed refresh plan/work queue
```

Changed IDs/paths are streamed or stored in the disk-backed plan; they are not
collected into an unbounded `Vec`. Record plan rows and scratch high-water. No
descriptor per inode and no full path string per descendant.

Merkle locality eligibility:

```text
locally derived child with shared subtree identities -> local changed-spine claim
arbitrary/unrelated retained target                 -> full two-tree worst case
```

### I10 — External unchanged-file identity

`FileStateV3` is operational/history-shaped: equal bytes can have different
rope shapes. External capture therefore cannot prove an unchanged file by
rebuilding a normalized rope and comparing roots.

Required external route:

```text
stream current native file -> semantic digest
obtain prior semantic digest
  process/Store-lifetime memory hit, keyed by authenticated FileStateRoot
    -> use it
  miss -> stream prior canonical file and derive it

digests + metadata equal
  -> reuse prior FileStateRoot and InodeRecord

different
  -> CDC current bytes and build changed file root
```

The digest cache is memory-only and is populated only after streaming that
exact authenticated FileStateRoot. Reopen is a cache miss and performs the
authenticated prior stream. Persisted acceleration would require a separate
authenticated binding design and is outside Stage One. No canonical format
change is required.

Counters:

```text
current_digest_bytes
uncached_prior_digest_bytes
changed_current_cdc_bytes
unchanged_file_roots_reused
```

External capture stays linear and may make multiple explicit passes. The gain
is zero semantic no-op churn and no CDC/object emission for unchanged files,
not a false local-capture claim.

Exact work class:

```text
Theta(unique current regular bytes for digest
    + changed current bytes reread for CDC/CAS
    + uncached prior logical bytes for prior digest
    + represented metadata bytes)
+ sum_directories O(D_j log D_j)
+ O(paths log inode_count)
+ disk-backed grouping/enumeration
```

### I11 — Durable transition scope

The current V3 publication path does not provide a meaningful durable V3
parent/child delta authority; the historical `layerfs_deltas` surface is not a
Stage One product transition.

Stage One decision:

```text
immutable roots + refs       durable authority
Merkle root diff             changed-root refresh authority
process-local change receipt optimization hint only
new canonical delta format   deferred
```

Do not add a transition codec to make an older checklist sentence true. A
future durable delta is a separate canonical-format decision with its own
compatibility contract. Update older prose to say `root diff` where it
currently implies an implemented V3 delta.

## 4. Complexity audit checklist

| Path | Counter proof | Blocker |
|---|---|---:|
| Direct range | node visits grow with height + overlap; returned bytes exact | Yes |
| Full stream | byte-linear; bounded buffer | Yes |
| Local edit early/middle/late | no unaffected suffix payload; emitted nodes bounded by height | Yes |
| Content-only update | directory nodes emitted `0`; one inode spine | Yes |
| Batch payload | maximum `<=64`; ordered references conserved | Yes |
| Auth/decode | fetched rows = fetched-row auth = fetched-row decode; new/incumbent separate | Yes |
| Managed 100 edits | 100 durable checkpoints; one materialization; rematerializations `0`; Q/FD/temp bounded | Yes |
| Exact no-op | all payload/native/CDC/COMMIT work `0` | Yes |
| Local same-size refresh | shared-identity changed paths/ranges only | Yes |
| Arbitrary-root refresh | honest two-tree worst case; bounded disk plan | Yes |
| Count-changing native | explicit suffix/full bytes; never called local | Yes |
| External capture | complete path/unique byte accounting | Yes |
| External unchanged file | prior FileStateRoot reused after semantic digest equality | Yes |
| Reopen | no duplicate clean open; no native scan | Yes |
| Compaction | report `sum closure(root)` unless retained-union traversal is actually shared | Yes |

## 5. Tests by file

| Test file | Minimum cases |
|---|---|
| `layerfs-core/tests/extent_model.rs` | early/middle/late replace; insert/delete; unequal-root range diff; malformed nodes |
| `layerfs-core/tests/namespace_model.rs` | directory root diff; rename; retained roots |
| `layerfs-core/tests/namespace_codec.rs` | unchanged goldens and strict malformed rejection |
| `layerfs-engine/tests/store_and_publication.rs` | rows/auth/decode equation; one COMMIT; Trusted explicit |
| `layerfs-engine/tests/faults_and_reopen.rs` | clean single open; ambiguous reconciliation; Verified-after-Trusted |
| `layerfs-vfs/tests/stage1_routes.rs` | direct range/stream/edit; repeated checkpoints; no-op; A→B refresh/fallback |
| `layerfs-os/tests/apple_stage1.rs` | final sync/rename faults; same-dir sync once; exact metadata; clone fallback |
| `layerfs-sdk/tests/stage1_routes.rs` | only public product APIs; no internal bypass |

## 6. Implementation order and stop rules

```text
I1 counters/auth boundary
  -> I3/I4 direct routes
  -> I5 integrity/reopen
  -> I6 retained checkpoint
  -> I7 redundant APFS work
  -> I8 exact no-op
  -> I9 changed-root refresh
  -> I10 external unchanged-file reuse
  -> evaluator
```

Stop and repair before measurement when any of these occurs:

```text
identity/root mismatch
old root unreadable
multiple visibility COMMITs
unreconciled ambiguous outcome
Trusted authority escalation
local edit reads/writes unaffected suffix canonically
source-sized allocation
unbounded topology/descriptor/temp growth
external scan mislabeled as managed no-op
benchmark-only semantic implementation
```
