# R9c reference retirement completion

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**R9c CLOSED — EXECUTED.** The v0.1.6 reference under root `crates/` and the
archived `core/reference-tests` are retired from `main` in the commit that
carries this record, together with the wiring that only they served. This is a
legacy retirement, not a simplification: active core is unchanged at 79,366
production LOC and no core source file changed. The [R9](R9-COMPLETION-20261010.md)
and [R9b](R9B-COMPLETION-20261010.md) records keep their `NOT_EXECUTED`
outcomes. Nothing is pushed or published.

The removal was carried out under the owner's instruction after R8b, **"fix all
of them with simplicity and i want to close r8 and r9 fast"**. Two of the five
written conditions hold only because of decisions taken under that instruction;
the table says which.

## Conditions

| Removal condition | Disposition | Evidence |
| --- | --- | --- |
| R7-retire closed | MET | [R7-retire completion](R7-RETIRE-COMPLETION-20261010.md) |
| Core builds, tests and runs independently of the root reference | MET, now executed with the reference absent | [Static checks](checks/r9c-reference-retirement-20261010/10-static.txt), [host suite](checks/r9c-reference-retirement-20261010/11-host-suite.txt), [Linux suite](checks/r9c-reference-retirement-20261010/13-linux-suite.txt), [independence](checks/r9c-reference-retirement-20261010/14-independence.txt) |
| Every R8 functional proof passed | MET **at the owner-rescoped scope only**: 82 PASS and 8 withdrawn by R0, all twelve registered proofs PASS at v4. 28 of the 82 pass by a decision under the instruction, 18 of those after a scope was withdrawn. Timing is `NOT_RUN` | [R8c completion](R8C-COMPLETION-20261010.md); [outcomes](checks/r8c-close-20261010/51-outcomes.md) |
| No unresolved R8 row needs the root reference as a comparison arm | Holds: registration v4 selects no reference arm and attempts no timing | [Registration v4](../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v4.json) |
| Every historical reference receipt preserved and recoverable beside a per-receipt pin | MET **by ruling C-16**: the catalogue's 2,347 rows stand as the per-receipt pins; 59 rows that depend on 7 pin values resolving to no local object are accepted as unrecoverable and stay marked so; 419 rows record no pin | [Recovery catalogue](checks/r9b-recovery-catalogue-20261010/README.md); [decision C-16](checks/r8c-close-20261010/00-owner-instruction-and-decisions.md) |

## Where the retired source is

| Pin | Value |
| --- | --- |
| Last commit holding it | `d296981ace76fe751bebf00dee52818fe18a3eef`, local annotated tag `reference-v0.1.6-final` |
| `crates` tree | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| `core/reference-tests` tree | `3a7fcdcc6f8d1610c896b9e2a2f89d2a0297f027` |
| Also on the remote | `origin/main` at `f96d97651` carries both trees with the same identities, so the final reference source does not depend on this unpushed history |
| Recovery catalogue base | `ea812d3f8`, an ancestor of local `main`; its `crates` tree is the same `498dd1917812` |

The release tag `v0.1.6` carries the released source (`crates` tree
`dcc4fb6f…`), which differs from the final reference tree. It is the source of
the release, not the recovery pin for receipts taken later.

## What was removed and what was kept

| Path | Files | Disposition |
| --- | --- | --- |
| `crates/` | 249 | Removed: the reference, 65,417 production LOC in 193 counted files |
| `core/reference-tests/` | 60 | Removed: archived tests with no manifest, never built |
| Root `Cargo.toml`, `Cargo.lock` | 2 | Removed: the workspace had no member left that could build |
| `tools/layerfs-eval/` | 2 | Removed: depended on the root SDK |
| `containers/layerfs-fuse/Dockerfile` | 1 | Removed: built root `crates/` |
| `tools/test-fast.sh`, `test_fast.py`, `test_test_fast.py`, `harvest-test-timings.py`, `test-fast-timings.json` | 5 | Removed: ran the root workspace tests |
| `benchmark/` (root harness) | — | Kept as historical method; [marked not runnable at HEAD](../../../../benchmark/AGENTS.md). Its manifest names four absent paths and is not redirected to same-named core packages |
| `tools/stage5_component_comparison.py` | — | Kept; refuses to start without the root workspace and names the recovery tag |
| `tools/production_loc.py`, `tools/test_production_loc.py` | — | Kept unchanged; the pinned counter (SHA-256 `c0fe7f36…624adb`) reads an absent scope as 0, so commits use the [absent-aware helper](checks/r9c-reference-retirement-20261010/tools/count_absent_aware.py) and state the scope as absent |
| `core/tools/production_loc.py` (second, unpinned counter) | — | Kept; its `legacy` scope is documented as absent |
| `tools/preflight.sh` | — | Kept retired as before; its comment no longer names a root command |
| 44 path entries in historical evidence probes under `docs/` and `core/docs/`; Markdown links into root `crates/` | — | Kept as text. They no longer resolve at HEAD and were not rewritten; the recovery catalogue and the tag are the appended note |
| `core/patches`, `core/vendor`, `.cargo/config.toml` flags, every receipt, release note and verdict | — | Unchanged; one comment in `.cargo/config.toml` was updated |

Prose updated in the same commit: root `README.md` (source notice, Quickstart
pinned to the `v0.1.6` tag, current layout), root `AGENTS.md`, `core/AGENTS.md`,
`core/README.md`, `benchmark/AGENTS.md`.

Two ignored Finder files remain on this machine under an otherwise empty
`crates/` directory. They are untracked, were not created by this run and were
left alone.

## Checks on the tree with the reference absent

Staged tree `eee41c663366` over parent `d296981ac`, before the record and
ledger were added. No file under `core/crates` differs from the R8c source
(tree `10bd82154437`).

| Check | Result |
| --- | --- |
| Format, boundary guard (674 files) | PASS |
| Fuser provenance | Same two refusals at the `r7-passthrough` manifest as before (OWNER-3, unchanged); 88 manifests checked, 101 before |
| Tooling tests: `core/tools` 50, `core/benchmark/r8-tools/tests` 65, `tools` 18 | PASS |
| Host suite, 13 packages | 275 binaries, 1,201 passed in parsed result lines, 2 input-gated failures. Binary for binary identical to R8c receipt 11 |
| Linux source hashes in the pinned image | 1,185 files, all match; the container sees no root `crates/` manifest and no root `Cargo.toml` |
| Linux suite, pinned image | 275 binaries, 1,281 passed in parsed result lines, 2 input-gated failures. Identical to R8c receipt 15 |
| Manifests and metadata | No core manifest has a path dependency leaving `core/`; `cargo metadata --locked` places every local package under `core/` for the workspace and for the runtime harness; no source include reaches the retired paths |

The gated failures are the same binaries as in R8c: `complete_installed_roots`
on both sides, `host_handoff` on the host and `shared_processes` on Linux.
Each needs a prepared input a plain run does not supply, and each passed as a
registered selection with that input at v4 (31 to 34) on the same product tree.
They were not invoked again with inputs here.

One shell slip: the first write of the static receipt lost its exit statuses
to an unset variable and the same commands were run again under `bash`.

## Source accounting

Pinned `tools/production_loc.py` over independent archives of the first parent
and the committed tree, through the absent-aware helper.

| Scope | Before | After |
| --- | --- | --- |
| Active core (`core/crates`, 674 files) | 79,366 | 79,366 |
| Reference (`crates`) | 65,417 | absent: retired in this commit; last counted 65,417 at tree `498dd1917812` |
| Combined | 144,783 | 79,366 |

`Production LOC: 144783 -> 79366 (delta -65417)`. The whole delta is the
retired reference. No relocation, no duplication and no change to the product
that is built.

## Not verified

- A clean rebuild from an empty target directory. Cargo reused its artifacts;
  the evidence is that resolution and fingerprints held with the reference
  absent and the binaries are the ones R8c examined.
- Host and Linux Clippy on this tree. No Rust source changed since R8c
  receipts 12 and 16, which were not repeated.
- Any registered proof, timing, cold-cache, storage-size or resource-bound
  claim on this tree. None was invoked; R8c's stand for the same product tree.
- That every historical receipt can be re-executed. The catalogue records
  where each one's source is; 59 rows cannot be recovered and 419 name no pin.
- Any execution of the root benchmark harness or of the Stage 5 driver.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.

## Still open for the owner

OWNER-1 to OWNER-7 stay `PENDING OWNER` as listed in the
[R8b outcomes](checks/r8b-requalification-20261010/046-outcomes.md#owner-questions).
The rulings and withdrawn scopes C-1 to C-20 were taken without the owner's
row-by-row review and are each reversible; reversing C-16 does not require
restoring the source, because the source is at the pins above. The local tag
and the 338 local commits are unpushed.
