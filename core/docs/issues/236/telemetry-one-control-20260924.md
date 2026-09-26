# One checked control connection: one functional telemetry diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The source-pinned diagnostic at commit
`0edc58ce8d4f21115a1eb27e2964290426f418bc` passed the live Docker
functional route after checked `SandboxHello` and the Workspace operation began
sharing one authenticated connection. It reused the immutable daemon image
`sha256:e08b4efc6d4ea1be3e8df2fbeaccd60581ecaeb0551c1a440c9fd538ecf2e961`;
daemon, bridge, Workspace and FUSE source were unchanged from the earlier run.
The live proof also checked restart-stale binding, current/older Commit readback,
bounded output, and `HeadMoved` after the measured route.

| Public phase | Previous SDK wall (ms) | One-connection SDK wall (ms) | Previous daemon control (ms) | One-connection daemon control (ms) |
| --- | ---: | ---: | ---: | ---: |
| Create | 258.825 | 314.123 | unavailable | unavailable |
| Mount | 78.102 | 61.739 | 37.205 | 30.116 |
| Exec | 64.296 | 69.830 | 24.015 | 32.309 |
| Commit | 104.094 | 92.354 | 57.559 | 56.251 |
| Unmount | 47.840 | 37.684 | 2.813 | 2.097 |
| **Enclosing route** | **553.229** | **575.815** | unavailable | unavailable |

The four SDK-minus-control residuals sum to 172.741 ms in the earlier run and
140.834 ms in this run. That is a **31.907 ms lower observed residual** with
one fewer authenticated TCP connection and bridge Hello per Workspace call.
Create was 55.298 ms slower and Exec's daemon dispatch 8.294 ms slower in this
separate, uncontrolled window, so the enclosing route was 22.586 ms longer.
These two runs are diagnostics, not paired benchmark arms. The residual difference
does not isolate handshake cost or establish a speedup.

The new enclosing host process window recorded 17.430 ms sampled user CPU,
24.805 ms sampled system CPU, 31.03 MiB sampled maximum RSS, 59 samples and
zero gaps at the 10 ms monitor interval. Host CPU is process-shared; RSS is a
sampled maximum, not a container total or exact peak. Source/OS cache state
was uncontrolled, so `functional_status=PASS` and
`performance_admission=INELIGIBLE` remain separate. One sample was taken at the
new source identity; the earlier source was not resampled.

The [raw host and daemon LFT1 records, exact command and identities, derived
report and SHA-256 manifest](evidence/sdk-route-one-control-20260924-01/report.json)
are retained in the append-only evidence directory. The earlier
[telemetry diagnostic](telemetry-result-20260924.md) is unchanged. The next
diagnostic should split Docker port discovery from authenticated Hello with the
existing LayerFS telemetry runtime before changing the current-port contract.
