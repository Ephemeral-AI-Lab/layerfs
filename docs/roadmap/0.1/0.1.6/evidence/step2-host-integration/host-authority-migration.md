# Host authority migration: ordinary Workspace route

Steps 2 and 3 of the issue-124 execution plan, executed as one migration. The
ordinary host-FUSE Workspace route no longer starts a legacy authority: one
`HostRuntime` (HostOverlay + HostOperations + HostSdk + CommitCoordinator +
local/TCP `BackingServer`) is constructed before projection attachment, the
mount is taken through `BackingServer::host_owner()`, and the SDK, status,
dirty and End/Discard routes reach that same owner. There is no legacy owner,
no freeze, and no fallback.

Read with:
[consumer audit](../phase3-host-operations/host-runtime-consumer-audit.md) (the caller
inventory this migration implements), [contract resolution](../../overlay-snapshot-contract-resolution.md) §2–§4,
and the [implementation plan](../../overlay-snapshot-implementation-plan.md). V1's
mapping-visibility half is out of scope here and remains open.

## 1. Changed files

| File | Change |
| --- | --- |
| `crates/layerfs-workspace/src/projection.rs` | Host-FUSE `attach` builds and mounts the host authority; `start_host_authority`; host-aware `is_dirty`, `end`, `record_read_metrics`, `record_write_metrics` |
| `crates/layerfs-workspace/src/lifecycle.rs` | `commit_host_session`; host branches in `edit_workspace_file_ranges`, `session`, `summary`, `diff`, `Workspaces::session`, `end_workspace_session`; `host_session`; focused tests |
| `crates/layerfs-workspace/src/worker.rs` | `host` slot + `host_runtime`/`install_host_runtime`/`take_host_runtime` |
| `crates/layerfs-workspace/src/host_runtime.rs` | `build_candidate` (the production construction, separated from `commit`), `covered_sequence`, `published_head`, `prepare_shutdown`, `maintain`; test-only build latch |
| `crates/layerfs-workspace/src/live_backing.rs` | `generation` is host-aware (host sequence, not the container's reset counter) |

`crates/layerfs-fuse/src/live_transport.rs` was **not** modified: `host_owner()`,
`control()`, `take_read_metrics()` and `take_write_metrics()` already existed and
were sufficient. No file outside the ownership list was touched.

`RemoteWorkspace::start_local` loses its production caller and now serves only
its own `live_backing.rs` tests; it and `BackingOwner` are otherwise unchanged.

## 2. What moved to the host authority

| Route | Previous authority | Now |
| --- | --- | --- |
| Production host-FUSE mount (`attach`, Linux + `host-fuse`) | `RemoteWorkspace::start_local` → `BackingOwner` over `Workspace.live`, `local_owner()` | `HostRuntime::start(snapshot, spool, policy, local = true)` → `mount_host(runtime.server.host_owner(), root, 0, 0)`; notifier and kernel root set exactly as before; host construction failure fails the attach (no fallback) |
| Ordinary Commit | freeze (`projection::pause`) → writer wait → quiesce → `projection::capture` (legacy fact export) → `Workspace::commit` → checkpoint install → resume, all under `worker.lifecycle` for the whole duration | `commit_host_session`: `begin_workspace_commit(Live)` → `host.maintain()` → `record_write_metrics` → `CommitCoordinator::commit` with `HostOverlay::snapshot()` as the owned cut → publication result. No freeze, no writer wait, no quiesce, no kernel cache flush, no legacy capture, no checkpoint install, no whole-duration lifecycle lock, no `Workspace.expected_head`/`pending_*` mutation |
| SDK range edits | `RemoteWorkspace::edit` (EDIT_BEGIN/PART/END over the container protocol) or the local freeze/capture/`EditCheckpoint`/refresh path | `HostRuntime::edit` → `HostSdk::edit` (one atomic `HostOverlay::edit_many_snapshot` plus the explicit kernel-coherence scope/read lease) |
| `session` / `Workspaces::session` | `remote_session` (OBSERVE) or `session_locked` using `Workspace.expected_head` | `host_session`: identity from the worker, `pinned_head` from `CommitCoordinator::published().branch`, `mutation_generation` from `HostRuntime::generation` |
| `summary` / `diff` dirty | OBSERVE generation `!= 0`, or `live.mutation_generation != 0` | `HostRuntime::is_dirty` = live sequence > published `covered_sequence`, plus `HostSdk::is_pending` and `CommitCoordinator::has_pending`. Never `sequence != 0` |
| `end_workspace_session` | pause on Clean, quiesce, dirty via OBSERVE, `end`, clear `remote`, `Workspace::discard`/`end_clean` | Same skeleton, plus: SDK coherence is resolved (`HostRuntime::recover_sdk`) before the unmount is requested; `projection::end` calls `HostRuntime::prepare_shutdown` (releases the preopened mount descriptor) before the verified unmount; after the unmount the coordinator's authoritative `abandon` runs through `HostRuntime::after_detach` (which then retires the SDK owner and the kernel references) before the authority is dropped. A failed settle leaves `BrokenCleanup` and the authority installed, so End/Discard stays retryable |
| Owner Drop (`Workspaces::drop`) | `End(Discard)` | unchanged route, now with the coordinator `abandon` inside it |
| Transport/read metrics | container/legacy server only | the installed host server is used when there is no `RemoteWorkspace`; the legacy spool fields keep their existing meaning (they are genuinely zero for a host session) |

Deliberate semantics preserved: mount and inode identity, open descriptors,
hardlink/rename/open-unlinked behaviour (unchanged host overlay paths; asserted
in the mounted test), branch CAS and conditional publication, same-branch
multi-Workspace operation with **no** lifetime-exclusive lease (two Workspaces
on one branch are exercised by `host_authority_head_movement_...`), authenticated
port, runtime-directory rollback on failed attach.

`workspace_stage` / `pending_stage` no longer gate ordinary operations on this
route because the host route never creates them: `Workspace::ensure_active` is
untouched and still means "lifecycle valid" for the host authority (its
`pending_*` terms are always false there).

## 3. What still uses the legacy path, and why

| Consumer | Why it is still legacy |
| --- | --- |
| Container placement (Docker) | Out of scope by instruction: `RemoteWorkspace::start`/`BackingOwner` and `DockerProjection` keep working exactly as before. No change was made for it |
| `WorkspaceProjection::Materialize` (host placement, non-Linux default) | The audit's "Materialized projection" row allows this explicit route to stay visibly separate until migrated. It still uses `Workspace::commit`, `refresh`, `pause/resume` and the legacy local live mirror |
| `Workspace::commit` / `commit_workspace_session_with_status` legacy branch / `install_checkpoint` / `live_backing::install_checkpoint` | Retained for the reconciliation-preview branch, the Materialize route and the container path. Not deleted: other (non-Commit) consumers still call them |
| Reconciliation publication, `recover_workspace_presentation`, `capture`, `MaterializedView` | Unchanged explicit routes (audit rows 7–9). The host route never sets `presentation_failed` and never refreshes |
| `reconcile.rs` creation/fingerprint, metrics/verification custody | Unchanged (audit rows 9–10); explicitly not migrated here |
| `layerfs-fuse` `live_owner.rs` freeze/cut/writeback-error channel | Unchanged: the ordinary SDK splice path and explicit shutdown still use it, exactly as the audit's §5.1 dispositions require |

## 4. Tests

New focused tests (all in `crates/layerfs-workspace/src/lifecycle.rs`, plus the
test-only latch in `host_runtime.rs`):

- `lifecycle::tests::host_authority_commit_in_flight_keeps_live_operations_and_owned_cut` —
  a real Commit is held mid-construction (attempt lock held, owned snapshot
  captured); a live write, a live read, `diff` and `sessions` complete while it
  is in flight; C1 is exactly the capture boundary (excludes the concurrent
  write, `covered_sequence` == the pre-write sequence) and C2 includes it.
- `lifecycle::tests::host_authority_dirty_tracks_covered_sequence_not_nonzero_generation` —
  committed session is clean at a nonzero monotonic sequence; a no-op Commit
  stays clean; `pinned_head` follows the published context; Clean End refuses
  only outstanding work and the refused End allocates nothing.
- `lifecycle::tests::host_authority_discard_resolves_publication_instead_of_erasing_it` —
  a lost publication reply (transaction committed, reply lost) is resolved from
  the retained receipt: the published Commit stays on the branch with the same
  root/head/commit count and the receipt is acknowledged
  (`acknowledge_workspace_publication` reports nothing left to delete). An
  outcome authoritative state cannot resolve (branch moved by a sibling
  Workspace after a rolled-back publication) is **retained**: Discard returns an
  error, the exact stage and the winning branch survive, and the session is
  `BrokenCleanup`.
- `lifecycle::tests::host_authority_head_movement_retains_stage_without_freezing_mutation` —
  the superseded expectation: after a moved branch the exact stage is retained
  (CAS preserved), but ordinary live mutation and status continue; explicit
  Discard removes exactly that stage and leaves the winning branch and the
  unpublished live bytes alone. (The pre-existing
  `head_movement_retains_stage_and_freezes_mutation_until_discard` is kept
  unchanged and still passes: it drives the retained legacy local
  `Workspace::commit` path, whose own live mirror still freezes.)
- `lifecycle::tests::mounted_host_authority_holds_commit_while_sdk_edit_and_live_read_proceed`
  (Linux + `host-fuse`, requires `LAYERFS_HOST_AUTHORITY_MOUNT=1`) — the real
  production path: `create_workspace_session(Fuse)` attaches through the new
  host mount, a real Commit is held mid-construction, an ordinary SDK range edit
  and kernel writes/reads through the mount complete against the same inode
  (stable `st_ino`), C1 excludes the splice, C2 includes it, and End(Clean)
  verifies the unmount.

### Commands and results (raw)

```
$ RUSTUP_TOOLCHAIN=1.85.1 cargo build --workspace --all-features --locked
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.81s   (no errors)

$ RUSTUP_TOOLCHAIN=1.85.1 cargo test -p layerfs-workspace --all-features --locked -- --test-threads=1
running 180 tests
test result: ok. 167 passed; 0 failed; 13 ignored; 0 measured; 0 filtered out; finished in 39.51s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.48s
running 6 tests
test result: ok. 0 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
```
(163 lib tests passed before this change; 4 new lib tests were added.)

```
$ RUSTUP_TOOLCHAIN=1.85.1 cargo test -p layerfs-workspace --all-features --locked -- --test-threads=1 host_authority
test lifecycle::tests::host_authority_commit_in_flight_keeps_live_operations_and_owned_cut ... ok
test lifecycle::tests::host_authority_dirty_tracks_covered_sequence_not_nonzero_generation ... ok
test lifecycle::tests::host_authority_discard_resolves_publication_instead_of_erasing_it ... ok
test lifecycle::tests::host_authority_head_movement_retains_stage_without_freezing_mutation ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```

Linux target (the mount path and the Linux-only `attach` branch are not compiled
on the host platform, so both were verified in a privileged
`rust:1.85.1-bookworm` container). The exact wrapper used in every Linux command
below was:

```
$ docker run --rm --privileged \
    -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs:/work \
    -v /Users/yifanxu/.cargo/registry:/usr/local/cargo/registry \
    -v /tmp/layerfs-linux-check:/ltarget \
    -e CARGO_TARGET_DIR=/ltarget -e CARGO_HOME=/usr/local/cargo \
    -e LAYERFS_HOST_AUTHORITY_MOUNT=1 -w /work rust:1.85.1-bookworm \
    <cargo command>
```

```
$ <wrapper> cargo test -p layerfs-workspace --all-features --locked --no-run
   (built; no errors; only the pre-existing warnings)

$ <wrapper> cargo test -p layerfs-workspace --all-features --locked --lib -- --test-threads=1 mounted_host_authority --nocapture
test lifecycle::tests::mounted_host_authority_holds_commit_while_sdk_edit_and_live_read_proceed ...
MOUNTED HOST AUTHORITY PASS: production attach, held Commit, concurrent SDK splice with stable inode, C1 excludes the splice while C2 includes it, verified unmount
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s

$ <wrapper> cargo test -p layerfs-workspace --all-features --locked -- --test-threads=1
running 182 tests
test result: ok. 169 passed; 0 failed; 13 ignored; 0 measured; 0 filtered out; finished in 36.03s
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.23s
running 6 tests
test result: ok. 0 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

```
$ RUSTUP_TOOLCHAIN=1.85.1 cargo test -p layerfs-sdk --all-features --locked -- --test-threads=1
unittests src/lib.rs 0 passed; tests/leases.rs 1 passed; tests/live_docker.rs 4 passed;
tests/live_fuse.rs 4 passed; tests/query_pagination.rs 1 passed; tests/v4.rs 1 passed
   (the live_docker/live_fuse bodies return early without their env gate, so these
    are compilation/regression passes, not live proof)

$ RUSTUP_TOOLCHAIN=1.96.0 cargo fmt --all      # clean; only the five owned files changed
```

## 5. Failures hit and how they were resolved

1. First `host_authority_commit_in_flight_...` run asserted `Created` and got a
   no-op publication: the test never mutated before the capture, so the owned cut
   equalled the predecessor. Fixed by making the pre-capture change explicit
   (`write(node, 0, b"A")` then capture, then the concurrent write at offset 1);
   the same defect existed in the mounted Linux test and was fixed the same way.
   This is a test-input defect, not a product change.
2. The first Discard test read the receipt table through a second `rusqlite`
   connection and failed with `DatabaseBusy` — the Store opens the file with
   `locking_mode = EXCLUSIVE`, so no second reader is possible. Replaced the raw
   SQL with the Store's own authoritative API: the test reconstructs the exact
   `WorkspacePublicationAttempt` and uses
   `resolve_workspace_publication`/`acknowledge_workspace_publication`.
3. The first mounted Linux run failed at `End(Clean)` with
   `failed to unmount ...: target is busy`. The cause was the test's own open
   descriptor on a file inside the mount, not the product: `prepare_shutdown`
   already released the client-owned preopened mount descriptor. Fixed by
   dropping the test's descriptor before End; the unmount then succeeded and is
   asserted.
4. `git status` shows no unrelated change: only the five owned files differ (an
   unrelated untracked `evidence/minimal-overhead-review/` directory exists in
   the checkout and was not touched).

## 6. NOT done / NOT proven

- **V1 (writable-mapping visibility) is untouched and remains open.** A Commit
  holds a captured host root; kernel-dirty mapped bytes are not part of it. This
  migration does not claim a compliant full-surface snapshot, does not add a
  freeze, and does not reintroduce `syncfs`/`freeze_kernel_cache` into the
  ordinary Commit path. Where kernel-side writeback errors are observed after
  this change is exactly where they were observed before it (the `live_owner`
  cut used by ordinary SDK splices and explicit shutdown), so the V1
  writeback-error-channel question is unchanged, not answered.
- **Container placement was not re-verified end to end here.** No Docker
  container helper was exercised in this work; the container path was only built
  and its existing unit tests run. `DockerProjection`, the daemon, and the
  authenticated TCP helper identity are unchanged but unproven by this evidence.
- **Public `Commit` metrics are partially unset for host sessions.**
  `Workspace::note_commit_edit_state` / `note_workspace_physical_spool` are not
  called on the host route (they read the legacy `live` mirror / `remote`
  backing), so `workspace_commit_edit_state`-style diagnostics stay unset rather
  than reporting fabricated zeros for a host session. `Capture`, publication and
  the transport read/write receipts are recorded; snapshot/arena/peak payload
  counters for the host overlay are not.
- **`fuse-oracle`/benchmark suites, `benchmark/` runs, releases, tags and issue
  closures were not run or attempted.** No performance number is claimed.
- **Non-Linux behaviour of the host route is unexercised**, because
  `mount_host` and the client kernel-coherence path are Linux-only. The macOS
  host-authority tests install the same authority and drive the same lifecycle
  routes but perform live I/O through the local `HostClient` port instead of the
  kernel; the SDK-splice part of check (a) is only proven by the mounted Linux
  test.
- **`layerfs-sdk`/`layerfs-cli` end-to-end runs against a mounted workspace were
  not performed** (env-gated `live_fuse`/`live_docker` suites skipped), so the
  public receipt fields seen by those clients after an ordinary host Commit were
  not re-observed.
- **Maintenance is driven, but its drain is bounded and cooperative.**
  `HostRuntime::maintain` runs at most 4096 steps before a new attempt and again
  before Clean End teardown, and returns immediately while an attempt is
  retained; remaining work is left for the next call. No measurement of the
  drain or of covered-change cleanup for a large session is provided.
- **`Workspace::discard` itself was not changed**: the coordinator's
  `abandon` is invoked by `end_workspace_session` (and therefore by owner Drop)
  before the authority is dropped. A caller that invoked `Workspace::discard`
  directly, bypassing End, would still bypass the coordinator — no such caller
  exists in the crate today, but this is a convention, not an enforced invariant.
