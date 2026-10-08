# R7 real daemon runtime harness

> **Status:** Current general guide.

This independent harness workspace uses only existing first-party dependencies.
It is exploratory, not admission eligible, and has no product source membership.
The lead runs every build or invocation under the checkout lock and a declared
wall stop. The harness performs one attempted operation and retains a container
on original failure. It never removes containers, volumes, fixtures or evidence.

`provision` performs host `ProjectApi::init` on an independent source copy,
including consuming seal, creates the real daemon with `SandboxApi`, and streams
the sealed file once through SDK installation into an existing named VM volume.
It writes the installed manifest as a private binary record and explicitly stops
the setup container. It retains the container, sealed master and volume. The
copy's root uid/gid must match the ordinary command identity. Init preserves
metadata; the harness does not rewrite ownership or weaken modes.

```sh
layerfs-r7-runtime provision \
  --socket /Users/yifanxu/.docker/run/docker.sock \
  --volume OWNED_NEW_VOLUME \
  --daemon /ABS/core/target/r7-linux-release/release/layerfs-daemon \
  --source-copy /tmp/INDEPENDENT_COPY \
  --sealed /tmp/NEW_SEALED.sqlite \
  --manifest /tmp/NEW_INSTALLED.manifest \
  --receipt /tmp/NEW_PROVISION.events.jsonl \
  --uid 501 --gid 20 --init-seconds 300
```

The image is fixed to
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
The daemon executable is uploaded through the SDK, with no image rebuild or
network dependency. Release identity and ARM flags must be checked by the caller.
Provisioning is setup and never a mounted timing sample.

The prospectively frozen resource selection uses `OwnerConfig::default()`:
8 MiB aggregate owner credits, 64 KiB Lifecycle reserve, 16 namespaces,
16 ordinary and 2 Lifecycle slots (Source has its separate 16 slots). The cache
allowance is the current S8 8 MiB logical charge. There are 2 Store readers,
4 ordinary controls and a 1024 KiB overlay pager suggestion. These settings never
increase in response to a workload, failure or measurement.

`serve` starts a real daemon from an explicitly already installed manifest and
keeps its Store sessions, immutable cache and overlay alive across explicit
commands. The host remains control-only. Mutable sample Stores require a fresh
independent byte-copy volume; the manifest describes authority and locator and
does not authorize reuse of mutated input.

```sh
layerfs-r7-runtime serve \
  --socket /Users/yifanxu/.docker/run/docker.sock \
  --volume OWNED_VOLUME_WITH_INSTALLED_STORE \
  --daemon /ABS/core/target/r7-linux-release/release/layerfs-daemon \
  --manifest /tmp/INSTALLED.manifest \
  --receipt /tmp/NEW_SERVE.events.jsonl --uid 501 --gid 20
```

Each stdin command is one line with **literal tabs** between fields. Scripts live
in files outside the product and are read before the command timer. Workspace
keys are 64 hexadecimal digits supplied by the controlling harness as authority
identities; they must be fresh for every Mount. Each command emits a newline JSON
event on stdout and in the append-only receipt. Wait for that event before the
next dependent command. EOF is an error that retains ownership; explicit `stop`
requires every Workspace already unmounted.

| Command | Fields | Operation and receipt |
| --- | --- | --- |
| `mount` | key | `WorkspaceApi::mount`, including Bind plus Attach to Ready |
| `command` | key, script filename | Ordinary `/bin/bash -o pipefail -c`, via runtime only; no filesystem Exec registration |
| `commit` | key | Exactly one `WorkspaceApi::commit`; records original typed history outcome |
| `snapshot` | key | SDK Status plus external backing/process observer; outside product timers |
| `observe` | none | External backing/process observer only |
| `verify` | key, oracle filename | Separate ordinary command, labelled verifier; caller enforces independent budget |
| `unmount` | key | Exactly one normal terminal `WorkspaceApi::unmount` to its reply |
| `stop` | none | One EndSession and explicit container stop; no deletion |

A key `native:/ABSOLUTE/NATIVE/ROOT` is permitted for `command` and `verify`.
It selects the container's own filesystem and the identical Bash, image, runtime
and nonroot identity. Prepare that independent native fixture outside timing;
neither the harness nor a mounted measurement materializes the immutable root to
implement the native arm. `docker cp -a` or equivalent declared byte-copy setup
can preserve original copy metadata. Native mount/unmount are N/A. A real product
daemon remains idle in this topology and its setup/resource cost is explicitly
separate from the native command.

An external runtime command creates output files before the timer and streams
stdout/stderr directly into those files. It records observed stream EOF separately
from runtime exit observation. The timer covers create, start, stream drain and
status observation. A nonzero exit remains a failure with files retained; it
never infers that published filesystem changes disappeared. Scripts that expect
a nonzero exit must declare the expected result in a future additive protocol
before use; current commands require exit zero.

For the initial changed-binary survival proof, use `mount`, an ordinary script
that writes a known file, `commit`, `unmount`, `mount` with a new authority key,
`verify` with an independent exact-byte script, then `unmount` and `stop`. The
fresh mount uses the same installed Branch with its acknowledged head.

Observers read `/proc/1/status` (including daemon `VmHWM`) and `stat` for Store,
its sidecars, overlay and overlay journal. Block counts times block bytes are
allocated bytes. `VmHWM` is a lifetime high-water mark, never a phase peak. The
daemon is PID 1 under the checked SDK container topology. Root observer commands
do no mutation, no cache eviction and no filesystem mount access.

The current control wire does not expose owner/statement/Store-reader/cache/Commit
construction counters, per-opcode arrays or physical cleanup Gone after normal
unmount. The raw receipt reports available native aggregate Status and explicitly
marks normal-unmount physical cleanup unavailable. Stage 0 cannot be considered
complete until legitimate additive product telemetry supplies those observations.
The harness must then consume that public telemetry, preserving the real binary
route. Per-file eviction and residency, cache-class eligibility, external identity
seals, sample uniqueness, complete-command wall budgets, reuse and full/scoped
oracles belong to the controlling prospective registry and runner. The harness
does not call a cold-state mismatch a pass or assign a numerical threshold.

Build this manifest from the repository root so the ARM64 Cargo config is found.
The lead generates its lockfile offline once from the repository's existing
dependency set, then all builds use `cargo +1.85.1 --locked`. Set a worktree-local
target. No build may overlap a Docker, test or measurement command.
