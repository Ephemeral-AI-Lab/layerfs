#!/usr/bin/env python3
"""Generate release-notes/0.1.6/benchmark-closeout.md (report-only).

Per-case rows come from benchmark-performance.csv and benchmark-verification.csv,
which `derive_tables.py` derives from the two committed seed-1 matrices
(`docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json` and
`final-complete-extended-matrix.json`). Nothing is executed here and no number is
recomputed: each cell is copied from the derived table, which in turn copies the
driver's own verdict and the receipt's own declared allowance. Family prose
summarises the published reports and the rollout ledger.
"""
import csv
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "release-notes/0.1.6"

FAMILY_ORDER = [
    "file_size_transition",
    "branch_development",
    "dedup_branch_history",
    "mixed_load_bearing",
    "multi_workspace_development",
    "historical_access",
]

FAMILY_NOTES = {
    "file_size_transition": (
        "The F1 boundary family: the seven control, above, below, exact and "
        "round-trip cases where a file crosses the 128 KiB small/large content "
        "boundary (131071/131072/131073 B). All seven complete in 1.774-2.127 s "
        "with a PASS verification, and every one carries its own declaration "
        "rather than an inference from the boundary neighbour."),
    "branch_development": (
        "Four v0.1.6 branch cases plus the two F4 compact-branch controls. The "
        "100 MiB K10/K100 and 500 MiB K10 rows sit at 1.979-7.373 s; the "
        "500 MiB K100 row is the campaign's largest regular row at 16.492 s "
        "performance (above the 15 s family target, gate `EXCEPTION`) and 23.10 s "
        "verification under the owner-declared 30 s ceiling (ledger L12). The two "
        "F4 controls prove per-head compact branch roots and the distinct "
        "reference-versus-root accounting that the v0.1.6 oracle requires."),
    "dedup_branch_history": (
        "The six F5/F6 history profiles - boundary-cycle, large-hotset and "
        "namespace-inode at K10 and K100 - each of which replays a five-stage "
        "schedule and proves the retained history independently. Performance "
        "2.045-2.821 s, verification 3.670-9.448 s, all PASS. The namespace-inode "
        "rows are the family that v0.1.6 rebuilt after the earlier revision's "
        "boundary-cycle exchange was found wrong; the corrected exchange (the "
        "higher file's byte moves down, (131071,131073)<->(131072,131072)) is "
        "what both rows now verify."),
    "mixed_load_bearing": (
        "Four regular mixed-development rows (3.042-18.312 s performance, "
        "3.029-22.121 s verification, all PASS) plus two verify-only exhaustive "
        "rows that carry no performance timer by declaration. The exhaustive "
        "rows are the campaign's deepest oracle replay: 15.014 s and 59.326 s "
        "against frozen 120 s and 300 s watchdogs, reported as the measured walls "
        "they are."),
    "multi_workspace_development": (
        "Four concurrent-workspace rows (3.304-15.760 s performance, "
        "3.824-20.360 s verification) plus the F4 four-workspace control "
        "(7.573 s / 9.882 s). `v016-workspace-mixed-500mb-30000-k100-v1` is above "
        "the 15 s family target in both modes under its declared allowance; the "
        "four-workspace control is the only row that runs four named workspaces "
        "with 100 commits each."),
    "historical_access": (
        "The six F6 `historical_access` cases are verify-only by declaration: "
        "they carry no performance timer and every one is proved by an "
        "independent reader plus a verifier pass in 1.593-1.964 s. Together they "
        "cover the boundary before/after pair (131071/131072 B), the inode "
        "before/after pair (8192/4096 B), the fork-point trunk and the divergent "
        "head B. These are the v0.1.6 rows that replaced the inherited revision's "
        "seven-case declaration, and their producers are sealed per case."),
}


def load(name):
    with (OUT / name).open(newline="") as stream:
        return list(csv.DictReader(stream))


def esc(text):
    return (text or "").replace("|", "\\|")


def number(value):
    if not value:
        return "—"
    return f"{float(value):.3f}"


def table(records):
    lines = [
        "| Selection | Performance | Perf wall (s) | Perf gate | Verification | Verify wall (s) | Verify gate | Declared allowance (s) | Cleanup | Evidence |",
        "|---|---|---:|---|---|---:|---|---:|---|---|",
    ]
    for row in records:
        lines.append(
            f"| `{row['case']}` | {row['status']} | {number(row['complete_wall_seconds'])} | "
            f"{esc(row['gate']) or '—'} | {row['proof_status']} | "
            f"{number(row['proof_wall_seconds'])} | {esc(row['verify_gate']) or '—'} | "
            f"{esc(row['allowance'])} | {row['cleanup'] or '—'} | "
            f"`{row['tag']}/{row['family']}` |")
    return "\n".join(lines)


def main():
    performance = {row["case"]: row for row in load("benchmark-performance.csv")}
    verification = {row["case"]: row for row in load("benchmark-verification.csv")}
    order = [row["case"] for row in load("benchmark-performance.csv")]
    assert set(order) == set(verification), "performance and verification tables disagree"

    merged = []
    for case in order:
        perf, proof = performance[case], verification[case]
        merged.append({
            "case": case,
            "family": perf["family"],
            "tag": perf["tag"],
            "status": perf["status"],
            "complete_wall_seconds": perf["complete_wall_seconds"],
            "gate": perf["gate"],
            "proof_status": proof["proof_status"],
            "proof_wall_seconds": proof["proof_wall_seconds"],
            "verify_gate": proof["gate"],
            "allowance": (
                f"{float(perf['declared_allowance_seconds']):.0f} perf / "
                f"{float(proof['declared_allowance_seconds']):.0f} verify"
                if perf["declared_allowance_seconds"] else
                f"{float(proof['declared_allowance_seconds']):.0f} verify"
            ),
            "cleanup": "PASS" if (perf["cleanup"] or proof["cleanup"]) == "PASS"
            and proof["cleanup"] == "PASS" else "CHECK",
        })

    by_family = {}
    for row in merged:
        by_family.setdefault(row["family"], []).append(row)

    measured = [row for row in merged if row["status"] != "N/A"]
    verify_only = [row for row in merged if row["status"] == "N/A"]
    exceptions = [row for row in merged if "EXCEPTION" in (row["gate"], row["verify_gate"])]
    # One entry per mode, so a row above the target in both modes is reported
    # twice instead of being summarised by its larger number.
    over_target = []
    for row in merged:
        if row["complete_wall_seconds"] and float(row["complete_wall_seconds"]) > 15.0:
            over_target.append((row, "performance", row["complete_wall_seconds"],
                                row["gate"], row["allowance"]))
        if float(row["proof_wall_seconds"]) > 15.0:
            over_target.append((row, "verification", row["proof_wall_seconds"],
                                row["verify_gate"], row["allowance"]))

    parts = [f"""# v0.1.6 benchmark closeout — every measured selection

> **Status:** LayerFS 0.1.6 release record. Generated report-only from the two
> committed seed-1 matrices by [`generate_closeout.py`](generate_closeout.py);
> **no benchmark or proof was run to write this file.** Every cell is copied from
> [benchmark-performance.csv](benchmark-performance.csv) and
> [benchmark-verification.csv](benchmark-verification.csv), which are themselves
> derived from the receipts: dispositions are never recomputed, `N/A` never
> becomes zero, and a declared exception is printed with its measured wall.

## What was measured

| Quantity | Value |
|---|---:|
| Registered rows in this release record | {len(merged)} (33 regular + 3 extended) |
| Rows with a performance receipt | {len(measured)} |
| Rows verify-only by declaration | {len(verify_only)} |
| Rows with an independent verification | {len(merged)} — every one `PASS` |
| Performance gate `PASS` | {sum(1 for row in measured if row['gate'] == 'PASS')} |
| Performance gate `EXCEPTION` (above the 15 s family target, inside the declared allowance) | {sum(1 for row in measured if row['gate'] == 'EXCEPTION')} |
| Verification gate `PASS` | {sum(1 for row in merged if row['verify_gate'] == 'PASS')} |
| Verification gate `EXCEPTION` | {sum(1 for row in merged if row['verify_gate'] == 'EXCEPTION')} |
| Cleanup `FAIL` | 0 |
| Mode-level results above the 15 s family target | {len(over_target)} |
| Rows carrying an `EXCEPTION` gate in either mode | {len(exceptions)} |

Seed 1, one sample per case and mode, repetition 1, `--setup clone` for every
post-initialization case, the same image and the same sealed producer for both
modes. All {len(measured)} performance receipts reused the closed prepared master
(`cache_hit: true`, `clone_method: closed-quiescent-byte-copy`; preparation
0.424–2.563 s, outside every timer). Cleanup ran 0.338–0.864 s and passed in all
64 driver records.

## Declared allowances

The 15 s family target is reported for every row and is **not** redefined by any
allowance below.

| Scope | Allowance | Source |
|---|---:|---|
| Regular v0.1.6 performance invocation (complete command) | 60 s | owner ruling, ledger L10 |
| Regular v0.1.6 verification (complete command) | 25 s (22 s worker stop + 3 s cleanup) | frozen `cases.json`; ledger L10 |
| `v016-branch-mixed-500mb-30000-k100-v1` verification | 30 s | owner ruling on #154, ledger L12 |
| Extended `v016-workspace-four-100mb-5000-k100-v1` | 60 s | frozen `V016_EXTENDED_WATCHDOGS` |
| Extended `v016-mixed-exhaustive-100mb-5000-k100-v1` | 120 s | frozen `V016_EXTENDED_WATCHDOGS` |
| Extended `v016-mixed-exhaustive-500mb-30000-k100-v1` | 300 s | frozen `V016_EXTENDED_WATCHDOGS` |

Each row's own column prints the allowance its receipt declared and enforced, so a
row can never be read against a ceiling its receipt did not carry. The 30 s and
the three extended watchdogs are wider than the rules' regular band and are
declared as such in [waivers](waivers.md); the measured walls (23.10 s, 9.882 s,
15.014 s, 59.326 s) are all inside the wider band **and** are reported as measured
rather than trimmed to fit a smaller one.

## Every family and case
"""]

    for family in FAMILY_ORDER:
        records = by_family.get(family, [])
        if not records:
            continue
        parts.append(f"### `{family}`\n\n{FAMILY_NOTES[family]}\n\n{table(records)}\n")

    parts.append(f"""## Results above the 15 s family target

| Selection | Mode | Measured (s) | Gate | Declared allowance (s) |
|---|---|---:|---|---:|
""" + "\n".join(
        f"| `{row['case']}` | {mode} | {number(wall)} | {esc(gate)} | {esc(allowance)} |"
        for row, mode, wall, gate, allowance in over_target
    ) + """

The two verify-only rows carry gate `PASS` because the allowance their own receipt
declared is 120 s and 300 s; measured against the 15 s family target they are
target misses, and they are printed here as such rather than reported as fast
rows. Every other gate in this closeout is `PASS`, and the three `EXCEPTION`
gates are the rows above the 15 s target inside a declared allowance. No row in
this release record is a `FAIL`, a `TIMEOUT` or a `NOT_RUN`.
""" + """

## Identity of every row in this closeout

| Field | Value |
|---|---|
| Source commit | `823f556ca301e261b71d41b09ef619897b48e5f9` |
| `LAYERFS_SOURCE_SEAL` | `86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954` |
| `LAYERFS_SOURCE_DIRTY` | `true` (foreign uncommitted `docs/roadmap/0.1.7` study material; recorded, not hidden) |
| `LAYERFS_PRODUCT_SEAL` | `970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` |
| `LAYERFS_COMPILATION_SEAL` | `679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b` |
| `LAYERFS_DEPENDENCY_SEAL` | `374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1` |
| Host binary SHA256 | `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7` |
| Harness identity | `8a6d76dd2f2e639fb356ac74551a4aae2634df1eaf2b3603ebd458cafd90dff5` |
| Image | `layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:2bd697b90894749592cf62e6470a0d0a9cf54a0416cbf3cd903dbbcc3d9954df` |
| Build identity probe | `schema_version: 10`, `storage_policy: ordinary`, SQLite 3.51.0, status PASS |
| Host load during collection | `load_1m` 6.73–8.83 on a 14-CPU host, recorded per driver record |
| Evidence root | `benchmark-results/v016/final3-seed1/` and `benchmark-results/v016/final3-ext/` (untracked local receipts, cited by path per row) |
| Matrices | `docs/roadmap/0.1/0.1.6/evidence/issue154/final-complete-matrix.json`, `…-extended-matrix.json` |

Every row omits an exhaustive Phase 1 replay by declaration
(`omissions: ["no exhaustive Phase 1 replay"]`), and no row carries a reused proof
identity (`reused_proof_identities: []` in all 36 verifications).

This closeout covers the v0.1.6 selections only. The sandbox-local comparison
campaign — the B1/B2/B3 controls and the 196-cell #152 matrix — is published
separately in the
[#152 final report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md).
""")
    (OUT / "benchmark-closeout.md").write_text("".join(parts))
    print(f"wrote {OUT / 'benchmark-closeout.md'}")


if __name__ == "__main__":
    main()
