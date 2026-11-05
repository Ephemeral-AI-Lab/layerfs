# Firecracker and SSH product restriction

> **Status:** Current general guide.

Implements [layerfs-project#7](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/7)
on `layerfs_v2_0_0`, starting from `f2896f78e100ddd71b2f869d94f492e7c1e67bb9`.
The owner explicitly requested permanent disabling with code retained on
2026-11-06. The [restriction contract](../../../../docs/architecture/connect/support-matrix/disabled-platforms.md)
is the current behavior and existing-resource guidance.

## Implementation

- A shared connect policy holds one fixed refusal per disabled platform.
  Machine support data, readiness, host setup and public dispatch consume it.
  It is available even when machine adapters are not compiled into the API.
- Public calls reject new work before home creation, locks, worker connections,
  direct daemon connections, or platform tools. Machine names are resolved from
  their records; Sandbox records also retain their platform. Transfer checks
  include historical leftover copies before recovery could retire one.
- Machine inspection retains recorded state with the disabled reason and does
  not probe tools. Existing explicit teardown, lower-level status and identity
  inspection remain available. No automatic stop, deletion or migration occurs.
- CLI help, generated Node/Python documentation, support JSON, `import-env`,
  doctor advice, active setup documents and the SDK skill reflect the restriction.
- Native macOS NFS, Linux namespaces, Docker, Lima, Apple Container and
  microsandbox routing is preserved. Lima's use of SSH as a transport remains;
  the disabled platform is SSH, not the transport executable.

## Verification

All commands used the registered private worktree, Rust 1.85.1, locked offline
builds and the repository ARM64 flags. Test invocations had explicit wall limits
of at most 100 seconds with compilation separate. Stores were Disposable.
Durable execution: **NOT_RUN — disabled by owner**.

The shared `api/test/disabled-platforms.json` drives 27 refusal selections for
each platform through Rust, CLI, Node and Python. Checks include direct platform
selection, a running saved worker, a forwarded daemon, implicit SQLite placement,
PostgreSQL Sandbox placement, mutations and transfers. It asserts fixed
`Unsupported` reasons, preserved record bytes and no worker/daemon connections.

- Machine crate: 78 passing cases across the retained covering runs, eight
  historical adapter qualification tests ignored. Support checks cover macOS,
  Linux, Windows and Other, both Store kinds. Conformance still checks enabled
  routes, backend/kernel mapping and explicit teardown.
- API: 36 distinct selected cases passed, including refusal, discovery,
  inspection, machine lifecycle, sandbox placement and home migration; three
  policy cases also passed with no machine feature.
- CLI: 22 passed, including the shared matrix and existing exit/stream behavior.
- Node: 25 passed, one daemon-dependent exec test skipped. Python: 39 passed,
  one daemon-dependent exec test skipped, with strict mypy samples enabled.
- Declaration drift checks, product boundary guard and 169 tooling tests passed.
- Warning-denying Clippy covered all targets of machine, sandbox, control, API,
  CLI, Node and Python. Rust formatting and changed-document link targets checked.
- Focused doctor checks cover the replacement of unsupported SSH setup advice
  with restoration of the existing machine record for inspection and cleanup.

These are macOS executions with host-independent policy checks for all hosts;
no new live Linux/Windows VM qualification or successful live transfer campaign
is claimed. The initial transfer selection is six native↔target directions for
Docker, Lima and Apple Container, plus Docker→native→Lima. Firecracker and SSH
are refusal selections only; microsandbox remains product-supported.

Retained raw logs and receipts are under
`~/.cache/layerfs-worktrees/layerfs-disable-fc-ssh-826ddf3700dd/` (`builds/`,
`review/`). Failed development receipts remain: an initial borrow-type compile
error; an old Firecracker boot test expecting a launch; the CLI matrix initially
passing file content positionally instead of on stdin; and a doctor assertion
expecting the superseded machine-creation advice. Each was diagnosed before its
focused correction and rerun. Passing unaffected checks were reused.

## Retained code and tests

No adapter implementation or test was deleted. The complete
`layerfs-machine/src/platforms/firecracker/` and `platforms/ssh/` trees are
unchanged. Eight tests remain compiled but ignored with an explicit permanent
policy reason, since they qualify launches or connections the product now refuses:

- `machine_ssh`: `connect_checks_the_host_key_kernel_and_data_root_before_setup`,
  `an_unknown_host_key_is_denied`, `a_host_that_is_not_linux_is_unsupported`.
- `machine_forward`: `ssh_forwards_a_free_port_to_the_daemon_and_close_ends_exactly_it`,
  `a_forward_that_ends_at_once_fails_once_and_leaves_nothing`.
- `machine_providers`: `firecracker_names_what_its_lima_host_lacks`,
  `firecracker_on_a_stopped_host_is_unavailable`,
  `a_microvm_chooses_its_huge_pages_at_each_boot`.

New refusal checks replace their role in product availability verification.
Their former successful outcomes remain historical evidence, not current support.
Re-enabling requires an explicit later owner decision and source-policy change,
regenerated support data, restored qualification and updated user documentation.
