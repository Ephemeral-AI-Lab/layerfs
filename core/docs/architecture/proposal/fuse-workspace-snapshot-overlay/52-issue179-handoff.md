# Issue 179 namespace/attribute implementation: handoff

Written after the implementation round that added the six operations spec
`51-implementation-completion-spec.md` lists as missing. It records exactly what
runs, exactly what fails, and the reasoning that is expensive to rediscover. The
governing plan is still `51-implementation-completion-spec.md`; this file does
not replace it and does not close the issue.

## 1. State in one paragraph

The production code for all six missing operations exists and the workspace
crate builds with no warnings, but only two of the ten registered namespace
cases pass. `mkdir_semantics` passes both its own check and the native
clean-close check, including **two explicit Commits across two generations**,
and `mknod` passes. Everything else fails for reasons that are listed in section
4 with the observed error. Nothing here is a performance, durability or
crash-recovery claim, and no numeric limit was raised.

## 2. What is verified

Run these two selections; both are bounded well under three minutes.

```sh
HOST=$PWD/core/target/pair1-evidence/binary-archive/d582e04078cbf4deb883b09d71459292dd90b0d58b96328c5f9a0ae69550fd54/host
python3 core/crates/layerfs-workspace/tests/mkdir_route.py --case semantics \
  --fixture core/target/pair1-evidence/fixtures/large-edit-master-01/result.json \
  --binaries "$HOST" --test-binary core/target-linux/debug/deps/mkdir-5c889e2cd6a39bdf \
  --image sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
  --output core/target/pair1-evidence/NEW-UNUSED/semantics-01
```

`mkdir_semantics` **PASS** (`nested-mode-umask-forget-listing-and-two-explicit-generations`,
`native-clean-close`). `namespace_route.py --case mknod` **PASS**
(`mknod-empty-regular-file-without-a-handle`, `native-clean-close`).

`mkdir_successor` fails at its second Commit; do not read it as passing.

## 3. Implementation that now exists

| Required operation | Product code | Verified |
| --- | --- | --- |
| Atomic portable `setattr` (mode/mtime, valid size combination) | `filesystem/resize.rs` (new), `PortableAttributes` in `types.rs`, FUSE `setattr` | no |
| Regular-file `mknod` without a handle | `filesystem/mknod.rs` (new), `Creation::File { open: None }` | yes |
| Regular-file hard link | `filesystem/link.rs` (new), `Creation::Link` | no |
| `unlink` with open-orphan lifetime | `filesystem/remove.rs` (new) | no |
| Empty `rmdir` | `filesystem/remove.rs` + `directory_empty` | no |
| Ordinary `rename` and `RENAME_NOREPLACE` | `filesystem/rename.rs` (new) | no |

Supporting changes, all of which the operations depend on:

- **Tombstones.** `overlay/directories.rs` gives `Directory` a `tombstones`
  page (value bytes 88..96, `T` + name → `[1]`). Absent from both entry and
  tombstone pages means *inherit from the origin*; a tombstone means *absent
  from the effective namespace even when the origin binds it*. `resolve_child`
  and `list_view` apply that rule. `Origin::Empty` carrying tombstones is an
  invalid state and `parse` refuses it.
- **Lowering.** `commit/directories.rs` was rewritten so a lowered name is
  `Some(serial)` for a binding and `None` for a removal, and a fresh directory
  is emitted as **both** a `new_directories` record and its own
  `DirectoryChange{parent: serial, changes: []}`. That second record is not
  optional: `check_prepared_additions` requires every declared serial to appear
  in the `directories` list with `parent == serial`.
- **One-shot declaration.** `runtime/state.rs` keeps a `declared` ledger of
  directory serials whose creation the canonical state has not accepted yet;
  `create.rs` registers a new directory serial, `commit/reconcile.rs` clears the
  ledger once the canonical successor is installed, and `commit/directories.rs`
  emits `new_directories` only while the ledger holds the serial. See section 5
  for why this is required.
- **Unbound identities.** `Dirty::Unbound` lets a fresh inode this generation
  created and no name binds any more be dropped from lowering instead of being
  declared.
- **Root patch exclusion.** A maintained directory that binds no name is never
  emitted as a **root** `directory_metadata` patch: the prepared profile refuses
  the root serial there. The root's selected mode and mtime reach the result
  through its own inode row.

## 4. Exact open failures

| Case | Observed |
| --- | --- |
| `setattr` | `CommitFailure{phase: Preparing, cause: Stage(StageFailure{phase: LocalBookkeeping, cause: Io})}` |
| `link`, `unlink` | `Service(Failure{code: Unknown, unknown: true})` |
| `unlink_fresh` | `LocalBookkeeping`/`Io`, and a `next_dirty` walk that stops two serials short of `captured.count` |
| `rmdir`, `rename` | `WorkspaceError::Io` |
| `rename_base` | `WorkspaceError::Exists` |
| `generation` | `index out of bounds: the len is 1 but the index is 1` |
| `notification_failure` | asserts `Err(Io)` where the test expects `Ok(())` for a mounted rename |
| `mkdir_successor` | second Commit refused by the Service with `InvalidInput` |

## 5. What is expensive to rediscover

**A generation may publish more than one root, so `CapturedBase.root` names a
root, not "the" root.** A maintained directory delta records the exact earlier
version it is a delta against as `Origin::Captured(CapturedBase{root, inode,
generation, revision})`. The correct value is the parent of the root the
operation's candidate is built on — that is the version the operation read its
entries from. Using the frozen submission's captured root, or the current
overlay root, is wrong as soon as another operation has published a root in the
same generation, and the failure surfaces much later as an `Io` in
`prepared_directories` or inside `directories::captured`. The accessor for the
correct value is `backing::metadata::MetadataHost::anchor(owner)`; all four
write sites (`create.rs`, `remove.rs`, `rename.rs`, `resize.rs`) use it.
`directories::captured` still requires `owner.parent.root() == reference.root`,
so the anchor must be that parent.

**`new_directories` is consumed once, ever.** The Service looks up every serial
in `directories` and `inodes` and requires "found" to agree exactly with
"declared new". A serial that a first Commit already declared is present in the
base, so re-declaring it is refused with `InvalidInput`. This is why the
declaration ledger exists and why it is cleared on commit rather than at
capture: a second Commit in the *same* generation must still declare a
directory the first Commit did not reach.

**Do not trust `core/target-linux`.** Cargo reports targets fresh and rebuilds
nothing there while the source has changed, so a stale test binary silently
reproduces the previous behaviour and every conclusion drawn from it is wrong.
Build the test binary in a container-local copy of the tree and stage it out:

```sh
docker run --rm -v "$PWD":/work -v "$HOME/.cargo/registry":/usr/local/cargo/registry \
  -e CARGO_BUILD_JOBS=2 -e CARGO_TARGET_DIR=/tmp/tb \
  -e RUSTFLAGS="--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2" \
  -w /work sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 \
  sh -c "rm -rf /tmp/s && cp -a /work/core /tmp/s && \
         find /tmp/s/crates -name '*.rs' -exec touch {} + && cd /tmp/s && \
         cargo test --manifest-path Cargo.toml -p layerfs-workspace --test namespace --no-run --locked --offline"
```

`find ... -exec touch` is not cosmetic: `cp -a` preserves the container's older
view of the source mtimes, and without it cargo will again decide nothing
changed. The image is `rust:1.85.1-bookworm`; the registry must be mounted at
`/usr/local/cargo/registry` for `--offline` to resolve `nix`. Verify the staged
binary actually contains the change before interpreting any run.

**The service-visible kind is not the local kind.** For a captured root, the
presence of an `I` record does not distinguish a file from a directory; the `N`
(namespace) record must be consulted first, or a directory is lowered as a file.

## 6. Accepted bounds and deferred work

Unchanged and not to be raised to obtain a PASS: 128 admission rules, 32 KiB
metadata, 256 Nodes, 128 handles, 4096 payload records, 8 MiB/256-edit replay,
1024 pieces, 8 MiB default memory budget, 4096-entry topology work bound, one
construction worker. Deferred as before: DSH, large workloads, R6, xattr/ACL/
atime/ctime, special files, rename exchange/whiteout, readdirplus, fallocate,
copy-file-range, crash recovery, durability.

Not started: mounted FUSE syscall coverage for the six operations, the
real-daemon small-project scenario in `core/crates/layerfs-daemon/tests/`,
whole-core final checks on frozen source, the architecture/operation matrix, the
final report's split of accepted bounds versus open failures.

## 7. Production LOC comparison

Method: the unchanged `tools/production_loc.py` counter, no flags, on exact
snapshots (parent `bb8bb6baf24b930f69f7434459558e052e3145bb` extracted with
`git archive HEAD core`, after = the committed tree).

| Scope | Before | After | Delta |
| --- | --- | --- | --- |
| Replacement core | 47,558 | 49,494 | +1,936 |
| Reference `crates/` | 65,417 | 65,417 | 0 |
| Combined | 112,975 | 114,911 | +1,936 |

The growth is new product implementation (`resize.rs`, `remove.rs`,
`rename.rs`, `link.rs`, `mknod.rs` and the tombstone/declaration/anchoring
changes), not relocation or a scope change.
