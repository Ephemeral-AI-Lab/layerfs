/goal Determine whether WP4-P should promote a v1 K/F mapping profile before
the proposed canonical-v2 36-byte reference format is accepted or rejected,
and write one evidence-backed decision report.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. The active WP4-M
task is concurrently modifying and testing the profile benchmark. Do not edit,
message, interrupt, steer, wait on, or infer a winner from that task.

You may create exactly one file:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-a-profile-promotion-order/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

Do not edit this prompt, the parent index, any other research document, tracked
source, implementation-detail file, Git state, or `target/` artifact.

Do not run Cargo, rustc, tests, SQLite, compression, filesystem experiments, or
performance measurements. Use only read-only inspection and arithmetic over
already sealed evidence. When a tracked working-tree file is dirty, read the
committed version with
`git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` unless a sealed
custody source is explicitly more authoritative.

If WP4-M has begun measured release rows, do not run local shell commands or
write the report until those rows are quiet or the active task is terminal;
web research and reasoning may continue.

## Authority and known state

- Branch: `codex/empty-worktree`.
- Campaign starting commit:
  `d781173a08ab4092eb539c3a0870056e6c6a77ff`.
- WP4-M is measuring, not selecting or promoting:
  K64/F64, K59/F101, K256/F256 and 64-KiB/256-KiB/1-MiB directory ceilings.
- Current v1 file reference width: `68` bytes
  (`raw_id[32] + raw_length[4] + canonical_id[32]`).
- Proposed compact v2 reference width: `36` bytes
  (`raw_length[4] + canonical_id[32]`).
- WP4-P must eventually choose one profile, delete private alternatives and
  selectors, regenerate goldens, and freeze a single mapping authority.
- No WP4-M profile result is final until the active task publishes its terminal
  manifest and audit.

## Read first

Read completely where relevant:

1. `research/phase-4/index.md`
2. `research/phase-4/decision-map.md`
3. `research/phase-4/core/canonical/v2-single-identity.md`
4. `research/phase-4/core/canonical/identity-and-hashing.md`
5. `research/phase-4/core/pipeline/full-create-pipeline.md`
6. `research/phase-4/foundations/invariant-matrix.md`
7. `implementation-detail/phase-4/mapping/logical-persistence.md`
8. `implementation-detail/phase-4/algorithm/spec.md`
9. `implementation-detail/phase-4/algorithm/complexity-analysis.md`
10. `implementation-detail/phase-4/wp4m/f-series/planning/full-create-plan.md`
11. `implementation-detail/phase-4/rollback/spec.md`
12. `implementation-detail/phase-4/rollback/implementation-plan.md`

Use the active task link only to confirm that WP4-M remains nonterminal. Do not
consume partial implementation or partial rows as evidence.

## Research question

Determine whether changing reference width from 68 to 36 bytes can change the
correct permanent K/F decision enough that promoting a v1 profile first would
create avoidable goldens, roots, migrations, or a second profile-selection
campaign.

Analyze at least:

- which K/F motivations depend on serialized reference width;
- verify the framing constants from authoritative source, then test the
  candidate equations `Leaf_v1(K) = 28 + 68K` and
  `Leaf_v2(K) = 28 + 36K` rather than assuming them;
- if verified, show the checkpoints `v1 K59 = 4040`, `v1 K60 = 4108`,
  `v2 K59 = 2152`, `v2 K64 = 2332`, `v2 K113 = 4096`, and
  `v2 K256 = 9244` bytes, and explain any disagreement;
- whether the v1 page-fit rationale for K59 could shift toward K113 under v2,
  without treating a page fit as a performance result;
- branch/root topology and height effects at 100 MiB, 512 MiB, and the
  analytical 100-GiB scale;
- SQLite page/overflow sensitivity as a model only, never a measurement;
- Q/frontier effects;
- range-authentication and same-count COW effects;
- whether directory ceilings and their serialized entries are truly
  independent of the file-reference change;
- which WP4-M evidence remains reusable under v2;
- cost and authority risk of promoting v1 and then migrating;
- whether WP4-P can select a topology family without freezing final v2
  capacities;
- profile-neutral optimizations and fair-comparison requirements.

Do not choose a WP4-M winner, invent terminal campaign data, or assume compact
v2 will be accepted.

## Evidence rules

Label every material claim:

- `Observed` — directly in committed source, specification, or sealed evidence;
- `Derived` — show operands and equation;
- `Hypothesis` — a proposition requiring future evidence;
- `Unavailable` — name the missing terminal result or observation.

Separate logical/canonical bytes from SQLite page bytes and physical I/O. Do
not turn page-fit arithmetic into a wall-time prediction.

## Required report

Write only the assigned `report.md`, containing:

1. executive disposition;
2. authority snapshot and explicit statement that WP4-M is incomplete;
3. v1/v2 reference and mapping-size equations;
4. profile-sensitivity table for K64/F64, K59/F101, and K256/F256;
5. dependency graph from WP4-M through WP4-P, canonical v2, WP5, and goldens;
6. what current WP4-M evidence remains valid under each ordering;
7. double-promotion/migration risks;
8. decision criteria to apply after the WP4-M terminal report;
9. exact next handoff, with no implementation steps beyond the decision
   boundary;
10. primary/local sources linked near supported claims.

End with exactly one disposition:

- `PROMOTE_V1_NOW`
- `DELAY_WP4_P_FOR_V2`
- `SELECT_TOPOLOGY_ONLY`
- `INSUFFICIENT_UNTIL_WP4M_TERMINAL`

If the disposition depends on the eventual profile table, provide an explicit
predicate using terminal operands rather than guessing the result.

## Completion

The task is complete only when the report is internally consistent, all local
links resolve, no other path changed, and the final response gives the report
path, disposition, main dependency, limitations, and SHA-256.
