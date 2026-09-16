# v0.1.7 implementation issues

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Parent: [#165 — implement the decoupled C1/C2 core with independent timing and qualified runtime integration](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).

These are seven native GitHub sub-issues. Stage 0 is bundled with Stage 1;
Stages 2–7 each have their own child. This is an execution breakdown, not seven
new crates. Design remains tracked by [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160),
release by [#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155), and completed
telemetry by [#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161).

| Issue | Scope | Depends on |
| --- | --- | --- |
| [#166](https://github.com/Ephemeral-AI-Lab/layerfs/issues/166) | Stages 0–1: establish core contracts and implement independently timed C1 construction | Existing telemetry and source guard |
| [#167](https://github.com/Ephemeral-AI-Lab/layerfs/issues/167) | Stage 2: implement real CAS/FULL persistence and independent C1/C2 timers | #166 |
| [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) | Stage 3: complete bounded delta, compression, pooling and pack policy | #167 |
| [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) | Stage 4: implement localized file edits, decoded COW boundaries and size transitions | #167, #168 |
| [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) | Stage 5: implement independent filesystem trees, attributes and reference ordering | #168, #169 |
| [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) | Stage 6: qualify the complete C1/C2 core for correctness, performance and memory | #166, #167, #168, #169, #170 |
| [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) | Stage 7: integrate one qualified Workspace/FUSE and host-daemon runtime with the new core | #171 |

## First handoff: Stages 0–2

Use [stages-0-2-handoff.md](stages-0-2-handoff.md) to complete #166 and #167.
The full prompt is also embedded in #167, including exact file paths, per-file and
recursive per-directory LOC estimates, external tests, actual commands and honest
completion criteria. The parent and later stages remain open after this handoff.

Use [stages-1-2-reviewer-handoff.md](stages-1-2-reviewer-handoff.md) for independent
acceptance review: resulting structure/LOC, criterion-by-criterion evidence,
simplification opportunities, speed/efficiency statistics and separate bounded-memory
and memory-safety conclusions. The review reports readiness without closing issues.

Selected package homes are core/crates/layerfs-content and
core/crates/layerfs-storage; only layerfs-telemetry is currently implemented.
The recommendation is 4,241–7,479 new production LOC across 46 source files:
1,408–2,479 for C1 and 2,833–5,000 for C2 including runtime SQL. These estimates
exclude tests/docs/examples/manifests/tooling and are not physical file ceilings.
Actual results and misses must be reported, never hidden by weakening correctness.

## Publication and status

Issue creation and document preparation do not implement or qualify the engine.
The current reviewed design/policy changes may be uncommitted; do not assume a
fresh checkout of main contains them. Issues have self-contained scope and #167
contains the complete handoff. No source commit/push, release or benchmark result
is claimed by this tracker. Read current GitHub state for issue completion; this
page records their mapping rather than mirroring a second status database.
