# Evidence: Stages 1-5 independent acceptance review (2026-09-17)

Reviewed snapshot: `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6` (branch `main`,
clean tree, tree `a4982eafbeaee7b019042c95b79588880c495607`).
Report: `../../component-decoupling/stages-1-5-review-20260917T160000Z.md`.

Nothing here modifies product, test, fixture or harness source. Every file was
created by this review; prior reports and receipts were read, never overwritten.
The `/tmp`-only artifacts (the `git archive` extractions of the four
comparison bases and the isolated fixture-reproduction copy) are reproducible from
the commands below and are not retained.

## How to reproduce

    # comparison bases (read-only; no worktree, no repo mutation)
    T=/tmp/lfs-loc-census-review; rm -rf "$T"; mkdir -p "$T"
    while IFS='=' read -r name sha; do
      mkdir -p "$T/$name" && git archive "$sha" | tar -x -C "$T/$name"
    done <<'SPECS'
    preS1=a8a1ba848429d5f2fbba83c2de22dada8c29def9
    preS5=4f1b7d847b706347d39721ba4ff1ba5d0de01034
    reviewed=c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6
    v016=44cf748486863ab7c21ca47e731bd88e2b9a7b4a
    SPECS
    python3 tools/production_loc.py --root "$T/reviewed" --detail
    python3 tools/production_loc.py --root "$T/reviewed" --files

    # sealed-fixture reproduction in an isolated copy (never in place)
    WORK=/tmp/lfs-fixture-repro; rm -rf "$WORK"; mkdir -p "$WORK"
    cp -R "$T/reviewed/." "$WORK/"
    cd "$WORK" && CARGO_TARGET_DIR=/tmp/lfs-fixture-target \
      cargo +1.85.1 test --locked -p layerfs-content --test stage5_reference_fixtures

    # reviewer diagnostics (public API only; own workspace, own target dir)
    cd diagnostics/stage5-diagnostics
    CARGO_TARGET_DIR=/tmp/lfs-diag-target cargo +1.85.1 build --offline
    /tmp/lfs-diag-target/debug/stage5-diagnostics fs
    /tmp/lfs-diag-target/debug/stage5-diagnostics store ../old-schema-store/store.sqlite
    /tmp/lfs-diag-target/debug/ordering /tmp/lfs-diag-run
    /tmp/lfs-diag-target/debug/dangling /tmp/lfs-dangling

    # old-schema Store built with the Python standard library alone
    python3 diagnostics/make_old_schema_store.py diagnostics/old-schema-store/store.sqlite

## Retained files

| file | what it is |
| --- | --- |
| `cargo-test.log` | the exact `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` transcript (363 passed, 0 failed, 0 ignored across 58 targets) |
| `cargo-clippy.log` | the exact clippy transcript, exit 0 |
| `checks-rerun.log` | `git diff --check`, the boundary guard, `fmt --check` and clippy re-run **after** this evidence directory existed |
| `loc-tables.md` | the four LOC tables used in report section 3 (per plan-named file, all other files, recursive directories, disjoint scopes) |
| `sa1-identity-codec.md` | component review: canonical identity, codecs, profile/schema compatibility, scoped identity |
| `sa2-cow-validity.md` | component review: sorted COW and whole-tree validity |
| `sa3-refs-ordering.md` | component review: reference accounting and the ordering subsystem |
| `sa4-reads-attrs.md` | component review: reads and attributes |
| `sa5-c2-timing.md` | component review: real C2 composition, telemetry/timing, Stage 5 verification evidence |
| `sa6-stages0-2.md` | component review: cumulative Stages 0-2 |
| `sa7-stages3-4.md` | component review: cumulative Stages 3-4 |
| `sa8-adapters-limits.md` | component review: environment neutrality, adapter seams, limits |
| `diagnostics/stage5-diagnostics/` | the external public-API diagnostic client (own `[workspace]`, three binaries); the only retained reproduction of findings R1, R2, R6, R7 and R8 |
| `diagnostics/run.log` | output of the `fs` and `store` probes |
| `diagnostics/ordering-growth.log` | the four-point spilling-versus-no-spill growth measurement (R3) |
| `diagnostics/dangling-reference.log` | the acknowledged-save-with-absent-objects reproduction and its control (R2) |
| `diagnostics/topology.log` | the second-parent, new-directory-cycle and same-batch-control probes (R1, R8) |
| `diagnostics/inode-header-arithmetic.log` | independent byte check of the 73-versus-81 inode-leaf header arithmetic against the sealed fixtures |
| `diagnostics/storage-footprint.log` | out-of-band storage accounting of the smoke runs |
| `diagnostics/smoke.log` | the three documented smoke commands into fresh paths |
| `diagnostics/make_old_schema_store.py` | the standard-library builder for the previous-revision Store (R7) |
| `diagnostics/verify_inode_header_arithmetic.py` | the sealed-fixture header verifier |
| `diagnostics/storage_footprint.py` | the storage accountant |
| `diagnostics/old-schema-store/` | the old-schema Store file and its build log |

Note on method: the component reviews under `sa*.md` were performed by
independent read-only reviewers working from the same frozen snapshot; each states
that it ran no build or test. Every finding they raised that this report treats as
load-bearing (R1, R2, R3, R4, R6, R7, R8, R10) was independently reproduced by the
lead reviewer through the public API, and the reproductions are in
`diagnostics/`.

## Identity manifest

Generated at the end of the review; any later edit to a file above invalidates its
row.
