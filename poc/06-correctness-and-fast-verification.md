# Correctness-first fast verification

Status: verification contract for the integrated Apple/APFS PoC. This is a
test ladder, not a benchmark evidence program.

Related design:

- [scope and decisions](00-scope-and-decisions.md)
- [architecture and file structure](01-architecture-and-file-structure.md)
- [data structures and algorithms](02-data-structures-and-algorithms.md)
- [operation workflows](03-operation-workflows.md)
- [Apple/APFS materialization and recovery](04-apple-apfs-materialization-and-recovery.md)
- [minimal implementation plan](05-minimal-implementation-plan.md)
- [native workspace and Bash verification](08-native-workspace-and-shell-verification.md)
- [portability and Apple completeness](09-portability-and-apple-completeness.md)
- [final handoff freeze](10-handoff-freeze.md)

## 1. Policy

```text
edit code
  -> touched deterministic tests
  -> component tests
  -> one workspace correctness closure
  -> one tiny release smoke benchmark
  -> stop
```

Rules:

- correctness and structural work are gates; wall time is diagnostic;
- use the product libraries and public example, never copied benchmark logic;
- one source change may add one focused regression, not a new evidence version;
- fix pre-row harness mistakes in place and keep normal test output;
- do not preregister deterministic unit tests;
- do not repeat unchanged bytes for a favorable number;
- no 100/500 MiB matrix, cold-cache theater, long warm-up or campaign farm;
- tests fail on semantic mismatch; measurement never converts failure to pass;
- existing plans, G5 claims and G6 projections are hypotheses until the new
  source passes its own tests.

## 2. Tests versus measurement

| Question | Mechanism | Gate? |
|---|---|:---:|
| are bytes, identities and roots exact? | deterministic tests/oracles | yes |
| is the tree canonical under its selected identity policy? | independent codec goldens + validator | yes |
| are old roots immutable and readable? | retained-root tests | yes |
| are publication and recovery atomic? | fault/restart tests | yes |
| is work local by construction? | observed structural counters and inequalities | yes |
| is memory bounded by design? | owned-Q/buffer/descriptor assertions | yes |
| does the whole small product work? | release smoke sequence | yes |
| is it fast on this Mac today? | elapsed/CPU/RSS diagnostics | no unless a gross stop rule fires |
| is it production-scalable? | not claimed by the PoC | no |

## 3. Per-edit fast loop

Run the smallest command covering the changed boundary:

```bash
cargo fmt --all -- --check
cargo test -p layerfs-core --test extent_codec
cargo test -p layerfs-core --test extent_model
cargo test -p layerfs-engine --test store_and_publication
cargo test -p layerfs-engine --test faults_and_reopen
cargo test -p layerfs-os apple_driver
cargo test -p layerfs-vfs --test poc_workflow
cargo test -p layerfs-sdk --test workflow
```

The exact filters become valid when the tests named in the implementation plan
exist. Do not run all commands after every edit. Run `cargo check` for a touched
crate when the compiler can close the loop faster than its test target.

Before merging one work package:

```bash
cargo test -p <touched-crate>
cargo clippy -p <touched-crate> --all-targets -- -D warnings
```

Before PoC release, once:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

No benchmark is required to merge an internal correctness repair.

## 4. Core differential model: the primary oracle

Use `Vec<u8>` as the semantic oracle and the persistent tree as the candidate.
No new property-testing dependency is needed; a small deterministic xorshift
generator in the test file is sufficient.

For each fixed seed:

```text
reference = Vec<u8>
candidate = empty tree
retained  = []

repeat operation:
  choose overwrite / insert / delete / append / truncate / replace
  generate valid offset, length and replacement bytes
  apply operation to Vec<u8>
  apply identical operation to candidate
  validate every reachable candidate node
  stream complete candidate and compare exact bytes
  compare selected point/range reads
  retain selected roots without mutation
  check structural counter bounds

after sequence:
  reopen every retained root
  compare with its saved Vec<u8>
```

Run a second deterministic oracle for namespaces. The reference is a nested
ordered map containing regular-file roots, directory maps and exact symlink
targets; the candidate is the persistent namespace tree. Generated operations
cover lookup, create, remove, replace, same/cross-directory rename, mode change,
symlink create/replace, deep split/borrow/merge and root collapse. The test uses
enough synthetic names to form multiple levels without increasing benchmark
file bytes.

### Required deterministic sequences

| Sequence | Operations | Purpose |
|---|---:|---|
| boundary table | explicit cases | empty, byte 0, EOF, leaf/node min/max, root grow/shrink |
| random-small A | 1,000 on files `0..256 KiB` | broad fast semantic coverage |
| random-small B | 1,000 with different seed | avoid one lucky shape |
| split/merge adversary | 512 alternating insert/delete | occupancy and root collapse |
| repeated-payload | 256 edits using repeated byte patterns | repeated object IDs and slice identity |
| retained history | 1,000 tiny revisions, verify fixed checkpoints plus final | immutability and long-history lookup |
| A-to-B-to-A | insert then inverse/delete and replace then inverse | byte equality and declared root-identity policy |
| namespace deep | 10,000 synthetic names with deterministic remove/rename waves | logarithmic path-copy, variable-name node occupancy and root shrink |

Tests freeze the root result required by
[`02-data-structures-and-algorithms.md`](02-data-structures-and-algorithms.md):
if roots are operational/history-shaped, equal bytes may have different
`FileStateRoot`s but must have equal streamed bytes and optional
`ContentDigest`; if one-content/one-root is chosen, roots must also match.
Do not accidentally assert both policies.

### Every-step assertions

```text
stream(candidate_root) == reference
root.total_bytes == reference.len
sum(leaf extents) == root.total_extents
every object hash == requested ObjectId
every extent source_offset + length <= payload length
every internal child measure == decoded child measure
namespace keys ordered; node encoded bytes/fill/root rules canonical
namespace nodes read/created proportional to changed root-to-leaf spines
occupancy/root rules hold
depth <= frozen maximum
old retained roots are unchanged
owned_Q_terminal == 0
```

## 5. Codec goldens and parity

### Independent goldens

Construct expected bytes manually in the test, not by calling the production
encoder twice. Positive decode/encode round-trip goldens cover:

- empty canonical `FileStateV3` plus zero-entry root leaf;
- one extent and one-byte file in mode-free `FileStateV3`;
- representative regular/directory/symlink mode and mtime vectors in
  `PortableMetadataV1` goldens;
- root leaf at 0/1/maximum entries;
- non-root leaf at minimum/minimum+1/maximum-1/maximum entries;
- non-root branch at minimum/maximum children;
- internal root at two-child grow and one-child collapse boundaries;
- maximum legal offsets/lengths;
- exact profile-ID and role-tag separation vectors.
- inode-table and metadata branch descriptors at minimum/maximum legal keys;
- `AppleAclV1` absent/one/128-entry vectors and exact `CURRENT` selector bytes.

Decode then encode must reproduce the exact golden bytes. Decoders reject
trailing bytes, alternate integer widths, noncanonical occupancy and redundant
topology.

Manually constructed **negative rejection vectors**, never positive round-trip
goldens, cover:

- two adjacent contiguous slices of the same payload;
- non-root minimum-1 and maximum+1 occupancy;
- root branch with zero/one child;
- representative source-offset/length and subtree-total overflow;
- wrong file/leaf/branch role tag with otherwise well-formed fields;
- wrong profile ID, child level, cumulative measure, and redundant topology.
- orphan/missing inode-table records, root/non-root ref-count mismatch,
  directory cycle/multiple parent and reachable-set inequality;
- invalid mtime nanoseconds, unused mode bits, ACL masks/count/order framing,
  selector checksum/filename/generation mismatch and trailing selector bytes.

### Old/new parity gate

The current repository has overlapping file codecs and distinct reusable and
benchmark SQLite implementations. Before deletion/extraction:

| Promoted behavior | Parity assertion |
|---|---|
| Phase-1 object envelope | old and new use byte-identical envelope/hash domain |
| frozen FastCDC | old and new emit identical chunk boundaries/IDs for selected vectors |
| immutable CAS | create/reuse/tamper outcomes match accepted behavior |
| expected head | stale parent fails before visible publication |
| transaction | one state-changing capture starts one writer transaction and dispatches one COMMIT |
| ambiguity | requested/prior/different/ambiguous classification matches the promoted contract |
| trust | Verified default, touched identity checks and Verified-after-Trusted scrub remain |
| mailbox | deferred from synchronous PoC v1; no background worker or pending queue is required |
| native publisher | each replaced file is old or new; a mixed directory has no Complete live authority and is rebuilt/reconciled |

Do not require new fresh-profile mapping IDs to equal K64/F64 IDs. Require
exact logical bytes, payload identities where reused, and explicit
old-profile/new-profile distinction. A test importing the benchmark module is
not parity; both arms must call independent old authority or frozen vectors and
the new library surface.

## 6. Corruption and malformed-input matrix

Keep the matrix compact by testing one representative at each shared trust
boundary:

| Fault | Required outcome |
|---|---|
| payload bytes do not match object ID | `IdentityMismatch`, no returned bytes |
| mapping bytes do not match node ID | `IdentityMismatch`, no traversal past node |
| wrong node role/level | typed role/level failure |
| missing child | typed missing-object failure |
| child measure differs | malformed-tree failure |
| extent outside payload | bounds failure |
| noncanonical occupancy/order | canonicality failure |
| cycle or excessive depth | cycle/depth failure without unbounded traversal |
| length/count arithmetic overflow | overflow before allocation/publication |
| corrupted SQLite row | no incumbent reuse and no visible new root |
| wrong/absent live projection authority/root/store binding | reject fast route; exact verification/rebuild policy |
| destination symlink/wrong file kind/substitution | fail closed; never overwrite followed target |

For every error assert primary error, visible head, retained roots, transaction
state, owned Q and temporary residue. Do not create one subprocess per byte
offset.

## 7. Publication, fault and restart tests

Use test-only failpoints in the shared product publisher. Keep the smallest
set that crosses each durable boundary:

```text
T0 after bounded operation/evidence preparation, before BEGIN
T1 after BEGIN, before expected-head check
T2 after expected-head and partial streamed object inserts
T3 after root rows, before visible-head update
T4 after visible-head update, before COMMIT dispatch
T5 COMMIT returns success
T6 COMMIT returns error/lost acknowledgement
```

After T0, no transaction exists. After T1–T4, rollback is attempted and a fresh
engine must observe the prior head; cleanup/rollback failure preserves both
causes. All SQLite BLOB/object rows inserted after `BEGIN` roll back with the
same transaction. At T6, fresh reconciliation returns exactly requested,
prior, different or ambiguous; never redispatch the mutation automatically.
Every terminal exit has zero live writer transaction and no partial visible
root.

Native projection cuts:

```text
N0 private temp created
N1 clone/full stream complete
N2 patch complete
N3 file sync complete
N4 rename attempted/acknowledgement lost
N5 directory sync attempted/acknowledgement lost
```

For N0–N3, final path is prior and owned temp is removed on cleanup/reopen. For
N4–N5, inspect destination identity and digest and classify exact prior/new/
ambiguous before any further attempt. The immutable LayerFS root remains the
authority regardless of projection failure.

## 8. Concurrency, reopen, trust and lifecycle

### Minimal concurrency test

```text
reader A pins root R0
reader B opens current head R0
writer publishes R1 with expected R0
reader A still reads exact R0
reader B either remains pinned R0 or explicitly refreshes to R1
fresh reader C sees R1
second writer with expected R0 receives ParentMismatch/Conflict
```

Set `busy_timeout=0` for the PoC runtime and assert immediate typed
Busy/Locked with no internal or application retry. This concurrency policy is
not part of canonical profile identity.

Assert one writer transaction/COMMIT, zero unexpected Busy/Locked, bounded
connections and no thread/worker residue. No test may pass because an
undocumented wait hid contention.

### Reopen and trust

- reopen a clean Verified store and read root/ranges;
- reopen after a TrustedLocalDev mutation in Trusted mode;
- reopen the same store in Verified mode and require complete scrub before a
  Verified edit authority exists;
- corrupt an untouched object before Verified reopen and require failure;
- prove a trusted assumption never creates a Verified receipt;
- rollback freshness remains `NotProtected` unless an external authority is
  supplied and tested.

### Workspace lifecycle

Test exactly one terminal action:

```text
Active -> Captured
Active -> Discarded
Active -> FailedTerminal or Active (as frozen by error contract)
```

`Drop` is cleanup assistance, never the only required cleanup API.

## 9. History, fork, rollback and compaction readiness

### History

Use 1,000 tiny revisions, not 1,000 large revisions:

- alternate same-size overwrite, insert, delete, append and truncate;
- keep every root immutable;
- read exact ranges at revisions 0/1/10/100/500/999/1,000;
- materialize three nonadjacent roots and compare directory oracles;
- record unique payload and mapping growth per operation;
- require no per-revision full mapping/file copy for local managed edits.

### Fork

```text
base R
  -> label A = R
  -> label B = R
  -> edit A -> RA
  -> edit B -> RB
```

Require `R`, `RA` and `RB` exact and independently readable. Fork itself adds
only reference metadata. A stale expected head on one label cannot retarget the
other. Merge/rebase is not tested because it is not in PoC scope.

### Rollback

Move one label from `R2` to retained `R0` with expected current `R2`, one
transaction and one COMMIT. `R1` and `R2` remain readable. A concurrent change
to `R3` makes the rollback conflict. Do not claim protection against external
store rollback without a monotonic external authority.

### Offline compaction

First test the independent mark oracle:

```text
retention roots = all visible labels + pinned readers + recovery pins
marked = authenticated traversal(retention roots)
candidate garbage = complete object index - marked
```

Verify objects reachable only from old/forked roots are marked. Current-head
unreachability is never sufficient. Then run the product offline compactor:

```text
reject while reader/writer/workspace/recovery pin exists
copy the marked union to one sibling Store with bounded buffers
verify schema/profile/generation, refs and every retained root
fault before/during/after sibling COMMIT, sync, swap, reopen and backup cleanup
recover to exactly one installed verified Store without losing the only good copy
prove candidate-garbage objects are absent and retained objects byte-identical
```

Do not call `VACUUM` or in-place row deletion graph GC. The new evidence
qualifies only explicit offline exclusive compaction.

## 10. APFS materialization and reconstruction suite

Use a temporary directory on an observed APFS volume. If the volume is not
APFS, run the correctness fallback and report clone route `NotApplicable`.

Fixture (about 3 MiB total):

```text
project/
  README.md                 4 KiB text
  src/main.rs              64 KiB deterministic text
  src/lib.rs              256 KiB deterministic text
  assets/blob.bin           2 MiB deterministic binary
  data/repeated.bin       768 KiB repeated-block binary
  scripts/check.sh          1 KiB executable deterministic script
  current-readme                 symlink -> README.md
  hardlink-readme                 hard link -> README.md inode
  empty/                         empty directory
```

Correctness sequence:

1. import and publish root `R0`;
2. cold materialize to an empty destination and compare exact tree;
3. warm exact materialize and assert zero native content rewrites;
4. point/range/full read selected files;
5. managed same-size 4 KiB overwrite;
6. managed middle 8 KiB insert;
7. managed middle 4 KiB delete;
8. append then truncate;
9. execute `/bin/bash ./scripts/check.sh` with the materialized workspace as
   its real current directory;
10. materialize/convert to `ExternalWorkspace`, then run one deterministic Bash script that performs
    direct redirect, same-size `dd`, append, truncate-to-empty, `mkdir`, create,
    `mv`, `rm`, `chmod`, symbolic-link/hard-link operations and one xattr;
11. hold one controlled background writer and prove capture returns
    `WorkspaceBusy`; stop/reap it;
12. run one tiny writable-mmap helper, flush/unmap/exit;
13. capture `R1` through the cooperative full scan and compare against the
    mutable native oracle, including kind/mode/link target;
14. close and reopen the engine;
15. reconstruct/materialize `R1` to a second directory, rerun read-only Bash
    assertions, and compare the exact tree;
16. fork `R0`, diverge both labels and read all roots;
17. rollback one label with expected head;
18. run one nonzero shell command and prove it does not automatically publish;
19. verify child/process-group, temp, seed, journal, worker and connection residue.

Comparison includes relative path bytes, canonical `InodeId`/hard-link groups,
file kind, regular-file bytes, length, selected supported mode bits, exact
symbolic-link target bytes, Apple extension metadata and empty directories.
Capture rejects device/FIFO/socket kinds with exact typed errors; it never
silently changes their meaning.
Directory enumeration order is normalized canonically; host enumeration order
is not identity.

Add case-only and normalization-equivalent sibling fixtures. On a destination
where APFS collapses them, materialization must fail with typed
`NativeNameCollision` before Complete live authority and leave the canonical
root valid; on a qualifying destination class, exact canonical-name enumeration must
round-trip.

## 11. Structural Big-O counters

Counters are observed from shared product code. Constants encode enforced
limits; they are not fabricated observations.

Minimum per operation:

```text
input_bytes
cdc_bytes_scanned
payload_objects_fetched / created / reused
payload_bytes_read / written / hashed
canonical_rows_fetched / authentication_passes / decode_passes
payload_batch_queries / references / maximum_references
mapping_nodes_read / authenticated / created / reused
mapping_bytes_read / written
tree_height_before / after
splits / merges / root_grows / root_shrinks
extent_count / fragmentation_ratio / newly_inserted_unreachable_objects
directory_nodes_read / created; inode_table_nodes_read / created
inode_lookups / directory_lookups / inode_records_changed
returned_bytes
sqlite_statements / writer_transactions / commit_dispatches
reconciliation_calls
native_bytes_read / written / patched
native_digest_pass_bytes / changed_cdc_pass_bytes / prior_digest_stream_bytes
hard_link_groups / paths / scratch_bytes
metadata_list/value_calls / bytes
clone_attempts / successes / fallbacks
owned_q_current / high_water / terminal
largest_buffer
open_descriptors / connections / temp_files
logical / apparent / allocated storage when available
```

Required inequalities for an ordinary one-island managed edit, with frozen
node capacities supplying constants `c1`/`c2`:

```text
mapping_nodes_read    <= c1 * H + split_merge_allowance
mapping_nodes_created <= c2 * H + replacement_tree_nodes + split_merge_allowance
cdc_bytes_scanned      <= replacement_input_bytes
unaffected_suffix_payload_writes == 0
owned_q_high_water    <= 8 MiB
largest_buffer        <= 1 MiB
writer_transactions   == 1
commit_dispatches     == 1
authentication_passes == canonical_rows_fetched
decode_passes         == canonical_rows_fetched
payload_batch_maximum <= 64
newly_inserted_unreachable_objects == 0
owned_q_terminal      == 0
```

For full import, full-workspace external capture, full materialization,
Verified scrub, reachability and compaction, linear counters are expected and
must be labeled by their correct class.

## 12. One deliberately small benchmark

Run only after every correctness gate above passes. The benchmark uses the same
roughly 3 MiB directory and product APIs from section 10.

Proposed command after the evaluator subcommand exists:

```bash
cargo build --release -p layerfs-eval
target/release/layerfs-eval apple-poc target/poc-smoke
```

One invocation performs three deterministic repetitions of the complete small
sequence in fresh sibling temporary directories. Three repetitions provide a
median and range but are one diagnostic campaign, not permission to rerun for
noise. `<=30 s` is a gross diagnostic stop for this approximately 3 MiB fixture,
not canonical correctness or a product SLO; preparation, post-check and cleanup
are included.

Rows:

| Row | Operation | Correctness condition |
|---|---|---|
| S0 | fresh import/open | exact source root and store profile |
| S1 | cold full materialize | exact tree |
| S2 | warm exact materialize | exact tree; zero content rewrites |
| S3 | 4 KiB same-size managed overwrite/capture | exact new root; structural bounds |
| S4 | 8 KiB middle insert/capture | exact new root; no unaffected suffix writes |
| S5 | 4 KiB middle delete/capture | exact new root; no unaffected suffix writes |
| S6 | mixed path add/remove/rename | exact directory tree/delta |
| S7 | real Bash read/execute | ordinary files and executable mode work before mutation |
| S8 | Bash redirect/dd/append/truncate/mkdir/mv/rm/chmod/symlink/hard-link/xattr | ordinary APFS paths; registered writer blocks capture; caller attests quiescence |
| S9 | external full-workspace capture | exact root; unchanged history-shaped files retain prior FileStateRoots |
| S10 | process reopen + fresh materialize + Bash assertions + 4 KiB historical range | captured physical view and canonical historical bytes exact |
| S11 | fork + divergent edits + rollback | every retained root exact |
| S12 | offline compaction | retained roots exact; unreachable objects absent; installed Store reopens |

Report per row:

- correctness/status and route class;
- elapsed wall, user CPU and system CPU when directly observable;
- structural counters from section 11;
- process RSS, owned Q, largest buffer and descriptor/connection high-water;
- SQLite logical/apparent/allocated bytes, journal/temp high-water;
- native logical bytes read/written/patched and APFS clone outcome;
- source commit/diff fingerprint and macOS/APFS/SQLite/Rust environment.

Do not publish p95/SLO claims from three repetitions. A row slower than hoped
is a diagnostic unless it violates `30 s` complete wall, structural locality,
memory bounds or correctness. Fix only an observed owner; do not expand the
campaign.

## 13. Minimal artifacts

One closure directory is enough:

```text
target/poc-smoke/
  environment.json
  test-receipt.json
  rows.jsonl
  summary.md
  stderr.txt                 only when nonempty/failure
```

`test-receipt.json` records source commit, dirty diff hash, commands and pass
counts. `rows.jsonl` is append-only within the one invocation. `summary.md`
separates observed, derived, unavailable and not-applicable fields. Do not
create a manifest of the manifest, version directories for syntax errors,
independent analyzers for arithmetic already asserted by tests, or frozen
preparatory copies of the repository.

## 14. Release checklist

- [ ] one fresh-profile codec/validator; no dual new-write codec
- [ ] no product import from `src/bin` or fixed benchmark fixture
- [ ] deterministic model, goldens, corruption and retained-root tests pass
- [ ] one reusable `layerfs_*` schema and one publication path
- [ ] stale head, one transaction/COMMIT and ambiguous reconciliation pass
- [ ] Verified/Trusted boundary and Verified-after-Trusted scrub pass
- [ ] cold/warm materialize, managed capture and external full-workspace-scan fallback pass
- [ ] persistent namespace model and structural path-copy bounds pass
- [ ] real `/bin/bash` read/edit/path/mode/symlink/hard-link/xattr workflow captures and rematerializes exactly
- [ ] APFS acceleration is optional and output-identical to full fallback
- [ ] fault/restart/temp cleanup, per-file old-or-new and incomplete-tree live-authority handling pass
- [ ] reader/writer, history, fork and rollback tests pass
- [ ] reachability marks all retained/pinned roots
- [ ] offline compaction rejects pins and passes mark/copy/verify/swap/reopen fault recovery
- [ ] ordinary managed edits meet structural and owned-memory bounds
- [ ] SDK example runs without test hooks
- [ ] workspace format/test/clippy/diff closure passes once
- [ ] one `<=30 s` small smoke invocation passes; no unchanged rerun
- [ ] limitations list external full-workspace scan, unsupported device/FIFO/socket kinds, rollback freshness, frozen Apple-profile qualification, cooperative shell quiescence and no online/in-place GC
