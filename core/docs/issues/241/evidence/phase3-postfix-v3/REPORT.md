# #241 position selection with Docker-assigned port: baseline Exec still unknown

The [prospective v3 selection](CONTRACT.md) ran once at source
`c1dc56acab406b2911dc1f67e515f2c5c9f0d9b6`, tree
`ce0fb20c17bea8a9b272a893d10a5eb38779852f`, with the unchanged
264-position manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`.
The locked release position-test binary SHA-256 was
`8d540da78f227a702cdf779978cbb698776eb5ea0ca0f4890f23cd69f7f02aa1`;
the release SDK Init binary SHA-256 was
`6ce1b76d7a937d8220b3b6f510d86a6706f9e1e68e03444a8ad29e45898a9ba3`.
The immutable Linux daemon/tool image was
`sha256:4a6aee843eef31f760016892e5d97cfd5d80183044341350ae68d4d99cac4c47`.
The [plan](raw/plan.json), SHA-256
`66dd363762c6e2c9bd2c03680c449136e5a994dc1ef4dd0fc094dfaaf32e7520`,
fixed all four fresh outputs, fixture/master/image hashes and telemetry IDs
before execution. All four [closed masters](raw/masters.json) were first-use
release SDK Init preparations at this source; each size used an independent
writable byte copy. Cache state was uncontrolled.

| Size | PASS | FAIL | NOT_RUN | Cleanup | Result |
| --- | ---: | ---: | ---: | --- | --- |
| 1 MiB | 31 | 1 | 34 | PASS | Baseline SDK Exec `Unknown,true` on case 32 |
| 10 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |
| 100 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |
| Capped 500 MiB | 66 | 0 | 0 | PASS | Full functional slice PASS |

Total: **229 PASS / 1 FAIL / 34 NOT_RUN**. The complete 264-position Phase 3
gate is **FAIL**. The 1 MiB size stopped at `1mib-overwrite-band15`, offset
1,016,765. Its preliminary `printf baseline > .position-baseline` through
public SDK Exec returned `Failure(Unknown, unknown=true)` after
**5.044964542 s**. No range EDIT or Commit ran for that case. Unmount returned
`Ok(())`; the edit Sandbox was deleted and list-confirmed absent. Each size's
final Sandbox list was empty. No v3 case was rerun to replace this outcome.

The failed case's [daemon LFT1](raw/1mib/1mib-overwrite-band15-edit-daemon-late.stderr)
records successful WorkspaceOpen, then two quick failed Inspect Service
calls while the baseline path was resolved. The path and refusal code are
inferred from the command/source; LFT1 does not record either. Its
HistoryCommand, needed to reserve an inode for the small baseline file,
spent **5.004232 s inside `daemon.service_connect`**. There
was no child `daemon.service_hello` or `daemon.service_call` for that command.
The host trace recorded 93 successful HistoryCommands for the 31 preceding
cases and no matching HistoryCommand for case 32. The shell child spawned
within about 6.25 ms and waited for the Service-dependent create. Docker
reported the daemon running without restart or OOM; host resource records
continued during the stall. The separate Docker control route later answered
owner Hello. These facts locate the first sustained no-progress stage at
daemon-to-host Service connection/authentication, before Service request
handling. The retained span combines TCP connect and Noise authentication;
it does **not** distinguish those phases or prove host session-capacity
exhaustion.

The failed case's private Store had a writer budget of two in a
[read-only post-run query](DERIVED-STORE-POLICY.tsv); that copy is excluded
from Git evidence. The host Service cap at this source was therefore four
sessions. The current native protocol closes a connection
after either of the expected negative Inspect refusals, causing rapid
reconnects before the mandatory HistoryCommand. This is a plausible
contributor, not a proved cause: no active-session, reap or capacity-drop
counter was retained. The prior 31 baseline Exec calls took 20.08–27.63 ms
(median 21.81 ms), without a rising trend. Raising a timeout or replaying
the case would not identify the stalled subphase.

The Docker-assigned port correction passed a separate live public SDK
lifecycle/restart test before this selection: [plan](raw/port-route-plan.json),
[stdout](raw/port-route.stdout). The 100 MiB size passed all 66 positions at
v3, including the case where v2 had a Docker host-port bind collision. The
original SDK Exec unknown remains unresolved despite those other fixes.

The [raw directory](raw/) retains plans, case/run/summary receipts, command
output and failure diagnostics with a [SHA-256 inventory](RAW-SHA256.txt).
Private writable Store/history copies stay at the local output paths in the
plan; fixture/master hashes and copy method are in the plan and run receipts.
Individual case TSVs do not repeat source, binary or image IDs; their
identity is joined through the prospective plan. Docker inspect contains
container State only, without control configuration. These are functional
observations, not latency or cold-cache admission. The four release
Edit→Commit samples and separate verifiers remain unrun.
