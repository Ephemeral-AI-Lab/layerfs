# Fixed sandbox control port: one functional telemetry diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

One telemetry-enabled live Docker route passed at clean source commit
`00e7374ff1e6d86d176aab90379bf040a3cf036f`, with the same immutable daemon
image as the earlier #236 diagnostics. The host owner now selects and verifies
one fixed loopback port during `sandbox.create`. The four measured Workspace
calls do not run `docker port`; each still authenticates Hello and checks the
daemon instance. The functional proof checked port stability after Docker
restart, old Workspace rejection, bounded Exec, exact Commit readback and
historical `HeadMoved`.

| Public phase | One-connection SDK wall (ms) | Fixed-port SDK wall (ms) | Fixed-port Hello (ms) | Fixed-port daemon control (ms) |
| --- | ---: | ---: | ---: | ---: |
| Create | 314.123 | 285.574 | unavailable | unavailable |
| Mount | 61.739 | 39.831 | 10.008 | 28.913 |
| Exec | 69.830 | 45.422 | 13.909 | 30.682 |
| Commit | 92.354 | 72.247 | 14.061 | 57.283 |
| Unmount | 37.684 | 11.662 | 8.315 | 2.254 |
| **Enclosing route** | **575.815** | **454.833** | unavailable | unavailable |

The four fixed-port SDK calls total **169.162 ms**, including **46.293 ms**
of authenticated Hello windows and **119.132 ms** of daemon control dispatch.
The corresponding four one-connection calls totalled **261.607 ms** in a
different machine/cache window. The source change deterministically removes
four Docker CLI lookups from this route; the wall difference is an observation,
not a causal estimate or speedup claim. The earlier
[lookup cause diagnostic](telemetry-owner-lookup-20260924.md) measured
98.736 ms across those four Docker lookups but was incomplete because its
enclosing route LFT1 event was dropped.

The fixed-port host route window had 46 samples, zero gaps, 20.006 ms sampled
user CPU plus 22.616 ms sampled system CPU, and 30.73 MiB sampled maximum RSS.
Host CPU is process-shared and RSS is a sampled maximum; neither covers the
Exec child or container cgroup. Source/OS cache state remains uncontrolled, so
`functional_status=PASS` and `performance_admission=INELIGIBLE` are reported
separately. This source was sampled once, with no rerun of the earlier arms.

The [raw host and daemon LFT1 streams, source/image/binary identities, exact
command, report and SHA-256 manifest](evidence/sdk-route-fixed-port-20260924-01/report.json)
are retained verbatim. The next cause to investigate is the checked Hello path:
its daemon operation itself is small, while the control listener currently
parks for up to 10 ms when no connection is ready. That relationship needs
targeted timing or a prospective implementation check before assigning an
exact amount of Hello wall to the accept wait.
