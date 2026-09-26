# #245 package F: frozen comparative target (pre-candidate contract)

> **Status:** prospective contract, frozen from the qualifying control receipts
> before any package D or E source edit. The control arm is spent; this document
> is the frozen target the one candidate attempt per case is measured against.
> No latency PASS is available under this target: every latency cell stays
> `INELIGIBLE` by the frozen cache contract.

Decision F-1 (owner-approved 2026-09-26): the candidate arm is admitted on
**functional parity plus a complete-command wall envelope of at most 2× the
control's walls**. Nothing else. Rejected alternatives (functional parity only;
making latency admissible) are recorded in
[HANDOFF_D_E_F.md §8](../../HANDOFF_D_E_F.md).

## 1. The control this target is frozen from

The qualifying control campaign is
`benchmark-results/fs-bench-pro/issue245-shell-package-v3-control-01/`
(prepared at `issue245-shell-package-v3-prepared-01`, image
`sha256:9f76aefe535a9a22daf9b2e6a0df30779dc3c08798864485c98b78c59accf48b`,
registry sha `1a7e1a3f7ea40ea14ed9f97865260c936df53601cfd5d0082c0db4041849cd3c`,
one construction worker, `shutil.copyfile` independent writable byte copy,
one attempt per case). Source identity: `fdfc41032d284d2f278fa0cbaa351314ea2eb7be`,
clean tree `2df347419f24a3480c1b673b3c9640570b73edf5`, product seal
`8269da4807c944d4bc2d6234c670c07399253325eec09f287ec1f41eaf614002`, harness seal
`aae7081d526d85c2b225aa6eacefbae5e964c6453d3216afcd228dde779e4d1f`.

All four registered cases are functional PASS, cleanup PASS, sealed-verifier
PASS. The control is **not** re-run; the earlier `v2-control-01` campaign's two
FAIL rows stay FAIL and are the evidence that located the last defect.

## 2. The frozen comparative target

Every candidate row must hold all of the following, under the same frozen
registry, the same enforced cache contract (container FUSE backing uncontrolled,
latency cells `INELIGIBLE`), and the same one-attempt-per-case rule:

1. **Functional parity:** functional PASS, cleanup PASS, and the identity-matched
   sealed verifier PASS on all four registered cases, with the
   `failed-command-no-commit-v1` case still publishing nothing (zero Commit
   calls, old head unchanged).
2. **Complete-command wall envelope:** the candidate's complete-command wall —
   product timers, SDK/container lifecycle, Status, Unmount, log capture and
   Delete, the same boundary the control's `complete_command_wall_ns` covers —
   must be at most **2× the control's wall** for the same case, and still inside
   the registered complete-command limit (15 s; the mixed refresh's declared 25 s
   exception). The envelope is per case, never pooled.
3. **Verifier observation:** the control's sealed verifier ran 0.049–0.159 s per
   case. This is an observation, not a new gate; the registered verifier limit
   (under 10 s, the frozen #243 selection's 9 s) still applies unchanged.

| Case | Control complete-command wall | Frozen envelope (2×) | Registered limit |
| --- | ---: | ---: | ---: |
| `mixed-refresh-v1` | 2,195,712,750 ns (2.196 s) | ≤ 4,391,425,500 ns (4.392 s) | 25 s declared exception |
| `overwrite-4k-v1` | 816,407,250 ns (0.816 s) | ≤ 1,632,814,500 ns (1.633 s) | 15 s |
| `repeated-one-byte-v1` | 1,088,400,750 ns (1.088 s) | ≤ 2,176,801,500 ns (2.177 s) | 15 s |
| `failed-command-no-commit-v1` | 5,882,061,916 ns (5.882 s) | ≤ 11,764,123,832 ns (11.764 s) | 15 s |

A candidate row that meets functional parity but exceeds its envelope is
reported **FAIL** against this target (with its measured wall), not
`INELIGIBLE`. A row whose functional result fails is FAIL regardless of wall. A
row not attempted is `NOT_RUN` with the reason. Latency cells are `INELIGIBLE`
in every case; no numeric latency admission, speedup headline or RSS gate
exists under this target.

## 3. Custody of the comparison

- The candidate arm runs at a **freshly prepared** post-D/E source: the v3
  prepared state is reusable for candidate attempts only through
  `shell_package.py run` when the current source matches the prepared identity,
  so a changed source requires a fresh `prepare` at that frozen source first.
- One attempt per case per arm, no best-of, no unchanged-arm rerun, no timeout
  or worker increase, no shortened case, no dropped cell, no rewritten receipt.
  One construction worker; `LAYERFS_CONSTRUCTION_WORKERS=1`.
- The control receipts are append-only evidence; the envelope numbers above are
  frozen from them and are not recomputed after a candidate attempt.
- Every registered cell is reported `PASS`/`FAIL`/`INELIGIBLE`/`NOT_RUN` with
  its receipt path in the round's report.
