# Coverage audit: `layerfs-fuse-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous FUSE projection (direct-I/O profile) over the previous Workspace. Every test is `#[ignore]`d in source. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **REMOVE after the one class B item is migrated (execution from the mount, `layerfs-daemon/tests/mounted_execute.rs`).**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 1447 in 5 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/adapter.rs` | 919 | Kernel callbacks: INIT, LOOKUP, FORGET, GETATTR, ACCESS, OPEN, READ, READLINK, FLUSH, RELEASE, OPENDIR, READDIR, RELEASEDIR, STATFS, FSYNC, SETATTR, WRITE, MKNOD, MKDIR, UNLINK, RMDIR, SYMLINK, RENAME, LINK, CREATE; xattr and READDIRPLUS refused | A+C | `layerfs-fuse/src/request/callbacks.rs` serves the same set and declares every other opcode in `operations/unsupported.rs` with a count (`request/accounting.rs`). Read path: D `native_mount.rs` (3), D `read_cost.rs` (5), D `directory_cost.rs` (4); mutation: D `native_mutation.rs` (2), D `native_coherence.rs` (6). Dropped with the mechanism: direct-I/O profile and per-WRITE invalidation [D4], the 128-handle table [D8], blocking reply permits [D5] |
| `src/lib.rs` | 9 | Exports `mount`, `mount_writable`, `MountHandle` | A | `layerfs-fuse/src/lib.rs`; one mutable mount (the read-only profile is not carried, [D13]) |
| `src/mount.rs` | 404 | Session ownership, mount/unmount and bounded callback drain | A+C | `layerfs-fuse/src/session/` and `mount/`: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`, D `mounted_drain.rs` (5), D `forced_unmount.rs` (6), F `fence.rs` (4). Startup deadlines are not carried [D5] |
| `src/replies.rs` | 104 | Workspace results to kernel replies | A | `layerfs-fuse/src/request/reply.rs`, `attributes.rs`: F `attributes.rs` (3 tests) |
| `src/trace.rs` | 11 | Env-gated per-callback diagnostics | A | Opcode accounting in the status receipt: `layerfs-fuse/src/request/accounting.rs`, read in D `native_coherence.rs` and D `mounted_drain.rs` |

Lines by class: A 124, A+C 1323.

## Tests

25 test functions in 3 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/kernel_resize.rs`** (9) | | |
| `kernel_resize_semantics` | A | D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon`, W `payload.rs::truncate_and_regrow_never_resurrect_bytes_across_capture_and_known_install` |
| `kernel_resize_open_trunc` | A | D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon` (O_TRUNC of a cached inherited file) |
| `kernel_resize_same_length` | A | W `captured_namespace_states.rs::a_base_file_with_only_metadata_changes_keeps_its_content_root` |
| `kernel_resize_refused` | A | D `native_mutation.rs::namespace_mutations_are_visible_at_once_and_refusals_stay_refused`, W `native_visit.rs::a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing` |
| `kernel_resize_failed_open` | C | [D12] (quota-refused SETATTR after OPEN) |
| `kernel_resize_metadata_failure` | C | [D3] (private metadata allocation failure) |
| `kernel_resize_envelope` | A+C | The 8 MiB zero-extension envelope is an artificial cap [D8]. Sparse extension: D `complete_installed_roots.rs::sparse_native_bytes_and_holes_survive_install`, W `captured_file_edits.rs::new_hole_above_four_gib_uses_bounded_original_run_work` |
| `kernel_resize_native_save` | A | D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit` |
| `kernel_resize_origin` | C | [D3] [D5] (projection-origin API of the old Workspace, no mount) |
| **`tests/kernel_write.rs`** (11) | | |
| `kernel_write_positional` | A | D `native_mutation.rs::write_append_and_truncate_are_exact_in_cache_and_daemon`; incremental Commits: D `mounted_commit.rs::r5_8_five_incremental_commits_on_one_mount_each_read_back_completely` |
| `kernel_write_append` | A | D `native_coherence.rs::size_never_shrinks_under_concurrent_append_and_refusals_stay_refused` |
| `kernel_write_read_race` | A+C | The old gate refused a WRITE while a READ was held; that refusal is withdrawn [D5]. Required behaviour now: D `mounted_parking.rs::fp8_a_sibling_write_and_a_same_mount_write_complete_while_cold_reads_are_parked` |
| `kernel_write_mappings` | A+C | Direct-I/O profile refusing shared mappings [D4]. Cached profile: D `native_coherence.rs::shared_mapping_stores_are_published_and_never_change_size`, D `mounted_commit.rs::r5_9_stores_through_shared_mappings_are_committed_once_written_back` |
| `kernel_write_exec` | B | A program stored in the Workspace executes, and executes as its replacement after an in-place rewrite. No active test executed anything from a mount. **Migrated**: D `mounted_execute.rs::a_stored_program_executes_and_its_in_place_rewrite_executes_next` (added in this stage; Linux receipt `execute-attempt1-linux-mounted_execute.txt`) |
| `kernel_write_native_save` | A | D `mounted_install.rs::writes_during_a_commit_stay_live_and_reach_the_next_commit`, D `mounted_concurrency.rs::r6_2_a_commit_against_twenty_writers_holds_an_exact_prefix_of_each` |
| `kernel_write_ingress` | A | W `payload.rs::writes_change_only_their_window_and_never_read_the_inherited_payload` |
| `kernel_write_quota` | A+C | Per-Workspace private-disk budget [D12]. Device-full behaviour: D `device_capacity.rs::real_owner_reclaims_at_device_full_after_last_owner_without_a_cleanup_job`, O `device_capacity.rs` |
| `kernel_write_backing_failure` | C | [D3] (failure of the private backing allocator) |
| `kernel_write_origin` | C | [D3] [D5] (projection reply-slot API of the old Workspace, no mount) |
| `kernel_write_completion_failure` | C | [D4] (failure of the per-WRITE notifier) |
| **`tests/mount_failure.rs`** (5) | | |
| `mount_failure_deadline` | A+C | INIT deadline [D5]. Failed attach custody: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached`, D `native_mount_routes.rs::lost_bind_and_attach_acknowledgements_are_located_without_replay` |
| `mount_failure_worker` | A | Fixed shared workers: F `dispatch.rs::panic_and_failure_retain_original_future_and_do_not_stop_other_mounts`, `handoff_after_pool_shutdown_retains_original_and_returns_terminal_error`; receive-loop spawn failure is proven in the vendored fuser lifecycle tests ([R2 fuser checkpoint](../../R2-FUSER-LIFECYCLE-20261008.md)) |
| `mount_failure_session` | A | D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached` |
| `mount_failure_admission` | A+C | Deadline half [D5]. Authority half: D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached` |
| `mount_failure_success` | A+C | Read-only mount profile [D13]. Writable lifecycle: D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount` |

Tests by class: A 11, A+C 7, B 1, C 6.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Package name `layerfs-fuse` duplicates the active member; depends on the replaced Workspace API; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt)) |
| `tests/check_mount.py` | C | Docker driver of the old mount tests |
| `tests/kernel_resize_route.py` | C | Docker driver of `kernel_resize.rs` |
| `tests/kernel_write_route.py` | C | Docker driver of `kernel_write.rs` |
| `tests/mount_failure_route.py` | C | Docker driver of `mount_failure.rs` |
| `tests/position_manifest.py` | C | Fixture manifest for the old position tests |
| `tests/position_master.py` | C | Fixture master for the old position tests |
