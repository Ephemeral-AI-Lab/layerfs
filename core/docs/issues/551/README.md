# #551 B0 machine/backend contract — implementation record

> **Status:** Implemented on LayerFS 2.0 (`layerfs_v2_0_0` line), branch
> `agent-b0ext2` forked from `agent-b0iso`; not published to `main`.
> This page is the audit and the delivery record for
> [#551](https://github.com/Ephemeral-AI-Lab/layerfs/issues/551), the B0
> slice of [#545](https://github.com/Ephemeral-AI-Lab/layerfs/issues/545).
> The design is [#533](https://github.com/Ephemeral-AI-Lab/layerfs/issues/533);
> the platform work it replaces is [#529](https://github.com/Ephemeral-AI-Lab/layerfs/issues/529)
> and PR [#530](https://github.com/Ephemeral-AI-Lab/layerfs/issues/530).

## 1. Baseline audit (B0-138)

The baseline was read at `agent-b0iso` (`12fb6f6d`), which already carries
the #551 isolation work: B0-144, B0-146…B0-149, B8-164/B8-165 and
B10-178/B10-179. This issue adds the contract, the capacity and schema
version, the remaining read models, the host checks and the contract tests.

### #533's layer map on this tree

| #533 term | This tree | What it owns |
| --- | --- | --- |
| Machine | `api/crates/layerfs-control/src/machine/`, `connect/crates/layerfs-machine/` | `machine/<name>/record.json`, `worker.json`, `start.lock`; one adapter per platform; the per-platform `Profile` table |
| Sandbox | `api/crates/layerfs-control/src/sandbox/`, `connect/crates/layerfs-sandbox/` | `sandbox/<name>/record.json`, daemon keys, Workspace records; one daemon per Sandbox |
| Backend | `connect/crates/layerfs-sandbox/src/backend/` (`docker`, `host_process`, `namespaces`) | how work runs inside a Sandbox; one typed `ProviderCapabilities` per backend |
| Isolated value | `connect/crates/layerfs-sandbox/src/types/isolation.rs` | `IsolationClass`, `ExecMode`, `KernelScope` |
| Store | `api/crates/layerfs-control/src/store/` | `store/<name>/record.json`, leases, projects |
| Worker | `api/crates/layerfs-control/src/machine/worker/` | the one process on a kernel the caller cannot open; one request per connection |

The layer guard (`tools/check_product_boundary.py`) still lists these
crates; no crate was added, so its map is unchanged.

### What #529/#530 did, and how #533 moved it

#529/#530 (both still open) made `sandbox.create({ platform })` the caller's
choice: `platform` was a per-Sandbox string (`docker`, `mac`, `linux`,
`apple-container`, `microsandbox`, `lima`, `firecracker`, `ssh`) in the
Sandbox record, and every later call followed that record. #533 keeps the
recorded platform but moves the decision one layer up: **compute is
machines**, a Sandbox binds to one machine, and the machine's platform is
its kind. On this tree:

- `machine` is the call argument and the record; `platform` is the machine's
  adapter, recorded on the machine and copied to the Sandbox record as its
  runtime fact (`sandbox.create` checks `platform` against the machine's
  backend and records it, #529).
- `isolation_class` is the Sandbox's boundary, derived from the machine's
  platform (or the Sandbox's backend for an old record). `kind` in a machine
  read model means the isolation class only; a provider name never becomes
  a kind.
- `exec_mode` separates "runs our worker" from "the provider execs"; #529's
  eight platforms are now machine kinds, and a provider is a constructor, not
  a new layer.
- `mac` and `linux` remain the platform names of the `native` machine's
  process backend (the backend's stored names are `native` and `namespaces`);
  no top-level `process` machine was introduced.
- Storage placement and the `StoragePlacement` refusal stay with #552; #551
  only records and shows the contract.

The one difference from #529's text is deliberate and is #533's model: a
Sandbox no longer chooses a raw platform on its own, it names a machine, and
a data-carrying (sqlite) Store limits that machine to its Store's machine.
The platform is still recorded and still decides every later call.

## 2. Gate alignment (B0-143)

#533 §3 lists 17 owner gates. The contract in this issue is the **shape** of
the records and read models; it does not choose a default that a gate owns.

| Gate | State on this line | What #551 does |
| --- | --- | --- |
| G0 base (#528/#530 merge) | open; this branch is on the 2.0 line | The audit above records the #529/#530 delta; no merge is claimed |
| G1 direction | accepted as the 2.0 direction by the branch's work | follows it |
| G2 wording | applied in `AGENTS.md` on the 2.0 line | none here |
| G3 #531 | held; #533 supersedes it | no VM-per-Store in the contract |
| G4 #521 | open | no Python/MCP work here |
| G5 library/crate layout | accepted | no new crate |
| G6 PyO3 | open | no new dependency |
| G7 progress | stream | `sandbox.get` stays a read model, no job registry |
| G8 backups | accepted | not touched |
| G9 removed calls | accepted | no call removed here |
| G10 `machine update` | accepted | the doctor's Docker/image fixes name `machine update` |
| G11 bundle | open | the doctor reports a missing bundle, never installs it |
| G12 Firecracker kernel | open | the doctor only probes the host tool |
| G13 PostgreSQL Sandboxes | `native`/`docker` now | unchanged |
| G14 old settings/homes | later step | the record migration here is read-only fallback |
| G15 naming | `fs`, `workspace.status`, snake_case | contract fields are snake_case |
| G16 Linux native | removed | no Linux `native` backend is reintroduced |
| O36 image | bundle image | recorded as the docker default; doctor reports it |

Two gates are load-bearing for later defaults, not for this shape: **D2**
(the default image/backend decision) and the provider gates D6/D12. The
contract can land first, which is what this branch does.

## 3. Boundaries (B0-139)

The three types now have one owner each; a field lives where it is written
and never in two places.

| Type | Owns | Does not own |
| --- | --- | --- |
| `MachineRecord` (`machine/record.rs`) | name, platform (the adapter), the #551 contract (`isolation_class`, `exec_mode`, `kernel`, `schema_version`), size, lasting state, `op`, built-ins' settings (`image`, `docker`, `daemon`, `wsl`, `host`, `nested`, `ssh`, `loaded_image`) | Sandbox or Store facts; a live probe answers `health`, not the record |
| `SandboxRecord` (`sandbox/record.rs`) | name, project, machine, the requested/effective class, `exec_mode`, the capacity contract, backend/runtime/image facts, `schema_version`, one `op` | the machine's class (copied at create, never re-read from the machine); any state (derived) |
| `ProviderCapabilities` (`connect/.../types/capabilities.rs`) | what one backend declares: class, kernel, mount driver, image need, search policy, resource limits, direct Store access, idle stop, snapshot, `schema_version` | which machine provides it; that is `Platform`/`Backend`'s table |
| `Platform`/`Profile` (`connect/crates/layerfs-machine/`) | the adapter facts: name, host, backend, kernel route, executor, options | a Sandbox's request or history |

`common/contract.rs` holds what the two read models share (`SCHEMA_VERSION`,
`Capacity`, the budget constants); `machine/capability.rs` maps a machine to
its backend's declaration; `common/isolation.rs` keeps the one legacy name
map used only to read a pre-#551 record. A read model never branches on a
provider name: it reads the class and the capability.

## 4. The contract

### 4.1 Records and `schema_version` (B0-152)

`SCHEMA_VERSION` is `1`. A new machine record (`MachineRecord::built_in` →
`with_contract`) and a new Sandbox record (`SandboxRecord::creating`) write
`schema_version: 1`. A capability writes its own
`CAPABILITY_SCHEMA_VERSION` (`1`).

An old record is **read** through explicit fallbacks, never rewritten:

- `MachineRecord::isolation_class` falls back to the platform's declared
  class (`common/isolation.rs` for a build without the machine feature);
  `exec_mode` falls back to `worker`, `kernel` to the class's scope;
  `schema_version` reads `1`.
- `SandboxRecord::requested_isolation`/`effective_isolation` fall back to
  each other and to the `platform`/`backend` name; `capacity` falls back to
  the build's contract; `schema_version` reads `1`.
- An explicit write migrates: `MachineRecord::with_contract` and
  `SandboxRecord::with_contract` add the missing fields (and are tested).
- `doctor` does not rewrite; the read-model `schema_version: 1` is how a
  caller sees that the record was read through the fallback.

### 4.2 Typed capability (B0-145)

`ProviderCapabilities` is a typed struct, not a `HashMap<String, bool>`:
`schema_version`, `isolation` (`IsolationModel` → `IsolationClass`),
`mount_driver` (`Fuse`, `LoopbackNfs`, `SharedFs`, `None`), `idle_stop`
(`Ours`, `Provider`, `None`), `snapshot: Option<SnapshotCapability>`, plus
its existing name, image, search-path, limits and direct-access flags.
`machine.get`/`machine.list` show it as `capability`, with the class,
kernel, strength word, provider, backend, mount driver, idle stop, snapshot,
notes and its schema version. Every field is snake_case.

The adapters are thin: `backend/docker|host_process|namespaces/adapter.rs`
construct the declaration; `machine/capability.rs` reads it; no read model
names a provider.

### 4.3 Requested/effective (B0-144/146/147/148/149, prior head)

Already on `agent-b0iso`: `IsolationClass`, `ExecMode`, `KernelScope`; the
machine record writes the class/exec mode/kernel; `sandbox.create` defaults
to `docker`, makes `process` opt-in, and refuses a class the machine cannot
provide with `Unavailable { requested, available }` (never a downgrade); the
Sandbox record keeps both classes. This branch only surfaces them in the
new read models and tests.

### 4.4 Per-Store quota, in-flight limit and queue bound (B0-151)

`common/contract.rs` holds the one contract a Sandbox records and a read
model reports:

```text
capacity: { schema_version: 1, per_store_quota: 4, in_flight_limit: 8,
            queue_limit: 16, in_flight: <measured>, queued: <measured> }
```

- `per_store_quota` is how many Stores one machine worker serves at once.
- `in_flight_limit` is how many calls one Store's daemon admits at once.
- `queue_limit` is how many callers may wait beyond that.
- A caller waiting for a control connection now refuses `Capacity` when the
  queue is full **or** its read budget runs out; the refusal detail carries
  `resource`, `store`, `in_flight`, `queued`, both limits, `waited_ms` and
  `retryable`. It is no longer `Busy`, and queue time is charged to the
  caller's 5 s read budget.
- The client keeps at most six connections per Sandbox and the daemon admits
  eight plus the installer slot; the contract is the bound a caller sees, not
  a second pool.

## 5. B8 read models

| Box | Change |
| --- | --- |
| B8-156 `machine list/get` | `kind` (isolation class), `schema_version`, `capability`, `worker` (`state`/`address`/`started`/`forward_pid`, from `worker.json`, no dial), `lease` (`start.lock`: `held`, `path`, `since`, probed without taking it), `health` (`ok`, `state`, `reason`) |
| B8-157 `sandbox list/get` | `capacity` on both; `get` measures `in_flight`/`queued` when the process owns the Sandbox; `state`, `project`, `machine`, `backend`, `image` were already there |
| B8-158 workspaces | each mounted Workspace now carries its `branch` (from its record); `sandbox.get` adds `running_command`, `running` (up to 8) and `recently_closed` (up to 16), recorded in `sandbox/<name>/activity.json` by the exec and unmount that ran |
| B8-159 bounded | machine live probes share a **2 s** budget (was 10 s); a Sandbox read waits at most **5 s** for its control connection; a read lends one connection, never all eight |
| B8-160 capacity | `sandbox.get`/`list` report the per-Store `in_flight` and `queue_limit`; a full queue is `Capacity` |
| B8-161 queue time | the wait for a connection is inside the 5 s read budget; past it, `Capacity` with `waited_ms` |
| B8-163 precise failures | reads name `Unavailable` (machine not reached) and `Capacity` (queue/budget) and never wait unbounded |
| B8-164/165 isolation/scheduling | already on the prior head, now covered by the contract tests |

`activity.json` is bounded and short-locked (`activity.lock`); an unreadable
file is no activity, never a failed `sandbox.get`. `recently_closed` is the
last closed Workspaces, not a history: a Workspace that is mounted again is
a new mount.

## 6. B10 doctor

One new check module, `doctor/host.rs`; the existing checks are unchanged.
Every check reads records, a socket or a tool's version; none starts a
machine or Sandbox and none writes the home (`doctor` still takes no lock).

| Box | Check |
| --- | --- |
| B10-171 Docker | a socket the docker machine **declares** (`machine update docker --docker`) must connect, else error; a default socket that exists but does not answer warns. A missing default on a host without Docker is not a finding |
| B10-172 VM provider | for a created machine that a Sandbox uses, the provider tool probe (`layerfs_machine::probe`, version query only) warns with `missing` when not ready. An unused machine is `machine.platforms`' business |
| B10-173 image | a Sandbox image that is a file must exist (error); a docker image digest the docker machine's record does not name warns |
| B10-174 mount driver | a recorded `process` Sandbox on a host missing the driver (`native_missing`: Linux root, `/dev/fuse`, cgroup v2; macOS `mount_fusefs`) warns once |
| B10-175 free disk | the home's free bytes below 1 GiB warn, naming `tools/cleanup.py check` |
| B10-176 worker/lease | a stopped/failed machine with a worker record, a running worker machine without one, and a recorded forward process that is gone each warn. It never dials; the build hello stays in `doctor/workers.rs` |
| B10-178/179 | already on the prior head: declared class vs the platform's, and requested vs effective |

Existing behaviour preserved: `ok` is false only for errors; a warning never
fails the report; a `findings` list always names the call that fixes it.

## 7. Acceptance 84–90

| # | Acceptance | Test |
| --- | --- | --- |
| 84 | `vm` requested, only `process` available → `Unavailable { requested, available }`, never a downgraded Sandbox | `layerfs-api/tests/contract.rs::acceptance_84_a_class_the_machine_cannot_provide_is_unavailable` (and `isolation.rs::a_class_the_machine_cannot_provide_is_unavailable_not_downgraded`) |
| 85 | `requested_isolation`/`effective_isolation` recorded and returned by `sandbox get` | `...::acceptance_85_requested_and_effective_are_returned`; the create path writes both in `SandboxRecord::creating` |
| 86 | `machine get`/`sandbox get` return `isolation`/`scheduling`; `doctor` checks the declared capability and warns only | `...::acceptance_86_read_models_and_doctor_capability`; `doctor.rs::a_machine_record_that_declares_another_class_warns` |
| 87 | full per-Store queue is `Capacity`; queue time inside 2 s/5 s | `layerfs-control/tests/contract.rs::a_full_queue_is_capacity_with_the_contract_bounds`; `acceptance_87_capacity_and_queue_bound`; the bound is enforced in `daemon/pool.rs::lend_within` |
| 88 | `schema_version` on record and capability; old record has an explicit migration path | `...::old_records_have_an_explicit_migration_path`; `acceptance_88_schema_version_and_old_record` |
| 89 | no `if provider == ...` in core; one naming convention | `acceptance_89_no_provider_branch_and_one_naming_convention` |
| 90 | `doctor` never starts a machine/Sandbox and never writes the home | `acceptance_90_doctor_never_starts_and_never_writes`; `doctor.rs` tree snapshots |

`contract_skeleton` (both test files) is B0-142: it lists the calls and the
fields the contract holds to, so a new field or call has one place to
register and cannot silently rot.

## 8. Checkbox ledger

| Box | State | Evidence |
| --- | --- | --- |
| B0-138 | Done | §1 baseline audit (this page) |
| B0-139 | Done | §3 boundaries; `common/contract.rs`, `machine/capability.rs` |
| B0-142 | Done | `contract_skeleton` in both contract tests |
| B0-143 | Done | §2 gate table |
| B0-144 | Done (prior head) | `types/isolation.rs`, machine/Sandbox records, `isolation.rs` tests |
| B0-145 | Done | typed `ProviderCapabilities`, `machine.get.capability`, `contract.rs` |
| B0-146 | Done (prior head) | `native`/`namespaces` are full backends; `process` opt-in |
| B0-147 | Done (prior head) | docker default, `process` opt-in |
| B0-148 | Done (prior head) | `Unavailable { requested, available }` |
| B0-149 | Done (prior head) | `requested_isolation`/`effective_isolation` |
| B0-151 | Done | §4.4; `contract.rs`, `daemon/pool.rs` |
| B0-152 | Done | §4.1; `schema_version` on both records + capability, migration tests |
| B8-156 | Done | `machine.list.rs` (`worker`, `lease`, 2 s probe), `record.rs` (`kind`, `health`, `schema_version`) |
| B8-157 | Done | `sandbox/record.rs` capacity, `sandbox/get.rs` measured capacity |
| B8-158 | Done | `sandbox/activity.rs`, `sandbox/get.rs`, `sandbox/dispatch.rs`, `workspace/serve.rs`; `sandbox_activity_records_running_and_recently_closed` |
| B8-159 | Done | `PROBE_BUDGET_MS`, `READ_BUDGET_MS`, `lend_within` |
| B8-160 | Done | `Capacity::refusal`, `daemon/pool.rs::lend_within` |
| B8-161 | Done | the same wait is charged to the 5 s budget |
| B8-163 | Done | `Unavailable` from `reach`, `Capacity` from the queue; no unbounded wait |
| B8-164/165 | Done (prior head) | `common/isolation.rs`, `record.rs` |
| B10-171…176 | Done | `doctor/host.rs`, `doctor.rs` tests |
| B10-178/179 | Done (prior head) | `doctor/checks.rs` |

Nothing in #551 is left unowned. B8-162 and B10-177 were already satisfied
by #535/#536; B0-140 is #548; B0-150 is #552; the provider adapters are #554.

## 9. Commits

| Commit | What it holds |
| --- | --- |
| `1294dcdf8704` `contract: schema_version, typed capability, capacity and read models` | `common/contract.rs`, the capability struct and adapters, machine/Sandbox record fields, the pool queue bound and `Capacity` refusal, the machine/Sandbox read-model fields, `sandbox/activity.rs`, `doctor/host.rs`, and the tests for them |
| the commit carrying this page | this page and the `machine`/`sandbox` skill pages |
| a follow-up `tests: prove the Sandbox activity read model` | the bounded activity test; this page's counts |

## 10. Verification

Run from the branch's workspace with `CARGO_TARGET_DIR=/tmp/layerfs-target`
and cargo 1.85.1:

```bash
cargo check -p layerfs-control -p layerfs-api --tests
cargo check -p layerfs-control -p layerfs-api --features machine --tests
cargo test -p layerfs-control --test contract
cargo test -p layerfs-control --test typed_records
cargo test -p layerfs-control --test doctor --features machine
cargo test -p layerfs-api --test contract --features machine
cargo test -p layerfs-api --test isolation --features machine
cargo test -p layerfs-api --test sandbox_machines --features machine
```

The sandbox and machine tests that need a live daemon, Docker, a PostgreSQL
server or a VM answer `NOT_RUN` without their settings and pass; they start
nothing.

Results on this branch (`cargo 1.85.1`, Linux guest, `CARGO_TARGET_DIR`):

| Command | Result |
| --- | --- |
| `cargo check -p layerfs-control -p layerfs-api --tests` (default) | PASS, no warnings |
| `cargo check -p layerfs-control -p layerfs-api --tests --features machine` | PASS, no warnings |
| `cargo test -p layerfs-control --test contract` | 4 passed |
| `cargo test -p layerfs-control --test typed_records` | 8 passed |
| `cargo test -p layerfs-control --test doctor` (default) | 8 passed |
| `cargo test -p layerfs-control --test doctor --features machine` | 10 passed |
| `cargo test -p layerfs-api --test contract --features machine` | 8 passed |
| `cargo test -p layerfs-api --test isolation --features machine` | 4 passed |
| `cargo test -p layerfs-api --test sandbox_machines --features machine` | 4 passed (live halves `NOT_RUN`) |
| `cargo test -p layerfs-sandbox --test runtime_enum --test conformance` | 11 passed |

The `layerfs-sandbox` run protects the capability struct change: every
backend still fixes its name, isolation model, mount driver and description.

## 11. Not verified

- The **live** daemon queue: the `Capacity` bound and the measured
  `in_flight` are proven at the contract/state layer and by the pool code;
  no real daemon was saturated in this pass.
- A **real Docker Engine** image lookup: the doctor checks the declared
  socket and the record, not `GET /images/<id>/json` (no second Engine
  client in doctor); a digest the record does not name is a warning.
- **VM provider** availability is proven through the adapters' probe but no
  VM was created or started here; the check is skipped for machines no
  Sandbox uses.
- The macOS mount-driver branch (`mount_fusefs` on `PATH`) is source-only
  here; the Linux branch is covered when the host lacks root/FUSE/cgroup v2.
- `Draining`/`Lost`, `StoragePlacement` and provider adapters are other
  issues (#548, #552, #554) and are not claimed here.
