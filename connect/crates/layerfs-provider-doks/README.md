# layerfs-provider-doks

The DigitalOcean Kubernetes (DOKS) provider adapter for LayerFS 2.0 (#554
acceptance 6, D6). It is **outside core**: no file under `api/`, `core/`,
`engine/crates/layerfs-bridge`, `connect/crates/layerfs-machine` or
`connect/crates/layerfs-sandbox` changes. The adapter produces the built-in
`ssh` machine's `Spec` and its typed capability; it adds no `Platform`,
`Backend` or `IsolationClass` variant.

## The machine shape

| Layer | Value | Where it lives |
| --- | --- | --- |
| `Machine.kind` (isolation class) | `remote` | `IsolationClass::Remote`, a typed value |
| `Machine.provider` | `doks` | adapter data (`record_fields`, `declaration`) |
| `platform` (the constructor core routes by) | `ssh` | the existing `Platform::Ssh` |
| `exec_mode` | `worker` | `ExecMode::Worker` |
| kernel scope | `separate-host` | `KernelScope::of_class(Remote)` |
| backend capability | `namespaces` | the ssh machine's backend declaration |

`Machine.kind` is the class, never a vendor name. `Platform::parse("doks")`
is `None`: the provider name is not a core enum, and nothing in core
branches on it.

## Environment

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `DOKS_NAMESPACE` | no | `default` | Kubernetes namespace of the pod. |
| `DOKS_POD` | **yes** | - | Pod that runs the machine's sshd and worker. |
| `DOKS_PVC` | **yes** | - | PersistentVolumeClaim mounted at the data root. |
| `DOKS_DATA_ROOT` | no | `/layerfs/data` | Guest path the PVC is mounted at. |
| `DOKS_SSH_HOST` | no | the pod IP from `kubectl` | ssh host or address. |
| `DOKS_SSH_USER` | no | `root` | ssh user. |
| `DOKS_SSH_PORT` | no | `22` | ssh port. |
| `DOKS_SSH_IDENTITY` | no | ssh-agent/default keys | key file. |
| `DOKS_KNOWN_HOSTS` | no | `$HOME/.ssh/known_hosts` | file holding the pod's host key. |

`DoksEnv::from_env()` resolves the pod IP with
`kubectl get pod -n <namespace> <pod> -o jsonpath={.status.podIP}` only when
`DOKS_SSH_HOST` is unset. `DoksEnv::from_pairs()` never calls `kubectl`;
tests pass every value, including `DOKS_SSH_HOST`.

`preflight(&kubectl, &env)` reads `{.status.phase}` for the pod and the PVC
and refuses anything but `Running` / `Bound`. It is a read-only pre-check;
the caller runs it before `connect`.

### Pod requirements

The pod must be a Linux guest the built-in ssh machine can drive:

- `sshd` reachable at `DOKS_SSH_HOST:DOKS_SSH_PORT` with the key or agent
  the caller configured, and its key in `DOKS_KNOWN_HOSTS` (`ssh` checks it
  strictly; `machine.connect` copies it to `machine/<name>/known_hosts`).
- The PVC `DOKS_PVC` bound and mounted at `DOKS_DATA_ROOT` in the pod. The
  adapter does not mount it; Kubernetes does. `/layerfs/data` is where the
  `ssh` machine expects the data disk.
- The `layerfs` bundle copied by the ssh adapter to `/layerfs/bundle` and the
  worker state at `/layerfs/state`; the pod needs enough privilege for the
  `namespaces` backend (its own mount, PID and IPC namespaces and a cgroup),
  typically a privileged pod or the equivalent security context.

## Mapping to the built-in ssh machine

`spec(name, work_dir, tools, &env)` returns exactly the `Spec` the built-in
`ssh` machine has:

```text
DOKS_NAMESPACE / DOKS_POD  -> vm_name "doks-<namespace>-<pod>" (plain, <=63)
DOKS_PVC / DOKS_DATA_ROOT -> SshSpec.data_root (the volume's mount point)
DOKS_SSH_HOST/USER/PORT   -> SshSpec.host/user/port
DOKS_SSH_IDENTITY         -> SshSpec.identity
DOKS_KNOWN_HOSTS          -> SshSpec.known_hosts
Spec.kind                 -> Platform::Ssh
Spec.image/os_image       -> None (the ssh machine copies the bundle)
Spec.state                -> Lasting::default()
```

`spec_from_env(name, work_dir, tools)` is the same call from the process
environment. `connect(&spec, &bundle)` calls `layerfs_machine::connect`, the
public machine call; core dispatches it to the existing `ssh` adapter. The
adapter never reads or writes a LayerFS home.

```rust
use layerfs_machine::{Bundle, Tools};
use layerfs_provider_doks::{capability, connect, record_fields, spec, DoksEnv};

let env = DoksEnv::from_env()?;
let spec = spec("doks-dev", work_dir, Tools::from_env(), &env);
let _fields = record_fields(&env);       // provider: "doks", kind: "remote"
let _cap = capability();                 // == the ssh machine's capability
let running = connect(&spec, &bundle)?;  // layerfs_machine::connect
```

## Zero core diff

`tests/zero_core_diff.rs` hashes every file under the core folders and
compares it with `tests/fixtures/core-files.sha256`, taken at the adapter's
base commit, `feat/551-b0@b5c5335b5` (the stacked #551 head the adapter lands
on). A changed, added or removed core file fails, and so does a core file that
names `doks` or `digitalocean`. Regenerate the fixture from a clean checkout of
that base commit with:

```sh
find api/crates/layerfs-control api/crates/layerfs-api \
     connect/crates/layerfs-machine connect/crates/layerfs-sandbox \
     engine/crates/layerfs-bridge core/crates \
     -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum
```

The only non-adapter changes are the crate's layer-map entry in
`tools/check_product_boundary.py` (tooling, not core) and the generated
`Cargo.lock` workspace member.

## Tests

```sh
cargo test -p layerfs-provider-doks
python3 tools/check_product_boundary.py --skip-sdk
```

`tests/capability_ssh.rs` proves the typed capability equals the ssh
machine's for every named host, that `kind` is `remote`, `exec_mode` is
`worker`, the `Spec` is `Platform::Ssh`, and the DOKS environment maps to
the ssh fields (including a stand-in `kubectl` for `preflight`/`pod_ip`).

## Acceptance 6 result

- **Zero core diff: provable.** The adapter is a lone new crate; the hash
  manifest and the branch diff show no core file changed. A new provider can
  be built against the public `layerfs-machine`/`layerfs-sandbox` contract
  without editing core.
- **The capability is identical to ssh: provable.** The adapter returns
  the same typed `ProviderCapabilities` the `ssh` machine's `namespaces`
  backend declares (mount driver `fuse`, idle stop `ours`, no snapshot).
  Its own `isolation` model is the backend's `MicroVm` (`vm`), while the
  machine's `Machine.kind` stays `remote`; `ssh` has exactly the same two
  levels, and the machine capability read model uses the record's class.
- **Selecting and displaying `doks` through core: not provable on this
  base.** `MachineRecord` names the adapter with the closed `Platform` enum
  (`platform`), and `machine.get.capability.provider` is
  `Platform::name()` (`ssh`). Core has no `provider` field or adapter
  registry, so `machine.create`/`machine.connect` cannot select `doks` and
  a `provider: doks` record would be refused `Failed: no known platform`.
  Making that work needs a core extension point (a string provider id or a
  registry), which is itself a core diff and belongs to a follow-up. This
  adapter therefore ships as a library a host can call directly with the
  public machine API; the final `provider: doks` read model is a documented
  gap, not a claim.

No live DOKS cluster, ssh host or `kubectl` was used: the adapter's
mapping is unit-tested with a stand-in `kubectl`, and `connect` is the
existing ssh machine's call.
