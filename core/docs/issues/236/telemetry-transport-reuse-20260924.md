# Live SDK route with reused Service and control transport

> **Status:** Dated functional diagnostic; not release evidence and not a
> performance claim.

Source `741a82cbc7a486dadaa3a7b15510879599bbafce` (tree
`6791d5a6a2a0ad0ca5f91800bbf834f2b4b95dd2`, clean) took one live Docker
diagnostic against the daemon image
`sha256:ee46256a7b4b0f67d60c1f4fa2ea87b6c4945178fd16203cccaaa43c55c17668`.
The route passed functionally, including the FUSE edit without publication
before Commit, bounded Exec output, exact Commit readback after reopening the
Branch head, exact older-Commit readback, `HeadMoved`, Unmount custody, and
stale Workspace rejection after a container restart.

| Public phase | SDK wall (ms) | Daemon window (ms) | Owner Hellos in window |
| --- | ---: | ---: | ---: |
| Create | 294.553 | unavailable | — |
| Mount | 24.901 | 16.593 | 1 |
| Exec | 29.838 | 28.393 | 0 |
| Commit | 49.127 | 47.729 | 0 |
| Unmount | 4.030 | 2.113 | 0 |
| **Enclosing route** | **402.519** | — | — |

Daemon windows are the first matching daemon operation for each phase. The test
runs three Execs in total (the measured one plus two later correctness probes),
so the daemon records three `WorkspaceExec` events; only the first lies in the
measured Exec window.

The four calls after Create total **107.896 ms**. Three of them issued **no**
checked Hello of their own: they ran on the control session Mount had already
established. The prior corrected receipt (`436a93ae6`) recorded one Hello in
each of those four windows. This row's first Create split shows where its 294.553 ms
went: Docker launch 190.603 ms, port verification 25.414, daemon readiness
4.426, shell readiness 73.880 ms.

## What this row does and does not show

It shows the reuse works on the real live route and that the correctness proof
still holds. It does **not** establish a speedup. One sample, uncontrolled OS
and source cache state, and a Create window that swings with Docker scheduling:
functional **PASS**, performance **INELIGIBLE**, `admission_eligible=false`.
The prior row's 404.007 ms and this row's 402.519 ms are not a comparison — they
come from different machine windows, and the enclosing route is dominated by
Create, which no change here addresses.

The connection reduction is countable, not inferred:

- Owner control sessions across the four post-Create calls: **4 → 1**. The
  remaining one is Mount's; Exec, Commit and Unmount reused it.
- Daemon-to-host Service connections inside Mount and Commit: the same five
  upstream operations still run (`HistoryQuery`, `Inspect`, `EditFile`,
  `UpdatePortableMetadata`, `HistoryCommand`), but they now share **one**
  connection per phase instead of one per call: **5 → 2**.
- Upstream operation counts are unchanged, which is the point: this changes
  transport lifetime, not the work requested.

`layerfs-daemon::transport` holds one session per delivery thread, so unrelated
concurrent FUSE requests cannot be serialized by it. Any failure drops the
session, so an uncertain mutation is never resent; an idle session past two
seconds is closed, inside the server's five-second idle limit. A retained
control socket is handed back only after a Hello on that same socket confirms
the live daemon still reports the validated instance, so a restarted daemon
cannot answer an operation addressed to its predecessor.

## Limits

- One sample; no distribution, median or repeatability result is implied.
- Cache state uncontrolled: no cold or warm claim.
- Host and Service share one process, so their sampled CPU/RSS are shared, not
  exclusive call costs. The Exec child and the Docker cgroup are excluded.
- Daemon windows are in the daemon's own clock domain and are not additive with
  the host windows.
- The route is dominated by Create (294.553 ms of 402.519 ms); this row says
  nothing about Create latency beyond splitting it.
- No Core benchmark family was run. This is the SDK route diagnostic only.

The [raw receipt](evidence/sdk-route-reuse-20260924-01/report.json) keeps the
exact command, source/binary/image identities, host and daemon LFT1 streams, the
report, and a SHA-256 manifest over all 16 retained files. The complete command
wall was **3.367 s**. Host and daemon summaries both recorded zero drops.
