# Coverage audit: `layerfs-daemon-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous daemon: native bridge, mount assembly, managed Exec and headless delivery. Its Rust tests are three; the rest of its verification was Docker route scripts. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **NOT REMOVED in this stage. It carries pinned read-only view code, which no active crate covers and whose future is the unanswered owner question O-10. Everything else in it is class A or C; the removal is prepared and needs only the owner's answer.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 2151 in 11 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/config.rs` | 286 | Mount, control and writable-profile configuration from arguments | A+C | `layerfs-daemon/src/application/config.rs`, `cli.rs`: D `native_application.rs::actual_daemon_install_hello_bind_status_no_constructor_and_session_end`, D `startup_cost.rs::admission_refusal_has_no_creation_receipt_or_artifact_effect`. Branch disk budget and Server endpoint are dropped [D12] [D1] |
| `src/control.rs` | 491 | One authenticated control session: attach, mount, status, unmount, close | A+C | `layerfs-daemon/src/control/` (operations, serve, registry): D `native_control.rs` (10). Separate close is withdrawn [D6]. Its dispatch of view operations is O-10 |
| `src/control_commit.rs` | 175 | Commit outcomes and writable observations on the control route | A+C | `layerfs-daemon/src/control/operations.rs`, `store/commit.rs`: D `native_control.rs::product_control_commit_captures_the_frontier_releases_its_owners_and_is_then_up_to_date`, D `mounted_commit.rs` (7). Staged/composite split and HeadMoved [D7] |
| `src/control_view.rs` | 199 | Authorized dispatch of the view-lease operations | O | Pinned read-only SDK view leases. Not covered by any active crate; whether the product keeps them is the unanswered owner question O-10 (303/08 K20, §4.2, §7) |
| `src/execution.rs` | 162 | Bounded shell execution inside one mounted Workspace | C | [D2] |
| `src/headless.rs` | 133 | Stdin/stdout framed headless delivery | C | [D9] (303/07 §4.1 names it for removal) |
| `src/lib.rs` | 11 | Declarations; exports `run` | A | `layerfs-daemon/src/lib.rs` |
| `src/lifecycle.rs` | 297 | The one native owner shared by control and process shutdown | A | `layerfs-daemon/src/application/owner.rs`, `control/registry.rs`: D `native_mount.rs::two_workspaces_share_one_dispatcher_and_unmount_independently`, D `cleanup_control.rs` |
| `src/main.rs` | 10 | Process entry | A | `layerfs-daemon/src/bin/layerfs-daemon.rs` |
| `src/run.rs` | 299 | Process and connection assembly | A | `layerfs-daemon/src/application/serve.rs`, `connection.rs`, `filesystem.rs`: D `native_application.rs`, D `observed_application.rs` (4) |
| `src/transport.rs` | 88 | Reuse of the daemon-to-host Service transport | C | [D1] [D9] (303/07 §4.1 names it for removal; the upstream pool that replaced it was itself retired by F12) |

Lines by class: A 617, A+C 952, C 383, O 199.

## Tests

3 test functions in 1 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/configuration.rs`** (3) | | |
| `mount_startup_rejects_invalid_inputs_before_connecting` | A | D `startup_cost.rs::admission_refusal_has_no_creation_receipt_or_artifact_effect`, D `e2_startup_receipts.rs::pre_creation_refusal_keeps_original_error_and_emits_unavailable_work` |
| `control_configuration_is_explicit_separate_and_bounded` | A | B `daemon_records.rs::private_setup_is_exact_bounded_and_redacts_secret` |
| `writable_profile_requires_branch_disk_budget_and_commit_control_endpoint` | C | [D12] [D13] (one mutable profile; no branch disk budget) |

Tests by class: A 2, C 1.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Depends on the active `layerfs-fuse` and `layerfs-workspace`, whose API it does not match; cannot be loaded |
| `Dockerfile` | C | Image of the old daemon. Current images are built by the Sandbox lifecycle and the harness |
| `examples/transport_probe.rs` | C | Probe of the daemon-to-host transport [D1] |
| `tests/control_attach.py` | A | Docker driver. D `native_mount.rs`, D `native_mount_routes.rs` |
| `tests/control_attach_startup.py` | A | Docker driver. D `native_mount.rs::attach_refusals_have_no_effect_and_a_failed_attach_returns_to_unattached` |
| `tests/control_close.py` | C | Separate close [D6] |
| `tests/control_commit.py` | A+C | Docker driver. D `mounted_commit.rs`, D `mounted_commit_failures.rs`; HeadMoved rows [D7] |
| `tests/control_commit_wire.py` | A | Wire helper. B `control_golden.rs` |
| `tests/control_mount.py` | A | Docker driver. D `native_control.rs` |
| `tests/control_status.py` | A | Docker driver. D `native_control.rs::status_retains_known_publication_when_the_engine_is_unavailable`, B `control_golden.rs::status_keeps_its_bytes_at_every_activity_and_native_phase` |
| `tests/control_unmount.py` | A | Docker driver. D `mounted_drain.rs`, D `native_custody.rs`, D `forced_unmount.rs` |
| `tests/docker_faults.py` | A+C | Docker fault driver over the host transport [D1]. Channel faults: B `native.rs` |
| `tests/docker_route.py` | C | Headless host route [D1] |
| `tests/history_route.py` | A+C | History over the host route [D1]. D `native_control.rs::native_mount_commit_status_fork_history_and_terminal_unmount` |
| `tests/metadata_route.py` | C | Service metadata constructor route [D1] [D9] |
| `tests/mount_startup.py` | A | Docker driver. D `native_mount.rs::ready_mount_serves_the_complete_root_to_unregistered_access_then_drains` |
| `tests/mounted_read.py` | A | Docker driver. D `native_mount.rs`, D `read_cost.rs` |
| `tests/transport_diagnostic.py` | C | Driver of `transport_probe` [D1] |
