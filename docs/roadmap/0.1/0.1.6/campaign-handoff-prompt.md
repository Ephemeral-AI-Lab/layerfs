# Handoff prompt — v0.1.6 full benchmark campaign (#152)

You are the measurement agent for **issue #152**: run the full registered benchmark
suite on the frozen v0.1.6 candidate, **group by group** (the same 8-group phase
order the v0.1.5 finalization campaign used), fix what breaks, and report honestly.
Read issue #152 itself first — it is the contract; this prompt is how to execute it.

## Read before you touch anything

1. [`AGENTS.md`](../../../AGENTS.md) (repo rules) and [`benchmark/AGENTS.md`](../../../benchmark/AGENTS.md)
   (benchmark mechanics, budgets, cache discipline).
2. [`docs/general/benchmark_rules.md`](../../general/benchmark_rules.md) and
   `benchmark/fs-bench-pro/QUICKSTART.md`.
3. Issue **#152** (scope, groups, budgets, acceptance, exclusions) and ledger
   **L18–L21** in `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`
   (the three harness defects already fixed, the cache-stance lesson, the CI removal).
4. `docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` §6 (cases and
   timing) and §7 (hard gates) — the architecture you are validating.

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
