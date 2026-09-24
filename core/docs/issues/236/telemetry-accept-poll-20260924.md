# Daemon control accept readiness: one functional telemetry diagnostic

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

At clean source `af04f21ae8aef3193f38691065005b85a8338827`, the daemon
control listener uses readiness polling instead of an unconditional 10 ms park
after a nonblocking accept miss. One live Docker route passed its functional
assertions with image
`sha256:bda2f50ebda7700b064ee10f014969f158192568f95d488ae7629c2fce63d1f5`.

| Public phase | SDK wall (ms) | Owner Hello (ms) | Daemon control (ms) |
| --- | ---: | ---: | ---: |
| Create | 306.839 | unavailable | unavailable |
| Mount | 46.680 | 6.729 | 38.575 |
| Exec | 41.368 | 4.836 | 35.638 |
| Commit | 71.621 | 5.578 | 65.313 |
| Unmount | 8.332 | 5.505 | 2.350 |
| **Enclosing route** | **474.935** | unavailable | unavailable |

The four Hello windows total **22.648 ms**; the prior fixed-port diagnostic
had **46.293 ms** in a separate window. The source change removes the daemon's
fixed accept park, but the observations do not isolate its causal contribution.
The enclosing route was **20.102 ms longer** than that prior diagnostic, with
Create and daemon dispatch varying. No speedup claim follows from these rows.
The current live test's host Service accept loop itself sleeps up to 10 ms on
`WouldBlock`; production Service already uses readiness polling. That test
artifact must be removed before interpreting daemon-to-Service RPC latency.

Host process telemetry reported 49 samples, zero gaps, 19.511 ms sampled user
CPU plus 23.976 ms sampled system CPU, and 32.61 MiB sampled maximum RSS.
Host and Service share this process; daemon and Exec child have different
resource scopes. The host and primary daemon LFT1 summaries recorded zero
drops. Cache state was uncontrolled. The row is a functional **PASS** and
performance **INELIGIBLE**, not a registered benchmark arm.

An earlier image packaging attempt at the same source copied the Linux daemon
binary without its executable bit. Docker could not start it; `sandbox.create`
failed with a retained sandbox and the test exited 101. That failed attempt is
preserved separately and is not treated as a performance sample. The corrected
image had mode `0555`; the daemon binary SHA-256 was
`f375a89893b98c65be45c75241e3cbb9a852c94f9353f4327bf80affb3c0b1fe`.

The [evidence directory](evidence/sdk-route-accept-poll-20260924-01/manifest.json)
holds both attempts, raw LFT1 streams, exact commands, source and image
identities, derived report, and SHA-256 manifests. The successful complete
command wall was **3.503 s**, below the 15 s exploratory command budget.
