# Sandbox lookup substeps: one incomplete route diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

At clean source commit `e448f534a1aecd79d2bcc0cd72305a6104d994dc`, the
telemetry-enabled live Docker functional route passed. The existing LayerFS
runtime recorded the checked owner's Docker port lookup and authenticated Hello
within each public Workspace call. The host LFT1 run summary reported one
dropped record: the enclosing `sdk.route` operation is absent, so the ordinary
five-step reporter refused to produce a complete route report. This attempt is
retained as `INCOMPLETE_MISSING_ROUTE`; it was not repeated or promoted into a
performance PASS.

| Public call | SDK wall (ms) | `docker port` (ms) | authenticated Hello (ms) | daemon control dispatch (ms) |
| --- | ---: | ---: | ---: | ---: |
| Mount | 78.677 | 24.298 | 13.577 | 39.577 |
| Exec | 71.794 | 26.351 | 12.723 | 31.684 |
| Commit | 86.653 | 25.269 | 8.865 | 51.853 |
| Unmount | 39.789 | 22.818 | 14.199 | 2.141 |
| **Four-call total** | **276.913** | **98.736** | **49.364** | **125.255** |

The two host lookup substeps total **148.099 ms**. Subtracting daemon control
elapsed from the four public-call elapsed values leaves **151.657 ms**; the
remaining arithmetic difference is **3.558 ms**. Thus the one-run evidence
locates almost all of the broader SDK boundary in Docker CLI port discovery and
authenticated Hello. These spans do not separate Docker process startup from
Engine work, or control accept wait from cryptographic and bridge handshakes.
Daemon and host clocks are independent; only elapsed durations are compared.

The checked lookup still resolves the current Docker-published port on every
call, as the #236 implementation plan requires for restart handling. The
one-connection SDK change was already present in this source. Changing port
discovery needs a prospective contract revision with the same verified
sandbox/daemon-instance behavior; a cached but stale port or an automatic
retry of an uncertain mutation is not acceptable.

The [raw LFT1 streams, exact identities and command, incomplete reporter error,
partial cause report, derivation script and SHA-256 manifest](evidence/sdk-route-owner-lookup-20260924-01/partial-report.json)
are retained together. The functional test exited 0; the report generator exited
1 because the enclosing route record was missing. Source and OS cache state were
uncontrolled, sampled CPU/RSS remain process-shared and incomplete for container
memory, and `performance_admission=INELIGIBLE`.
