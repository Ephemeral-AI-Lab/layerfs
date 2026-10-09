# R9 conditional reference retirement completion

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**R9 CLOSED — NOT_EXECUTED — conditions unmet.** Root `crates/` and
`core/reference-tests` are retained intact. No reference or obsolete root wiring
was deleted, moved or redirected. This is the truthful conditional closure
allowed by the R5-R9 handoff; it is not retirement or product acceptance.

## Conditions and receipts

| Removal condition | Actual disposition | Evidence |
| --- | --- | --- |
| R7-retire closed | MET | [R7-retire completion](R7-RETIRE-COMPLETION-20261010.md) |
| Core builds/tests/runs independently of root reference | UNMET: product manifest/production includes are independent, but active fixture-seal test reads root generator source | [Pinned audit037](checks/r8-qualification-20261010/037-r9-dependency-audit.json); fixture_seal.rs87 |
| Every R8 functional proof passed | UNMET:45 scoped PASS/35 PARTIAL/8 withdrawn/1 unrun/1 full-fixture FAIL | [R8 completion](R8-COMPLETION-20261010.md); [all outcomes061](checks/r8-qualification-20261010/061-outcomes.json); [full timeout053](checks/r8-qualification-20261010/053-full-mounted-proof-v2/result.json) |
| No unresolved R8 row still needs root reference as comparison arm | No root-reference arm selected: registry uses L/N/P, A2 supplies none; do not misstate pending P threshold as a root-reference arm | [Registrationv2](../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v2.json); unrun/owner dispositions preserved |
| Every historical reference receipt preserved and recoverable beside a per-receipt pin | Preserved existing receipts/pins and complete current reference recovery tree; exhaustive per-receipt catalog NOT_VERIFIED | Recovery commit/tree below; all original historical evidence remains in place |

The unmet complete functional condition cannot be made to hold by an unchanged
rerun, larger fence, weaker oracle, timing optimization or invented acceptance.
Outstanding source/runtime/count/resource scopes remain unqualified and owner
questions remain pending. Root independence requires a separately proven guard/
generator migration, not removal of its assertion. No condition is waived.

## Prepared removal plan and audited source size

Pinned countertools/production_loc.py SHA
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
exact staged/committed snapshots used throughout this run:

| Scope at retained tree | Production LOC | Classification |
| --- | --- | --- |
| Active core | 79258 |13 active packages; unchanged |
| Root reference | 65417 |Retirement candidate, not simplified implementation |
| Archived core/reference-tests |0 |External historical tests; no product source contribution |
| Combined product |144675 |No retirement executed; delta+0 for every run commit |

Root tree498dd1917812ae90efb8841f57e22bfc284e96fb and archived-test tree
3a7fcdcc6f8d1610c896b9e2a2f89d2a0297f027 are recoverable at
f8a0a5ff1cd5f93b90bf16f964705b4c7d773615 and remain identical today. Preserve
original older execution/source pins as well; this recovery pin never relabels
a historical result. Before any future deletion, finish the exhaustive receipt
catalog, with an appended recovery mapping beside each affected record.

If every condition later holds, prepare a new exact parent/final staged count,
then remove root reference and archived tests in an isolated retirement commit.
Recompute actual counts if other source changed;65417 is today's audited candidate
size, not an estimated future final product or algorithmic improvement. The
counter and its caller must handle an absent reference scope explicitly rather
than silently changing classification. Confirm committed tree against that count.

Before deletion, migrate the active content fixture-seal generator contract:
keep frozen binary fixture hashes, ignored-only generation and the explicit
LAYERFS_SEAL_FIXTURES gate, with independent proof. No private/reference source
include or production fallback may replace it; deleting the guard is forbidden.
Reaudit active manifests, includes, examples/tests, scripts/harnesses and needed
documentation at the final source. Required scoped builds/checks then apply;
there is no aggregate CI/preflight wrapper.

## Root benchmark and obsolete wiring decision

Conservative decision: retain the root benchmark harness as historical evidence
at its compatible recovery/source pin; do not redirect its dependencies to
same-named core packages. Current root wiring remains unchanged because removal
is not executed. Alternative: after all conditions pass, explicitly archive or
withdraw its current-checkout execution instructions while preserving raw receipts
and the last runnable commit; then retire obsolete evaluator/container wiring.

The audit identified root Cargo.toml's ten reference members and benchmark/eval
members, benchmark/fs-bench-pro's root dependencies and schema-source reader,
tools/layerfs-eval's root SDK dependency, and containers/layerfs-fuse/Dockerfile's
root copy/build route. Removing only the source leaves these dangling. They need
an explicit audited disposition in that future isolated retirement. Shared
.cargo/config.toml, provenance, pinned counter and all evidence remain required.
No active core runtime dependency on those legacy entrypoints was found.

Historical proposal/research source links keep original facts and source pins.
After a qualifying removal, append recovery notes without rewriting verdicts.
Today all linked reference source remains available; no documentation migration
is presented as implemented retirement.

## Decisions, verification and stop boundary

R9 chooses preservation while any condition is unmet. Alternative is to fulfil
all conditions first, then perform the audited isolated deletion and scoped
checks. It never chooses forced deletion or an owner acceptance threshold.
Inherited owner decisions are listed in R8 and remain PENDING OWNER.

No deletion-dependent final build ran because no product/reference source changed.
Current core build/test/Clippy/fmt/guard assertions reuse identical source scope,
with new missing-precondition proofs separately recorded in R8. Full R8 acceptance,
root independence and exhaustive per-receipt recovery catalog are NOT_VERIFIED.
The core README's obsolete package/retirement/integration claims were corrected
in preparation; root source, benchmark, evaluator, container and archived tests
were preserved. Every run commit has Production LOC144675→144675(delta+0),
core79258/reference65417, no retirement; see R8 accounting and commit messages.

R9 is the authorized stop boundary. The completion commit records this closed
state with the complete outcomes, failures, retained custody and verification
limits. Nothing is pushed or published.
