# Agent SDK route and v0.1.6 legacy route: one diagnostic each

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The v0.1.6 tag resolves to product commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.
One host diagnostic on that clean tree used immutable image
`sha256:cabfe428d4db68e122ed3f7f40fe9ca0dd7cf0693e8b283b3259faf1cbe1e6ab`.
It is shown beside the existing Core SDK diagnostic from source commit
`59a12f22ab4475c3d26f417b509a9792e872ff4f`; Core was **not** resampled.
Both completed their functional sequence and independently reopened the published
one-file result. Both have uncontrolled OS/source cache state, so neither is a
performance admission PASS.

| Phase | v0.1.6 legacy host wall (ms) | Current Core SDK host wall (ms) | Current minus v0.1.6 (ms) |
| --- | ---: | ---: | ---: |
| Container Create/Start/Connect vs `sandbox.create` | 751.606 | 258.825 | -492.781 |
| FUSE session create vs `workspace.mount` | 14.668 | 78.102 | +63.434 |
| Exec and output drain vs `workspace.exec` | 4.912 | 64.296 | +59.384 |
| `commit_workspace_session` vs `workspace.commit` | 8.053 | 104.094 | +96.041 |
| End(Clean) vs `workspace.unmount` | 2.639 | 47.840 | +45.201 |
| **Enclosing five-step route** | **782.075** | **553.229** | **-228.846** |

| Enclosing host process observation | v0.1.6 | Current Core SDK |
| --- | ---: | ---: |
| Sampled CPU (user + system) | 40.854 ms | 44.226 ms |
| Sampled maximum RSS | 16.73 MiB | 28.52 MiB |
| Samples / gaps | 79 / 0 | 57 / 0 |

The v0.1.6 host probe uses the **existing Core `layerfs-telemetry` crate as an
external diagnostic recorder**, with the same 10 ms process CPU/RSS monitor. It
does not alter the v0.1.6 product. The legacy daemon has no LFT1 instrumentation,
so its process CPU/RSS and control-dispatch time are unavailable here. A 10 ms
sample window left the legacy 8.053 ms Commit with CPU unavailable; short-phase
CPU figures are not exclusive and can exceed phase wall due sampling boundaries.
Neither host RSS figure includes complete container or file-cache memory.
The probe did not retain a complete external command-wall measurement; the table
reports only LayerFS operation windows.

The operations are similar but **not identical comparison arms**. v0.1.6 splits
container Create and Start and requires `Client::connect_with_container` before a
FUSE Workspace session; all three are in its first timer. Current `sandbox.create`
uses Docker Run, port discovery, authenticated readiness and a shell check. The
legacy Exec API returns an execution handle, so its timer includes output drain
and exit acknowledgement; current `workspace.exec` returns a bounded completed
result directly. Legacy End(Clean) and current Unmount have different ownership
and routing contracts. The images, daemon implementations, host SDK/Service
topology, and control protocols also differ. The wall differences are therefore
descriptive observations, **not a version speedup or regression claim**.

The contrast isolates one useful direction for investigation: outside the first
step, the four legacy calls total 30.271 ms and the four current SDK calls total
294.332 ms. The retained Core LFT1 records already attribute 121.591 ms of its
four calls to daemon control dispatch and leave 172.741 ms at the broader SDK
boundary. The legacy diagnostic does not identify the corresponding internal
boundaries, so no single component is assigned that difference.

The v0.1.6 run used release-tag product **source** and a release-matched Linux
image. Its debug host probe was built in an external Cargo project so the tag
remained clean and the existing Core `layerfs-telemetry` crate could record it.
That project's generated lockfile differs from the release `Cargo.lock`:
21 package names have different selected version sets. Thus the host probe is
**not an exact released v0.1.6 binary/dependency build**, another reason these
numbers cannot qualify a version comparison. Both host diagnostics used debug
builds, but the current Core test used its locked Core workspace dependencies.
The legacy probe wrote
`printf first > note` through the FUSE mount, explicitly committed, ended the
session, then reopened the Branch in a new session and checked the exact bytes.
The complete command exited 0, removed its container, and its telemetry run
summary reported zero dropped or failed records. The [raw evidence and derived
report](evidence/sdk-route-v016-20260924-01/report.json), [source/image/probe
identities](evidence/sdk-route-v016-20260924-01/measured/identities.json), and
[SHA-256 manifest](evidence/sdk-route-v016-20260924-01/manifest.json) are retained.
The original [Core diagnostic](telemetry-result-20260924.md) remains unchanged.

An earlier probe attempt failed before telemetry startup and before Docker
creation because its runner had already created the output directory. It emitted
no operation records. Its raw files are retained as
[`premeasurement-failure`](evidence/sdk-route-v016-20260924-01/premeasurement-failure/);
the corrected probe produced the one measured v0.1.6 route above. No performance
arm was repeated or replaced.
