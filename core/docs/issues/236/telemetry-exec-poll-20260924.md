# Exec pipe-readiness diagnostic

> **Status:** Dated planning checkpoint; functional diagnostic only.

Source `ee876081ecfe96bc8e077aeea3d3a7a419c56660` lets Workspace Exec
poll its child stdout/stderr pipes rather than sleeping for a fixed 5 ms while
either remains open. A live Docker route using immutable daemon image
`sha256:498b326fd66e9d0e39316ff3923305f722f7756ce9c03bad54f52b6a517a9ded`
passed the FUSE edit, bounded output, Commit readback, restart binding and
historical conflict assertions.

| Public phase | SDK wall (ms) | Owner Hello (ms) | Daemon control (ms) |
| --- | ---: | ---: | ---: |
| Create | 334.737 | unavailable | unavailable |
| Mount | 28.964 | 5.460 | 22.636 |
| Exec | 31.994 | 4.495 | 26.678 |
| Commit | 51.043 | 4.477 | 45.826 |
| Unmount | 8.015 | 4.943 | 2.568 |
| **Enclosing route** | **454.830** | unavailable | unavailable |

The four post-Create calls total **120.017 ms**. In the preceding source's
single diagnostic, they totalled **110.843 ms** and Exec was **28.506 ms**.
These are separate uncontrolled machine/cache windows. The pipe wait is now
event-driven when a pipe remains open, but this receipt does **not** demonstrate
a wall-time gain; the observed Exec and route walls were higher. No repeated
sample was taken to seek a faster reading.

Host route telemetry had 47 samples, zero gaps, 17.628 ms sampled user CPU
plus 21.305 ms sampled system CPU, and 28.125 MiB sampled maximum RSS.
Host/Service CPU and RSS are process-shared samples; Exec child and Docker
cgroup resources are excluded. The complete command wall was **3.288 s**.
Cache state was uncontrolled, so functional status is **PASS** and performance
admission is **INELIGIBLE**.

The [raw receipt](evidence/sdk-route-exec-poll-20260924-01/report.json) holds
the exact command, binary/image/source identities, host and daemon LFT1
streams, report, and SHA-256 manifest. Mount and Commit daemon windows still
need internal attribution before a larger algorithm change is justified.
