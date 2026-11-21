# Doctor

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Six doctors each check one thing in a home (`home.doctor`,
`machine.doctor`, `store.doctor`, `project.doctor`, `sandbox.doctor`,
`backup.doctor`), then each thing directly on it, shallowly, and report what
is wrong and the step that fixes it, without changing anything. What
interrupted operations left on disk is in no report: LayerFS removes it
itself ([Checks: leftovers](checks.md#leftovers)). The
user-facing contract is [calls.md: The doctors](../../../versioned/2.0.0/calls.md#the-doctors).

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-control` (`doctor/`), `layerfs-api` (`calls/doctor/`, each family's `doctor.rs`), `layerfs-cli` (`output.rs` `doctor` for `--text`) |
| Source | [`layerfs-control/src/doctor`](../../../../control/crates/layerfs-control/src/doctor), [`layerfs-api/src/calls/doctor`](../../../../api/crates/layerfs-api/src/calls/doctor), [`layerfs-cli/src/output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) |
| Uses | [Home](../home/README.md), [Machines](../machines/README.md) and [workers](../machines/workers-and-reach.md), [Stores and projects](../stores-and-projects/README.md), [Sandboxes](../sandboxes/README.md), [Backups](../backups/README.md), [Store diagnostics](../../engine/store/diagnostics.md), [Daemon control calls](../../engine/daemon/control-calls.md), [Bridge](../../engine/bridge/README.md) |
| Used by | [SDK](../../api/sdk/README.md): [`home.doctor`](../../api/sdk/home.md#homedoctor), [`machine.doctor`](../../api/sdk/machine.md#machinedoctor), [`store.doctor`](../../api/sdk/store.md#storedoctor), [`project.doctor`](../../api/sdk/project.md#projectdoctor), [`sandbox.doctor`](../../api/sdk/sandbox.md#sandboxdoctor), [`backup.doctor`](../../api/sdk/backup.md#backupdoctor) |
| Flows | each is a table call of its family, dispatched like any other; every doctor is `unopened` (it never creates the home) |

## Picture

```text
 layerfs home doctor            layerfs.home.doctor()         (Node, Python)
 layerfs machine doctor <m>     layerfs.machine.doctor(m)
 layerfs store doctor <s> [--machine M]
 layerfs project doctor <p> [--full]
 layerfs sandbox doctor <x>
 layerfs backup doctor <b> [--full]
          |  call table: <family>/doctor.rs -> calls/doctor/answer.rs run
          v
 layerfs_control::doctor::run(home, Thing {kind, name}, Options {full, build, machine})
   frame.rs  locate(thing) in the home's records (kinds.rs, home_tree)
               not there -> NotFound
             visit(thing)       its kind's checks, its own budget
                                (none for a Full check)
             each thing on it   shallow: own checks only, nothing beneath;
                                blocked by a stopped/failed parent -> not tried
             one progress event per finished thing
          v
 Report { tree, observed_ms }   report.rs
   long form  { ok, findings, not_checked, status, counts, observed_at, tree }
   short form { ok: true } alone, or { findings, not_checked }
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Six table calls | Each family's `doctor.rs` is one table entry (`unopened`, the one doctor `FORM`); each reads its thing's name and options and calls `answer::run` with its `Kind` | [`calls/doctor/mod.rs`](../../../../api/crates/layerfs-api/src/calls/doctor/mod.rs), [`answer.rs`](../../../../api/crates/layerfs-api/src/calls/doctor/answer.rs) |
| One runner | `run` locates the thing in the home's records (refused `NotFound` when it is not there), runs its kind's checks under the kind's budget, then gives each thing directly on it a shallow check, or a `not_checked` node with `blocked_by` when the parent blocks it | [`frame.rs`](../../../../control/crates/layerfs-control/src/doctor/frame.rs), [`kinds.rs`](../../../../control/crates/layerfs-control/src/doctor/kinds.rs) |
| Per-kind checks | Each kind registers its check ids in the order they run, each marked when only its Full check makes it, and the function that makes them (`KindDoctor`) | [`kinds.rs`](../../../../control/crates/layerfs-control/src/doctor/kinds.rs) |
| Report | A tree of nodes (`status`, `coverage`, one row per check), with the flat `findings` and `not_checked` lists front ends read first; see [Check report](checks.md#check-report) | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs), [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| Leftovers | No call: LayerFS sweeps them at fixed points in later calls, one attempt each, only where it can prove nothing owns them (spec v20); see [Leftovers](checks.md#leftovers) | [`sweep.rs`](../../../../control/crates/layerfs-sandbox/src/sweep.rs), [`image_sweep.rs`](../../../../control/crates/layerfs-sandbox/src/image_sweep.rs), [`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs), [`project.rs`](../../../../control/crates/layerfs-control/src/project.rs) |
| Print | JSON by default (short form; `--detail` the long form); `--text` prints a count line (`N error(s), N warning(s), N notice(s), N not checked`, or `ok: every check was made and nothing was found`), each finding with its `from` and `fix`, then each `not_checked` entry | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) `doctor`, [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |

### The doctors

| Call | CLI | Its thing, then what it checks shallowly | Options | Budget per thing | Source |
| --- | --- | --- | --- | --- | --- |
| `home.doctor` | `layerfs home doctor` | the home; each recorded machine, each PostgreSQL Store (a sqlite Store is on its machine) and each bucket | `--progress` | home 30 s; machine 30 s, Store 10 s, bucket 10 s | [`home/doctor.rs`](../../../../api/crates/layerfs-api/src/home/doctor.rs), [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| `machine.doctor` | `layerfs machine doctor <machine>` | the machine; each sqlite Store and each Sandbox on it | `--progress` | machine 30 s; Store 10 s, Sandbox 30 s | [`machine/doctor.rs`](../../../../api/crates/layerfs-api/src/machine/doctor.rs), [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `store.doctor` | `layerfs store doctor <store> [--machine M]` | the Store; each of its projects | `--machine` (a PostgreSQL Store only: one more `store.connection` probe from M; a sqlite Store refuses it `InvalidInput`), `--progress` | Store 10 s; project 10 s | [`store/doctor.rs`](../../../../api/crates/layerfs-api/src/store/doctor.rs), [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| `project.doctor` | `layerfs project doctor <project> [--full]` | the project | `--full` (`project.data`), `--progress` | 10 s; none with `--full` | [`project/doctor.rs`](../../../../api/crates/layerfs-api/src/project/doctor.rs), [`project_target.rs`](../../../../control/crates/layerfs-control/src/doctor/project_target.rs) |
| `sandbox.doctor` | `layerfs sandbox doctor <sandbox>` | the Sandbox; each Workspace its daemon lists or the home records | `--progress` | Sandbox 30 s; Workspace 10 s | [`sandbox/doctor.rs`](../../../../api/crates/layerfs-api/src/sandbox/doctor.rs), [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs), [`workspace_target.rs`](../../../../control/crates/layerfs-control/src/doctor/workspace_target.rs) |
| `backup.doctor` | `layerfs backup doctor <bucket> [--full]` | the bucket | `--full` (the write test); no progress events | 10 s; with `--full` none overall, the S3 probe capped at 30 s | [`backup/doctor.rs`](../../../../api/crates/layerfs-api/src/backup/doctor.rs), [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |

A Workspace has no doctor of its own; its node is made by its Sandbox's
doctor. A check still running when its thing's budget ends is `not_checked`
with `reason: timeout`. In Node: `await layerfs.machine.doctor("lima1")`; in
Python: `layerfs.store.doctor("team-db", machine="lima1")`.

### Home

`home.doctor`'s own checks, in the order they run (`HOME` in `kinds.rs`).
Never dials a worker. An absent home is reported and stays absent; a folder
that is not a LayerFS 2.0 home fails `home.layout` and its records are not
read. A check that does not apply is left out.

| Check | What is observed | Source |
| --- | --- | --- |
| `home.build`, `home.prerequisites` | The calling executable; native runtime prerequisites | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |
| `home.layout` | pass for a current layout; fail for a folder that is not a LayerFS 2.0 home (a `layerfs.json` of another layout or one that cannot be parsed, or anything but `layerfs.lock`, the integration folders and `.DS_Store` with no `layerfs.json`), "this build writes nothing in it", `fix: mv <home> <home>.old && layerfs home init`; fail with the OS error and no fix when the system refuses to read `layerfs.json` or the folder; left out for a home not made yet | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs) |
| `home.records` | The records scan and record checks (`checks::check_local`, on a current-layout home only), each finding as an error or warning on the thing it is about, with its fix; no verdict when the record inventory changed or could not be read; see [Checks](checks.md) | [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs), [`host_inventory.rs`](../../../../control/crates/layerfs-control/src/doctor/host_inventory.rs) |
| `home.bundle` | When a record names a non-native machine: the Linux bundle manifest (this version, `linux-<arch>`) and its files present | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |
| `home.native_daemon` | The daemon the native machine runs, as its Sandbox launch chooses it (`NativeDaemon`, shared with `home.get`): its record's `daemon`, else the one beside this host's bundle; read from files. Missing or not executable: an error for a recorded one, a warning for the default (`fix: chmod +x <path>` where only the mode is wrong); its stamped commit (`layerfs-build-identity:`) against the calling build's (`Options.build`): another commit is a warning in `DaemonMismatch`'s words; unknown or dirty passes, saying it cannot compare | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs), [`home.rs`](../../../../control/crates/layerfs-control/src/home.rs) |
| `home.artifacts` | Each non-native machine's recorded `daemon` executable | [`host_install.rs`](../../../../control/crates/layerfs-control/src/doctor/host_install.rs) |
| `home.capacity` | `/bin/df -Pk` on the home or its nearest existing ancestor, capped at 5 s, killed and reaped when late; under 2 GiB free is a warning (`capacity::room`), evidence `disk_free_bytes` | [`host_capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/host_capacity.rs), [`capacity.rs`](../../../../control/crates/layerfs-control/src/doctor/capacity.rs) |

The inventory fingerprints only management paths (records, `worker.json`,
`control.json`, lease stamps under `open/`, `store.sqlite.incoming`, project,
Workspace and command records) before and after the records scan. A symlink
or an unreadable path makes it fail. It never descends into daemon runtime
trees, mounts, logs or SQLite payloads, and it is not a transaction: a
change undone between the two reads is not seen.

### Machine

Ten checks, in order (`MACHINE` in `kinds.rs`). What a probe finds is read
as `machine get --with live` reads it (`layerfs_machine::state`). A check
that does not apply is left out: `native` has no provider, worker or data
disk, and a stopped machine's worker checks are not made. After a failed
check the ones that need it are not made, and the machine blocks what is on
it with the reason found.

| Check | What is observed | Source |
| --- | --- | --- |
| `machine.record` | `v`, name, platform, state, sizes and port; a broken record fails, `fix: layerfs machine remove <m>` | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.operation` | A running `op` is `busy` (no verdict) and stops the probes; an unresolved one is an error whose fix is `layerfs_machine::state::settle` (`layerfs machine remove <m>` after a create or remove, else `layerfs machine get <m> --with live`); a certain one a gone owner left holds nothing | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs), [`state.rs`](../../../../control/crates/layerfs-machine/src/state.rs) |
| `machine.configuration` | Its record makes a valid request to its provider (the home's id included) | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.provider` | The provider asked of the VM alone, capped at 10 s: running or stopped pass; a Docker Engine that does not answer is `engine_down` (fix starts Docker); a VM the provider does not hold is `missing` (`fix: layerfs machine remove <m>`); one it reports broken `failed` (`fix: layerfs machine start <m>`) | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs), [`calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs) |
| `machine.state` | Recorded state against the observed one; a record a live read would change is a warning, `fix: layerfs machine get <m> --with live`; the record is not refreshed | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.worker.endpoint` | Saved loopback address, nonzero port, nonempty secret | [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.worker.auth`, `.compatibility` | One authenticated hello within 5 s returning `ready: true` (`not_answering`, `fix: layerfs machine start <m>`); protocol and sealed bundle match this build (`outdated`, fix `replace_worker`: stop and start, the stop forced while Sandboxes are recorded on it) | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs), [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |
| `machine.worker.ping`, `machine.capacity` | `worker.ping` with `capacity: true`: kind `worker`, positive pid, this version, and the data disk's free bytes; `machine.capacity` is a warning under 2 GiB free (`capacity::room`), its evidence adding `reclaimable_image_bytes` from the machine's image listing (what `machine prune --unused` frees), and that prune is its fix only when it is above 0 | [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs), [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |

### Sandbox

`sandbox doctor` follows the saved route without `sandbox.get`, deployment,
Store open, lease, record refresh, new forward or lifecycle change, in the
17 frozen checks' order. Its records checks always run. A machine that is
not ready keeps every live check from being made: the Sandbox is one
`not_checked` entry with `blocked_by` its machine (and its node `coverage`
`none`). A running owner's operation is one with `reason` `busy`. Each live
check runs only when the one it needs did not fail; one that cannot apply
is left out. A failed check's `fix` is `layerfs sandbox remove <sandbox>`.
The Sandbox's records state comes from one function in `sandbox_target.rs`
(`recorded_state`), the place to switch to the Sandbox entity's own state.

| Check | What is observed | Source |
| --- | --- | --- |
| `sandbox.record`, `.machine_reference`, `.store_reference`, `.project_reference`, `.store_placement` | Record and the records it names; a sqlite Store must be on the Sandbox's machine | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| `sandbox.operation` | Live owner `pass`; an ended owner's certain `op` a notice, its uncertain one an error | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |
| `sandbox.runtime`, `.forward_process` | Native or namespaces daemon/init process, or a forward's carrier, matched by pid, start time and boot; never by pid alone. A runtime seen gone is not dialled | [`sandbox_runtime.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_runtime.rs) |
| `sandbox.route`, `.worker_compatibility` | Native: direct. Forward: the machine's `worker.json` must exist (else `not_checked`, blocked by the machine); no worker call. Other machines: one request of the internal worker op `doctor.sandbox` to an authenticated worker of this build, whose report (checks, `workspaces`, `listing`) is read in; a malformed or cut report leaves the Sandbox `not_checked` with `reason` `lost` | [`sandbox_remote.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_remote.rs) |
| `sandbox.daemon_keys`, `.daemon_connect`, `.daemon_auth`, `.daemon_instance`, `.daemon_ready` | Saved loopback endpoint and 32-byte keys; a refused connection is an error (nothing listens), any other failure `not_checked`; authenticated channel; `Hello` without waiting for startup must name the saved instance and `ControlReady` (still starting: warning; retained: error) | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |
| `sandbox.session` | `EndSession` for the doctor's own control session, only after a clean exchange | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |
| `sandbox.observation` | The saved keys file did not change while the daemon was read | [`sandbox_target.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_target.rs) |

**Workspaces.** A Workspace has no doctor of its own. After a ready daemon,
`sandbox.doctor_workspaces` is read in pages of at most 32 rows under one
generation ([`diagnostic.rs`](../../../../engine/crates/layerfs-daemon/src/control/diagnostic.rs)); each Workspace the daemon lists or the Sandbox's folder
records (the home's on `native` and for a forward, the worker's own
elsewhere) gets one shallow node named `<sandbox>:<name>` (its name, else
its path, else its id), recorded ones first: `workspace.mounted` (the mount
is present; being made or removed is a notice; held without a mount, or
recorded but not held, an error) and `workspace.activity` (`failed` or
`uncertain` is an error). Both fixes are `layerfs workspace unmount
<workspace> --discard --force`. A list that gave no verdict (`Busy`, a
changed generation, a lost reply, the deadline) leaves each Workspace
`not_checked` with `busy`, `lost` or `timeout`, or the Sandbox when none
is recorded. A daemon that is not seen ready blocks its Workspaces: each is
`not_checked` with `blocked_by` the thing at the root (`sandbox` with
`lost`, `unreachable`, `missing`, `not_answering`, `failed` or `busy`, or
the `machine` whose worker does not answer or is `outdated`), and none is
tried ([`workspace_target.rs`](../../../../control/crates/layerfs-control/src/doctor/workspace_target.rs)). The runner adds the Workspaces the daemon listed to those the
home records before the first is checked, so `--progress` sends `self`,
then one `workspaces` event each.

### Store

Seven checks in `CHECKS` order inspect Store records and mount admissions.
Object contents are outside this check. A sqlite Store is reached where it lives: in this process
for `native`, else through its machine's worker (the internal worker op
`store.diagnose` with `cut: schema`, connect 5 s). A PostgreSQL Store is
reached from this computer, then once more from each machine with a Sandbox
of it and from `--machine`, one `store.connection` row each with `from`.
Under a machine that is not ready only `store.record` is made: the rest are
`not_checked`, blocked by the machine.

| Check | What is observed | Source |
| --- | --- | --- |
| `store.record` | Its record reads and names it, its engine and its id; a PostgreSQL record that alone makes the Store `broken` (no saved login, one the home does not hold, or old fields, V23-6) fails with the `fix` `store list` gives (LF12) | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| `store.operation` | The record's `op`: running passes; one whose owner is gone with its outcome unknown is an error, `fix` the clearing call (`store move <s> <to>`, `store remove <s>`, `store compact <s>`, else `store get <s> --with live`); a stale lease is a notice. For a sqlite Store also the `<store>.compact-op` record beside its file, read through its machine: a live run passes, one left unresolved fails with `fix: layerfs store compact <store>` | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs), [`intent.rs`](../../../../control/crates/layerfs-store/src/intent.rs) `clearing` |
| `store.connection` | A sqlite Store's file is where its record says, read through its machine; a PostgreSQL Store's server answers with the saved login; connect and login get 5 s, and a server silent past that fails the check, recorded as this computer's reach without a second dial (LF13) | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |
| `store.identity` | The id in its file is the one in its record; when it is not, `store.schema` is not made | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs), [`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs) |
| `store.schema` | Its schema is the one this build reads | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs), [`diagnostic.rs`](../../../../control/crates/layerfs-store/src/diagnostic.rs) |
| `store.mount_holders` | Checks durable mount admissions against the recorded sandbox and workspace. See [Historical mount holders](checks.md#historical-mount-holders). | `mount_holders.rs` |
| `store.backups` | sqlite only: each bucket link's backup heads against the Store's; one behind is a notice, `fix: layerfs backup create <store> <bucket>` | [`store_target.rs`](../../../../control/crates/layerfs-control/src/doctor/store_target.rs) |

### Project

| Check | What is observed | Source |
| --- | --- | --- |
| `project.record` | The project's record and its Store's record read | [`project_target.rs`](../../../../control/crates/layerfs-control/src/doctor/project_target.rs) |
| `project.listed` | The Store's own list (read where its calls run) holds the record's stack id, or its name for a record without one. Not listed: a notice, `fix` `layerfs project delete <project>`, which removes such an entry. A Store this host cannot open or whose file is gone, and a worker that does not answer (`machine`, `not_answering`), block it | [`project_target.rs`](../../../../control/crates/layerfs-control/src/doctor/project_target.rs), [`live.rs`](../../../../control/crates/layerfs-store/src/project/live.rs) |
| `project.mount_holders` (`--full` only) | Checks admissions affecting this project using its stack and branch identities. | `mount_holders.rs` |
| `project.data` (`--full` only) | Everything main's head and each live Branch's head reach, read and checked once per project in one pinned read view: each tree's namespace qualified, each object located, reconstructed, admitted and its children's context checked, as the whole-Store scan checks its rows ([Store diagnostics](../../engine/store/diagnostics.md#one-projects-data)). sqlite on this kernel and PostgreSQL in this process; sqlite on a machine through its worker's `store.diagnose` with `cut: project`, its `data` events passed back. The first 64 problems are findings (an unreadable object `not_checked` `lost`), the rest one counted finding | [`project_target.rs`](../../../../control/crates/layerfs-control/src/doctor/project_target.rs), [`diagnostic.rs`](../../../../control/crates/layerfs-store/src/diagnostic.rs) |

`project.data` runs only after `project.listed` passed and only for the
named project: a shallow check (`store doctor`'s of each project) never
makes it. It writes nothing to the Store and holds no lease; the Full
check has no time budget.

### Backup

Uses the bucket's own S3 configuration, signer, TLS policy and HTTP codec,
with the bridge's bounded resolver. The write test is the bucket's Full
check (`backup doctor --full`).

| Check | What is observed | Source |
| --- | --- | --- |
| `bucket.configuration`, `bucket.credentials` | Record version and name, provider `r2` or `minio`, endpoint, region, bucket, prefix; credential reference resolves (values never shown) | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| `bucket.transport`, `bucket.list` | One signed ListObjectsV2 under the prefix with `max-keys=1`; keys discarded | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |
| `bucket.write_capability` | `--full` only: versioning must read `Enabled`, else no PUT | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |
| `bucket.put`, `bucket.reconcile` | `--full` only: one single-part PUT with `If-None-Match: *` at `<prefix>/_layerfs_doctor/<64-hex token>/probe`; after an uncertain PUT, a GET of that key (token, bytes, version) instead of a resend | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| `bucket.get`, `bucket.delete` | `--full` only: GET and DELETE of the owned version only; a failed GET does not stop the DELETE | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |

Evidence keeps `cause`, `http_status`, `phase` (`dns`, `tcp`, `tls`,
`connection_before_send`, `exchange_after_send`, `http_response`) and an
allowlisted `s3_code`; never response bodies or listed keys. A DELETE or
reconcile that fails leaves `possible_retained_key` and `owned_version`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A doctor changes nothing: no lock, lease, record write, conversion, stale-lease removal, machine start or deployment; an absent home stays absent (`unopened`) | [`mod.rs`](../../../../control/crates/layerfs-control/src/doctor/mod.rs), [`host_target.rs`](../../../../control/crates/layerfs-control/src/doctor/host_target.rs), [`calls/doctor/mod.rs`](../../../../api/crates/layerfs-api/src/calls/doctor/mod.rs) |
| Each thing runs under its own budget; a Full check has none. No probe outlives its thing's budget (`Budget::limit`) | [`frame.rs`](../../../../control/crates/layerfs-control/src/doctor/frame.rs) `visit`, [`target.rs`](../../../../control/crates/layerfs-control/src/doctor/target.rs) |
| A thing whose parent is stopped or failed is not tried: one `not_checked` node with `blocked_by` the root of the block | [`frame.rs`](../../../../control/crates/layerfs-control/src/doctor/frame.rs), [`kinds.rs`](../../../../control/crates/layerfs-control/src/doctor/kinds.rs) |
| `ok` is true only when every check was made and nothing was found; a notice makes it false. A `not_checked` entry has `blocked_by` or `reason`, never both | [`report.rs`](../../../../control/crates/layerfs-control/src/doctor/report.rs), [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill` |
| Every check id a kind gives is in its kind's list (a debug build panics otherwise); the lists are compared with `calls.json` `doctor.checks` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) `fill`, [`doctor_frame.rs`](../../../../control/crates/layerfs-control/tests/doctor_frame.rs) |
| One attempt: a lost worker, daemon or S3 reply is `not_checked` `lost`; nothing is resent, replayed or retried | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs), [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs), [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| A doctor's worker receives only the internal ops `worker.ping`, `doctor.sandbox` or `store.diagnose`, after a compatible hello, and answers from its fixed root, never a caller path | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs), [`ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) |
| The backup write test writes only after versioning is `Enabled`, and deletes only the version it owns; a refused or colliding PUT authorizes no delete | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| Secrets and arbitrary record fields are not reported | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs), [`machine_target.rs`](../../../../control/crates/layerfs-control/src/doctor/machine_target.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| A thing not in the home | `NotFound` ("no `<kind>` named `<name>`"), exit 125; a machine only a Store or Sandbox record names is not there (that record's own check reports the reference) | [`kinds.rs`](../../../../control/crates/layerfs-control/src/doctor/kinds.rs) `locate` |
| `store doctor --machine` on a sqlite Store | `InvalidInput` | [`store/doctor.rs`](../../../../api/crates/layerfs-api/src/store/doctor.rs) |
| A wrong command line | exit 2 | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |
| A report, whatever it finds | exit 0 | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |
| Retained checks, message, evidence | 1024 checks, 4096 characters, 16 KiB; beyond them `truncated` | [`check_report.rs`](../../../../control/crates/layerfs-control/src/doctor/check_report.rs) |
| Budgets per thing | home, machine, Sandbox 30 s; Store, project, Workspace, bucket 10 s; a Full check none | [`kinds.rs`](../../../../control/crates/layerfs-control/src/doctor/kinds.rs) `KINDS` |
| Probe caps | `df` 5 s; provider 10 s; worker hello 5 s; Store worker connect 5 s; bucket probe 10 s, 30 s with the write test | target sources above |
| Frames | hello and ping 4 KiB (`MAX_HELLO`); diagnostic request and answer frames 64 KiB (`TooLarge`) | [`diagnostic.rs`](../../../../control/crates/layerfs-machine/src/worker/diagnostic.rs) |
| Daemon registry held | `Busy` makes each Workspace `not_checked` `busy`; `Changed` or `UnknownMethod` `lost`; no retry | [`sandbox_channel.rs`](../../../../control/crates/layerfs-control/src/doctor/sandbox_channel.rs) |
| Build without `tls` for R2 or `https://` | `bucket.configuration` gives no verdict; nothing sent | [`backup_target.rs`](../../../../control/crates/layerfs-control/src/doctor/backup_target.rs) |
| R2 versioning | Not `Enabled` through its S3 API: list checks run, the write test stays unsupported | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs) |

## Demonstration

Commands are written from source and were not run while writing this page.
Give each a wall limit of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`doctor_frame.rs`](../../../../control/crates/layerfs-control/tests/doctor_frame.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_frame` | any | The one runner with made-up kinds: a budget that runs out is `timeout`, a failed parent blocks what is on it without calling it, one progress event per finished thing, counts and status; every kind's check ids against `calls.json` `doctor.checks` |
| [`doctor.rs`](../../../../control/crates/layerfs-control/tests/doctor.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor` | macOS, Linux | `home.records`: one finding per planted problem with its fix; a foreign home (a `layerfs.json` of another layout) fails `home.layout` with the move fix and is left as it was; nothing written |
| [`sweep_run_root.rs`](../../../../control/crates/layerfs-control/tests/sweep_run_root.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test sweep_run_root` | macOS, Linux | The run root sweep removes an orphan run folder and an empty private root and keeps every owned entry (a recorded folder, a live daemon's, no or a fresh `owner` marker, a create in flight, a private root beside its folder or holding something, the image cache, a link's target); a worker sweeps when it starts and before a `sandbox create` |
| [`init_copy_hold.rs`](../../../../control/crates/layerfs-store/tests/init_copy_hold.rs) | `cargo +1.85.1 test --locked -p layerfs-store --test init_copy_hold` | macOS, Linux | A `project.stage` discard leaves a folder copy an Init holds and removes a free one |
| [`image_sweep.rs`](../../../../control/crates/layerfs-sandbox/tests/image_sweep.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test image_sweep` | macOS, Linux | The `image.artifact` step `sweep` removes the unpack temporaries of free images and keeps a live copy's and a running unpack's |
| [`sweep_points.rs`](../../../../api/crates/layerfs-api/tests/sweep_points.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test sweep_points` | macOS, Linux | A `sandbox create` on `native` sweeps its run root first; the next `backup create`, or a `backup restore` that replaces the Store, removes stale staged snapshots, and keeps them while a create holds the Store's create lock |
| [`worker_fault_store.rs`](../../../../control/crates/layerfs-control/tests/worker_fault_store.rs) | `cargo +1.85.1 test --locked -p layerfs-control --features worker-fault --test worker_fault_store` | macOS, Linux | A `project init` whose answer is lost names its copy in `log/uncertain.jsonl`; `project delete` of the project removes the copy and logs it in `log/sweep.jsonl` |
| [`doctor_host.rs`](../../../../control/crates/layerfs-control/tests/doctor_host.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_host` | macOS, Linux | The home's own checks: absent home stays absent; no worker dial; concurrent record rewrites give no verdict |
| [`doctor_report.rs`](../../../../control/crates/layerfs-control/tests/doctor_report.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_report` | any | Checks with no verdict are kept apart from established failures; retention bounds |
| [`doctor_machine_target.rs`](../../../../control/crates/layerfs-control/tests/doctor_machine_target.rs) | `cargo +1.85.1 test --locked -p layerfs-control --features machine --test doctor_machine_target` | macOS, Linux | The ten machine checks with a stand-in `docker` and workers: `engine_down` and its fix, no worker checks on a stopped machine, an outdated worker's forced stop, a nearly full data disk; the home unchanged |
| [`doctor_sandbox.rs`](../../../../control/crates/layerfs-control/tests/doctor_sandbox.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_sandbox` | macOS, Linux | Real authenticated channel; the 17 Sandbox checks then one node per Workspace; `Busy`, `Changed`, lost reply give no verdict and nothing is replayed |
| [`store_doctor.rs`](../../../../control/crates/layerfs-control/tests/store_doctor.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test store_doctor` | macOS, Linux | The Store's checks in the frozen order on this computer and through a real worker's `store.diagnose` |
| [`project_doctor_worker.rs`](../../../../control/crates/layerfs-control/tests/project_doctor_worker.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test project_doctor_worker` | macOS, Linux | `project doctor --full` of a project whose sqlite Store is on another machine: the list and the data walk run in its worker, `data` events passed back |
| [`doctor_backup_target.rs`](../../../../control/crates/layerfs-control/tests/doctor_backup_target.rs) | `cargo +1.85.1 test --locked -p layerfs-control --test doctor_backup_target` | macOS, Linux | S3 fault peer: no PUT without versioning, owned-version evidence after a lost PUT |
| [`doctor_targets.rs`](../../../../api/crates/layerfs-api/tests/doctor_targets.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test doctor_targets` | macOS, Linux | The six doctors through the call table, short and long forms; a thing not there is refused; no doctor makes or converts a home; one progress event per finished thing |
| [`doctor_home_machine.rs`](../../../../api/crates/layerfs-api/tests/doctor_home_machine.rs) | `cargo +1.85.1 test --locked -p layerfs-api --features machine --test doctor_home_machine` | macOS, Linux | `home doctor` and `machine doctor` with a stand-in `docker` and worker: a clean home is `{"ok":true}`; a stopped machine blocks what is on it |
| [`doctor_sandbox_calls.rs`](../../../../api/crates/layerfs-api/tests/doctor_sandbox_calls.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test doctor_sandbox_calls` | macOS, Linux | `sandbox.doctor` with a daemon that lists its Workspaces, and with one that is lost |
| [`project_doctor_full.rs`](../../../../api/crates/layerfs-api/tests/project_doctor_full.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test project_doctor_full` | macOS, Linux | `project doctor --full`: a damaged object is a `project.data` error in each project that reaches it; nothing written |
| [`doctor_targets.rs` (CLI)](../../../../api/crates/layerfs-cli/tests/doctor_targets.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test doctor_targets` | macOS, Linux | Each doctor's help, a wrong command line and a missing thing never make a home; bad flags exit 2 |
| [`doctor.rs` (CLI)](../../../../api/crates/layerfs-cli/tests/doctor.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test doctor` | Unix | `layerfs home doctor` prints one report (JSON or `--text`), exits 0 and changes nothing; `home leftovers` and `home prune` exit 2 |

The stand-ins and fault peers check the observation boundary; they do not
qualify live Docker, Lima, Apple Container, R2 or MinIO installations.

## Pages

| Page | What it covers |
| --- | --- |
| [Checks](checks.md) | The home's records check (scan and record checks), the report and its node rows, and where LayerFS removes leftovers itself |
