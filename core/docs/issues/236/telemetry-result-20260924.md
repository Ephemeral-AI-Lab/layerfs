# Agent SDK route: one LayerFS telemetry diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source commit `59a12f22ab4475c3d26f417b509a9792e872ff4f`; immutable
functional image `sha256:e08b4efc6d4ea1be3e8df2fbeaccd60581ecaeb0551c1a440c9fd538ecf2e961`.
One diagnostic run followed the declared
[`create → mount → Exec → Commit → unmount` boundary](telemetry-diagnostic.md)
with a fresh one-file Project. `functional_status=PASS`;
`performance_admission=INELIGIBLE` and `admission_eligible=false` because OS
cache state was uncontrolled and the process observations are not complete
container or phase-peak resource measurements. No comparison or speedup claim
is made.

| Public phase | SDK call wall (ms) | Host process CPU sampled (ms) | Host sampled max RSS (MiB) | Daemon control wall (ms) | Daemon process CPU sampled (ms) | Daemon sampled max RSS (MiB) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `sandbox.create` | 258.825 | 14.034 | 25.00 | unavailable | unavailable | unavailable |
| `workspace.mount` | 78.102 | 6.130 | 25.95 | 37.205 | 10.000 | 10.80 |
| `workspace.exec` (`printf first > note`) | 64.296 | 4.911 | 25.98 | 24.015 | 10.000 | 36.91 |
| `workspace.commit` | 104.094 | 16.718 | 28.52 | 57.559 | 20.000 | 37.16 |
| `workspace.unmount` | 47.840 | 2.433 | 28.52 | 2.813 | 10.000 | 37.10 |
| **Enclosing SDK route** | **553.229** | **44.226** | **28.52** | unavailable | unavailable | unavailable |

The five SDK call timers sum to **553.157 ms**; the 553.229 ms enclosing
LayerFS timer includes their small orchestration and telemetry handoffs.
Host CPU is 19.571 ms user + 24.655 ms system for the enclosing window.
Host SDK and Service share one macOS process; the daemon is a separate Linux
process. These CPU windows are process-shared samples, not exclusive operation
costs. The daemon's Linux CPU values have 10 ms tick granularity, so its
2.813 ms Unmount can coincide with a reported 10 ms sampled CPU change.
Daemon process metrics exclude the Exec shell child. Both RSS columns are
sampled process resident bytes, not incremental memory or exact peaks; no
Docker cgroup/file-cache or `memory.peak` value was collected. The host route
window had 57 samples, zero gaps and a 12.405 ms largest gap with the selected
10 ms monitor interval.

The later verification reopened the current and older Commits and confirmed
their bytes. The older-Commit write then produced the expected `HeadMoved`
failure outside the measured five-call route; the raw host and daemon streams
each retain its `HistoryCommand` failure event. A restart also invalidated the
old Workspace binding. No failure or non-passing line was discarded.

Raw command output, all host and daemon `LFT1` events, source and binary hashes,
the derived [report](evidence/sdk-route-telemetry-20260924-01/report.json), and
the verified [SHA-256 manifest](evidence/sdk-route-telemetry-20260924-01/manifest.json)
are retained in the same append-only evidence directory. The parser was
`core/tools/sdk_telemetry_report.py` at the source commit above; the exact
invocation and run identity are in the retained `command.txt` and
`identities.json`. Historical benchmark receipts remain unchanged.
