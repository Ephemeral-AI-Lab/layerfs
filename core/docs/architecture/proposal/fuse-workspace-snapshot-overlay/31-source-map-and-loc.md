# Current source map and exact LOC

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Implementation parent: `9060c26bcc3e905e031415da20cec54352b94192`.
> Frozen production input seal: `e5589871b9e32f54fbccd458fccdc187302b99dc1f7873f51f65a039721df963`.
> Exact implementation commit: **pending**; counts bind the frozen production seal above.
> These are actual source counts, not 04's historical planning allowances.

`P` means production LOC: nonblank, non-comment implementation lines, including
imports, declarations and runtime SQL. `L` means all physical lines, including
comments and blanks. They must not be confused: the 999-file/200-entry-file ceilings
apply to L. `N` means the recursive number of production files in a folder.
JSON includes all 255 core and 193 reference production files separately,
plus each scope/crate and every recursive production-folder total.
Tests, fixtures, examples, tools, docs and manifests are excluded from P; their
folder locations are still shown in the continuation handoff.

Reproduce with the unchanged counter (blob
`b5b9617d08204977176302311e0b2c72a811b420`): run
`python3 tools/production_loc.py --root <source-snapshot> --files` and `--json`
against the frozen source snapshot. Once committed, archive that exact commit's
`crates` and `core/crates` and rerun the same commands for confirmation.
The linked JSON uses the same counter's per_file() function and records the parent
and product seal without representing the unchanged parent as the counted source.
[Current per-file machine-readable inventory](evidence/mounted-symlink/source-loc.json)
records this new frozen production inventory and unchanged reference
tree. The prior [symlink-constructor inventory](evidence/construct-symlink/source-loc.json) and its
[commit confirmation](evidence/construct-symlink/commit-confirmed.json), plus the
[mounted-create inventory](evidence/mounted-create/source-loc.json) and its
[commit confirmation](evidence/mounted-create/commit-confirmed.json), plus the
[native-create inventory](evidence/native-create/source-loc.json) and its
[commit confirmation](evidence/native-create/commit-confirmed.json), plus the
[prepared-file inventory](evidence/prepared-files/source-loc.json) and its
[commit confirmation](evidence/prepared-files/commit-confirmed.json), plus the
[constructor inventory](evidence/construct-metadata/source-loc.json) and its
[commit confirmation](evidence/construct-metadata/commit-confirmed.json), plus the
[mounted-mkdir inventory](evidence/mounted-mkdir/source-loc.json) and its
[commit confirmation](evidence/mounted-mkdir/commit-confirmed.json), plus the
[native-mkdir inventory](evidence/native-mkdir/source-loc.json) and its
[commit confirmation](evidence/native-mkdir/commit-confirmed.json), plus the
[prepared-directory inventory](evidence/prepared-directories/source-loc.json) and its
[commit confirmation](evidence/prepared-directories/commit-confirmed.json), plus the
[Commit inventory](evidence/control-commit/source-loc.json) and its
[commit confirmation](evidence/control-commit/commit-confirmed.json), plus the
[control-Attach inventory](evidence/control-attach/source-loc.json),
[native attachment inventory](evidence/attachment-owner/source-loc.json),
[control-Mount inventory](evidence/control-mount/source-loc.json) and its
[commit confirmation](evidence/control-mount/commit-confirmed.json), plus the
[original continuation inventory](evidence/continuation-handoff/source-loc.json),
retain their original source pins and counts. No historical receipt is relabeled.

## Crate ownership and totals

| Core crate | P | L | Production files | Responsibility |
| --- | ---: | ---: | ---: | --- |
| `layerfs-daemon` | 1364 | 1458 | 8 | Process/configuration, authenticated control and one current lifecycle owner shared with shutdown |
| `layerfs-fuse` | 1207 | 1313 | 4 | Linux kernel projection, replies, mount/session ownership |
| `layerfs-workspace` | 11945 | 12185 | 45 | Shared portable filesystem semantics, private backing, capture and Commit orchestration |
| `layerfs-bridge` | 5570 | 6100 | 26 | Logical operation contract, Source, authorization identity and native framing/delivery |
| `layerfs-service` | 2242 | 2438 | 17 | Authorized operation dispatch and service-local C1/C2/C5 assembly |
| `layerfs-content` | 12549 | 16356 | 70 | C1 canonical content/filesystem algorithms and attributes |
| `layerfs-storage` | 7676 | 10653 | 47 | C2 Store, physical persistence and writer admission |
| `layerfs-history` | 2722 | 3578 | 15 | C5 stages, Branch/Commit/Layer catalog transactions |
| `layerfs-telemetry` | 2257 | 2743 | 23 | Shared bounded operation observation/output |

Core total: **47532 P / 56824 L**, across 255 files. Reference total:
**65417 P / 91466 L**, across 193 files. Combined: **112949 P / 148290 L**. Root `crates/` is reference-only, including its
same-named packages; never import, link or include it into the replacement product.
These are source-size observations, not memory or performance evidence.

## Mounted symbolic links

FUSE projects the shared symlink operation with existing mutation permits and
checked entry coherence. Native target storage and save paths remain unchanged;
see [48](48-mounted-symlink.md).

## Native symbolic links

Workspace publishes kind3 children through existing bounded metadata and payload
ownership, reads local and captured targets, and emits fresh symlink identities
through the prepared v3 trailer. Mounted creation remains refused in this round;
see [47](47-native-symlink.md).

## Prepared fresh symbolic links

Bridge adds a kind3 fresh-ID subset and optional v3 prepared trailer. Service
reuses canonical role validation and C1 reference derivation; Workspace emits an
empty new list until its native symlink operation. Limits remain unchanged;
see [46](46-prepared-symlinks.md).

## Shared symlink-content construction

Bridge adds one bounded target constructor using the existing Saved result and
content-construction grant. Service uses the existing C1 symlink builder and
SaveHandoff. No new response type, inode attachment or history operation is added;
see [45](45-construct-symlink.md).

## Mounted regular-file creation

ProjectionMutationPermit now owns CREATE and its exact Projection handle/reference.
The adapter validates Linux CREATE arguments and retains that permit through reply.
SDK mounted creation reuses checked entry notification and published-handle custody.
No state format or resource bound changes; see [44](44-mounted-create.md).

## Native regular-file creation

Workspace shares child publication between mkdir and create_file; original
resolution moved to filesystem/original.rs. Fresh identity, ready handle rights,
capture and both-root reconciliation use the existing backing and Commit path.
See [43](43-native-create.md), including its unresolved capacity gate failure.

## Prepared fresh regular files

Bridge declares fresh file IDs as a bounded subset of inode rows; prepared request
codecs now live in protocol/prepared.rs. This is code relocation plus the optional
v2 file-ID trailer. Service shares existing role validation and filesystem update
for direct/C5 callers; C1 still derives references and rejects unbound new files.
Workspace currently emits an empty new-file list. See [42](42-prepared-files.md).

## Portable metadata constructor

Bridge adds the no-base constructor and typed result under the existing metadata
grant. Service shares the portable metadata helper for fresh construction and
existing-root patches under the same save/finish/abort owner. No Workspace state
or persisted format changes. See [41](41-construct-portable-metadata.md).

## Mounted directory creation

FUSE delegates mkdir to the single-use Workspace projection permit. SDK mkdir
uses the shared checked completion with a borrowed parent/name target; parent
attributes and the entry are invalidated after publication and outside locks.
Projected mkdir relies on the kernel creation reply and sends no notification
under the kernel parent lock. No persistent layout changed. See [40](40-mounted-mkdir.md).

## Native directory overlay ownership

`filesystem/mkdir.rs` owns one native namespace creation and its checked atomic
publication. `namespace_view.rs` shares local/captured/canonical lookup and ordered
directory merging. Handles retain a base/overlay view and pin the existing immutable
Node locator; this relies on the current mkdir-only namespace mutation scope.

`overlay/directories.rs` encodes directory origins, portable attributes and entry
root/counts. The existing backing page index adds N/E roles and a packed Cell;
entry values contain serials, not physical edges to child inodes. Capture and
`commit/directories.rs` share the existing dirty frontier and lower directory rows
through the common prepared Service operation. Reconciliation reuses D1 entry
roots and substitutes only acknowledged canonical origins. File and directory
publication share exact request capacity accounting. See [39](39-native-mkdir.md).

## Prepared-directory ownership

Bridge's existing prepared request owns new-directory declarations and portable
patches to existing directories. Its optional metadata trailer keeps old empty
bodies unchanged, and all inode forms share the old aggregate bound. Service's
filesystem handler reuses bootstrap metadata construction and the existing portable
patcher under one save. Canonical directory pages remain C1's responsibility.
At the shared-preparation checkpoint Workspace supplied empty new fields; native
mkdir now supplies its captured directory declarations and patches through that
same interface. See [38](38-prepared-directories.md).

## Commit control and codec ownership

`control_commit.rs` translates native Commit reports/failures and writable
submission observations into Bridge DTOs. It preserves known versus observed
outcomes, the native failure disposition and original service failure context;
it does not implement saving or reconciliation. `control.rs` admits the new
identity-only operation under its independent grant. `config.rs` selects the
explicit writable CLI profile, while `lifecycle.rs` chooses the existing native
mount adapter from that immutable profile.

`run.rs` establishes the control owner before initial writable callbacks are
published. Signal shutdown shares the lifecycle exclusion with control and keeps
Commit/Status available if clean closure refuses a dirty or retained owner.
The earlier Commit-control round left Workspace and FUSE product algorithms unchanged.

Bridge `contract/workspace_commit.rs` defines the typed Commit/writable Status
records and validates their identities and known/observed relationships.
`protocol/workspace_commit.rs` encodes those bounded records with the existing C5
Commit outcome and contextual failure grammars. Existing control payload routines
moved from `protocol/response.rs` to `protocol/control.rs`; C5 Commit/stage
validators moved into `contract/history.rs` for shared use. Those movements are
relocation, not deletion or algorithmic simplification. The new control contract,
codec and native-result conversion account for new implementation alongside that
relocation. Service continues to reject daemon control before Store admission.

## Daemon ownership after remote Attach

`lifecycle.rs` owns the daemon's one selected Workspace capability or exact
unresolved selector, optional native MountHandle and immutable configured Attach
profile. Authenticated control and signal shutdown use that same owner. Before an
Attach attempt, it installs the requested selector; it restores the prior closed
capability only after exact native absence is established. Failed resources and
semantic attachment state remain exclusively in WorkspaceHost's existing registry.
The daemon does not copy that registry or infer disposal from a failed observation.

`control.rs` owns independent grant checks, exact selection, complete-input and
deadline admission, and dispatch to the shared owner. `run.rs` assembles the native
process and keeps this owner during failed startup and signal cleanup. The
Bridge Attach outcome and failed-attachment Status union carry bounded typed
observations with exact client correlation; Service refuses daemon controls before
Store admission. Workspace and FUSE source are unchanged in this operation round.
This inventory records source ownership and size, not resource or timing evidence.

## Workspace ownership after the failed-Attach prerequisite

`runtime/attachment.rs` owns exact-incarnation observation and failed-Attach
resource custody: the existing registry entry transitions between Attaching,
Attached and Failed, and an explicit cleanup attempt moves its retained resources
out of that entry while backing I/O runs. It preserves the original failure,
records cleanup progress and validates the acquired mount-leaf identity before
removal. `runtime/host.rs` retains host configuration, admission, remote delivery,
and attachment assembly; `types.rs` exposes the bounded observation/result types.
There is one registry and one live Workspace semantic owner, with no state mirror.

Workspace remains behind the portable Bridge delivery contract. Daemon owns
assembly; FUSE uses public Workspace semantics; Service owns C1/C2/C5 composition.
The new file imports no daemon, FUSE, native client or service implementation.
The runtime, backing, filesystem, overlay and commit groups remain private.

The registry reserves its actual `Entry` allocation size. Each entry's name charge
also covers the bounded optional HistoryFailure and StageWire allocations that an
original failure may retain. This records source responsibility for accounting;
it is not an RSS, cgroup or resource-performance qualification.

| Workspace group | P | L | N | Responsibility |
| --- | ---: | ---: | ---: | --- |
| `runtime/` | 1532 | 1589 | 6 | Lifecycle, admission, failure custody and projection coherence |
| `filesystem/` | 1663 | 1688 | 6 | Semantic lookup, directory traversal, open, read and write |
| `overlay/` | 654 | 656 | 3 | Immutable edit pieces and captured/live roots |
| `backing/` | 4414 | 4476 | 13 | Consumer-wide payload and metadata ownership, budgets and explicit reclaim |
| `commit/` | 1304 | 1319 | 7 | Capture/lowering, service save and Commit reconciliation |
| Root exports/types | 501 | 525 | 3 | Public semantic types and thin crate exports |

## Every current core production file

Directories below show recursive P/L/N totals; each file shows its individual P and L.
The crate roots contain Cargo.toml and optional tests/examples/sql folders in
addition to the counted production paths shown here. No unimplemented future
module is implied by this inventory.

### layerfs-daemon

```text
core/crates/layerfs-daemon/  [P=1364; L=1458; N=8]
`-- src/  [P=1364; L=1458; N=8]
    |-- config.rs  [P=251; L=266]
    |-- control.rs  [P=311; L=332]
    |-- control_commit.rs  [P=174; L=187]
    |-- headless.rs  [P=133; L=134]
    |-- lib.rs  [P=8; L=10]
    |-- lifecycle.rs  [P=243; L=266]
    |-- main.rs  [P=10; L=10]
    `-- run.rs  [P=234; L=253]
```

### layerfs-fuse

```text
core/crates/layerfs-fuse/  [P=1207; L=1313; N=4]
`-- src/  [P=1207; L=1313; N=4]
    |-- adapter.rs  [P=699; L=749]
    |-- lib.rs  [P=7; L=10]
    |-- mount.rs  [P=403; L=448]
    `-- replies.rs  [P=98; L=106]
```

### layerfs-workspace

```text
core/crates/layerfs-workspace/  [P=11945; L=12185; N=45]
`-- src/  [P=11945; L=12185; N=45]
    |-- backing/  [P=4488; L=4551; N=13]
    |   |-- budget.rs  [P=54; L=56]
    |   |-- directory.rs  [P=206; L=208]
    |   |-- metadata.rs  [P=806; L=807]
    |   |-- metadata_build.rs  [P=76; L=77]
    |   |-- metadata_index.rs  [P=505; L=507]
    |   |-- metadata_pages.rs  [P=205; L=207]
    |   |-- metadata_reclaim.rs  [P=505; L=511]
    |   |-- mod.rs  [P=12; L=13]
    |   |-- ownership.rs  [P=822; L=827]
    |   |-- payload.rs  [P=618; L=620]
    |   |-- reader.rs  [P=132; L=134]
    |   |-- reclaim.rs  [P=224; L=229]
    |   `-- segments.rs  [P=323; L=355]
    |-- commit/  [P=1633; L=1650; N=8]
    |   |-- completion.rs  [P=340; L=343]
    |   |-- directories.rs  [P=106; L=107]
    |   |-- lower.rs  [P=316; L=318]
    |   |-- mod.rs  [P=7; L=9]
    |   |-- operation.rs  [P=55; L=56]
    |   |-- reconcile.rs  [P=329; L=335]
    |   |-- save.rs  [P=325; L=326]
    |   `-- source.rs  [P=155; L=156]
    |-- filesystem/  [P=2760; L=2817; N=11]
    |   |-- create.rs  [P=555; L=567]
    |   |-- directory.rs  [P=152; L=153]
    |   |-- mkdir.rs  [P=36; L=39]
    |   |-- mod.rs  [P=10; L=10]
    |   |-- namespace.rs  [P=211; L=214]
    |   |-- namespace_view.rs  [P=316; L=318]
    |   |-- open.rs  [P=230; L=236]
    |   |-- original.rs  [P=124; L=128]
    |   |-- read.rs  [P=254; L=258]
    |   |-- symlink.rs  [P=153; L=164]
    |   `-- write.rs  [P=719; L=730]
    |-- overlay/  [P=898; L=902; N=4]
    |   |-- directories.rs  [P=181; L=183]
    |   |-- mod.rs  [P=3; L=3]
    |   |-- pieces.rs  [P=338; L=339]
    |   `-- snapshot.rs  [P=376; L=377]
    |-- runtime/  [P=1656; L=1726; N=6]
    |   |-- attachment.rs  [P=256; L=266]
    |   |-- coherence.rs  [P=370; L=407]
    |   |-- host.rs  [P=499; L=506]
    |   |-- lifecycle.rs  [P=161; L=168]
    |   |-- mod.rs  [P=5; L=5]
    |   `-- state.rs  [P=365; L=374]
    |-- commit_types.rs  [P=73; L=75]
    |-- lib.rs  [P=17; L=20]
    `-- types.rs  [P=420; L=444]
```

### layerfs-bridge

```text
core/crates/layerfs-bridge/  [P=5570; L=6100; N=26]
`-- src/  [P=5570; L=6100; N=26]
    |-- adapters/  [P=3761; L=3935; N=16]
    |   |-- native/  [P=3760; L=3934; N=15]
    |   |   |-- protocol/  [P=2479; L=2555; N=9]
    |   |   |   |-- control.rs  [P=166; L=168]
    |   |   |   |-- frame.rs  [P=89; L=96]
    |   |   |   |-- history_failure.rs  [P=163; L=169]
    |   |   |   |-- metadata.rs  [P=767; L=793]
    |   |   |   |-- mod.rs  [P=13; L=13]
    |   |   |   |-- prepared.rs  [P=215; L=229]
    |   |   |   |-- response.rs  [P=795; L=813]
    |   |   |   |-- state.rs  [P=55; L=56]
    |   |   |   `-- workspace_commit.rs  [P=216; L=218]
    |   |   |-- client.rs  [P=506; L=522]
    |   |   |-- connection.rs  [P=416; L=471]
    |   |   |-- mod.rs  [P=7; L=7]
    |   |   |-- payload.rs  [P=146; L=148]
    |   |   |-- pipe.rs  [P=112; L=132]
    |   |   `-- server.rs  [P=94; L=99]
    |   `-- mod.rs  [P=1; L=1]
    |-- contract/  [P=1805; L=2160; N=9]
    |   |-- caller.rs  [P=13; L=19]
    |   |-- control.rs  [P=135; L=151]
    |   |-- history.rs  [P=342; L=580]
    |   |-- metadata.rs  [P=23; L=26]
    |   |-- mod.rs  [P=16; L=16]
    |   |-- outcome.rs  [P=185; L=208]
    |   |-- request.rs  [P=855; L=913]
    |   |-- source.rs  [P=29; L=34]
    |   `-- workspace_commit.rs  [P=207; L=213]
    `-- lib.rs  [P=4; L=5]
```

### layerfs-service

```text
core/crates/layerfs-service/  [P=2242; L=2438; N=17]
`-- src/  [P=2242; L=2438; N=17]
    |-- input/  [P=33; L=35; N=2]
    |   |-- mod.rs  [P=2; L=2]
    |   `-- sequential.rs  [P=31; L=33]
    |-- native/  [P=302; L=328; N=3]
    |   |-- config.rs  [P=124; L=142]
    |   |-- mod.rs  [P=3; L=3]
    |   `-- startup.rs  [P=175; L=183]
    |-- operation/  [P=1742; L=1873; N=9]
    |   |-- dispatch.rs  [P=60; L=69]
    |   |-- failure.rs  [P=45; L=46]
    |   |-- filesystem.rs  [P=202; L=225]
    |   |-- history.rs  [P=620; L=662]
    |   |-- history_bootstrap.rs  [P=278; L=322]
    |   |-- metadata.rs  [P=144; L=153]
    |   |-- mod.rs  [P=9; L=9]
    |   |-- read.rs  [P=136; L=137]
    |   `-- write.rs  [P=248; L=250]
    |-- lib.rs  [P=6; L=7]
    |-- main.rs  [P=9; L=9]
    `-- owner.rs  [P=150; L=186]
```

### layerfs-content

```text
core/crates/layerfs-content/  [P=12549; L=16356; N=70]
`-- src/  [P=12549; L=16356; N=70]
    |-- file/  [P=4056; L=5152; N=20]
    |   |-- cdc/  [P=499; L=548; N=2]
    |   |   |-- gear.rs  [P=494; L=538]
    |   |   `-- mod.rs  [P=5; L=10]
    |   |-- edit/  [P=1645; L=2130; N=8]
    |   |   |-- apply.rs  [P=420; L=506]
    |   |   |-- compare.rs  [P=94; L=121]
    |   |   |-- concat.rs  [P=19; L=29]
    |   |   |-- finish.rs  [P=38; L=54]
    |   |   |-- input.rs  [P=286; L=390]
    |   |   |-- mod.rs  [P=15; L=20]
    |   |   |-- split.rs  [P=23; L=32]
    |   |   `-- tree.rs  [P=750; L=978]
    |   |-- mapping/  [P=1437; L=1853; N=6]
    |   |   |-- build.rs  [P=328; L=422]
    |   |   |-- codec.rs  [P=304; L=345]
    |   |   |-- mod.rs  [P=22; L=27]
    |   |   |-- predecessor.rs  [P=171; L=220]
    |   |   |-- read.rs  [P=422; L=573]
    |   |   `-- types.rs  [P=190; L=266]
    |   |-- content.rs  [P=259; L=353]
    |   |-- mod.rs  [P=18; L=24]
    |   |-- read.rs  [P=111; L=127]
    |   `-- view.rs  [P=87; L=117]
    |-- filesystem/  [P=7354; L=9502; N=40]
    |   |-- attributes/  [P=1288; L=1541; N=8]
    |   |   |-- build.rs  [P=386; L=435]
    |   |   |-- codec.rs  [P=322; L=375]
    |   |   |-- keys.rs  [P=77; L=107]
    |   |   |-- mod.rs  [P=16; L=19]
    |   |   |-- patch.rs  [P=173; L=218]
    |   |   |-- portable.rs  [P=70; L=95]
    |   |   |-- read.rs  [P=171; L=201]
    |   |   `-- value.rs  [P=73; L=91]
    |   |-- directory/  [P=473; L=566; N=4]
    |   |   |-- codec.rs  [P=156; L=190]
    |   |   |-- mod.rs  [P=6; L=9]
    |   |   |-- read.rs  [P=272; L=315]
    |   |   `-- update.rs  [P=39; L=52]
    |   |-- inode/  [P=409; L=495; N=4]
    |   |   |-- codec.rs  [P=145; L=178]
    |   |   |-- mod.rs  [P=6; L=9]
    |   |   |-- read.rs  [P=186; L=211]
    |   |   `-- update.rs  [P=72; L=97]
    |   |-- references/  [P=1587; L=2198; N=7]
    |   |   |-- backing.rs  [P=246; L=360]
    |   |   |-- merge.rs  [P=173; L=260]
    |   |   |-- mod.rs  [P=15; L=18]
    |   |   |-- record.rs  [P=120; L=166]
    |   |   |-- reduce.rs  [P=472; L=594]
    |   |   |-- release.rs  [P=167; L=205]
    |   |   `-- runs.rs  [P=394; L=595]
    |   |-- sorted/  [P=1599; L=1973; N=6]
    |   |   |-- budget.rs  [P=83; L=111]
    |   |   |-- finish.rs  [P=156; L=180]
    |   |   |-- format.rs  [P=592; L=744]
    |   |   |-- merge.rs  [P=278; L=312]
    |   |   |-- mod.rs  [P=9; L=12]
    |   |   `-- page.rs  [P=481; L=614]
    |   |-- identity.rs  [P=32; L=63]
    |   |-- input.rs  [P=158; L=233]
    |   |-- limits.rs  [P=24; L=113]
    |   |-- mod.rs  [P=28; L=33]
    |   |-- objects.rs  [P=101; L=166]
    |   |-- path.rs  [P=147; L=197]
    |   |-- read.rs  [P=212; L=296]
    |   |-- root.rs  [P=119; L=160]
    |   |-- symlink.rs  [P=70; L=94]
    |   |-- update.rs  [P=454; L=550]
    |   `-- validate.rs  [P=653; L=824]
    |-- object/  [P=809; L=1161; N=7]
    |   |-- access.rs  [P=44; L=86]
    |   |-- codec.rs  [P=107; L=139]
    |   |-- id.rs  [P=70; L=96]
    |   |-- inode_leaf.rs  [P=345; L=467]
    |   |-- mod.rs  [P=23; L=29]
    |   |-- output.rs  [P=156; L=242]
    |   `-- predecessor.rs  [P=64; L=102]
    |-- error.rs  [P=161; L=255]
    |-- lib.rs  [P=32; L=49]
    `-- policy.rs  [P=137; L=237]
```

### layerfs-storage

```text
core/crates/layerfs-storage/  [P=7676; L=10653; N=47]
|-- sql/  [P=63; L=81; N=1]
|   `-- schema.sql  [P=63; L=81]
`-- src/  [P=7613; L=10572; N=46]
    |-- cas/  [P=2262; L=3341; N=15]
    |   |-- batch.rs  [P=58; L=87]
    |   |-- collision.rs  [P=72; L=77]
    |   |-- dependencies.rs  [P=81; L=119]
    |   |-- finish.rs  [P=16; L=25]
    |   |-- lifecycle.rs  [P=239; L=300]
    |   |-- membership.rs  [P=46; L=76]
    |   |-- mod.rs  [P=19; L=24]
    |   |-- owner.rs  [P=194; L=410]
    |   |-- placement.rs  [P=175; L=246]
    |   |-- pool_lane.rs  [P=339; L=441]
    |   |-- provider.rs  [P=120; L=188]
    |   |-- read.rs  [P=172; L=273]
    |   |-- save.rs  [P=74; L=113]
    |   |-- selection.rs  [P=131; L=164]
    |   `-- store.rs  [P=526; L=798]
    |-- encoding/  [P=3218; L=4265; N=16]
    |   |-- delta/  [P=1213; L=1726; N=5]
    |   |   |-- candidates.rs  [P=271; L=417]
    |   |   |-- mod.rs  [P=4; L=8]
    |   |   |-- read.rs  [P=379; L=545]
    |   |   |-- record.rs  [P=215; L=270]
    |   |   `-- select.rs  [P=344; L=486]
    |   |-- pool/  [P=1085; L=1355; N=7]
    |   |   |-- counters.rs  [P=71; L=94]
    |   |   |-- delta.rs  [P=261; L=287]
    |   |   |-- index.rs  [P=158; L=211]
    |   |   |-- leaf.rs  [P=103; L=145]
    |   |   |-- mod.rs  [P=11; L=16]
    |   |   |-- read.rs  [P=404; L=499]
    |   |   `-- value_group.rs  [P=77; L=103]
    |   |-- codec.rs  [P=516; L=676]
    |   |-- decode.rs  [P=207; L=258]
    |   |-- full.rs  [P=185; L=225]
    |   `-- mod.rs  [P=12; L=25]
    |-- pack/  [P=798; L=1020; N=4]
    |   |-- assemble.rs  [P=261; L=319]
    |   |-- layout.rs  [P=398; L=494]
    |   |-- mod.rs  [P=14; L=19]
    |   `-- placement.rs  [P=125; L=188]
    |-- sqlite/  [P=1018; L=1346; N=8]
    |   |-- cleanup.rs  [P=69; L=79]
    |   |-- connection.rs  [P=97; L=145]
    |   |-- lookup.rs  [P=139; L=178]
    |   |-- mod.rs  [P=11; L=16]
    |   |-- ownership.rs  [P=164; L=196]
    |   |-- pool.rs  [P=130; L=187]
    |   |-- schema.rs  [P=284; L=355]
    |   `-- write.rs  [P=124; L=190]
    |-- error.rs  [P=109; L=159]
    |-- lib.rs  [P=12; L=39]
    `-- policy.rs  [P=196; L=402]
```

### layerfs-history

```text
core/crates/layerfs-history/  [P=2722; L=3578; N=15]
|-- sql/  [P=155; L=167; N=1]
|   `-- schema-v1.sql  [P=155; L=167]
`-- src/  [P=2567; L=3411; N=14]
    |-- sqlite/  [P=1794; L=2139; N=9]
    |   |-- allocation.rs  [P=65; L=85]
    |   |-- branch.rs  [P=168; L=189]
    |   |-- commit.rs  [P=263; L=293]
    |   |-- layerstack.rs  [P=348; L=390]
    |   |-- mod.rs  [P=9; L=18]
    |   |-- open.rs  [P=357; L=421]
    |   |-- query.rs  [P=135; L=195]
    |   |-- rows.rs  [P=232; L=301]
    |   `-- staging.rs  [P=217; L=247]
    |-- catalog.rs  [P=71; L=142]
    |-- error.rs  [P=115; L=185]
    |-- identity.rs  [P=237; L=343]
    |-- lib.rs  [P=21; L=44]
    `-- records.rs  [P=329; L=558]
```

### layerfs-telemetry

```text
core/crates/layerfs-telemetry/  [P=2257; L=2743; N=23]
`-- src/  [P=2257; L=2743; N=23]
    |-- output/  [P=653; L=703; N=5]
    |   |-- collector.rs  [P=72; L=81]
    |   |-- encode.rs  [P=105; L=113]
    |   |-- mod.rs  [P=7; L=8]
    |   |-- queue.rs  [P=309; L=340]
    |   `-- retention.rs  [P=160; L=161]
    |-- platform/  [P=88; L=92; N=3]
    |   |-- linux.rs  [P=46; L=47]
    |   |-- macos.rs  [P=32; L=35]
    |   `-- mod.rs  [P=10; L=10]
    |-- runtime/  [P=355; L=384; N=3]
    |   |-- mod.rs  [P=4; L=5]
    |   |-- monitor.rs  [P=233; L=247]
    |   `-- session.rs  [P=118; L=132]
    |-- timer/  [P=873; L=1198; N=8]
    |   |-- bounded_json.rs  [P=53; L=58]
    |   |-- format.rs  [P=71; L=84]
    |   |-- json.rs  [P=115; L=136]
    |   |-- limits.rs  [P=43; L=59]
    |   |-- mod.rs  [P=11; L=29]
    |   |-- recording.rs  [P=226; L=288]
    |   |-- report.rs  [P=209; L=326]
    |   `-- scope.rs  [P=145; L=218]
    |-- health.rs  [P=30; L=31]
    |-- lib.rs  [P=13; L=27]
    |-- observation.rs  [P=88; L=124]
    `-- operation.rs  [P=157; L=184]
```

## File ceilings and expansion rules

Every production file must stay at most 999 physical lines. Every lib.rs/mod.rs
must stay at most 200 physical lines and contain declarations/reexports/thin
forwarding only. The folder map is ownership guidance, not permission to create
one interface/factory per file. Add a real responsibility when needed; do not
scaffold absent namespace or platform groups. Keep current private implementation
groups behind narrow production exports. Update this inventory when its source
pin changes; do not relabel the counts above as a newer commit without recounting.
