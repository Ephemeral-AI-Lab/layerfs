# Handoff: Stage 5 — filesystem trees, inodes, attributes and reference ordering

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Complete [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) under
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).
Exact source/test paths and per-file/directory LOC recommendations:
[stage-5-file-plan.md](stage-5-file-plan.md).
This is the last planned C1/C2 feature-implementation stage; Stage 6 qualifies the
whole core and Stage 7 designs/integrates the surrounding runtime. Neither stage
absorbs incomplete Stage 5 behavior or its own required verification.

## Copy/paste assignment

You are the coordinating implementation agent for Stage 5 in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Deliver independently callable filesystem-tree construction, update and reads over
explicit native inputs, using the existing finalized-object and authenticated-read
contracts. Compose it with real C2 CAS, metadata pooling, packs and SQLite. Complete
the entire issue, including reference accounting, topology, attributes, external
tests, independent timing, limits and required evidence. Do not stop at a directory
codec, a sorted builder or a green subset.

Owner scope decision (2026-09-17): implement portable mode/mtime and bounded generic
attributes. Remove Apple-specific codecs, flags and platform xattr rules from the
replacement core; APFS materialization is out of scope. This supersedes the earlier
proposal's platform-specific attribute requirements. See the
[attribute contract](filesystem-tree.md#c1-logical-attributes).

```text
caller: stable logical changes + authorized scoped identities + explicit resources
                                      |
                                      v
                 C1 filesystem: validation and retained membership
                     /               |                  \
              directories       reference effects      attributes
                     \               |                  /
                            final typed inode values
                                      |
                            sorted inline inode table
                                      |
                            canonical filesystem root
                                      |
                         finalized objects / backpressure
                                      |
                          C2 CAS / pooling / packs / SQL
                                      |
                            acknowledged storage result
```

C1 construction completion is distinct from all-output persistence. A root is an
ObjectId; necessary count/level summaries stay in the algorithm or the specific
operation result that needs them. No generic summary object, checkpoint service,
plugin registry, Workspace-shaped planner or new crate is required.

## 1. Read the actual foundation and source contracts

Read [repository rules](../../../../../AGENTS.md),
[core rules](../../../../../core/AGENTS.md), this handoff and its file plan, then:

- [Filesystem tree design](filesystem-tree.md), the primary semantic contract.
- [Canonical objects](canonical-objects.md), [file content](file-content.md),
  [finalized output](finalized-object-handoff.md) and [I/O](content-io.md).
- [Physical encoding/pooling](physical-encoding-and-packing.md),
  [persistence](admission-and-persistence.md) and [memory audit](content-io-memory-audit.md).
- Current core API/README/source and the [Stage 3–4 closure record](stages-3-4-completion-round-20260917.md).
- [Benchmark rules](../../../../general/benchmark_rules.md),
  [benchmark instructions](../../../../../benchmark/AGENTS.md),
  [quick start](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
  [release](../../../../general/release-policy.md) and
  [documentation policy](../../../../general/documentation-policy.md).

Planning HEAD is `b6b83162a8f4ca3ddb1a59adf1b1ea0cfb6d3369`; record the actual start
commit/tree, relevant uncommitted inputs, source/build identities and contract.
Preserve unrelated work. Current C1/C2 contain real file COW, pooling and resource
checks. #168/#169 are closed; do not restart their tasks. Their closure includes
explicit unmeasured owner-waived performance rows, not a measured v0.1.6 speedup.
Do not turn that waiver into a Stage 5 waiver or inherit a performance claim.

The pinned algorithm/oracle reference is v0.1.6
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
Relevant current reference tree/filesystem source matches that revision at planning
time; verify identity again before deriving fixtures. Root crates stay isolated
reference/oracle inputs, never a candidate dependency, include or runtime fallback.
Read complete relevant implementations and callers before porting their algorithms.

### Critical compatibility gate already identified

The existing core `object/inode_leaf.rs` encoder currently checks/writes
`subtree_bytes = row_count * 73`; the pinned reference
`tree/compact.rs::encode_inode` writes the row-byte summary as `row_count * 81`.
Those are different bytes and therefore different object IDs. Stage 3's own
save/reopen round-trip does not establish reference filesystem identity.

Before using that codec for Stage 5, create a golden-byte comparison with fixed
valid rows and diagnose the precise header semantics. Keep one canonical
InodeKind/InodeValue/leaf definition. Resolve writer/reader/pool expansion and
schema/profile compatibility explicitly; preserve required old data or reject an
unsupported profile according to its declared contract. Do not silently rewrite
stored objects, normalize malformed bytes, duplicate the leaf codec, or alter the
reference fixture to bless a mismatch. If a needed change conflicts with an
existing compatibility promise, produce the concrete decision/evidence instead
of hiding it. Closed scope does not excuse wrong Stage 5 canonical bytes.

Current candidate schema is version 4, four tables and twenty-one columns.
Stage 5 needs new logical object roles and broader persisted role validation.
Define the exact accepted schema/profile and old-Store behavior before mutation;
do not copy the old nineteen-column proposal or claim schema-10 compatibility.

## 2. Invariants and ownership

### Logical representation

```text
filesystem root: profile + allocation scope + root serial + inode-table root
                                                      |
                         serial -> inline InodeValue
                                      |
               kind / binding count / content root / attribute root
                         |                              |
       directory: name -> inode serial             key -> value root
       regular file: existing file root                  |
       symlink: exact target object                extent-only file mapping
```

Identity is scope plus serial. A directory binding points to an inode identity,
not the file's content ObjectId. Therefore a content/attribute update changes its
inode-table path and filesystem root without rewriting every ancestor directory.
A rename changes affected bindings and their directory inode values.

- Root inode: directory with binding count zero. Other directories and symlinks:
  exactly one binding. Regular files: at least one. Preserve the reference's
  supported hardlink semantics; do not introduce directory or symlink hardlinks.
- Final counts include aliases outside the changed paths. Private intermediate
  counts/effects are not final records. New inode counts are derived from checked
  retained bindings, not accepted from an unchecked caller field.
- No dangling bindings, wrong kinds, duplicate names/identities, count underflow/
  overflow, multiple directory parents or effective-tree cycles.
- Old filesystem roots remain readable. Removing the last current binding removes
  that inode from the new logical filesystem; it does not delete canonical objects
  needed by retained versions or turn logical removal into garbage collection.
- Sorted-tree page finality and whole-operation inode/directory retention are two
  different proofs. Never emit a locally final page for a directory that is later
  discarded as disconnected input, or serialize a provisional inode count.

### Environment-neutral native inputs

Use checked base roots/profiles/scopes, sorted unique per-directory binding changes,
typed inode/content/attribute changes, stable supplied data, authorized identities,
bounded authenticated reads/output and declared ordering resources. New identities
are provided by the caller's allocator; C1 validates scope/range/duplicate/conflict
use and never calls SQLite or allocates durable serial ranges itself.

The reference allocator's durable burn behavior is evidence of uniqueness/lifecycle,
not permission to introduce durability in C1. Document the allocator precondition
and effects at the caller boundary; do not hide it in ObjectStore.

Public native entry points enforce their real trust preconditions. Internal sorted
primitives may consume types whose invariants were established by those entry
points, but a serialized checked/retained/count flag is not proof. Authentication
of object bytes is not full filesystem topology validation.

For updates to a valid immutable base, validate affected bindings, identities and
effective parents using bounded work. Arbitrary graph import may need more work:
declare a checked prepass/compact state instead of claiming scan-free validation.
If final membership cannot be established in the chosen input order, account the
necessary records/passes before promising final output. Do not put an all-files
manifest or unbounded parent map behind an ostensibly streaming API.

A path-command facade is optional. Native directory/inode changes must not route
through Workspace commands to become usable. FUSE, daemon, containers, cloud,
history, public diff/conflict/resolution and live Workspace COW remain outside Stage 5.

### Hard product rules

No retry/fallback, fsync/fdatasync/sync_all/sync_data, WAL, third-party patches,
unbounded payload staging or successful-version rollback. Planned ordering passes,
backpressure and an explicitly chosen backing capability are ordinary work;
failed I/O or sorting cannot trigger another algorithm. Keep existing MEMORY journal,
synchronous OFF, zero busy timeout, visibility watermark and one failed-save cleanup.

Use existing dependency pins/stdlib. Files <=999 physical lines; lib.rs/mod.rs <=200
and declarations/reexports/direct delegation only. External tests/examples/oracles.
No product test hooks, fake clocks, feature-gated benchmark algorithms or gratuitous
public internals. No aggregate preflight/CI/workflow wrapper.

## 3. Implement through six complete checkpoints

Each checkpoint has an explicit deliverable; test-target count is not progress
substitution. Own the whole assignment through integration. If separate agents are
explicitly assigned, canonical/sorted/reads, attributes, and reference-accounting
work may be separated after the shared contracts are settled. Give exact file
ownership, preserve others' changes and use one integration owner for shared
codecs/APIs/schema. Build/measurement work still cannot overlap resource-sensitive
runs. No new agent is launched by this document.

### A. Executable contract, canonical oracles and one tiny real root

Freeze active write profile, supported readers, native input semantics, supplied
identity contract, logical roles/direct references, size limits and resource owners.
Resolve the inode-leaf header gate first. Reuse existing leaf/value types and C2
pooling; add filesystem-root/directory/inode-branch/attribute/symlink roles only as
actually needed. Never cast inode serials into fake canonical object identities.

Create separately sealed reference fixtures with fixed scope, serials, names,
metadata and normalized changes. Verify identical operations before comparing
bytes/roots. No candidate round-trip as its own oracle, no raw command sequence
compared with a differently normalized final-state operation. Test each selected
primitive against its equivalent reference route and complete roots against a
matching filesystem operation.

Deliver a real empty filesystem and a tiny directory/file/symlink tree through C1,
C2 finish, close/reopen and authenticated reads. Canonical references must be derived
from the actual typed values/children. This checkpoint is foundational only, not
Stage 5 completion.

### B. Sorted directory and direct inline-inode COW

Port the reference bounded sorted engine, preserving untouched-subtree reuse,
unresolved neighboring pages, exact fill/partition/height rules and reserve-before-
growth. Its internal 4-MiB budget is not the entire operation's memory allowance.
Keep grouped child acquisition and separate format-specific checks.

Consume optional base roots and strictly sorted unique final changes. Initial
construction needs no provisional empty seed; emit a canonical empty directory only
when it is the real result. Return known count/level internally without rereading
a new root. Feed typed inode values/tombstones directly into compact leaves; remove
synthetic legacy record IDs and encode/temp-store/reread/decode cycles only after
auditing every consuming callback and preserving real page identity/origin hints.

Preserve sorted-route semantics; no input-length guess, iterator rewind and point-
mutation recovery. Invalid order, size, decode or output errors fail once.
Tiny supported budgets must work or reject before an unsafe allocation; do not
silently drop to a slower route. Test branch/leaf boundaries and both finality and
full-operation retention.

### C. Reads and attributes

Implement bounded resolve/stat/list/readlink, directory/inode batch lookup, ordered
repeated demands and shared ancestor acquisition. Reuse checked root/profile/value
state within an operation; do not create a global cache or load the whole inode
table. Listing limits apply to bytes as well as count; continuation must advance
correctly across variable-length names and page boundaries.

Implement checked portable mode/mtime and generic domain/key-to-value-root storage.
Use structural key/length/order/reference validation, reserving portable keys for
their typed grammar; remove the reference's platform-domain whitelist. Other
domains have opaque values with no OS-specific interpretation or enforcement.
Do not port Apple ACL/flag codecs or add uid/gid/atime semantics, native xattr calls,
APFS integration or a speculative adapter. Keep one generic read/set/remove/patch
route. Patches preserve untouched keys and roots without decoding opaque values.
Extent-only value ropes remain extent-only even below the regular-file cutoff;
use existing mapping bodies instead of duplicating CDC/file storage.

Document this deliberate acceptance-contract change. Reference-byte equivalence
covers common supported inputs; new generic domains need separate grammar tests.
Structurally accepted old attributes can remain opaque data, with no claim of
platform validation or permission enforcement. Unsupported required profiles fail
explicitly before mutation; no silently stripped attributes or automatic migration.

Retain MetadataTreeBuilder's exact partition and tail-rebalance algorithm, replacing
trial clone/encode-for-fit with checked exact sizing shared with the encoder.
A real codec/validation failure is not a page-full signal. Decode without canonical
re-encoding only after every order, reserved-field, length and summary invariant
has independent malformed-input/golden-byte coverage. Reuse small exact local
metadata caches if needed; never return an unchecked previously cached kind/mode.

### D. Topology, reference effects and ordered final values

Implement a complete native filesystem update, including create/link/move/unlink
and subtree removal semantics expressed by the logical inputs. Check the effective
final tree, not just ancestry in the old tree. Detect cycles formed by multiple
changes, invalid root operations, name replacement/type conflicts, conflicting
new identities, duplicate parents and disconnected dirty changes.

```text
directory merge: old binding -> new binding
                         |
                  compact edge events
                         |
            complete additions across the operation
                         |
          removals / zero-count descendant traversal
                         |
          newest pending typed values override base reads
                         |
               sorted final inode values/tombstones
                         |
                      inode COW
```

A move or rename must not transiently drop an inode to zero before its new binding
is counted. New counts come from retained final bindings and are not counted again
as ordinary additions. Unseen aliases preserve existing counts. A child moved out
of a deleted directory survives; descendants of a genuinely removed directory are
released through bounded listing. Root/non-root record invariants apply at final
emission; private provisional state must not escape.

Name order and inode order differ. Preserve the reference's tiered merge work bound
instead of repeatedly rewriting the full accumulated prefix. Freeze a compact
semantic record grammar (version/length/endian/tag rules, scope ownership, serial,
value/tombstone/effect and precedence as required); omit Workspace NodeId,
checkpoint receipts and temporary inode-record ObjectIds. Prove each removed field
has no remaining semantic consumer. Do not assume a specific reduced row width
until encoding, merge and failure tests establish it.

The default ordering algorithm has explicit memory/disk quotas and narrowly scoped
caller-supplied seekable record handles/cleanup capability. Keep bounded pending
maps, merge buffers, run descriptors and release cursors. Account simultaneous old/
new runs during merge, all temporary bytes and OS cache residency. Caller-owned
backing is not free memory/disk or an excuse to omit its cost. Release it on success,
error and cancellation/drop under an explicit checked-completion contract; do not
hide explicit finish errors in Drop or add fsync. Unsupported backing fails
explicitly before required work; no general scratch server/protocol is introduced.

Test in-memory-small and record-backed workloads through the same reducer; a
declared resource threshold is not error recovery. Preserve newest-over-base and
newest-run precedence across tier carries, tombstones and cascaded removals.
Reference accounting cannot be replaced by an object-graph reachability pass.

### E. Real C2 and independent timing

Feed all new logical roles through existing C2 batching/admission and supported
ordinary/pool encodings. Preserve physical inode-value pooling, locators, selected
bases, stored canonical identity and retained read visibility. No SQL flush per
directory/file/inode and no extra database for C1 ordering. Plain inode identity
bindings are not canonical-object references; leaf content/attribute roots and
actual child pages/root objects are.

Implement actual C1-only, supplied-object C2-only and integrated operations using
the same bodies. Complete-operation timing includes required input validation,
identity acquisition when performed, ordering, reads, output waits and final save
acknowledgement. Low-level sorted-only diagnostics declare prevalidated/preordered
input exclusions; they are not a complete filesystem-update benchmark.

Use coarse bounded timing, never one retained trace node per inode:

```text
filesystem.update
  validate / establish retained membership
  attributes
  directories
  references (including record backing)
  inodes
  root.encode
storage: actual acquisition / pooling / packing / SQL / finish
```

Provider work must be attributed honestly. Current read-provider APIs do not accept
a timer scope; inspect the actual path before claiming a pure C1/C2 time split.
Use narrow real scope plumbing or explicitly inclusive labels; do not hide C2 work
inside claimed pure C1 CPU. Disabled timing preserves results, errors and algorithm.
Bounded/clipped details remain labelled incomplete; coarse operation reporting must
remain useful without per-object retention. JSON is caller-owned ordinary output.

### F. Reference parity, limits, qualification and final review

Complete every test and original issue criterion, including reference ordering and
the selected portable/generic attribute contract. Check common supported operations
preserve reference canonical bytes/roots under identical profile/scope/identities/
normalized input; test the intentional domain-acceptance change separately. Check malformed
and unsupported inputs fail once, old roots survive and no emitted filesystem tree
objects become unreachable on a successful update.

Preserve initialization's existing parallel-producer exception at the orchestration
boundary. Pure C1 primitives can be single-threaded; do not serialize the permitted
initialization composition or add workers to single-producer mutation cases.

Report enforced/configurable/format/theoretical/verified limits separately. Reference
name/path defaults are 255 bytes/component, 4,096 bytes/path and 256 components;
the sorted profile uses 8-KiB pages and depth 31, with 50–100 compact inode leaf
occupancy and 64–127 non-root inode branches. Re-derive exact root exceptions,
directory variable-width occupancy and practical combined bounds from code.
Test byte versus character rules, UTF-8, separators, root path, boundary and first
invalid values. No host PATH_MAX assumption, arbitrary new inode count limit, or
claim of unlimited workspace size from u64. Directory depth/path length, entries,
stored object count, logical bytes and disk bytes are different dimensions.
Identity exhaustion and reference-count arithmetic fail explicitly.

The final report includes actual resource ceilings/backing requirements and largest
verified inputs. Stage 5 establishes filesystem-core behavior; workspace lifecycle/
quota, transport topology and full-runtime scale remain later owners.

## 4. External tests and exact checks

Use the [file plan's external test paths](stage-5-file-plan.md#6-supporting-files-and-tests).
Keep prior file/delta/pooling regression targets active. Minimal acceptance matrix:

| Target | Must establish |
| --- | --- |
| filesystem_codec | Golden bytes/IDs for root, names, directory pages, inode branches/leaf compatibility, attributes/symlinks; malformed/trailing/reserved/overflow input rejected. |
| filesystem_reference | Sealed reference operations with identical scope/serials/names/attributes; exact complete-root and page partitions, unchanged subtree identities. |
| filesystem_sorted | Optional-base initialization, valid empty output, leaf/branch/fill boundaries, final sibling rebalance/root collapse, duplicate/unsorted changes rejected once. |
| filesystem_read | Resolve/stat/list/readlink, count+byte-limited pagination, ordered duplicates, shared ancestor acquisition, bad summaries/kinds/scopes and exact EOF. |
| filesystem_updates | Create/update/move/remove through native inputs; content/attribute-only updates leave unrelated directory roots unchanged; same-name no-op and type replacement match the selected reference route. |
| filesystem_hardlinks | Unseen aliases, new-inode initial counts, rename/move additions-before-removals across directories and ordering batches, moved-out child survives subtree deletion, last-link removal affects only new root. |
| filesystem_topology | Root invariants, duplicate allocation/scope errors, two parents, effective multi-change cycles, orphan/disconnected dirty records, invalid asserted counts/retention cannot bypass validation. |
| filesystem_attributes | Portable mode/mtime checks; generic key grammar and opaque values without platform dispatch; untouched-key/root preservation; extent-only small/large values; exact-size page/tail partition; explicit unsupported-profile failure; no swallowed encode error. |
| filesystem_ordering | Actual configured pending/run thresholds, tier carries/merge precedence, compact row golden decode, truncated/bad rows, memory/disk quotas, backing cleanup and no runtime fallback. |
| filesystem_failure | Late input/order error, corrupt demanded page, failed output/backing read/write/seek, cancellation/drop; no success after failed cleanup and no repeated operation. |
| filesystem_bounds | Empty/wide/deep trees, many directories/aliases, released subtrees, decoded page plus C2/backing overlap, limited read waves, counters showing no unrelated whole-tree scan. |
| filesystem_timing | DB-free native C1 operation, sorted-only versus complete scopes distinguished, real timer on/off equality and errors, coarse bounds under many entries. |
| C2 filesystem_pipeline | All new roles, direct references, real pooled inode leaves and value roots; shared save across directories; finish/reopen/exact readback and independent/integrated root equivalence. |
| C2 filesystem_failure | Private early output hidden, late filesystem failure causes one known-owned cleanup, catalogue/dependency correctness, retained old roots, unavailable owner/SQL failure. |

Use real external inputs/backing failures and existing public capabilities, not
product fault hooks or fake clocks. Oracle/model memory is verification-only and
separate from product ownership. A passing round-trip cannot replace reference
comparison; a declared capacity cannot replace a measured peak. Missing required
coverage stays visible and blocks that criterion.

Focused commands after implementing their real targets:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_codec --test filesystem_reference --test filesystem_sorted --test filesystem_read --test filesystem_updates
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_hardlinks --test filesystem_topology --test filesystem_attributes --test filesystem_ordering --test filesystem_failure --test filesystem_bounds --test filesystem_timing
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test filesystem_pipeline --test filesystem_failure
```

Final affected-workspace checks, run individually when the implementation is stable:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

fmt has no --locked flag. Reconcile any documented formatter pin before changing
unrelated source formatting; report the actual toolchain and outcome. No aggregate
preflight and no repeated unchanged qualification without a relevant reason.

### Runnable demonstrations

Implement the planned examples with this interface; these flags are requirements,
not a claim of an existing CLI:

```text
filesystem_timing_c1 --case empty|directory-update|inode-update|hardlink-move|subtree-remove|attributes --output FRESH_DIRECTORY
measure_filesystem --mode c1|c2|pipeline --case <same cases> --output FRESH_DIRECTORY
```

Examples use fixed identities/deterministic bounded fixtures and explicit local
ordering resources owned by the caller. Report input/preparation, included/excluded
work, root/result, exact readback and actual text/JSON timing. No unbounded fixture
collector. C1-only needs no SQLite; C2-only receives supplied bounded canonical
objects and includes no timed filesystem construction. Pipeline includes actual
construction, bounded handoff and final save, with readback separately labelled.

```sh
stage5_smoke="$(mktemp -d /tmp/layerfs-stage5.XXXXXX)"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-content --example filesystem_timing_c1 -- --case hardlink-move --output "$stage5_smoke/c1"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_filesystem -- --mode c2 --case inode-update --output "$stage5_smoke/c2"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_filesystem -- --mode pipeline --case subtree-remove --output "$stage5_smoke/pipeline"
```

These are correctness/wiring smoke runs. They do not establish a cold-cache latency
or comparative performance claim.

## 5. Optimization targets against v0.1.6

The reference already has inline compact inodes, sorted COW, grouped directory
reads, bounded working state and tiered ordering. Preserve them. The intended gains
come from removing specific surrounding work, not declaring those algorithms new.

```text
reference bridge:
typed inode -> encode temporary object -> hash/store -> write ID into ordering row
            -> read/decode record -> sorted inline inode leaf

target:
typed final inode value / tombstone -> compact ordering if needed
                                   -> sorted inline inode leaf

reference attribute fit:
push -> clone entries -> trial encode -> discard -> eventual final encode

target:
push -> checked exact size -> final partition -> encode once
```

| Source-backed cost | Intended reduction | Required proof |
| --- | --- | --- |
| [filesystem/apply.rs](../../../../../crates/layerfs-content/src/filesystem/apply.rs) point loops, sorted-route preview and provisional empty seed | Direct declared sorted update/initial construction | Identical partitions/roots, fewer page operations/encoded drafts; no failed-fast route earns a win |
| [tree/batch.rs](../../../../../crates/layerfs-content/src/tree/batch.rs) synthetic inode_value_id and wrapper rereads | Typed leaf values and known summaries | Same observer/reference effects and page identities, fewer synthetic hashes/reads |
| [Workspace changes.rs](../../../../../crates/layerfs-workspace/src/changes.rs) encode/store/ID-rewrite/reread chain and 192-byte rows | Direct typed final rows; only necessary semantic ordering data | Record layout proven, actual temporary bytes/passes and combined live state reduced or preserved |
| [inode/table.rs](../../../../../crates/layerfs-content/src/tree/inode/table.rs) repeated root clone/point descent | Shared bounded ancestor traversal | Same ordered/cardinality results; fewer node/group acquisitions |
| [metadata/tree.rs](../../../../../crates/layerfs-content/src/tree/metadata/tree.rs) trial serialization and clones | Exact size accounting, one final encode | Exact boundary/tail partitions and failures, codec/alloc/copy counts |
| Checked decode followed by canonical re-encode solely for validation | Full direct invariant validation | Malformed/golden fixtures cover every removed check; no weakened trust boundary |
| Caller metadata readback solely to recover constructed mode/mtime | Carry checked typed results locally | Same supported-domain preservation and less acquisition; do not claim an unintegrated Workspace speedup |

Ordering cost remains real work. A subtree deletion must inspect released entries;
initialization must consume its inputs; variable-size metadata may need several
pages. Bound work by changed regions plus required siblings/released subtree and
ordering passes. Do not claim universal O(changes), zero temporary disk or constant
RSS from a diagram. C2 pooling/index improvements already landed in Stage 3 are
preserved and measured in composition, not credited again as Stage 5 inventions.

### Required measured proof

Before benchmark implementation/collection, create and commit
`stage-5-verification.md` linked to #170. Freeze exact family/case IDs, physically
realized sizes, fixed scope/serial/name/value data, matched public operations,
profile, sorted/native-input semantics, cache/index state, worker policy, source/
product/harness/oracle seals, timing/acknowledgement boundaries, resource/storage
limits and numerical gates. Baseline-derived limits must precede candidate
optimization/collection. A generic table of future cases is not a frozen campaign.

At minimum cover:
- Empty/small and wide/deep initial builds; preserve the init_namespace exception
  only for an actually equivalent initialization operation.
- Few-key updates in increasingly large real trees; unrelated-subtree work counters.
- Cross-directory moves, aliases outside the changed set, and subtree removal with
  a moved-out or externally linked child.
- Attribute patch/read with portable and opaque generic domains and large extent-backed values; compare only common supported reference inputs.
- Ordering threshold/tier/reopen-like resource setup where applicable, slow output,
  late failure and quota boundaries.
- Real pooled inode storage, final pack/DB footprint, independent and integrated
  time, ordering I/O and actual simultaneous memory, including file cache.

Compare equivalent successful v0.1.6 operations; native preordered C1 cannot be
compared with an old complete Workspace pipeline. Component comparison that excludes
normalization/order preparation says so; complete-operation comparison includes it.
Where no equivalent public surface exists, retain structural/correctness/resource
proof and labelled non-comparative diagnostics; no invented speedup ratio.

Use one sample per case/arm unless explicitly approved otherwise, declared equal
cache conditions, setup clone for applicable post-initialization cases, immutable
prepared inputs, fresh evidence, separate verification, sealed build reuse and
the resource lock. No warm-credit/extra worker/workload shrink/timeout inflation.
Ordinary complete selection <=15 s; declared exceptions <=25 s; verification <=60 s
under the actual applicable contract. Preserve every fail/ineligible/not-run row.

Report elapsed ns and complete command wall, exact units/sample counts, page/SQL/
pool calls, canonical/copy/hash/encoding work, event/run bytes read/written, retained
Store footprint and allocation/peak observations with scope. Missing metrics are
unavailable, not zero. Lifetime high-water is not phase-local peak. No measured
performance superiority is claimed by this handoff; it is established only by
eligible completed cases. Stage 3–4 owner waivers do not waive #170 automatically.

## 6. Completion report and issue closure

Use `stage-5-report.md` and fresh v0.1.7 evidence directories. Include:

1. Exact source identity, profile/role/schema choices and compatibility disposition,
   including the inode-leaf header gate and same-input reference oracles.
2. Actual folder tree and per-file/directory estimates versus actual, physical caps
   and exact per-commit production LOC with migration subtotals.
3. All #170 criteria and every checkpoint above, with code/test/evidence links and
   separate correctness/resource/performance/cleanup statuses.
4. Independent C1/C2/integrated actual timing, scope/exclusions and on/off behavior.
5. Memory and ordering-disk ledger: owner, capacity, maximum multiplicity, transient
   coexistence, release/cleanup event and measured scope/limitations.
6. Enforced versus theoretical versus actually verified limits, including identities,
   path/name/depth, entries, reference counts, attributes, backing and file/Store size.
7. Concrete cuts with reference source map and actual work evidence; no fabricated
   performance gain, count-only test completion or undocumented partial scope.
8. Remaining simplifications, exact blockers and explicit Stage 6/7 boundaries.

Run a final source/criterion review, especially cross-directory reference ordering,
topology and preserved metadata domains. A local sorted-tree PASS does not cover the
filesystem operation. Close #170 only when its full acceptance criteria have evidence;
do not move missing correctness, reference accounting or its own qualification into
Stage 6. Keep #165/#171/#172 open; no release/tag, reference retirement or runtime work.

If externally interrupted, retain a precise continuation with source identity,
completed checkpoints, remaining files, failing case/proof and next action. A run
limit is not a technical blocker or completed stage. Continue the same assignment
until the full result is implemented and verified.
