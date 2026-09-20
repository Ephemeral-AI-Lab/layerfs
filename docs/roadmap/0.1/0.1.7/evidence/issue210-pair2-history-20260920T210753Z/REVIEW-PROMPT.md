# Reviewer prompt — co-design pair 2 (history), end to end

> Status: task prompt for an independent reviewer. Read-only review. Copy this
> document into the reviewer's context verbatim; it is self-contained.

## Your role

You are an independent reviewer of a completed implementation. Your job is to **falsify the
author's claims**, not to summarise them. The author wrote the code, the tests and the handoff; the
handoff's own numbers are *inputs to check*, never evidence. Anything you cannot establish from
source or from a command you ran yourself is **unverified**, and you must say so rather than repeat
it.

Three failure modes are disqualifying for your review, so avoid all three:

- accepting a claim because the handoff states it, or because a test with a matching name exists;
- reporting a green check you did not run, or a check that ran against a different commit;
- softening a finding because the author's summary sounds confident.

## What you are reviewing

Co-design pair 2 of the LayerFS replacement core: the history component (C5) and the service
capability built on it, implemented for [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210)
under design parent [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).

The reviewed contract is the specification and its pre-publication audit, pinned at
`c85cf6b69b3809d860caaad764a09e86a54ece9a` (PR #211, not merged). Read them first; they are the
standard you hold the code to, not the author's description of the code:

```sh
git show c85cf6b69b3809d860caaad764a09e86a54ece9a:core/docs/architecture/proposal/commit-history/implementation.md
git show c85cf6b69b3809d860caaad764a09e86a54ece9a:core/docs/architecture/proposal/commit-history/review-20260921.md
```

The author's description of what was built is
[`core/docs/architecture/16-history.md`](../../../../../../core/docs/architecture/16-history.md), and the
handoff with every claimed identity and result is [`HANDOFF.md`](HANDOFF.md) beside this file.

## Repository and exact revision

Worktree: `/Users/yifanxu/.codex/worktrees/pair2-history-implementation/layerfs`
Branch: `codex/pair2-history-implementation`
Reviewed tip: `6e310b2497bbb56371e3d5ec6459ea4e5a82cac2`
Base (audited baseline): `a02168adbb1b02571941654919cefca12dbc1f42`
Pull request: [#212](https://github.com/Ephemeral-AI-Lab/layerfs/pull/212)

Begin by recording, in your output:

```sh
git -C <worktree> rev-parse HEAD
git -C <worktree> status --porcelain
```

If the tree is not at that commit or is dirty, stop and say so — every finding is pinned to a
revision. Do not review a moving tree.

Read [`AGENTS.md`](../../../../../../AGENTS.md) and
[`core/AGENTS.md`](../../../../../../core/AGENTS.md) before judging anything: they define product-source
purity, the physical-line ceilings, the per-commit production-LOC rule, the no-CI/no-preflight rule
and the dependency rules you will hold the change to.

## Review assignment A — history, schema and transition correctness

Files: `core/crates/layerfs-history/src/**`, `core/crates/layerfs-history/sql/schema-v1.sql`,
`core/crates/layerfs-history/tests/**`.

Falsify each of these. For every one, either quote the source that makes it true or produce the input
that makes it false.

- **A1 Schema.** Schema 1 defines **exactly** the seven tables the specification names
  (`history_meta`, `layer_stacks`, `layers`, `branches`, `commits`, `workspace_stages`,
  `scope_allocator`), sets its own application id and `user_version = 1`, and contains **no foreign
  key, trigger or statement that reaches C2's schema 7**. Check the DDL yourself; do not trust the
  table list in the handoff.
- **A2 Constraints.** Every constraint in specification section 5 is actually enforced, and enforced
  where it claims to be: unique genesis per stack, one accepted child per `(stack, parent)`, one
  publication per `(source Branch, source Commit)`, genesis with parent **and both** source fields
  absent and every other Layer with all three present, same-stack parent/base ancestry for Commits,
  Branch head/base consistency, and immutable-record equality for a duplicate derived identity.
  For each, say whether it is a declarative constraint or an in-transaction check, and construct the
  input that would violate it if the check were removed.
- **A3 Identity.** Widths and tag bytes match the reference encodings, and `CommitId::derive` /
  `LayerId::derive` reproduce the reference identities for the same semantic inputs. A Layer's
  provenance is outside its hash — verify that two records deriving the same `LayerId` with different
  provenance are an **integrity failure**, not an overwrite.
- **A4 `commit_staged`.** Outcome precedence and CAS semantics: a moved Branch retains the exact
  stage and reports `HeadMoved` with typed expected/actual context; `UpToDate` consumes only the
  exact stage and does not move the head; several no-change stages can each succeed; exactly one
  state-changing Commit wins against a fixed expected head/base; a wrong token is `StageChanged` and
  changes nothing. Try to find an interleaving where two winners occur or where a loser loses its
  stage.
- **A5 `add_layer`.** The exact earlier publication is checked **before** any expected-head
  comparison (so asking twice is idempotent after the stack advanced); a Commit whose root equals its
  base Layer root is `NoChanges`; the stack head is CAS'd; the Branch head and base are unchanged.
- **A6 Fork.** A fork from a historical Commit uses **that Commit's** base Layer, not the source
  Branch's current base, and shares ancestry without copying content or history rows.
- **A7 Cursors and pages.** The cursor binds catalog, incarnation, range, subject and immutable
  anchor, and a tampered body fails; every range resumes strictly **after** the last delivered
  record (look for an off-by-one that repeats or skips a record — the author fixed one, so check the
  fix is complete for both ancestry and publication walks); a live Branch advancing does not
  invalidate an anchored cursor; the 4096-row work ceiling is `Capacity`/unproven and **never**
  `NotFound`.
- **A8 Allocation.** Serials are in `1..=i64::MAX`, a reservation is half-open and strictly
  increasing per scope, consumption is unconditional (survives failure and discard), the terminal
  endpoint is a checked refusal and not an overflow, and nothing recycles an exposed serial.
- **A9 Bounded queries.** A page never exceeds 128 records or the encoded byte budget, and the
  catalog's declared worst-case record widths are **at least** what the codec writes. The author
  fixed one understated width; recompute all five from the codec and say whether any is still
  understated.

## Review assignment B — C1/C2 multi-writer integration and failure behavior

Files: `core/crates/layerfs-service/src/operation/history.rs`,
`core/crates/layerfs-service/src/operation/history_bootstrap.rs`,
`core/crates/layerfs-service/src/operation/{dispatch,write,filesystem}.rs`,
`core/crates/layerfs-service/src/owner.rs`, `core/crates/layerfs-service/tests/history.rs`,
`core/crates/layerfs-history/src/sqlite/rows.rs`.

- **B1 Lock span.** No C5 transaction or catalog lock is held across an upload, C1 construction, C2
  encoding or `save.finish`. Trace every catalog call in the command paths and show the lock is
  taken and released inside each call. A held lock here would serialise the multi-writer content
  path, which is the failure this rule exists to prevent.
- **B2 No nested arbitration.** C2 arbitration and C5 locks are never nested in either order.
- **B3 Metadata-only means metadata-only.** `commit_staged`, `discard_stage`, `reserve_inodes`,
  `fork`, `add_layer` and every query start **no** C2 save and write no object. The existing test
  compares the content-store file before and after; judge whether that observation is sufficient,
  and say what it does not prove.
- **B4 Stage-after-finish.** A stage is inserted only after that exact operation's known successful
  `finish`. Root existence is not proof; authenticated bytes are not proof of the semantic role.
  Specifically: can a second writer publishing an **identical root** turn the first writer's failed
  or unknown finish into an acknowledged stage? The handoff marks this PARTIAL — decide whether
  "partial" is honest, or whether the code makes the stronger claim true or false.
- **B5 Admission.** Service W=2 admission, two private C2 saves per Store and Q=0 are unchanged; one
  construction producer per ordinary operation; namespace initialization keeps its existing worker
  exception. Show the constants and the code paths.
- **B6 Unknown outcomes.** An unknown persistence outcome causes no replay, no guessed rollback and
  no guessed discard; a known success is never converted into a claimed abort. Check both the
  service and the provider.
- **B7 Discard.** `discard_stage` removes exactly one metadata row: no content deletion, no Branch
  deletion, no serial refund, no token recycling.
- **B8 Bootstrap.** Initialization is real production code, not
  `core/crates/layerfs-service/examples/prepare_store.rs`. The manifest is bounded (128 entries
  including the root, 32 KiB envelope), the service assigns every serial from one consumed C5
  reservation, prerequisite attribute/symlink objects are saved and finished **before** the
  filesystem build that refers to them, and no combined unpublished reader/sink is assumed. Judge
  whether `build_namespace` really has no ordering hole, including the empty one-directory case.
- **B9 Role validation.** A stored wrong-role root is refused: a regular file's content root must
  open as a file representation, a symlink's must decode as a stored target, and every kind's
  metadata root must decode as an attribute tree with portable fields valid for that kind. Try to
  construct a wrong-role value that passes.

## Review assignment C — service, bridge, authorization, protocol and portability

Files: `core/crates/layerfs-bridge/src/contract/history.rs`,
`core/crates/layerfs-bridge/src/contract/{request,outcome,mod}.rs`,
`core/crates/layerfs-bridge/src/adapters/native/protocol/{metadata,response}.rs`,
`core/crates/layerfs-bridge/src/adapters/native/client.rs`,
`core/crates/layerfs-service/src/native/{config,startup}.rs`,
`core/crates/layerfs-bridge/tests/history_protocol.rs`,
`core/crates/layerfs-daemon/tests/history_route.py`.

- **C1 Authorization.** The permission mapping is total and checked; opcodes 6 and 7 map to bits 5
  and 6; a legacy mask of 31 grants neither; an unknown opcode has no bit at all. Authorization is
  Store-wide and the change must **not** imply per-Branch ACLs. Catalog/stack/Branch/stage
  membership is validated on every suboperation, and no caller can assert a principal.
- **C2 Versioning.** History uses operation profile 2 and legacy operations keep profile 1; a
  profile/opcode disagreement is refused before any mutation; unknown profiles and unknown
  suboperations fail before mutation; there is no automatic downgrade, resend or replay. HELLO and
  framing version stay separate from the operation profile.
- **C3 Dispatch.** `opcode >= 3` is gone, replaced by exhaustive semantic matching; a metadata-only
  command is distinguished from a content mutation and never reaches the content save path.
- **C4 Codecs.** Every vector count and length is validated **before** allocation; every reply shape
  is matched to the request that produced it and a mismatched reply is a delivery failure; a
  truncated or over-long record is refused. Look specifically for a count pre-check that is stricter
  than the encoding (the author fixed one such bug) — recompute every `count(max, min_width)` in the
  history paths against the real minimum width.
- **C5 Failure decoding.** The new classes round-trip and stay typed; `unknown` is set exactly when
  the outcome is unknown; no failure class is reconstructed from a message string.
- **C6 Relay.** No second production daemon parser was added; the daemon's generic framed relay is
  reused and the new operations are driven externally.
- **C7 Portability.** No new non-Unix assumption is introduced. The no-native build proves contract
  separation only — it must not be read as a persistence or Windows/WASM/cloud claim.
- **C8 Legacy compatibility.** Opcodes 1–5, profile 1, their payloads, their result tags and their
  failure codes are unchanged for an existing client. Diff the legacy arms and say whether any
  legacy byte, bound or classification moved.

## Cross-cutting checks

- **X1 Dependencies.** No `[patch]`/`[replace]`, no vendoring, no registry edit, no forked dependency;
  builds stay `--locked`. The only lock change should be the one added workspace member.
- **X2 Product purity.** No `#[test]`, `#[cfg(test)]`, fixture, demo or test-only public item in any
  `src/`; no `unwrap`/`expect`/`panic!` on a product path; no fault hook or test-only branch added to
  make an acceptance case pass.
- **X3 Ceilings.** Every production file under 999 physical lines; every `lib.rs`/`mod.rs` under 200
  and free of implementation constructs.
- **X4 Documentation.** `core/docs/architecture/16-history.md` describes the source as it is. Find at
  least one claim in it that the source does not support, or state that you could not.
- **X5 LOC.** Reproduce the per-commit production LOC in `HANDOFF.md` with
  `python3 tools/production_loc.py --root <tree> --detail` against each first parent and the
  committed tree, and say whether every figure matches.
- **X6 Honest gaps.** The handoff declares H04 and the Linux Docker route NOT_RUN, H06 and H08
  partial, and the independent review not performed. Verify each declaration is **honest**: that the
  code and the tests do not in fact claim more than the handoff admits, and that no NOT_RUN item is
  quietly counted as a pass anywhere in the documents or the PR.

## Commands

Run these from `core/` in the reviewed worktree (`--offline` is required; there is no network):

```sh
cargo +1.85.1 test --offline --workspace
cargo +1.85.1 build --offline --workspace --examples
cargo +1.85.1 fmt --manifest-path Cargo.toml --all -- --check
cargo +1.85.1 clippy --offline --workspace --all-targets -- -D warnings
cargo +1.85.1 build --offline -p layerfs-history --no-default-features
python3 tools/check_product_boundary.py
python3 -m unittest discover -s tools -p 'test_*.py'
python3 crates/layerfs-daemon/tests/history_route.py --output <fresh-directory>
```

Run the daemon driver with a **fresh** output directory; its receipt is append-only evidence and must
never be overwritten. The driver starts the production service and the production daemon; it does not
need Docker, and it provisions the C2 store with the existing fixture executable, which the handoff
states.

## Hard rules

- **Read-only on the reviewed branch.** Do not commit, amend, rebase, push, stash, clean, reset or
  checkout in the reviewed worktree. You may copy the tree elsewhere to write a probe, and if you do,
  say where and what you changed.
- Do not add product fault hooks, sleeps, delays, alternate algorithms or test-only branches to make
  a case pass. If a proof needs a schedule the product cannot expose, record it NOT_RUN with the
  reason — that is the correct outcome, not a failure of your review.
- Do not run benchmarks, do not retune workers, timeouts, cache policies or workloads, and do not
  start a performance campaign. This review makes no performance claim.
- Do not run `tools/preflight.sh`; it is permanently retired, and do not create an equivalent
  aggregate gate.
- Do not patch, vendor or fork a third-party crate, and do not edit anything under the Cargo registry.
- Do not merge the pull request, and do not close or comment on #179, #180, #192, #193 or #209.
- If a claim depends on a measurement, a container run or an environment you do not have, say
  **UNVERIFIED — environment** and name exactly what is missing. Do not guess.

## Required output

1. **Revision record.** `git rev-parse HEAD`, `git status --porcelain`, and the exact command list
   you ran with each result. Report failures verbatim; do not paraphrase a failing run.
2. **Findings table**, most severe first. One row per finding:

   | ID | Severity | Assignment | Claim or code | Evidence | Disposition |
   | --- | --- | --- | --- | --- | --- |

   Severity: `BLOCKER` (the contract is violated, or a claim is false in a way that would mislead a
   consumer), `MAJOR` (a real defect or an overstated claim that does not break the contract),
   `MINOR` (clarity, naming, documentation, duplication), `NIT`. Evidence must be a `file:line`
   quotation, a command with its output, or a minimal input that demonstrates the behaviour.
   Disposition must be one of: confirmed, refuted, partially confirmed, unverified — and say what
   would settle an unverified row.
3. **Claim-by-claim verdict** for assignments A, B and C: for each bullet, `HOLDS` / `FAILS` /
   `UNVERIFIED`, with one line of justification. Do not skip a bullet.
4. **Unverified list.** Everything you could not establish, with the reason (environment, missing
   fixture, requires a schedule the product cannot expose, out of scope).
5. **Honesty audit.** For each gap the handoff declares, whether the declaration is accurate,
   optimistic or pessimistic.
6. **Verdict.** One of: *ready to merge*, *ready with named fixes*, *not ready*. Name the smallest
   set of changes that would move it to the next category, and state plainly what your review does
   **not** establish.

Stop when the six sections are complete. Do not fix the code; report.
