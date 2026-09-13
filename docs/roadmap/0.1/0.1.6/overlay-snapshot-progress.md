# Snapshot-Isolated Workspace: progress and resume state

Status: Dated planning checkpoint; not release evidence or a product contract.
Owner work: [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124)
(implementation) and [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125)
(final full non-#122 benchmark campaign). This file is the durable resume point
for that work; the append-only record of individual checks is
[overlay-snapshot-verification-ledger.md](overlay-snapshot-verification-ledger.md).

## Current position

| Item | Value |
| --- | --- |
| Phase | 2 in progress; V1 acceptance remains OPEN and V2/V3/V4 remain implementation obligations |
| Next concrete action | Finish cumulative host spool/inline limits; compile and check shared snapshot candidate with the new attempt coordinator; wire generic FUSE host-operation transport and public lifecycle |
| Phases 2–6 | Phase 2 disk roots/payload/ranges/owned reads have component evidence; host ordinary-operation, logical quota, kernel-reference and server/cookie cores have focused evidence; the FUSE client and Phase 4 candidate adapter are being integrated. No whole phase or public-path acceptance claimed |
| Phase 7 | Not started. No benchmark case was executed for #124/#125 |
| Active processes | No root Cargo process. Candidate agent next compiles owned candidate + coordinator; host agent adds generic host_client and kernel callback ownership; budget agent reviews host cookies/replay. Root server/cookie checks passed. No benchmark running |
| Measurement lock | Not held |
| Unresolved contract | V1: NOTIFY_RETRIEVE supplies dirty bytes but retains mutable pages, not a snapshot. See follow-up investigation; no relaxation approved |

Scheduling correction (2026-09-14): the owner requested continued execution through
terminal success. The predecessor's blanket wait before Phases 2–5 is superseded
by the [successor handoff](overlay-snapshot-handoff-prompt.md). This corrects the
next action, not the recorded probe result. V2/V3/V4 contract decisions still need
implementation evidence; full public snapshot acceptance and Phase 7 remain pending.

## Frozen source and identities

| Identity | Value |
| --- | --- |
| Authoritative documents | rule, spec, spec-review, architecture, implementation-plan, this file, contract resolution, exclusion manifest — all under `docs/roadmap/0.1/0.1.6/` |
| Documentation commit | `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b` (rule/spec/review/architecture/plan/handoff + exclusion manifest) |
| Successor frozen instruction commit | `554b866838e50468b312f9d8e9ef70dc2988a38e` on `codex/snapshot-isolated-workspace`; preserves all three scoped local edits from takeover |
| Takeover comment | https://github.com/Ephemeral-AI-Lab/layerfs/issues/124#issuecomment-5655720706 |
| Reviewable implementation | https://github.com/Ephemeral-AI-Lab/layerfs/pull/126 (draft; not terminal) |
| Initial product component commit | `5996231830eb05c986f7f9dcbb4b24cc52288281`; formatter repair `2728aebd3`, Store Clippy repair `6d907bc43` |
| CI status | Original fmt failure repaired. Rust 1.85.1 full native run passed 550 selections in 95s on `2728aebd3`; latest CI34782823313 at c95588f still fails on unwired overlay dead code; index scan arity is repaired. Complete integration; no lint disablement |
| Source baseline named by the specification | `0814cc37f1dafb6041930c74489107f4a5035a26` |
| Phase-1 enumerating binary | `target/release/fs-benchmark-pro`, `LAYERFS_SOURCE_COMMIT=536aaf9ded4e09a9ccfcf2b251aa4db995b383e6` (documentation-only difference from the documentation commit), product seal `276c5970…`, compilation seal `8b63c852…` |
| Regression suite source commit used for the frozen pass ledger | `3e308a8f2` (recorded by the existing v0.1.5/#120 evidence) |
| V1 probe image | `rust:1.85.1-bookworm`; container kernel `6.12.76-linuxkit` |

## Evidence locations

| Evidence | Location |
| --- | --- |
| V1 kernel-visibility probe (source, log, hashes) | `docs/roadmap/0.1/0.1.6/evidence/v1-probe/` |
| V1 retrieval correction (new mechanism, retained-page counterexample) | `docs/roadmap/0.1/0.1.6/evidence/v1-investigation/` |
| Phase 2 focused component logs | `docs/roadmap/0.1/0.1.6/evidence/phase2-components/`; cursor attempt01 selected zero tests and is invalid; attempt02 executed one PASS |
| Owned index relocation / uncertain I/O | `evidence/phase2-components/index-relocation-manifest.json`, `index-ownership-recovery-manifest.json`; all failed attempts retained |
| Host payload ranges | `evidence/phase2-payload/`; 64 MiB -> 4 KiB -> zero physical payload, shared-source append, cross-owner failure repairs |
| Disk file ranges / canonical correspondence | `evidence/phase2-ranges/`, `evidence/v3-correspondence/`; component tests, not public Commit acceptance |
| Integrated owned Snapshot content | `evidence/phase2-components/snapshot-content-attempt01.log`: 8 targeted tests PASS; replay retained separately |
| CI failures and repairs | `evidence/ci/`; full remote logs retained with exact heads |
| Phase-1 benchmark catalog and scope ledger | `docs/roadmap/0.1/0.1.6/evidence/phase1-catalog/` |
| Local raw run directory for the same probe | `benchmark-results/v016/v1-probe/` (git-ignored raw evidence, preserved in place) |
| Local raw catalog directory | `benchmark-results/v016/phase1-catalog/` |

## Scope reminders that must not drift

- #122 is excluded at case level, never by family. Excluded rows are
  `EXCLUDED_ISSUE_122`, not PASS and not NOT_RUN. `shared/v016_matrix.py` is not
  the Phase 7 driver.
- No new numerical performance gate, capture target, or replacement benchmark
  family is introduced by this work.
- Phase 7 requires a correct sealed candidate; #125 does not wait for #124 to be
  closed.
- No release, tag, website publication, or #122 execution belongs to this work.
- Owner clarification: the solution must be **generic FUSE**. LinuxKit is only
  the observed Docker Desktop test-kernel identity, not a product dependency.
  Do not turn a hypothetical custom kernel/VM adapter into an approved solution.

## Current independent integration ownership

- `v1_mechanism`: `host_overlay.rs`, concrete host ordinary-operation core over
  Overlay/Index/Payload/Ranges plus generic FUSE host wire/client and kernel-lifetime integration; host core has three recorded passing checks, cumulative quota wiring remains underway.
- `publication_receipts`: `changes.rs` / optional `snapshot_candidate.rs`, owned
  input adapter to the existing shared canonical construction; V3/range components
  are receiving shared read/aggregate and persisted-description prerequisites. No second content encoder or all-node map permitted.
- `overlay_index`: core `limits.rs` / `overlay_budget.rs`, bounded defaults and
  aggregate host admission and Payload logical charge classification. Index source is stable with physical relocation and
  exact uncertain ownership/writeback-receipt repairs.
- Root: Snapshot/InodeRecord/binding integration, per-Workspace attempt/lifecycle
  and production FUSE/SDK entry integration, CI, evidence and issue updates.

Remaining generic V1 obligations prevent full-surface acceptance, final sealing
and Phase 7. The host core, candidate adapter, policy/coordinator/ordinary adapter
work listed above remains executable, so this is not an all-paths external block.

## Resume instructions

1. Read [contract resolution](overlay-snapshot-contract-resolution.md) §1 for the
   open V1 decision, then §2–§4 for the selected V2/V3/V4 contracts.
2. Implement dependency-ready Phase 2–5 components immediately, while keeping V1
   open. Do not ship or claim a weaker visibility model as full snapshot support.
3. Reuse the frozen pass ledger rather than re-running unaffected checks; the
   ledger records why each retained pass is still valid.
4. Re-enumerate the benchmark catalog from the sealed candidate before any Phase 7
   execution; the recorded scope ledger is provisional by construction.

## Ordered next actions, subject to actual dependencies

1. Phase 2: add the host overlay root bundle and typed persistent indexes
   (`overlay.rs`), atomic leased-source-root installation, dual change indexes,
   payload arenas with declared block release, replay window, reclamation
   accounting; verify with the root-race, replay, quota/short-I/O,
   partial-retention, stale-reference and tombstone tests named in
   [resolution §2](overlay-snapshot-contract-resolution.md).
2. Phase 3: add the owned snapshot reader/cursors (`snapshot.rs`) and the
   host-installed acknowledgment path for ordinary mutations; keep mount identity,
   inode identity, descriptors and working directories; verify exact
   live/snapshot separation on the public FUSE/SDK path.
3. Phase 4: move Commit to independent attempts with the owned snapshot, the V4
   receipt and the V3 correspondence, keeping `workspace_stages` and conditional
   publication; verify C1-excludes-x/C2-includes-x, no-op coverage and lost replies.
4. Phase 5: remove the freeze/quiesce/checkpoint coupling after auditing the
   non-Commit consumers listed in [resolution §5](overlay-snapshot-contract-resolution.md).
5. Phase 6: correctness/integration, then the million-changed-file proof with fresh
   Store reopen, then seal the candidate and hand it to #125.
6. Phase 7 (#125): re-enumerate the registry, freeze the include/exclude manifest,
   execute under the measurement lock, publish real numbers, repair measured
   defects and re-run only invalidated evidence.
