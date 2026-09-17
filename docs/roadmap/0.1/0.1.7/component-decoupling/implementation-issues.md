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
core/crates/layerfs-storage. The [implementation report](stages-0-2-report.md)
records the completed foundation and subsequent fixes; read its limitations and
amendments with the exact source under review. The original recommendation was
4,241–7,479 new production LOC across 46 source files:
1,408–2,479 for C1 and 2,833–5,000 for C2 including runtime SQL. These estimates
exclude tests/docs/examples/manifests/tooling and are not physical file ceilings.
Actual results and misses must be reported, never hidden by weakening correctness.

## Current continuation: Stages 3–4

Use the [continuation prompts](stages-3-4-continuation-prompt.md) for #168/#169:
correct D's oracle and complete stored-tree split/concat/finality; independently
finish pooling coverage and component qualification; then final combined acceptance.
The [original handoff](stages-3-4-handoff.md) and
[exact file/LOC plan](stages-3-4-file-plan.md) retain the full acceptance scope.
The first [report](stages-3-4-report.md) recorded missing pooling and stored-node
reuse. The later [completion report](stages-3-4-completion-report.md) records E's
implementation and D's oracle. Pooling boundary/chain/resource evidence, stored-node
reuse and exact-root/finality proof remain outstanding. The batch-normalized oracle
comparison must first align current-result coordinates on both sides; source
inspection found the candidate harness adds the insertion offset a second time.
Stage 3 includes physical inode-value pooling; Stage 4 includes both transitions
and bounded multi-edit finality. Filesystem algorithms, full-core qualification
and runtime integration stay with #170, #171 and #172 respectively.

The planning baseline has 5,420 C1+C2 production LOC; the recommended final range
is 8,720–14,230, including existing source and runtime SQL. This is capability
growth, not an asserted code-size or performance win. Actual v0.1.6 improvement
requires the handoff's matching successful operation, storage and resource evidence.
No aggregate preflight or replacement wrapper is allowed; verify the affected
workspace with explicit commands under current repository rules.

Use the [Stages 3–4 reviewer handoff](stages-3-4-reviewer-handoff.md) for independent
acceptance: actual structure/LOC, complete criteria, simplification, measured
efficiency/memory and a limits audit distinguishing enforced bounds, tested ranges
and deferred filesystem/Workspace ownership. It reports readiness without changing
issue state.

The [completion handoff](stages-3-4-completion-handoff.md) retains detailed proof
requirements. Current scheduling comes from the three continuation prompts; it
does not require all pooling evidence before D, or D before independent pooling
measurements. Combined edit qualification waits for the corrected algorithm.

The owner-closed Stage 0–2 scopes remain closed; their separate carried inventory is
[#174](https://github.com/Ephemeral-AI-Lab/layerfs/issues/174). Do not treat those
residuals as passed or reopen unrelated work. Any residual that affects a new
Stage 3–4 claim must be accounted for in its own acceptance evidence.

## Publication and status

Issue creation and document preparation do not implement or qualify the engine.
The current reviewed design/policy changes may be uncommitted; do not assume a
fresh checkout of main contains them. Issues have self-contained scope and #167
contains the complete handoff. No source commit/push, release or benchmark result
is claimed by this tracker. Read current GitHub state for issue completion; this
page records their mapping rather than mirroring a second status database.

## Stages 3-4 closeout (2026-09-17)

The Stages 3-4 batch (#168 / #169, under #165) was closed out against the
independent review of `91c3a0741fff64e8161d5c1b6e759f347ffbf757`. The completion
gate table, the per-packet changes, every control run, the memory ledger and the
remaining owner decisions live in
[`stages-3-4-closeout-report.md`](stages-3-4-closeout-report.md); the raw evidence is
in `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-closeout-20260916T235641Z/`.

Two items need an owner answer and are recorded there as escalation E2/E1: the
pooled lane assignment against the design table (value groups in v6, pooled leaf
records in the v1 ordinary lane, v5 refused by scope) and the repeated-sample
measurement campaign. Everything else the review named is fixed or measured.

All twenty completion-gate rows are satisfied: eighteen PASS on collected evidence
and two, G13 (the matched campaign) and G15 ("existing-or-better"), PASS by the
owner's written waiver of 2026-09-17 on the finding that no flaw is open in the
recorded audits. The waiver is recorded in
[`stages-3-4-closeout-report.md`](stages-3-4-closeout-report.md) §1 with exactly what
it does **not** claim: latency, storage and memory remain unqualified against v0.1.6,
no comparison campaign exists, and the qualification is deferred to Stage 6 (#171).
The row-by-row text posted on each issue is the report's §4, and the verification
contract's case registry is RUN or NOT_RUN in
[`stages-3-4-verification.md`](stages-3-4-verification.md) §2.1.
