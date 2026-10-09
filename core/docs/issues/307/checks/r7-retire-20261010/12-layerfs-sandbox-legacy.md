# Coverage audit: `layerfs-sandbox-legacy`

> **Status:** Dated checkpoint receipt; coverage audit written before any removal.

The previous host Docker owner: a registry of sandboxes with checked routes to the host Server. Classes and the directions `[D1]`–`[D13]` are defined in [00-scope-and-method.md](00-scope-and-method.md). Test directories: D daemon, W workspace, O overlay, F fuse, B bridge, S sandbox, K SDK, P project, C content, PE persistence.

Verdict: **REMOVE. Every behaviour is class A or C.**

## Source

Production lines from the pinned counter at `cf5bd62ce` ([receipt 01](01-production-loc-head.txt)). Total 1106 in 5 files.

| File | Lines | Behaviour | Class | Covered by / recorded direction |
| --- | ---: | --- | --- | --- |
| `src/docker.rs` | 324 | Docker CLI invocation: create, port lookup, delete, bounded log capture at delete | A+C | Engine API client `layerfs-sandbox/src/backend/docker/` (S `engine_protocol.rs` 9, S `engine_lifecycle.rs` 6). Delete-time log capture is not in the selected SandboxApi scope [D11] |
| `src/lib.rs` | 10 | Exports `SandboxOwner`, routes and `LogCapture` | A+C | `layerfs-sandbox/src/lib.rs`, SDK `sandbox/api.rs` |
| `src/owner.rs` | 588 | Host registry of sandboxes: create, list, delete, checked routes and Workspace bindings to the host Server | A+C | Create, start, stop, delete with one-attempt flags: SDK `sandbox/api.rs` + `layerfs-sandbox/src/backend/docker/container.rs` (S `engine_lifecycle.rs::partial_create_identity_and_nonlocal_volume_are_not_adopted`, `lost_upload_and_start_keep_original_owner_and_refuse_replay`). The registry routed Workspace calls to the host Server and is retired with it [D1]; `list` is not in the selected scope [D11] |
| `src/readiness.rs` | 62 | Wait for the daemon's readiness marker | A | `layerfs-sandbox/src/backend/docker/listener.rs`: S `engine_lifecycle.rs::stderr_and_substring_markers_never_satisfy_readiness`, `marker_inside_truncated_frame_keeps_evidence_and_fails_readiness` |
| `src/session.rs` | 122 | Bounded reuse of one authenticated Workspace control session | A+C | SDK `control/connection.rs` (K `observed_control.rs` 3 tests; D `native_control.rs::control_busy_keeps_the_channel_healthy_for_a_later_explicit_call`). Session reuse across the host registry [D1] |

Lines by class: A 62, A+C 1044.

## Tests

1 test functions in 1 files. None can be built at HEAD ([receipt 02](02-cargo-cannot-load.txt)), so none is coverage today; each is classified by the behaviour it asserted.

| Test | Class | Covered by / recorded direction |
| --- | --- | --- |
| **`tests/docker_port_pending.rs`** (1) | | |
| `docker_allocates_port_and_pending_record_cannot_route` | A+C | Pending registry record [D1]. Endpoint from the Engine: S `engine_lifecycle.rs::streamed_private_archive_exact_marker_endpoint_and_borrowed_cleanup`, `widened_or_missing_native_access_is_never_reported_as_the_endpoint` |

Tests by class: A+C 1.

## Other tracked files

| File | Class | Note |
| --- | --- | --- |
| `Cargo.toml` | C | Depends on `../layerfs-api/core`, removed by F12; cannot be loaded ([receipt 02](02-cargo-cannot-load.txt)) |
