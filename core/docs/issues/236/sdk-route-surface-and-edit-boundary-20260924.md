# Agent SDK route surface, telemetry provenance and edit-family boundary

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [live SDK test](../../../crates/layerfs-api/sdk/tests/agent_route.rs) initializes
a Project through `ProjectApi::init` and forks a Branch with a direct host
`Service::handle(HistoryCommand::Fork)` during setup. Its measured five-call
route uses public `SandboxApi::create`, then `WorkspaceApi::mount`, `exec`,
`commit` and `unmount`. The measured edit is
`workspace.exec("printf first > note")`: a shell command through the mounted
FUSE directory. Project Init, Branch fork, later readback/conflict/restart
checks and cleanup are outside the route timer.

The measured entrypoints are SDK methods, but the product route includes the
host sandbox owner, Docker, native Bridge transport, Linux daemon, FUSE
Workspace, host Service and Store. The live proof is a Workspace/Exec/FUSE
workflow diagnostic. It is not an SDK range-edit operation or a registered
fs-bench-pro edit-family sample.

All reported SDK phase time, daemon windows, sampled CPU and sampled RSS in
the [source-pinned receipt](evidence/sdk-route-reuse-20260924-01/report.json)
come from LFT1 operation/resource records emitted by the existing
`layerfs-telemetry` runtime and parsed by
[`sdk_telemetry_report.py`](../../../tools/sdk_telemetry_report.py). The
external complete-command wall is separately measured by the Python runner's
monotonic clock; exit status and source/image hashes are runner custody data.
Host/Service CPU and RSS are process-shared samples. Exec child and Docker
cgroup resources are excluded. The receipt declares uncontrolled OS/source
cache and remains functional PASS, performance INELIGIBLE. Its host-side
`owner.hello_count` records newly opened host Hello spans, not every checked
Hello request on a retained socket.

[Issue #236](https://github.com/Ephemeral-AI-Lab/layerfs/issues/236) now has an
owner-directed follow-on implementing this agent workflow. Its acceptance
text explicitly excludes the Exec/FUSE edit from
[#232's](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232) 56 active
direct SDK edit cases. The [measurement rule](../../../../docs/general/benchmark_rules.md)
requires those cases to call `Client::edit_workspace_file_range` or
`Client::edit_workspace_file_ranges`, followed by their declared explicit
Commit, with zero edit-caused FUSE WRITE. Current Core agent `WorkspaceApi`
exposes `exec` and `commit`, but not that direct range-edit API. The legacy
root `crates/layerfs-sdk` reference does expose it; its historical rows are
not v0.1.7 admission evidence.

For benchmark planning, keep three claims separate:

| Claim | Public operation | Current owner |
| --- | --- | --- |
| Project Init | host-direct SDK `init_project` | #236 functional proof and the Core SDK Init selection |
| Agent Workspace workflow | `WorkspaceApi::exec` through FUSE, then `commit` | #236 functional diagnostic; a new prospective workflow family would need its own frozen specification |
| Direct SDK range edit | `Client::edit_workspace_file_range(s)`, then explicit Commit | #232's 56-case pilot; Core needs the public authenticated edit route first |

Do not relabel the 402.519 ms five-call workflow as SDK range-edit latency or
compare it to historical per-edit-and-Commit rows. Before collecting a new
numeric edit family, freeze public API/route, fixture and cache state, edit and
Commit timers, counters, verifier, resource limits and source identities.
Retain one sample per source/case/arm and every non-passing receipt under the
repository's benchmark rules.
