# #245 Phase 1 ordinary-shell repair: root cause, fix, and functional proof

> **Status:** the #243 mixed package refresh and the two rename shapes now pass
> functionally with an independent full-tree oracle on a repaired candidate
> source; every latency row is **INELIGIBLE**; the piece index, the streaming
> Commit, the continuously writable Commit proof and the frozen performance
> selection are **NOT_RUN**. No release admission is claimed.

Source identity: `dd00c92e2` (FUSE diagnostic traces) on `a0258cd6f`
(correctness fixes), whose first parent is the #245 planning identity
`d63379b95`. The post-fix attempts share one diagnostic build — release
`aarch64-unknown-linux-musl` daemon
`a660d7b0426a8b53ff9cab471204881b0d7c97d5d4892ffffb2989881dd103b9` and image
`sha256:26e081383867ec0e59276f38470bbb2002e09167843d7eacaaca13067ed429d1`. The
sealed #243 state, binary, fixture and clone identities are in each
`receipt.json` and in the [diagnostic contract](DIAGNOSTIC_CONTRACT.md).

## 1. What was failing

The retained [ #243 Phase 2 report](../../../243/evidence/phase2-ordinary-shell-v1/REPORT.md)
recorded the mixed three-package refresh failing before Commit: the shell's
final `mv -f package-lock.json.next package-lock.json` returned `EIO`, the
driver issued no Commit, and the receipt could not name the Workspace error
because several variants map to `EIO`.

Reproducing the command on a fresh copy of the sealed `package` master through
the same public route confirmed the failure and localized it:

| Diagnostic attempt | Command | Observed |
| --- | --- | --- |
| `run-11-root-stage-diag` | `mv -f package-lock.json new-name.json` on a canonical root-level file | `Exec status Some(1)`, `LFS_FUSE_ERROR … error=Io`, stage marker after the destination handling |
| `run-13-nested-long-name` | `cp package-lock.json parser/lib/package-lock.json.next; mv -f … package-lock.json` | same `Io` at a **nested** path |
| `run-32-mixed-fix4` | the full mixed refresh | `Exec` zero, then Commit `Failure { code: Io }` in phase `Preparing` |
| `run-10/run-05/run-08` | isolated root renames of a canonical file | `Io`; the failure tracked the **name length**, not the directory depth |

Three independent defects on the shared namespace path produced those `Io`
results. The first two are in the private metadata page/record layer, the third
is the binding rule the architecture document already states.

## 2. The three defects and their smallest shared-path fixes

**D1 — a fixed 17-byte key ceiling in the name-page builder.**
`backing/metadata_build.rs::build_ordered` validated every cell it wrote with
`cell.key_len > 17`. A metadata key is one kind byte plus its identity: `D` keys
are 17 bytes and `I`/`N`/`R` keys are 9, but a name kind (`E`, `T`) carries up
to 255 name bytes. Rebuilding a directory's entry or removal page therefore
refused **any name longer than 16 bytes** — `package-lock.json` produces an
18-byte removal key. Fix: the bound now belongs to the key kind
(`metadata_index::key_limit`, and `stored_key_limit` for a key written into a
page, which also replaces the duplicated check in `check_index`).

**D2 — the removal-page walk's cursor.**
`overlay/directories.rs::keep_name` derived exclusivity from the names it *kept*
(`!names.is_empty()`). When the first removal record on the page is the name
being cleared, nothing is kept, the next cursor step re-reads the same cell and
the call is refused with `Capacity`. Fix: the cursor advances by the cell
visited, exactly as `drop_entry` already does.

**D3 — binding a name left its removal record in place.**
`filesystem/create.rs` inserted the entry without clearing a removal record the
same generation had written. The kernel resolves a POSIX overwrite that
`cp` issues as `UNLINK` followed by `CREATE`; `remove.rs` writes a removal page,
the re-anchor cannot clear it (the published root has no `parent`), and the
create then bound the name. One delta held both a binding and a removal for one
name, which hides the name from listings and resolutions and makes lowering
refuse the two rows (`Failure { code: Io }`). Fix: the bind clears the record
through the same `keep_name` the rename path already uses, and the generation's
name-row counters take the replacement rather than an addition. This is the
documented rule — "one name owns a binding or a removal, never both" — applied
to every binding publication instead of only the rename destination.

The affected architecture document
([`02-overlay-snapshot.md`](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/02-overlay-snapshot.md))
states the extended rules.

## 3. Post-fix results

One attempt per command on the same candidate source and image, each on a fresh
independent byte copy of its closed master, each verified by the sealed
read-only verifier over the published Store and History.

| Attempt | Case | Exec | Commit | FUSE callbacks | Independent verifier |
| --- | --- | --- | --- | --- | --- |
| [run-49](attempts/run-49-mixed-refresh/receipt.json) | mixed package refresh | exit 0 | one Commit | `create=7 mkdir=1 unlink=5 rename=2 write=23` (1,114,488 B) | **PASS** — 18 paths, 9 files, 1,114,516 B new tree; old Commit intact |
| [run-50](attempts/run-50-overwrite-4k/receipt.json) | 4 KiB positional overwrite | exit 0 | one Commit | `write=1` (exactly 4,096 B) | **PASS** — 18 paths, 10 files, 11,600,261 B both trees |
| [run-51](attempts/run-51-repeated-one-byte/receipt.json) | sixteen one-byte writes | exit 0 | one Commit | `write=16` (16 B), `open=16` | **PASS** — 18 paths, 10 files, 1,180,037 B both trees |
| [run-52](attempts/run-52-failed-command/receipt.json) | deliberate failure, no Commit | nonzero | **zero Commit calls** | `setattr=1 write=1` (3 B) | **PASS** — head still the old Commit, complete old tree |
| [run-53](attempts/run-53-root-lockfile-replace/receipt.json) | root temp file over a canonical file (the #243 failing shape) | exit 0 | one Commit | `create=1 write=1` (203 B), `rename=1` | **PASS** — 17 paths, 9 files, 1,114,501 B both trees |
| [run-54](attempts/run-54-nested-long-name/receipt.json) | long-name rename at a nested path | exit 0 | one Commit | `create=1 write=1` (203 B), `rename=1` | **PASS** — 18 paths, 10 files, 1,114,704 B |

Every row reports zero `range_state`, zero `range_edit` and zero accepted range
payload bytes: the route is ordinary POSIX/FUSE, and the historical ioctl
campaign is untouched. `write` in the Status projection is an aggregate that
includes namespace mutations; the counts above are the traced FUSE callbacks,
which is why they can be stated separately.

### Latency

The host Store copy and the container FUSE backing cache are uncontrolled in
every attempt, so all six rows are `performance_status: INELIGIBLE`, exactly as
[the contract](DIAGNOSTIC_CONTRACT.md) requires. The raw values are retained in
each `receipt.json` (for example `run-49`: Exec 1,164 ms, Commit 170 ms,
complete command 2,130 ms from the drive wall, cleanup included) and are **not** compared with the retained #243
numbers, which used a different source, image and command set.

### Cleanup attribution observation

The retained #243 receipts recorded about 5.5 s of cleanup/log capture on the
mixed and failed-command rows and could not attribute it. In this set the same
term appears exactly on the attempts whose daemon log carries
`sandbox shutdown retained: Busy`:

| Attempts | `sandbox shutdown retained: Busy` | Cleanup |
| --- | --- | --- |
| `run-52-failed-command` (nonzero Exec, uncommitted private state) | present | 5,523 ms |
| `run-13`, `run-32` (pre-fix failures, one of them uncommitted) | present | 5.5 s complete-command tail each |
| `run-49`, `run-50`, `run-51`, `run-53`, `run-54` (committed rows) | absent | 488–579 ms |

This is a count-and-timing observation across the retained attempts, not a
completed diagnosis: it says the retained shutdown is paid by rows that keep an
uncommitted Workspace, and it is consistent with the retained #243 rows, but the
cause of the `Busy` result and the eleven #232 `Exec Unknown` rows still need
their own labelled diagnostic. No deadline was changed, and the failed-command
row still makes no Commit.

## 4. Gates that remain open

| #245 Phase 1 gate | Status | Why |
| --- | --- | --- |
| Item 1 — root-cause and repair the rename failure | **PASS** (functional, with oracle) | D1–D3 above; `run-53`/`run-54` pass where the retained row failed |
| Item 2 — keep the four correctness oracles | **PASS** (functional) | `run-50`, `run-51`, `run-49`, `run-52`; the failed-command row still makes no Commit and leaves the old head |
| Item 2 — diagnose the five-second silent Exec `Unknown` and the ~5.5 s cleanup term | **NOT_RUN** | not instrumented in this round; the eleven #232 v2 failures remain observed `Exec Unknown`, and the 8 MiB over-limit stays a source/registry deduction |
| Item 3 — freeze a new ordinary-shell selection and collect a qualifying control | **NOT_RUN** | no change was made to the #243 registry; these are labelled diagnostics on a changed identity |
| Item 4 — scalable range-based COW file index and coordinated streaming Commit | **NOT_RUN** | `arena.pieces` still collects the whole piece list and `build_pieces` still rewrites every piece page per callback; Bridge/server/C1 replay limits are unchanged |
| Item 5 — G1 → G2 → G3 continuous Commit proof | **NOT_RUN** | `reconcile_commit` can still return `Busy` after a successful Store publication |
| Item 6 — independent verifier and frozen receipt schema | **PARTIAL** | the sealed verifier ran for every post-fix attempt and each new receipt records route, identities, provenance, failure class and cache state; piece/Commit counters and container resources remain `UNAVAILABLE` |

The later [load-bearing cases](../../LOAD_BEARING_CASES.md) and #232's 56 shapes
stay open; nothing here promotes them.

## 5. Checks run at this source identity

At `dd00c92e2`, all five owning Core checks passed once:

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets` — exit 0 on the host (Darwin ARM64); the Linux-gated mounted suites compile out of this run and were not executed.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --all-targets --locked -- -D warnings` — exit 0.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — exit 0.
- `python3 core/tools/check_product_boundary.py` — exit 0 after the trace helper was moved into its own module, which a mid-round failure showed was required: the inline traces had pushed `layerfs-fuse/src/adapter.rs` to 1,016 physical lines against the 999-line production-file ceiling.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — 9 tests, OK.
- The Linux daemon used by every attempt was built with
  `cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-daemon`.

No CI ran and the retired root preflight was not run.

## 6. Residual risk

- The three fixes are exercised end-to-end through the mounted route here, but
  the Linux-gated `layerfs-workspace` test suites were not executed in this
  round; the diagnostics are the behavioural evidence.
- The `create` path now clears a removal record and adjusts the generation's
  name-row counters; a directory whose removal page already holds 128 names is
  still refused with `Capacity`, and that bound is unchanged.
- Names longer than 255 bytes remain refused by `entry_key`/`tombstone_key`, and
  a page that must hold many long names can still split into pages whose body is
  below `MIN_BODY`; that bound is unchanged and untested here.
- Every latency number in this round is ineligible, so no speed claim — positive
  or negative — follows from the repair.
- The trace does not cover `mknod`, `symlink` or `link`; those callbacks are
  `UNAVAILABLE`, not zero.
