# #512 A VM Sandbox backend: one Linux VM per project, Sandboxes as namespaces inside it

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract. Owner decisions V1–V10 (§10) are open, and nothing here is
> implemented. Facts were read at `35d36575c1`. The evidence is the
> [#512 experiment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/512),
> run on 2026-10-30.

## 1. Summary

Today LayerFS has one Sandbox backend: a local Docker Engine. Each Sandbox is one
container ([`backend/docker/`](../../../../engine/crates/layerfs-sandbox/src/backend/docker/)).
Its daemon mounts the Workspace with FUSE, and commands run in the same
container as a nonroot identity. This proposal adds a second backend, so that
agent-sandbox platforms (microVMs, VM-per-container tools, cloud VMs) can host
LayerFS with every SDK operation Docker supports today.

The proposal in one paragraph: **each project (strictly, each Store) gets one
Linux VM.** Inside it, the existing worker runs the SDK with the Store opened
in-process, and a new **namespace backend** replaces Docker. A Sandbox becomes a
set of Linux namespaces with a cgroup, its daemon runs as root inside them, and
commands enter as a nonroot identity with no capabilities and no-new-privs. Only
a small **provider adapter** in `@layerfs/runtime` knows about the platform: it
creates the VM, attaches its Store disk, copies in the sealed bundle, starts the
worker and opens one private stream to it. Docker stays the default backend.

## 2. What #512 established

| Environment | FUSE and the privilege split | Store | Real LayerFS |
| --- | --- | --- | --- |
| Microsandbox (libkrun) | pass | the VM's ext4 disk volume | `calls_real` 15/15 through Docker in the VM |
| Apple Container | pass with `--cap-add SYS_ADMIN` | the named ext4 volume | 15/15 through Docker in the VM; **15/15 with no Docker** (below) |
| Daytona open source, local | pass (privileged sandbox) | a loop-mounted ext4 inside the sandbox | 15/15 through Docker in the sandbox |
| Lima VM, as a stand-in for EC2 | pass | ext4 | `sandbox_real`, `calls_real`, `runtime_real` pass through Docker in the VM |
| Firecracker guest | pass (`CONFIG_FUSE_FS=y` in its CI kernels) | a second ext4 drive | probes and nested Docker only |

**The no-Docker run.** A test-only stand-in answered the Engine API calls LayerFS
makes, with each "container" as namespaces inside an Apple Container VM. The
unchanged sealed bundle passed `calls_real.mjs inner workspace` (15/15) with no
`dockerd`, `containerd` or `runc` in the VM. It passed with user namespaces open
and with them blocked. The stand-in is not committed; its design is in
[the #512 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/512#issuecomment-6514298914).

Four results shape this proposal:

1. **VMs don't share a kernel or a Store disk.** A second VM can't attach a
   Microsandbox or Apple disk, and SQLite's WAL and the directory lock only
   coordinate within one kernel
   ([direct Store §2](../352/DIRECT_STORE.md#2-the-safety-condition-non-negotiable)).
   So a VM per Sandbox can't work; a VM per Store can.
2. **No platform blocks user namespaces by default.** Without Docker's seccomp
   filter, a command can create a user namespace and hold its own copy of the
   Workspace mount. `user.max_user_namespaces=0` closes this VM-wide.
3. **Docker's privilege split carries over.** Root mounts with `allow_other`. A
   uid that `setpriv` drops to no capabilities and no-new-privs can read and
   write the mount, but can't unmount it.
4. **Every platform has a private stream into the VM:**
   - a loopback port forward (Microsandbox `-p 127.0.0.1:…`, Apple `-p`);
   - an SSH forward (the Daytona gateway, or any Linux host);
   - an exec stdio bridge (Apple `container exec -i`);
   - vsock (Firecracker).

## 3. Shape

```
host (macOS, Linux or Windows)               project VM (Linux, one per Store)
┌───────────────────────────────┐           ┌──────────────────────────────────────────┐
│ caller → @layerfs/runtime     │  private  │ worker (server.js, real SDK, root)       │
│   provider adapter (vm/*.js)  │══stream══▶│  ├─ Store calls: in-process, Store on     │
│   useWorker(endpoint)         │  + secret │  │   /layerfs-store (ext4 disk)          │
└───────────────────────────────┘           │  └─ Workspace calls: namespace backend   │
                                            │       Sandbox 1: mnt+pid+ipc ns, cgroup  │
                                            │         layerfs-daemon (root) ─ FUSE     │
                                            │         commands: uid:gid, no caps, NNP  │
                                            │       Sandbox 2: …                       │
                                            └──────────────────────────────────────────┘
```

- **Inside the VM, everything is today's Linux topology.** The SDK opens the
  Store in-process on a local ext4 disk on the same kernel as the daemons, which
  is what [direct Store access](../352/DIRECT_STORE.md) allows. The only change is
  that Sandboxes are namespaces, not Docker containers.
- **The host reaches only the worker,** the way macOS and Windows already do
  ([`worker/server.js`](../../../../api/runtime/src/worker/server.js)). It uses
  one authenticated stream with one attempt per call and no replay. Daemon
  control ports never leave the VM.
- **The daemon doesn't change.** It still implements no launcher, namespace
  or command supervisor
  ([integration contract §6](../303/06-cluster-one-integration.md#6-store-visibility)).
  Command identity and mount visibility belong to the Sandbox backend, as today.

## 4. The namespace backend

### 4.1 What a Sandbox is

| Docker today | Namespace backend |
| --- | --- |
| a container from an image digest | a root directory unpacked once per pinned image digest (§4.7), with a per-Sandbox overlay upper layer |
| its own mount, PID, IPC and UTS namespaces, and a network namespace | its own **mount, PID and IPC** namespaces. Network: shared with the VM by default, or its own namespace (V5) |
| cgroup per container | **a cgroup v2 directory per Sandbox** (`/sys/fs/cgroup/layerfs/<id>`), used for stop (`cgroup.kill`) and for limits |
| Engine `--init` reaps orphans | the helper's init stage is PID 1 in the Sandbox's PID namespace and reaps |
| `CAP_SYS_ADMIN`, `/dev/fuse`, `Privileged: false` | the daemon runs as root in the VM. The Sandbox's `/dev` holds `/dev/fuse` and the basic devices only |
| seccomp denies user namespaces | V4: `user.max_user_namespaces=0` VM-wide, or a per-command seccomp filter |
| Store volume mounted at `/layerfs-store` | a bind mount of the VM's Store directory at `/layerfs-store` |
| daemon port published on host loopback | the daemon listens in the VM; only the worker (in the VM) dials it |

### 4.2 The contract, item by item

The B items are the Docker backend's steps, as listed in the #512 issue body.

| Item | Namespace backend |
| --- | --- |
| B1 Create | Validate the request, then record a creation intent (as `create.intent` does today). Make the root overlay, cgroup and record, and return a 64-hex identity in the existing `ContainerId` format |
| B2 Store | Bind the VM's Store directory. `kernel_guard.rs` runs unchanged and accepts only ext4, xfs or btrfs |
| B3 Upload | Write the executable (0755) and config (0600) into the stopped Sandbox's upper layer: plain file writes, no tar stream needed |
| B4 Start | Spawn the helper's init stage, which unshares the namespaces, mounts the root, the Store and `/dev`, pivots root and runs `layerfs-daemon --config …` as root |
| B5 Startup log | The daemon's stdout and stderr go to a bounded log file per Sandbox. The listener wait reads the same marker `listener.rs` reads |
| B6 Endpoint | `127.0.0.1:<port>` in the VM (shared network) or the Sandbox's namespace address (V5) |
| B7 Exec | Spawn the helper's exec stage: enter the namespaces, drop to the uid:gid, set no-new-privs, run argv with env and cwd. stdin, stdout and stderr are plain pipes. Inspect gives running, exit code and PID. **Cancel kills the exec's process group**, which Docker can't do (it returns `Unsupported`) |
| B8 Reopen | Read the record, then check the init PID's start time (`/proc/<pid>/stat`) and its cgroup. A mismatch is "gone", as today's reopen reports a missing container |
| B9 Stop, delete | Stop with one write to `cgroup.kill`. Delete unmounts and removes the root and the cgroup, and keeps the Store |
| B10 Discover | The per-Sandbox records under the LayerFS home, as `discovery.rs` keeps them now |

### 4.3 Command identity

The exec stage runs, in this order: `setgroups([gid])`, `setresgid`, `setresuid`
to the requested nonroot uid:gid, then `PR_SET_NO_NEW_PRIVS`. Moving from root to
a nonzero uid clears the permitted and effective sets. The bounding set is left
as Docker leaves it for a nonroot exec, and no-new-privs stops file capabilities
and setuid. The #512 probe confirmed the resulting state: CapEff 0, NoNewPrivs 1,
reads and writes work, unmount is denied. `CommandIdentity::check` already
refuses root.

### 4.4 User namespaces

Without a block, a command can copy the Workspace mount into a namespace of its
own, outside the daemon's normal and forced unmount. Two options (V4):

- **`user.max_user_namespaces=0` set by the worker at VM start.** #512 tested
  it on Microsandbox, Apple, Lima and the stand-in. It needs no code, and the
  daemon doesn't use user namespaces. It applies VM-wide, which is right for a
  VM owned by one project. It's wrong where the kernel is shared, as on a
  Daytona runner.
- **A seccomp filter in the exec stage,** like Docker's: deny `CLONE_NEWUSER` to
  `unshare` and `clone`, and answer `clone3` with `ENOSYS`. It works on a
  shared kernel. It needs a BPF filter installed with `prctl`, which the repo's
  existing crates don't provide in safe Rust, so it needs a new dependency or
  an unsafe exception.

### 4.5 One helper binary, no unsafe Rust

`layerfs-sandbox` forbids unsafe code. `setns` and `unshare` act on the calling
process, so the backend uses a small helper binary, single-threaded when it
calls them, in two stages:

- **init:** `unshare(CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWIPC)`, then
  `std::process::Command::spawn` of its own second stage, which is PID 1 of the
  new PID namespace. That stage mounts the root, the Store and `/dev`, pivots
  root, starts the daemon and reaps.
- **exec:** `setns` into the Sandbox's namespaces (from `/proc/<init>/ns/*`),
  drop identity (§4.3), then spawn the command. The command is created inside
  the Sandbox's PID namespace. The helper waits and passes the exit status up.

All of this has safe wrappers in `nix`, which the crate already uses. Its
`sched`, `mount`, `user` and `process` features would need enabling (V6). No
`util-linux` (`nsenter`, `setpriv`) is needed in the VM. The #512 stand-in
ran those tools as child processes; the backend makes the same calls itself.

### 4.6 Records, one attempt and restarts

- Every effect is attempted once:
  - a failed create removes only what it made, then reports;
  - an uncertain create keeps its intent, which refuses later starts until a
    person resolves it, as `sandbox.rs` does today.
- Sandbox processes are not children of the worker. They outlive a worker
  restart and are reopened from the record (B8).
- An exec whose stdout reader was the dead worker has lost its streams. Its
  outcome is reported as unknown, never replayed. This matches a lost Docker
  attach today.
- `cgroup.kill` stops descendants that left the process group, so stop does not
  depend on a command's cooperation. Commands keep no filesystem lifetime
  beyond what FUSE already tracks
  ([execution ownership](../../../../engine/AGENTS.md#filesystem-access-and-execution-ownership)).

### 4.7 The Sandbox image

Today `deploy.image` is a local Engine image digest (`sha256:<64 hex>`). The
namespace backend needs a root filesystem with no Engine. Proposal (V7): the
bundle ships the Sandbox image as an OCI archive. The backend unpacks it once
per digest into a read-only directory, after checking that its config digest
equals `deploy.image`. Each Sandbox overlays its own upper layer on that
directory.

## 5. The Store inside the VM

- **Placement.** One ext4 disk per project VM, mounted at the Store directory.
  On each platform:
  - Microsandbox: a `disk` volume (`/dev/vdc`);
  - Apple Container: a named volume (`/dev/vdc`);
  - Firecracker: a second drive;
  - Lima or EC2: a block device;
  - Daytona: a loop-mounted image file on the sandbox's own disk, remounted at
    every start.

  The guard is unchanged.
- **Install.** The host-side Init seals the Store, as today. The adapter copies
  the sealed file into the VM, and the existing install (temporary name, then
  rename) puts it on the Store disk.
- **Durability (V8).** A VM disk that survives stop and start (as on all four
  platforms tested) is enough for the Disposable profile. Where the platform
  discards disks, the Store goes through LayerFS's remote Store push and pull
  (the #506 work, PR #507) before the VM stops and after it starts. Daytona's S3
  volumes can hold a whole checkpointed Store file, never a live one.

## 6. Host side: provider adapters

`@layerfs/runtime` gets a placement setting (V9): `local` (today's Docker,
macOS worker container or WSL2 worker) or `vm:<provider>`. A VM adapter does
five things, each attempted once:

1. ensure the project VM exists and is running (create or start), with the
   Store disk attached;
2. copy the sealed bundle in (once per bundle hash), or use a pinned VM image
   that holds it;
3. start the worker as root with its secret, and set the user-namespace block
   (V4);
4. open the private stream and return an endpoint for `useWorker`;
5. stop or delete the VM on request. Deleting a VM never deletes a Store that
   hasn't been pushed (V8).

| Provider | VM and disk | Private stream | Gaps from #512 |
| --- | --- | --- | --- |
| Microsandbox | `msb create` from an image digest, `disk` volume | `-p 127.0.0.1:H:G` | the name is the only identity; a failed create can leave hidden state; `msb exec -u` capability drop unverified (the helper sets identity anyway) |
| Any Linux host over SSH (EC2 and similar) | the host itself, or a VM the user made; a block device | SSH `-L` to loopback | Store disk provisioning is the user's job |
| Apple Container | `container run --cap-add SYS_ADMIN --cap-add SYS_RESOURCE --cap-add NET_ADMIN`, named volume | `-p 127.0.0.1:H:G`, or `container exec -i` as a stdio bridge | one `-p` failure seen once, cause unknown; modes need a tar stream through exec |
| Daytona | a privileged sandbox; loop ext4 Store | SSH gateway `-L` | shared runner kernel (seccomp needed, V4); amd64-only open-source runner; no stdin EOF on async exec |
| E2B, Vercel Sandbox, Modal VM | each SDK's VM API; disk per platform | each platform's port or tunnel | not tested: needs accounts; check each guest kernel for `CONFIG_FUSE_FS` and for root |
| Firecracker directly | drive per Store | vsock (`CONNECT <port>`) | needs a guest agent for exec; useful as a self-hosted target |

**The worker route widens (V2).** Today Store calls leave the caller's process
only on macOS and Windows. With `vm:` placement they go to the worker in the
project VM from any host OS, including Linux. The worker's rules stay:
- the stream is private;
- a secret is required;
- each call is made once;
- a lost reply is never replayed.

## 7. Concrete types, not a plugin interface

The [engine file rules](../../../../engine/AGENTS.md#production-file-size-and-responsibility)
ask for ordinary functions and concrete types, not an interface per algorithm,
so `layerfs-sandbox` exposes one
`Runtime` enum with two variants, `Docker` and `Namespaces`. Its methods match
today's `Sandbox` methods. The callers that now name `backend::docker` types
switch to the enum:
- `layerfs-api`: `base/internal.rs`, `sandbox/{create,exec,values}.rs`;
- `layerfs-node`: `sandbox.rs`, `discovery.rs`, `registry.rs`, `workspace.rs`.

The Docker variant's behaviour, wire format and tests are unchanged. A
deployment chooses the variant (V9); records store which one made each Sandbox,
and a record without the field is Docker.

## 8. Source layout

Scope: commit `35d36575c1`, counted with `tools/production_loc.py --files`. The
listing covers `layerfs-sandbox` (whole), the parts of `layerfs-api`,
`layerfs-node` and `@layerfs/runtime` that change, and the planned files. Tests
live outside `src/` and are not counted.

```diff
  engine/crates/layerfs-sandbox/src/  [3417]
    lib.rs                               8   (exports the Runtime enum)
    backend/                         [3213]
      mod.rs                             1   (declares runtime and namespaces)
+     runtime.rs                         —   new (the Runtime enum over both backends)
      docker/                        [3212]  (all 18 files unchanged, not listed)
+     namespaces/                        —   new
+       mod.rs                           —
+       create.rs                        —   (intent, root overlay, cgroup, record)
+       start.rs                         —   (init stage spawn, listener wait)
+       exec.rs                          —   (exec stage, pipes, inspect, cancel)
+       teardown.rs                      —   (cgroup.kill, delete, reopen)
+       rootfs.rs                        —   (unpack the pinned image once per digest)
    types/                            [196]  (all 5 files unchanged, not listed)
+   bin/                                 —   new
+     layerfs-sandbox-helper.rs          —   (init and exec stages, single-threaded)
```

```diff
  engine/crates/layerfs-api/src/       [704]  (partial: the files that change)
    base/                              [130]
      internal.rs                      130   (Docker Sandbox type becomes Runtime)
    sandbox/                           [574]  (partial)
      create.rs                        353   (create through either variant)
      exec.rs                           39
      values.rs                        182   (ManagedSandbox holds a Runtime)
```

```diff
  engine/crates/layerfs-node/src/     [1594]  (partial: the files that change)
    discovery.rs                       566   (record names its variant; reopen by variant)
    exec.rs                            271   (streams from either variant)
    registry.rs                        123
    sandbox.rs                         401   (start, close and reclaim by variant)
    workspace.rs                       233
```

```diff
  api/runtime/src/                     [433]  (partial)
    index.js                            96   (placement setting)
    settings.js                         49   (vm:<provider> and its settings)
    platform/                          [105]  (partial)
      macos/                           [105]
        index.js                       105   (unchanged; the worker-start pattern vm/ follows)
+     vm/                                —   new
+       index.js                         —   (the five adapter steps, §6)
+       microsandbox.js                  —
+       ssh.js                           —   (any Linux host, e.g. EC2)
+       apple-container.js               —   cond.
+       daytona.js                       —   cond.
    worker/                            [183]  (partial)
      server.js                        153   (unchanged; runs inside the VM)
      call.js                           30   (unchanged)
```

## 9. Verification

Every test command has a wall timeout of at most 120 s. Tests are built before
they run, use one attempt, and use Disposable Stores only.

1. **Fast tests, no root:** request validation, record encode and decode
   (including a record written before this change, read as Docker), and the
   identity checks. Fast lane.
2. **Linux root tests:** the nightly mount lane, in a privileged container or
   VM, where nightly's FUSE tests already run.
   - Namespace backend lifecycle: create, upload, start, listener, exec as uid
     with separate streams and an exit code, cancel, stop, delete, reopen after
     the backend process exits.
   - The #512 parity checks against a real Workspace mount: CapEff 0,
     NoNewPrivs 1, read and write, unmount denied, `unshare -U` denied under
     V4's block, and a leftover process reaped.
   - Two Sandboxes on one Store: neither sees the other's mount.
3. **Real VM proofs, per adapter:** `calls_real.mjs inner workspace` and
   `sandbox_real.mjs inner` with `vm:microsandbox` and `vm:ssh` (a local Linux
   VM), each one command under 120 s. Apple Container and Daytona follow, if V3
   selects them. A test that covered behaviour only through the Docker route is
   kept, not moved; the namespace backend gets its own.
4. **Acceptance shapes** from the
   [integration contract §11](../303/06-cluster-one-integration.md#11-platform-and-acceptance):
   - frequent fresh mounts;
   - long-lived multi-call Workspaces with incremental Commits;
   - more than one daemon on one Store.

## 10. Owner decisions

| ID | Question |
| --- | --- |
| V1 | Accept the shape: one Linux VM per Store, Sandboxes as namespaces inside it, Docker kept as the default backend? |
| V2 | Widen the worker exception so Store calls may go to a worker inside a project VM from any host OS, Linux included, under the existing worker rules? |
| V3 | Which adapters first? The proposal is Microsandbox and SSH-to-Linux first, Apple Container next, Daytona and the hosted providers later (they need accounts and a check of each guest kernel). |
| V4 | User-namespace block: the VM-wide sysctl only (no code, VM per project), or also a per-command seccomp filter, which needs a new dependency or an unsafe exception and is required for shared-kernel platforms such as Daytona? |
| V5 | Network: share the VM's network between Sandboxes (simplest; commands can reach other Sandboxes' authenticated daemon ports on loopback), or give each its own network namespace, which needs loopback set up through netlink? |
| V6 | Enable `nix`'s `sched`, `mount`, `user` and `process` features in `layerfs-sandbox`, and add the helper binary to it? |
| V7 | The Sandbox image as an OCI archive in the bundle, unpacked once per digest in the VM, in place of an Engine image ID for this backend? |
| V8 | Store durability on platforms that discard disks: require a persistent disk, or push and pull through the remote Store (#506) at VM stop and start? Should VM delete refuse while the Store isn't pushed? |
| V9 | Settings names: the placement (`local`, `vm:<provider>`) and the backend choice inside the VM. Proposed `LAYERFS_PLACEMENT` and `LAYERFS_SANDBOX_BACKEND`, read only as `LAYERFS_*` per the plugin-neutral rule. |
| V10 | Should the test-only Engine API stand-in from #512 be kept under `tools/` as a fast way to run the unchanged SDK without Docker, or left out of the repository? |

## 11. Not changed and out of scope

- The daemon, FUSE, Overlay, Commit and the Store format.
- The Docker backend and its default use.
- Direct Store access on a Linux host with local Docker.
- The Disposable-only profile; Durable stays disabled.
- The one-attempt and no-replay rules; the worker's loopback-or-private, secret
  and one-attempt rules.
- Out of scope:
  - a VM per Sandbox;
  - sharing a Store across VMs;
  - hosted-provider accounts and billing;
  - performance claims. #512 timings are single samples under a slow network
    and are not measurements under the
    [measurement workflow](../../../../docs/general/agent-measurement-policy.md).

## 12. Rules that apply to the implementation

Read the root [AGENTS.md](../../../../AGENTS.md), the
[engine guide](../../../../engine/AGENTS.md) and the
[API guide](../../../../api/AGENTS.md) first.

- Production LOC goes in every commit message.
- The file ceilings apply: 999 lines per file, 200 for `lib.rs` and `mod.rs`.
- Cargo `+1.85.1`.
- Locked dependencies, and no dependency change beyond V6 without the owner.
- Delete tests that are hard to adapt rather than nursing them, and list them
  in the commit.
