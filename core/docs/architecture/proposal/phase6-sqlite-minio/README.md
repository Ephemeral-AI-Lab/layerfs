# Phase 6: SQLite metadata and MinIO CAS architecture

> **Status: Current planning checklist; no release candidate exists.**
> Owner direction and initial design folder, 2026-10-01.
> Tracking: [#293](https://github.com/Ephemeral-AI-Lab/layerfs/issues/293).

## Purpose and authority

**Proposed:** simplify LayerFS around daemon-owned embedded SQLite for live
Workspace metadata, immutable MinIO packs and a small global metadata service
owning SQLite. Move streaming C1/C2 construction to the daemon/client. Preserve
bounded memory, ordinary filesystem mutations, history, concurrent ownership and
exact failure/Unknown custody.

The owner selected the simplified ownership model on2026-10-02 and authorized
implementation. [ARCHITECTURE](ARCHITECTURE.md) and [S1-SPEC](implementation/S1-SPEC.md)
record the chosen authority/trust path and exact first delivery. Broader profiles,
concurrency,import/cutover,compatibility and persistence remain **open — required**. No checkbox in #293 is
satisfied merely by creating these documents. Phase 6 is the successor design
direction to [#290](https://github.com/Ephemeral-AI-Lab/layerfs/issues/290);
it is distinct from the historical Core Stage 6 numbering.

## Source baseline and prior work

Selected published product baseline:
[`7edddbdb8e8512627aed0ed42533ef099d802384`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/7edddbdb8e8512627aed0ed42533ef099d802384).
Audit this source before freezing a reuse or retirement decision. The primary
checkout and other owners' worktrees remain independent of this proposal branch.

The Phase 5 [#287](https://github.com/Ephemeral-AI-Lab/layerfs/issues/287)
candidate product
[`b2c6bcc97`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/b2c6bcc9735c6267576908cac31642aca6527d23)
and final report
[`5d693cc76`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/5d693cc7644e6f08fe5011b9b6020c27a9713db1)
are unmerged into the selected baseline. Its recorded R1 disposition is PARTIAL;
R2–R7 are unstarted. Preserve its evidence and failures. No main revert or
candidate deletion is part of creating this folder.

## Reading order

| Document | Design responsibility |
| --- | --- |
| [ARCHITECTURE](ARCHITECTURE.md) | Authority split, data/control paths and trust decisions |
| [BOUNDED_STREAMING](BOUNDED_STREAMING.md) | Population storage, admitted windows, work laws and physical resource proof |
| [COMMIT](COMMIT.md) | Capture, construction, publication, installation and outcome custody |
| [DESIGN_PLAN](DESIGN_PLAN.md) | Required decisions, cutover inventory and smallest complete delivery |

Labels follow the [proposal conventions](../README.md): **proposed** describes
intent; **open — required** blocks a contract freeze; **deferred** names work
outside this folder's delivery. Source-backed descriptions stay in the existing
architecture set until an implementation changes them.

## Evidence entry points

[#291](https://github.com/Ephemeral-AI-Lab/layerfs/issues/291) owns the standalone
SQLite/MinIO experiments. Their product binaries have the older
`ffdfa022f21930f3e7325b95e6ae5c2b11b7d2a0` source pin. The following reports are
pinned to their published evidence snapshot; they do not qualify the selected
Phase 6 baseline:

- [Full deepseek-harness import](https://github.com/Ephemeral-AI-Lab/layerfs/blob/45f2b67601a759d7ebdef44f517125e31332353d/docs/roadmap/0.1/0.1.7/issue290/RESULTS-DEEPSEEK-FULL-V1.md).
- [Construction attribution](https://github.com/Ephemeral-AI-Lab/layerfs/blob/45f2b67601a759d7ebdef44f517125e31332353d/docs/roadmap/0.1/0.1.7/issue290/RESULTS-CONSTRUCTION-ATTRIBUTION-V1.md).
- [Importer optimizations](https://github.com/Ephemeral-AI-Lab/layerfs/blob/45f2b67601a759d7ebdef44f517125e31332353d/docs/roadmap/0.1/0.1.7/issue290/RESULTS-CONSTRUCTION-OPTIMIZATION-V1.md).
- [Python WAL/fsync profiles](https://github.com/Ephemeral-AI-Lab/layerfs/blob/45f2b67601a759d7ebdef44f517125e31332353d/docs/roadmap/0.1/0.1.7/issue290/RESULTS-PYTHON-WAL-V4.md).

Numerical speed rows retain cache INELIGIBLE / `performance_claim=false`.
Byte correctness, speed, physical containment, namespace certification and
cloud durability remain separate proofs. The existing qualification companion
[#288](https://github.com/Ephemeral-AI-Lab/layerfs/issues/288) has not been run by
this design work; future qualification scope must be reconciled explicitly.

The first real SDK/FUSE/SQL/MinIO/C5 integration proof is recorded in
[RESULTS-INTEGRATION-V3](experiments/RESULTS-INTEGRATION-V3.md), with its finite
research profile and remaining speed/resource/canonical gates.
