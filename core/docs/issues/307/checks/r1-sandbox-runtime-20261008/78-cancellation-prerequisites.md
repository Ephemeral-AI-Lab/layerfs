# Exact ordinary runtime cancellation prerequisites

> **Status:** Dated research checkpoint; capability is not yet qualified.
> Main inspected actual runtime [59](59-runtime-components.json); reviewers
> performed read-only primary-source research, with no installation or signal.

Engine29.5.2/API1.54 uses containerd2.2.4/runc1.3.5 here. Engine has no per-Exec
kill route; its HTTP Start passes context.Background, so local transport/CLI
cancellation is not a remote command signal. Whole-Sandbox stop is separate
explicit scope. [Moby router](https://github.com/moby/moby/blob/docker-v29.5.2/daemon/server/router/container/container.go),
[Start handler](https://github.com/moby/moby/blob/docker-v29.5.2/daemon/server/router/container/exec.go#L129-L145).

The acknowledged Engine ExecID maps unchanged to containerd ExecID, under the
same full ContainerID. A kill-only external standard client can name these IDs
without a LayerFS PID guess or daemon launcher:
`ctr --address /run/containerd/containerd.sock --namespace moby tasks kill --exec-id <ExecID> --signal <explicit> <ContainerID>`.
[Moby identity mapping](https://github.com/moby/moby/blob/docker-v29.5.2/daemon/exec.go#L219),
[exact standard kill](https://github.com/containerd/containerd/blob/v2.2.4/cmd/ctr/commands/tasks/kill.go).

Published official static ARM64 archive:
`containerd-static-2.2.4-linux-arm64.tar.gz`,29.1MB; SHA256
`5f01ac0ad46caf685afee7614e36e406c48e73c1d84a71378dd4afe5d63a11d2`.
Extract unchanged `bin/ctr` only; no containerd/shim/runc installation/start is
needed. The artifact has not yet been downloaded or binary-verified. Source
stability is version-specific: ctr is an administrative/debug client without a
cross-release command promise. [Official assets](https://github.com/containerd/containerd/releases/expanded_assets/v2.2.4),
[stability statement](https://github.com/containerd/containerd/blob/v2.2.4/cmd/ctr/app/main.go#L64-L73).

Required next observations are actual trusted client/socket access and absence
of `ctr.cni-containerd.metadata` on the exact owned Docker container: ctr kill
calls CNI removal before signaling. The administrative client must be separate
from ordinary commands, with no Store/Overlay/config/FUSE mount and no untrusted
command access to its socket. Signal acceptance is not exit; Engine inspection
must independently retain matching IDs/Running=false/non-null exit. Exec-only
kill excludes descendants and rejects `--all`; file descriptors/processes can
survive. Containerd2.2.4 retains an internal exit/PID-reuse limitation; no stronger
new guarantee is claimed. [CNI hook](https://github.com/containerd/containerd/blob/v2.2.4/cmd/ctr/commands/tasks/kill.go#L122-L130),
[extension](https://github.com/containerd/containerd/blob/v2.2.4/cmd/ctr/commands/cni.go#L30-L38),
[scope](https://github.com/containerd/containerd/blob/v2.2.4/cmd/ctr/commands/tasks/kill.go#L88-L105),
[shim limitation](https://github.com/containerd/containerd/blob/v2.2.4/cmd/containerd-shim-runc-v2/task/service.go#L623-L640).

Current image's client probe reports unavailable [38](38-runtime-client-probe.json).
An exact VM `/usr/bin/ctr` bind probe failed before an acknowledged create because
that source does not exist [39](39-containerd-reachability-probe.json). No source
path/directory was created and no runtime task was mutated. These are retained
prerequisites/failures, not an owner waiver or proof that all external paths are
impossible. Meaningful lifecycle/native filesystem and standard-client setup work
continues; R1 remains open. No original failed signal will ever be replayed or
reclassified as already-canceled success.
