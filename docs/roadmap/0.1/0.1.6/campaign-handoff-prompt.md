# Handoff prompt — v0.1.6 full benchmark campaign (#152)

You are the measurement agent for **issue #152**: run the full registered benchmark
suite on the frozen v0.1.6 candidate, **group by group** (the same 8-group phase
order the v0.1.5 finalization campaign used), fix what breaks, and report honestly.
Read issue #152 itself first — it is the contract; this prompt is how to execute it.

## Read before you touch anything

### Normative rules — read fully, they bind this campaign

| file | what you need from it |
|---|---|
| [`AGENTS.md`](../../../AGENTS.md) | repo-wide agent rules: no warm-cache credit, `--setup clone` reuse discipline, budgets, the third-party ban, no-CI/preflight |
| [`benchmark/AGENTS.md`](../../../benchmark/AGENTS.md) | benchmark-tree mechanics: prepared inputs, `--reuse-pass`, clone semantics, budgets, the v0.1.6 hosting exception |
| [`docs/general/benchmark_rules.md`](../../general/benchmark_rules.md) | the measurement contract: cache state, memory domains, reuse, reporting fields |
| [`docs/general/release-policy.md`](../../general/release-policy.md), [`docs/general/documentation-policy.md`](../../general/documentation-policy.md) | what may be claimed, committed and released |
| [`benchmark/fs-bench-pro/QUICKSTART.md`](../../../benchmark/fs-bench-pro/QUICKSTART.md) | build/reuse/run mechanics, `--setup fresh` vs `--setup clone`, family entrypoints |
| [`tools/preflight.sh`](../../../tools/preflight.sh) | the local gate you run before **every** push — this repository has no CI |

### This campaign's contract

| file | what you need from it |
|---|---|
| issue **#152** | scope, the 8 groups, budgets (15 s, exceptions 25 s), acceptance (<50 % or <10 ms), #122 exclusion, reporting protocol |
| this prompt | the execution protocol below |
| issue **#125** (closed, superseded) | the original objective and the #122 exclusion reference; the JSON draft it names is **not** in the tree — confirm the exclusion set with the owner instead of inventing one |
| [`docs/roadmap/0.1/0.1.6/experimental-implementation-pipeline.md`](experimental-implementation-pipeline.md) | the I0–I6 + B1–B3 checklist and exit criteria this campaign closes |

### The architecture you are validating

| file | what you need from it |
|---|---|
| [`sandbox-local-snapshot-spec-and-plan.md`](sandbox-local-snapshot-spec-and-plan.md) | the frozen spec and plan: §2 ownership boundary, §3.4 volatile contract, §5 resource gates (64 MiB accounted, 8 MiB staging, 32 MiB 25k backing), §6 cases/timing and §6.3 counters to collect, §7 hard gates, §9 focused proofs |
| [`sandbox-host-connection-architecture.md`](sandbox-host-connection-architecture.md) | the simplified host↔docker connection: what crosses the boundary and the opcode surface |
| [`sandbox-host-connection-review.md`](sandbox-host-connection-review.md) | review findings and dispositions — including what was deliberately left unsolved (dirty shared mmap) |
| `crates/layerfs-fuse/src/local_spool.rs` | the sandbox spool and its bounded resident window (L18) |
| `crates/layerfs-fuse/src/live_owner.rs` | the snapshot lane: `CAPTURE`, `SNAP_RECORDS`, `SNAP_READ`, `COMPLETE_*`, and the post-serve cache drop |
| `crates/layerfs-fuse/src/live_backing.rs` | the host-side immutable-base service (`SEED`/`LOOKUP`/`LOOKUP_METADATA`/`DIRECTORY_PAGE`/`READ_BASE`) |
| `crates/layerfs-daemon/src/protocol.rs` | opcode inventory and wire bounds |
| `crates/layerfs-workspace/src/{lifecycle.rs,remote_commit.rs,changes.rs,snapshot_input.rs}` | Commit dispatch, the remote commit route, the single construction gate/worker limit, bounded transfer windows |

### Evidence, history and the reporting shape

| file / path | what you need from it |
|---|---|
| [`evidence/issue151-experiment-ledger.md`](evidence/issue151-experiment-ledger.md) | L12–L14 gate history and host-state spread; **L18** bounded-cache repair, cache-stance evidence, memory-metric finding; **L19** B3 and the three harness defects; **L20** accepted dispositions, limitations, adoption recommendation; **L21** CI removal |
| [`evidence/issue151-implementation-design.md`](evidence/issue151-implementation-design.md), [`evidence/issue151-execution-continuation.md`](evidence/issue151-execution-continuation.md), [`issue151-handoff-agent-prompt.md`](issue151-handoff-agent-prompt.md) | how the implementation was built, what was already repaired, and how it was handed over |
| `docs/roadmap/0.1/0.1.5/issue120/finalization-contract.md` | the disposition, severity and cache rules this campaign inherits (Tier 1/2/3, S0–S3, H) |
| `docs/roadmap/0.1/0.1.5/issue120/final-report.md` and `family-*.md` | the `family → per-test` reporting shape to match, and the v0.1.5 context for old ratios |
| `benchmark-results/host-store/issue120/{performance,verification,diagnostics}` | the recorded v0.1.5 comparator rows you cite instead of re-running |
| [`README.md`](README.md) | the current recorded state: accepted B1/B2/B3 dispositions and the limitations list |
| archived drafts | tag `archive/main-branch-cleanup-20260915-1005/heads/archive/main-uncommitted-20260915T0930Z` = `c5f85d546bd09b5384f5c5c1183123cc723e5798` holds the pre-promotion drafts (including `overlay-snapshot-*.md`). They describe the **discarded** host-overlay direction and an old exclusion draft — read for history only, never as contract |

### Environment facts you can rely on

- **Candidate**: `main` at `1558a121f` or later, product seal
  `31a42c95197a21c5acd54cb12e7398bd8cb5308fab916e62b9439ca0a17bf01d`, image
  `layerfs-bench-infra:9e3a4c91187729ec` (rebuild with `--build-host` / `--build-image`
  after any `benchmark/` change).
- **Control arm** (v0.1.5 comparators when a row is missing): `/Users/yifanxu/layerfs-v016-control`
  — outside the lab directory, product seal `276c5970…`, `SOURCE_DIRTY=true` by
  construction; verify the product seal reproduces before trusting a rebuild.
- **Prepared inputs**: `benchmark-results/host-store/prepared` (242 entries, ~43 GB) —
  reuse them; preparation runs automatically on a cache miss.
- **Banked receipts** (cite, do not re-run): `benchmark-results/issue151/perf-candidate5-…`,
  `perf-candidate3-…`, `perf-CAND-25k-r5` and their `verify-…` counterparts.

## Hard rules (from AGENTS.md and #152 — not negotiable)

- **One sample per case per arm. No n3, no alternating pairs, no repeats.** Comparability
  comes from the declared cache contract plus matched identities.
- **No warm cache may credit a measured phase.** Never prime, never re-run back-to-back
  and pool the warm row, never measure one arm warm and the other cold, never move work
  outside a timer, never inflate a timeout or change worker counts to pass.
- **Budgets:** preparation is reused, not repeated; a performance selection's complete
  command is **≤ 15 s** with a small declared owner-approved exception list up to
  **25 s**; verification typically **< 15 s** inside a **60 s hard budget**.
- **Acceptance (not an optimization campaign):** a comparative cell is accepted when it
  is **< 50 % worse** than its v0.1.5 comparator **or** the absolute delta is **< 10 ms**.
  Registered absolute targets stay pass/fail. Do not chase micro-optimizations and do
  not "fix" an already-accepted cell.
- **Never patch, vendor or locally modify a third-party crate/package**; builds stay
  `--locked` (AGENTS.md §4).
- **No CI exists.** Run `tools/preflight.sh` before every push; a push may never claim
  "CI green".
- Banked receipts — B1 `tiny-create-500-mixed-v4`, B2 `tiny-bulk-create-500-mixed-v3`,
  B3 `local-snapshot-create-25000-onebyte-v1` — are **cited, not re-collected**.

## Do not stop until the campaign is finished

The campaign runs to completion — all 8 groups, every registered selection terminal — as
one continuous effort. There is no partial campaign, no "collected a few groups and
paused", and no group left open for someone else.

- **Drive every open item to a terminal disposition** and then keep going. Terminal means
  PASS, WARN, FAIL (diagnosed, with a recorded escalation or owner waiver), REUSED-FROM or
  NOT_RUN_OPTIONAL. "Not started", "in progress", "flaky" and "unexplained" are not
  terminal.
- **Prefer fixing over parking.** An S2 FAIL is a work item: reproduce it in the smallest
  case, find the root cause, fix it, run `tools/preflight.sh`, re-seal, re-run the affected
  cases, and continue. A FAIL stays terminal only when its cause is understood and it has
  been escalated to the owner (or waived).
- **A blocker does not stop the campaign.** If a group is waiting on something only the
  owner can supply (G8's sealed `historical_access` store, the `repository_history`
  opt-in) or on an escalation decision (S0/S1), record the blocker precisely on #152,
  continue with the remaining groups, and return to the blocked group when the input
  arrives.
- **Do not stop early to report a favourable subset**, do not stop because the numbers
  look good, and do not stop to optimize: acceptance is already bounded (<50 % or <10 ms),
  so an accepted cell is done and the next group is the work.
- The campaign ends only with the final report described in "Definition of done": all
  groups posted, every selection terminal, bug ledger, architecture-guardrail evidence,
  resource tables, limitations and open owner decisions.

## Working protocol — group by group

For each group G1…G8 of #152, in order:

1. **Freeze the case list** with `fs-benchmark-pro infra-list <family>` (authoritative),
   and check which cells already have a qualifying v0.1.5 comparator
   (`benchmark-results/host-store/issue120/…`) or a banked receipt.
2. **Collect**: one performance sample per case, then a separate identity-pinned
   verification, both pinned to the candidate identities in the issue. Reuse by citation
   wherever evidence already exists.
3. **Fix what fails** (see the bug policy below), iterating fast.
4. **Append the facts to the ledger** (identities, exact command, numbers, arithmetic).
5. **Post the group summary on #152 before starting the next group** — requirement 1.
   Every case in the group appears, with its result; nothing is silently dropped.
6. Only then move to the next group.

### Group comment template

```markdown
## Group N/8 — <families>

Candidate: <source seal> @ <commit>; product <seal>; harness <id>; workload <hash>

| case | timer | candidate | v0.1.5 comparator | ratio | Δ abs | disposition | verify | cleanup | cache contract |
|---|---|---|---|---|---|---|---|---|---|

Absolute targets: <target → measured → PASS/FAIL>
Architecture guardrails: <per-item result + numbers, see below>
Bugs found & fixed: <commit — one-line RCA — impact set re-run>
Gaps / omissions / escalations: <none, or exactly what and why>
```

## Bug policy — expect bugs, fix them fast (requirements 2 and 3)

v0.1.6 was promoted days ago and has passed only a handful of end-to-end cases. **Assume
product bugs exist.** The campaign is also a bug hunt: a failing benchmark is a lead, not
a verdict, and a passing gate is not proof that the mechanism behind it is sound.

- Reproduce in the **smallest case that shows the defect** (usually a compact-1 or 100
  tier), read the code path, fix the cause, add or extend a focused test, run
  `tools/preflight.sh`, commit, re-seal, and **re-run only the affected cases** (impact
  set by call path, not by family). Report the fix, its RCA, and the re-run.
- Classification: **S0** correctness (stop collection, freeze the artifact, escalate
  immediately, no waiver), **S1** resource/custody/lifecycle (stop that family, escalate),
  **S2** gate miss (keep collecting, mark `FAIL — unrepaired` until fixed or waived),
  **S3** permitted WARN, **H** harness/test defect (product fine → fix it in `benchmark/`,
  re-seal both arms, re-run only that case; never touch the product for an H).
- A fix that changes a case that already passed **invalidates that case** — re-run it and
  say so.
- If a verification fails *identically in both arms* with a structural message, it is
  almost certainly **H**, not a product bug. #152 lists the three failure modes already
  seen (root-mode expectation, per-Commit evidence collision, missing bounded native
  recipe → ENOSPC on a huge namespace) and their fast-path fixes.

## Architecture guardrails you must verify (requirement 6) — evidence, not claims

v0.1.6's whole point is the **overlay snapshot mechanism**: a non-pausing workspace whose
mutable state and payload live in the sandbox, with the host↔docker connection reduced to
an immutable-base service plus a snapshot lane, and the **main transaction happening at
snapshot commit time**. For every group, check the claims you can reach and report the
result with evidence; a claim without evidence is not a result.

1. **Non-pausing, continuous workspace.** Commits must not freeze the workspace; commands
   keep running while a Commit is in flight, and state is discarded only on an explicit
   End/Discard. Confirm no `pause`/`freeze`/`quiesce` path remains in the candidate
   (grep the tree), confirm the existing tests cover it, and where the workload allows,
   exercise concurrent work during a Commit. Report what you actually exercised.
2. **Simplified host↔docker connection, transaction at commit.** Ordinary writes and
   reads must not round-trip to the host; the wire surface must be the immutable-base
   service (SEED/LOOKUP/LOOKUP_METADATA/DIRECTORY_PAGE/READ_BASE) plus the snapshot lane
   (CAPTURE/SNAP_RECORDS/SNAP_READ/COMPLETE_BEGIN/NODE/END). Publication must be atomic at
   Commit. Report the opcode inventory you find and, where a receipt exposes counts, the
   per-case host interaction counts.
3. **Snapshot is built incrementally.** A new generation extends its predecessor: records
   are paged and bounded, payload is pulled by range with a bounded window, and per-Commit
   work scales with the **change**, not with the tree. Collect the cheap counters before
   C1, while a snapshot is retained, after each Commit and at End (spec §6.3), and show
   that repeated Commits do not rebuild the namespace or retain an always-growing chain.
4. **Storage and RAM stay bounded and safe** (requirement 7). Hold the 25k transient
   backing ceiling (32 MiB), the 64 MiB aggregate accounted allocations and the 8 MiB
   staging/transfer bound; keep spool residency constant — a 500 MiB payload must not
   produce ~500 MiB of cgroup file cache (the L18 defect); report anonymous, file cache,
   dirty, writeback, shmem and kernel/slab separately, never quote a lifetime cgroup peak
   as a phase peak, and never excuse file-size-proportional page-cache growth.
5. **The overlay snapshot creates very few metadata entries.** Report metadata
   (pages/entries/records) separately from payload bytes and check that metadata does not
   scale with file count beyond a bounded factor: compare tiers (e.g. 500 vs 25k) and
   successive Commits. A per-file metadata record created per Commit, or metadata that
   grows with the namespace on an unchanged re-commit, is a red flag — investigate before
   accepting it, and report the numbers either way.

## Reporting and honesty (requirements 1 and 5)

- **Post the group result on #152 after every group**, family by family and case by case,
  before starting the next group.
- Every registered selection ends **terminal**: PASS / WARN / FAIL / REUSED-FROM /
  NOT_RUN_OPTIONAL, each with its reason. No silent omissions, no dropped failing cells.
- Report ratios, absolute deltas, absolute-target outcomes, verification status, cleanup
  status, and every non-passing line. Never round up, never re-label or promote a
  historical receipt, never present a diagnostic row as a qualifying one.
- The same facts go into the ledger, with identities and the exact reproduction command.

## Escalate immediately (do not decide alone)

- Any **S0/S1** finding; any **absolute-target miss**; any selection that cannot fit the
  25 s exception budget.
- Owner-supplied inputs for **G8**: the sealed full157 Store for `historical_access`, and
  explicit opt-in for the three `repository_history` profiles (`NOT_RUN_OPTIONAL`).
- Anything that would require changing `crates/` in a way that invalidates the frozen
  candidate beyond a local bug fix.

## Definition of done

All 8 groups posted on #152; every registered selection terminal; a final report with the
`family → per-test` table, the bug ledger (commit, RCA, impact set re-run), the
architecture-guardrail evidence above, resource tables (transient backing, canonical
Store growth, memory domains), the limitations that remain, and any owner decision still
open. Passing this campaign does not merge, close or release anything.
