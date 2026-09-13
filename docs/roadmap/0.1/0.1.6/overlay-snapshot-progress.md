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
| Phase | 1 (contracts and frozen source of truth) — complete for V2/V3/V4 and for V1's acknowledged-bytes half; V1 mapping visibility is an open blocker requiring an owner decision |
| Next concrete action | Owner selects one option in [contract resolution §1](overlay-snapshot-contract-resolution.md#the-decision-required); then Phases 2–5 implementation begins |
| Phases 2–6 | Not started. No product code for this feature exists |
| Phase 7 | Not started. No benchmark case was executed for #124/#125 |
| Active processes | None. No build, container, mount, or benchmark is running |
| Measurement lock | Not held |
| Blocking external input | Yes: V1 option 1/2/3 decision. Everything else in Phase 1 is closed |

## Frozen source and identities

| Identity | Value |
| --- | --- |
| Authoritative documents | rule, spec, spec-review, architecture, implementation-plan, this file, contract resolution, exclusion manifest — all under `docs/roadmap/0.1/0.1.6/` |
| Documentation commit | `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b` (rule/spec/review/architecture/plan/handoff + exclusion manifest) |
| Source baseline named by the specification | `0814cc37f1dafb6041930c74489107f4a5035a26` |
| Phase-1 enumerating binary | `target/release/fs-benchmark-pro`, `LAYERFS_SOURCE_COMMIT=536aaf9ded4e09a9ccfcf2b251aa4db995b383e6` (documentation-only difference from the documentation commit), product seal `276c5970…`, compilation seal `8b63c852…` |
| Regression suite source commit used for the frozen pass ledger | `3e308a8f2` (recorded by the existing v0.1.5/#120 evidence) |
| V1 probe image | `rust:1.85.1-bookworm`; container kernel `6.12.76-linuxkit` |

## Evidence locations

| Evidence | Location |
| --- | --- |
| V1 kernel-visibility probe (source, log, hashes) | `docs/roadmap/0.1/0.1.6/evidence/v1-probe/` |
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

## Resume instructions

1. Read [contract resolution](overlay-snapshot-contract-resolution.md) §1 for the
   open V1 decision, then §2–§4 for the selected V2/V3/V4 contracts.
2. Do not start Phase 2–5 code that would ship a weaker visibility model; the
   implementation plan's Phase 1 exit condition forbids it.
3. Reuse the frozen pass ledger rather than re-running unaffected checks; the
   ledger records why each retained pass is still valid.
4. Re-enumerate the benchmark catalog from the sealed candidate before any Phase 7
   execution; the recorded scope ledger is provisional by construction.
