# Stage 5 remediation evidence — 2026-09-17 (UTC)

Remediation of the independent Stages 1-5 acceptance review, started at
`c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6` (branch `main`), the reviewed
snapshot. Every log below was produced by the binary it names, on the commit
recorded next to its section. The reviewer's own evidence under
`../stages-1-5-review-20260917T160000Z/` was **copied, never edited**.

## diagnostics/

A copy of the reviewer's four-binary diagnostic client with crate paths made
relative, so the same client runs against the before and after trees. The
`Cargo.toml` is the only file that differs from the retained copy.

Build and run (from `diagnostics/stage5-diagnostics/`):

    CARGO_TARGET_DIR=/tmp/lfs-remediation-diag-target cargo +1.85.1 build --bins --locked

    /tmp/lfs-remediation-diag-target/debug/stage5-diagnostics <run-dir>   # smoke + boundary + listing
    /tmp/lfs-remediation-diag-target/debug/topology                        # T1..T4 topology shapes
    /tmp/lfs-remediation-diag-target/debug/dangling <run-dir>              # R2 arms
    /tmp/lfs-remediation-diag-target/debug/ordering <run-dir>              # four-point growth probe

### Before (HEAD `c99a8d9f9`)

| log | what it shows |
| --- | --- |
| `smoke.log` | listing byte bound returns an empty page; build cycle accepted |
| `topology-before.log` | T1/T2/T3 accepted, T4 refused |
| `dangling-reference-before.log` | both arms acknowledged, dangling arm's root unreadable |
| `ordering-growth-before.log` | 3.65x/3.74x/3.83x per doubling against a 2.14x/2.16x/2.38x control |

### After WP1 (HEAD `a1601b92f`)

| log | what it shows |
| --- | --- |
| `smoke-after-wp1.log` | `ObjectLimitExceeded` for bounds 1..15; `build.disconnected_cycle=REFUSED` |
| `topology-after-wp1.log` | T1/T2/T3/T4 all refused |

### After WP2 (HEAD `4f2e4ca9a`)

| log | what it shows |
| --- | --- |
| `ordering-growth-after-r3.log` | 3.01x/3.24x/3.46x per doubling; `rows_read` grows with the work (42,370 -> 3,378,078 at 1,600 entries) |

`dangling-reference-after-wp1.log` reproduces the before result byte for byte:
that is the point of R2's option (b) decision, not a regression.

## Deliberately not committed here

`docs/roadmap/0.1/0.1.7/component-decoupling/stages-1-5-review-20260917T160000Z.md`,
`.../stage-5-remediation-handoff-20260917.md`,
`docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T160000Z/` and
`docs/roadmap/0.1/0.1.7/study/cloudflare-computer/` are another owner's
artifacts. They were read as the governing contract and left untracked.
