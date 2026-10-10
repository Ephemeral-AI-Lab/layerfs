# R9b conditional reference retirement completion

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**R9b CLOSED — NOT_EXECUTED — conditions unmet.** Root `crates/` (tree
`498dd1917812`, 65,417 production LOC) and `core/reference-tests` are intact.
Nothing was deleted, moved or redirected. Two of the written removal
conditions do not hold, so the removal was not attempted. The earlier
[R9 record](R9-COMPLETION-20261010.md) keeps its outcome.

## Conditions

| Removal condition | Disposition | Evidence |
| --- | --- | --- |
| R7-retire closed | MET | [R7-retire completion](R7-RETIRE-COMPLETION-20261010.md) |
| Core builds, tests and runs independently of the root reference | MET by source audit and by the final suites; not executed in a tree with the reference absent | [Re-audit 047](checks/r8b-requalification-20261010/047-r9b-independence-reaudit.txt); guard migration `a8aa0b2b6` |
| Every R8 functional proof passed | **UNMET**: 54 PASS, 27 PARTIAL, 1 FAIL, 8 withdrawn; one registered proof failed | [R8b completion](R8B-COMPLETION-20261010.md); [outcomes](checks/r8b-requalification-20261010/046-outcomes.md) |
| No unresolved R8 row needs the root reference as a comparison arm | Holds: registration v3 selects arms L, N and P only, and A2 supplies none | [Registration v3](../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v3.json) |
| Every historical reference receipt preserved and recoverable beside a per-receipt pin | **UNMET**: the catalogue exists, but 59 rows depend on 7 pin values that resolve to no local object, 419 rows record no pin, and the recovery commit is local only | [Recovery catalogue](checks/r9b-recovery-catalogue-20261010/README.md) |

Neither unmet condition can be made to hold by a rerun, a weaker oracle or an
invented acceptance. The first needs the owner rulings listed in the R8b
record. The second needs the missing pins located or their receipts accepted
as unrecoverable, and the recovery commit carried by a pushed ref or tag.

## Independence work completed

| Item | Commit | Result |
| --- | --- | --- |
| Fixture-seal guard | `a8aa0b2b6` | The one active core target that read root `crates/` now reads nothing outside `core/`. Frozen hashes, the seal list and every sealed fixture are unchanged; the generator is pinned by path, blob, SHA-256 and recovery commit; a scan fails when a source that could reseal lacks the ignore or the `LAYERFS_SEAL_FIXTURES` gate; four negative cases prove the guard fails when it should |
| Recovery catalogue | `02dd96b0b` | 2,347 rows pinned to `ea812d3f8`; deterministic generator with a 26-check self-check. Appended only: no receipt or verdict is rewritten |
| Content/storage harness | `77a9b52db` | Builds against current public APIs; 16 test binaries pass; no registered row executed |
| Re-audit at `b22fa088f` | this record | No tracked core manifest has a path dependency leaving `core/` except the substitution-proof arms, which point at `/tmp` copies of core crates; no root-only package name; no core source joins upward into root `crates/` |

Open in the independence work, both listed for the owner: whether a generator
pinned by blob satisfies "ignored-only generation" (it drives the reference's
own tree API and cannot be ported without making the oracle
self-referential), and the catalogue's 1,131 weak-signal rows.

## Wiring dispositions prepared for a future removal

Not executed. Removing only the source would leave each of these dangling, so
each needs its disposition in the same isolated retirement commit.

| Consumer | Needs | Prepared disposition |
| --- | --- | --- |
| Root `Cargo.toml` and `Cargo.lock` | 10 reference members, `tools/layerfs-eval`, `benchmark/fs-bench-pro` | Retire with the reference; whether a root manifest survives is an owner decision |
| `benchmark/fs-bench-pro` manifest, `shared/runner.py`, `Dockerfile.layerfs` | Path dependencies on root crates, source seals over `crates/`, the root build, the schema-source reader | Preserve at the recovery pin as historical evidence; do not redirect to same-named core packages; withdraw current-checkout execution instructions |
| `tools/layerfs-eval` | Root `layerfs-sdk` | Retire with the reference |
| `containers/layerfs-fuse/Dockerfile`, `.dockerignore` | Copies and builds root `crates/` | Retire with the reference |
| `tools/stage5_component_comparison.py` | A `reference` arm built from the root workspace | Mark non-runnable at HEAD with its recovery pin, or retire |
| `tools/test-fast.sh`, `test_fast.py`, `harvest-test-timings.py`, timings file | The root workspace test run | Retire with the reference |
| `tools/production_loc.py` (pinned counter) | Scope `reference` is root `crates/`; an absent directory counts as 0 silently | Keep unchanged. The commit message must state that the reference scope is absent and where it was last counted, not report a bare 0 |
| `core/tools/production_loc.py` (second, unpinned counter) | Scans root `crates/` as scope `legacy` | Decide keep or retire explicitly |
| `core/benchmark/fs-bench-pro-storage-content/shared/test_isolation.py` | Scans the root benchmark harness for a retired lock; a missing directory yields nothing | Follows the root-benchmark decision; if dropped, drop that entry explicitly |
| Root `README.md`, `AGENTS.md`, `core/AGENTS.md`, `core/README.md` | Prose describing the reference | Update in the retirement commit |
| 109 historical Markdown files, 864 links into root `crates/` | Become dead relative links | Keep the text; the recovery catalogue is the appended note |

`core/patches` and `core/vendor` are not shared with the root workspace and
are unaffected.

## Prepared removal plan

If every condition later holds: recount from the exact parent and staged
trees, remove root `crates/` and `core/reference-tests` with `git rm` in one
isolated retirement commit together with the dispositions above, and report
the result as retirement of the reference, not as simplification. Today's
audited candidate is 65,417 production LOC, leaving active core at 79,336;
both must be recounted then. Run the scoped builds and checks on the resulting
tree before the commit.

## Verification and limits

No deletion-dependent build ran, because nothing was removed. Every commit of
this run that touched R9b reports `Production LOC` with delta +0. Not verified:
a core build and test run in a tree where the reference is absent; that every
historical receipt is recoverable as executed; any execution of the root
benchmark harness.
