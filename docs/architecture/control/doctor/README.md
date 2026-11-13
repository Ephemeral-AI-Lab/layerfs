# Doctor

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The doctor observes a home, or one explicitly selected target in it, and
reports what is wrong and the step that fixes it, without changing anything.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-control` (`doctor/`), `layerfs-api` (`doctor/`), `layerfs-cli` (`doctor.rs`) |
| Source | [`layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor), [`layerfs-api/src/doctor`](../../../../api/crates/layerfs-api/src/doctor), [`layerfs-cli/src/doctor.rs`](../../../../api/crates/layerfs-cli/src/doctor.rs) |
| Uses | [Home](../home/README.md), [Machines](../machines/README.md) and [workers](../machines/workers-and-reach.md), [Stores and projects](../stores-and-projects/README.md), [Sandboxes](../sandboxes/README.md), [Backups](../backups/README.md), [Store diagnostics](../../engine/store/diagnostics.md), [Daemon control calls](../../engine/daemon/control-calls.md), [Bridge](../../engine/bridge/README.md) |
| Used by | [Front ends](../../api/front-ends/README.md): `layerfs doctor`, `doctor()` in Node and Python |
| Flows | none: the doctor is outside the call table and its dispatcher |

## Picture

```text
 layerfs doctor ...            doctor({home, target, ...})  (Node, Python)
   layerfs-cli doctor.rs         layerfs-api doctor_options_json
          \                          /
           doctor_request(options)        layerfs-api doctor/options.rs
             |                 |
        empty options      DoctorRequest {target, timeout, quick,
             |                            write_test}
             v                 v
   doctor(home)            doctor_target(home, request)    dispatch.rs
   whole-home survey         Budget: one overall deadline
   (checks.md)               |
   detect -> scan            +-- host     install, local records, df
   -> checks -> capacity     +-- machine  record, provider, worker
   -> workers                +-- sandbox  record, saved route, daemon
             |               +-- store    integrity | connection
             v               +-- backup   config, list [, write test]
   { ok, findings }          v
                           Checks -> { ok, complete, target, operation,
                                       mode, checks, findings, counts,
                                       truncated, timing, home }
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Parse options once | Empty or null options select the whole-home survey; otherwise `target` is required, `name` is required for every target but `host` (which refuses one), unknown or duplicate keys are refused, before any home or probe is read | [`options.rs`](../../../../api/crates/layerfs-api/src/doctor/options.rs) |
| Route outside the call table | `doctor_with_options` calls `doctor` or `doctor_target`; neither goes through ordinary dispatch, which could initialize or convert a home | [`run.rs`](../../../../api/crates/layerfs-api/src/doctor/run.rs), [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs) |
| Survey the whole home | `layerfs doctor` with no target: records, modes, references, intents, leases, partial copies, worker builds and Sandbox daemon capacity; see [Checks](checks.md) | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| Diagnose one target | Six targets (below), each with a stable entity, operation and default overall budget | [`target.rs`](../../../../control/crates/layerfs-control/src/doctor/target.rs), [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs) |
| Bound every probe | `Budget::limit(max)` gives each probe the smaller of its own cap and the remaining overall budget; when the budget ends, `diagnostic.deadline` is added as `unknown` | [`target.rs`](../../../../control/crates/layerfs-control/src/doctor/target.rs), [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs) |
| Report checks | Each check has `id`/`code`, `outcome`, `severity`, `message`, `evidence`, `fix`, `blocked_by`, `observed_ms`, `duration_ms`; see [Check report](checks.md#check-report) | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| Print | JSON by default; `--text` prints every retained check outcome, or for the survey `ok: nothing to report` or the counts and one line per finding with its fix | [`doctor.rs`](../../../../api/crates/layerfs-cli/src/doctor.rs), [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) |

### Targets

| Target | CLI | SDK options | Default budget | Source |
| --- | --- | --- | --- | --- |
| whole home | `layerfs doctor [--text]` | none, or `{}` | none overall; worker hello 10 s, daemon capacity 2 s | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs) |
| host | `layerfs doctor host` | `target: "host"` | 30 s | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| machine | `layerfs doctor machine <machine>` | `target: "machine", name` | 30 s | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| sandbox | `layerfs doctor sandbox <sandbox>` | `target: "sandbox", name` | 30 s | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| Store integrity | `layerfs doctor store integrity <store> [--quick]` | `target: "store.integrity", name, quick?` | none | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| Store connection | `layerfs doctor store connection <store>` | `target: "store.connection", name` | 10 s | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| backup connection | `layerfs doctor backup connection <bucket> [--write-test]` | `target: "backup.connection", name, write_test?` | 10 s; 30 s with the write test | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |

Every target command takes `--timeout <duration>` (whole `ms`, `s`, `m` or
`h`; Node and Python pass `timeout: "10s"`) and `--text`. The report's
`mode` is `full` or `quick` for Store integrity, `write-test` when the backup
write test was asked for, and `observation` otherwise; `operation` is
`integrity`, `connection` or `diagnose`. In Node:
`await layerfs.doctor({ home, target: "machine", name: "lima1", timeout: "10s" })`;
in Python: `layerfs.doctor(home, target="store.connection", name="team-db")`.

### Host

Never dials a worker or reads a remote transfer file (`host.remote` is
`skipped`). An absent home is reported and stays absent; layout 1.1.0 and
layout 2 homes are read without conversion.

| Check | What is observed | Source |
| --- | --- | --- |
| `host.build`, `host.prerequisites` | The calling executable; native runtime prerequisites (`skipped` in a build without the `machine` feature) | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |
| `host.capacity` | `/bin/df -Pk` on the home or its nearest existing ancestor, capped at 5 s, killed and reaped when late; evidence `filesystem`, `path`, `free_bytes`; no free-space threshold | [`host_capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/host_capacity.rs) |
| `host.layout` | `pass` current, `warning` 1.1.0 (needs `layerfs import-env`) or layout 2, `skipped` absent, `fail` unreadable | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| `host.records` | The survey's scan and record checks without remote transfer files (`checks::check_local`), each finding as a `fail` or `warning` with its fix; `unknown` when the record inventory changed or could not be read | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs), [`host_inventory.rs`](../../../../control/crates/layerfs-control/src/doctor/host_inventory.rs) |
| `host.native_daemon`, `host.artifacts` | Default native daemon executable; each machine's recorded `daemon` executable | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |
| `host.bundle`, `host.bundle_hashes` | When a record names a non-native machine: the Linux bundle manifest (this version, `linux-<arch>`, 64-hex hash) and `layerfs`, `layerfs-daemon`, `layerfs-sandbox-helper` and the image archive present; hashes are not recomputed (`skipped`) | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |

The inventory fingerprints only management paths (records, `worker.json`,
`control.json`, lease stamps under `open/`, `store.sqlite.incoming`, project,
Workspace and command records) before and after the survey. A symlink or an
unreadable path makes it fail. It never descends into daemon runtime trees,
mounts, logs or SQLite payloads, and it is not a transaction: a change undone
between the two reads is not seen.

### Machine

| Check | What is observed | Source |
| --- | --- | --- |
| `machine.record`, `machine.platform` | `v`, name, platform, state (`running`, `stopped`, `failed`), positive `cpus`/`memory`/`disk`, port 1–65535, decodable `op` | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.operation` | Live `op`: `warning`; unresolved: `fail`; both block every later probe. Stale: `warning`, probes go on. Evidence holds only standing, `uncertain`, a well-formed id and recognized name and phase | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.provider`, `machine.provider.tool_version` | `provider_within` through the platform adapter, capped at 10 s: running `pass`, stopped `warning` (not started), missing or failed `fail` | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs), [`calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |
| `machine.state` | Recorded state against the observed one; the record is not refreshed | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.worker.endpoint` | Saved loopback address, nonzero port, nonempty secret | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.worker.auth`, `.compatibility` | One authenticated hello within 5 s returning `ready: true`; protocol and sealed bundle match this build | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs) |
| `machine.worker.ping`, `machine.capacity` | `worker.ping` with `capacity: true`: kind `worker`, positive pid, this version, and `free_disk` from the worker's own fixed root (`unknown` when null) | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |

A native (in-process) machine skips `machine.worker` and `machine.capacity`.
A runtime that is not running skips the worker checks; a failed dependency
blocks the checks after it.

### Sandbox

The saved route is followed without `sandbox.get`, deployment, Store open,
lease, record refresh, new forward or lifecycle change.

| Check | What is observed | Source |
| --- | --- | --- |
| `sandbox.record`, `.store_reference`, `.project_reference`, `.machine_reference`, `.store_placement` | Record and the records it names; a SQLite Store must be on the Sandbox's machine | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| `sandbox.operation` | Live owner `pass`; ended owner's certain or uncertain `op` `warning` | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| `sandbox.runtime`, `.forward_process` | Native or namespaces daemon/init process, or a forward's carrier, matched by pid, start time and boot; never by pid alone | [`sandbox_runtime.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_runtime.rs) |
| `sandbox.route`, `.worker_compatibility`, `.worker_report` | Native: direct. Forward: the machine's `worker.json` must exist; no worker call. Other machines: one `doctor.sandbox` request to an authenticated compatible worker, whose checks are merged | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| `sandbox.daemon_keys`, `.daemon_connect`, `.daemon_auth`, `.daemon_instance`, `.daemon_ready` | Saved loopback endpoint and 32-byte keys; authenticated channel; `Hello` without waiting for startup must name the saved instance and `ControlReady` | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |
| `sandbox.workspaces`, `.workspace_retained` | `sandbox.doctor_workspaces` pages of at most 32 rows under a generation; counts, mounted, activities and up to 16 failed or uncertain examples | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs), [`diagnostic.rs`](../../../../engine/crates/layerfs-daemon/src/control/diagnostic.rs) |
| `sandbox.session` | `EndSession` for the doctor's own control session, only after a clean exchange | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |

Always `skipped`: `sandbox.filesystem_serving` (FUSE/NFS reads, overlay and
Store integrity), `sandbox.overlay_state` (dirty counts, pending merges) and
`sandbox.runtime_containment` or `sandbox.runtime_placement`.

### Store

| Check | What is observed | Source |
| --- | --- | --- |
| `store.record`, `store.identity` | Record version and name, kind, saved binding and cursor identity | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| integrity checks | SQLite on this kernel: read-only audit of the file. SQLite on a machine: `store.diagnose` on its worker (connect 5 s), events streamed back; `store.worker` says whether it completed. PostgreSQL: read-only admission, then the audit. Check ids and coverage: [Store diagnostics](../../engine/store/diagnostics.md) | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs), [`diagnostic.rs`](../../../../control/crates/layerfs-store/src/diagnostic.rs) |
| `store.integrity.access`, `.coverage` | `unknown` and `blocked` when the audit did not complete | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| connection checks | PostgreSQL only (`store.connection.*`); a SQLite Store gives `store.connection.kind` `fail` | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |

A `store.advisory.*` failure is a warning; `*.progress` events are dropped.

### Backup

Uses the alias's own S3 configuration, signer, TLS policy and HTTP codec,
with the bridge's bounded resolver.

| Check | What is observed | Source |
| --- | --- | --- |
| `backup.configuration`, `backup.credentials` | Record version and name, provider `r2` or `minio`, endpoint, region, bucket, prefix; credential reference resolves (values never shown) | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| `backup.transport`, `backup.list` | One signed ListObjectsV2 under the prefix with `max-keys=1`; keys discarded | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |
| `backup.write_capability` | `--write-test` only: versioning must read `Enabled`, else no PUT | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |
| `backup.put`, `backup.reconcile` | One single-part PUT with `If-None-Match: *` at `<prefix>/_layerfs_doctor/<64-hex token>/probe`; after an uncertain PUT, a GET of that key (token, bytes, version) instead of a resend | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| `backup.get`, `backup.delete` | GET and DELETE of the owned version only; a failed GET does not stop the DELETE | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |

Evidence keeps `cause`, `http_status`, `phase` (`dns`, `tcp`, `tls`,
`connection_before_send`, `exchange_after_send`, `http_response`) and an
allowlisted `s3_code`; never response bodies or listed keys. A DELETE or
reconcile that fails leaves `possible_retained_key` and `owned_version`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| The doctor changes nothing: no lock, lease, record write, conversion, stale-lease removal, machine start or deployment; an absent home stays absent | [`mod.rs`](../../../../control/crates/layerfs-control/src/doctor/mod.rs), [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs), [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| Options are validated before any home or probe is read: `quick` only for `store.integrity`, `write_test` only for `backup.connection` | [`options.rs`](../../../../api/crates/layerfs-api/src/doctor/options.rs), [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs) |
| No probe outlives the overall budget | [`target.rs`](../../../../control/crates/layerfs-control/src/doctor/target.rs) |
| `ok` means no error was counted, not health: `blocked`, `unknown` or truncation make `complete` false, and a confirmed `fail` stays after a later timeout | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| One attempt: a lost worker, daemon or S3 reply is `unknown`; nothing is resent, replayed or retried | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs), [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs), [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| A worker receives only `worker.ping`, `doctor.sandbox` or `store.diagnose`, after a compatible hello, and answers from its fixed root, never a caller path | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs), [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| The backup write test writes only after versioning is `Enabled`, and deletes only the version it owns; a refused or colliding PUT authorizes no delete | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| Secrets and arbitrary record fields are not reported | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs), [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Relative home on a target | `InvalidInput` "doctor home must be absolute" | [`dispatch.rs`](../../../../control/crates/layerfs-control/src/doctor/dispatch.rs) |
| Bad option, target, name or timeout | `InvalidInput` "doctor: …"; the CLI exits 2 | [`options.rs`](../../../../api/crates/layerfs-api/src/doctor/options.rs), [`doctor.rs`](../../../../api/crates/layerfs-cli/src/doctor.rs) |
| A report with findings | CLI exit 0; a refusal exits 125 | [`doctor.rs`](../../../../api/crates/layerfs-cli/src/doctor.rs) |
| Retained checks, message, evidence | 1024 checks, 4096 characters, 16 KiB; beyond them `truncated` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| Probe caps | `df` 5 s; provider 10 s; worker hello 5 s; Store worker connect 5 s; PostgreSQL admission 10 s when no deadline | target sources above |
| Frames | hello and ping 4 KiB (`MAX_HELLO`); diagnostic request and answer frames 64 KiB (`TooLarge`) | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs) |
| Daemon registry held | `Busy`, `Changed` or `UnknownMethod` make `sandbox.workspaces` `unknown`; no retry | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |
| Build without `machine` | `machine.platform` `unknown` | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| Build without `postgresql`; SQLite off this kernel | `Unsupported`, shown as `store.integrity.access` `unknown` | [`diagnostic.rs`](../../../../control/crates/layerfs-store/src/diagnostic.rs) |
| Build without `tls` for R2 or `https://` | `backup.configuration` `unknown`; nothing sent | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| R2 versioning | Not `Enabled` through its S3 API: list checks run, the write test stays unsupported | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |

## Demonstration

Commands are written from source and were not run while writing this page.
Give each a wall limit of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`doctor.rs`](../../../../control/crates/layerfs-control/tests/doctor.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor` | macOS, Linux | One finding per planted problem with its fix; a layout 2 home is not converted; capacity warnings; nothing written |
| [`doctor_host.rs`](../../../../control/crates/layerfs-control/tests/doctor_host.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_host` | macOS, Linux | Absent home stays absent; no worker dial; concurrent record rewrites leave the result incomplete |
| [`doctor_report.rs`](../../../../control/crates/layerfs-control/tests/doctor_report.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_report` | any | `unknown`/`blocked` are incomplete without an error; retention bounds |
| [`doctor_machine_target.rs`](../../../../control/crates/layerfs-control/tests/doctor_machine_target.rs) | `cargo +1.85.1 test --locked -p layerfs-control --features machine --test doctor_machine_target` | macOS, Linux | Stand-in providers and worker peers: dependencies block, the home is unchanged |
| [`doctor_sandbox.rs`](../../../../control/crates/layerfs-control/tests/doctor_sandbox.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_sandbox` | macOS, Linux | Real authenticated channel; `Busy`, `Changed`, lost reply stay incomplete with no replay |
| [`doctor_backup_target.rs`](../../../../control/crates/layerfs-control/tests/doctor_backup_target.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_backup_target` | macOS, Linux | S3 fault peer: no PUT without versioning, owned-version evidence after a lost PUT |
| [`doctor_targets.rs`](../../../../api/crates/layerfs-cli/tests/doctor_targets.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test doctor_targets` | macOS, Linux | Every help level and target runs without creating a home; bad flags exit 2 |

The stand-ins and fault peers check the observation boundary; they do not
qualify live Docker, Lima, Apple Container, R2 or MinIO installations.

## Pages

| Page | What it covers |
| --- | --- |
| [Checks](checks.md) | The whole-home survey (scan, record checks, layout 2 conversion, worker builds, daemon capacity) and both report shapes |
