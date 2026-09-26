# Live SDK route with production-style host Service accept

> **Status:** Dated planning checkpoint; functional diagnostic only.

The live SDK test's host Service listener previously slept 10 ms after an
empty nonblocking `accept`. Production Service already polls its listener for
readiness. Source `436a93ae6ec8b678dc6c6d2c38a2e830f1ea55ab` made the
test listener poll too, then took one live Docker diagnostic with the unchanged
daemon image
`sha256:bda2f50ebda7700b064ee10f014969f158192568f95d488ae7629c2fce63d1f5`.
The functional route passed, including FUSE edit, Commit readback, restart
binding rejection, and historical `HeadMoved` conflict.

| Public phase | SDK wall (ms) | Owner Hello (ms) | Daemon control (ms) |
| --- | ---: | ---: | ---: |
| Create | 293.095 | unavailable | unavailable |
| Mount | 26.212 | 4.500 | 20.789 |
| Exec | 28.506 | 7.592 | 20.251 |
| Commit | 49.133 | 4.163 | 44.325 |
| Unmount | 6.992 | 4.051 | 2.471 |
| **Enclosing route** | **404.007** | unavailable | unavailable |

The four calls after Create total **110.843 ms**, of which the four Hello
windows total **20.305 ms**. The daemon operation windows total **87.836 ms**;
these are in the daemon's own clock and are not additive to the host windows.
Within the host SDK phase windows, Service operations total **3.287 ms** for
Mount (HistoryQuery and Inspect), **2.446 ms** for Exec (Inspect), and
**14.855 ms** for Commit (EditFile, UpdatePortableMetadata, HistoryCommand).
The remaining daemon time includes transport, Workspace and FUSE work; this
receipt does not divide those terms further.

The prior accept-poll receipt had a **474.935 ms** enclosing route and
**168.001 ms** across its four post-Create calls. This test-harness change
removes a real measurement artifact, but these observations came from distinct
machine/cache windows and do not quantify its causal contribution. They do not
support a version speedup claim or a v0.1.6 performance comparison.

The host route had 41 samples, zero gaps, 15.214 ms sampled user CPU plus
20.874 ms sampled system CPU, and 28.11 MiB sampled maximum RSS. The host
and Service share a process; Exec child and Docker cgroup resources are
excluded. Host and primary daemon telemetry summaries recorded zero drops.
The complete command wall was **4.740 s**. Cache state remained uncontrolled:
functional **PASS**, performance **INELIGIBLE**.

The [raw receipt](evidence/sdk-route-host-poll-20260924-01/report.json) keeps
the exact command, source/binary/image identities, host and daemon LFT1
streams, report, and SHA-256 manifest. The next product investigation should
split daemon Mount, Exec and Commit time before changing their algorithms.
