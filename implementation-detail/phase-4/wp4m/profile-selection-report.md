# WP4-M terminal profile report

## Current controlling disposition — CP-0006

Status on 2026-08-21:

```text
CP-0006 fixed-radix compact lane: PASS / RETAIN
WP4-M:                            COMPLETE
WP4-P:                            COMPLETE / PASS
WP4:                              COMPLETE
WP5:                              ELIGIBLE / PENDING
K64/F64 + DIR256K:                one compatibility-promoted profile
DIR256K selection basis:          unmeasured fallback
compatibility promotion:          true by WP4-P only
Phase 4:                          not complete
```

WP4-P is COMPLETE / PASS under `../wp4p/promotion.md`. K64/F64 + DIR256K is
the one compatibility-promoted profile; losers/selectors are deleted;
production profile ID is
`b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1`;
and selected goldens plus core/benchmark/parity/workspace/clippy gates pass.
Both independent audits pass after the exact 2,010-entry maximum delta-page
corpus case was added. WP4 is complete and WP5 is eligible/pending; overall
Phase 4 is not complete.

Verification counts are core 44 PASS, selected goldens 2 PASS/1 intentionally
ignored printer, benchmark 54 PASS, parity 14 PASS, full workspace
all-target/all-feature PASS, and clippy `-D warnings` PASS. Golden TSV, golden
test, and selected benchmark hashes are respectively `6de8c752...a7330`,
`727fe668...49701`, and `2209930d...6dbea`. No benchmark was rerun for WP4-P.
CP-0006 remains `qualification=false` and `promotion=false`; compatibility
authority comes only from the later WP4-P promotion gates.

The compact campaign completed 27/27 rows under the configured 120-second
ceiling: six warmups, 18 measured capture/edit rows, and three nonmedian
complete-roundtrip writes. The terminal console reported an observed 50-second
wall. No 512-MiB fixture or row ran. All temporary databases and fixtures were
deleted; the retained compact bundle is approximately 0.5 MiB.

Every row passed exact CDC/identity/root/transition/closure checks, one
transaction and one successful COMMIT, timer equations, W/D/Q accounting, and
terminal Q zero. Six immutable database/authority/expectation masters were
byte-copied per sample and rehashed unchanged after the final row. Python and
Ruby analyzers independently return `PASS` with no reasons and agree after
canonical JSON sorting.

Measured medians:

| Operation | Size | Median publication |
|---|---:|---:|
| full write | 1 MiB | 7.191667 ms |
| full write | 10 MiB | 64.032292 ms |
| full write | 100 MiB | 603.327666 ms / 165.747 MiB/s |
| same-count middle | 100 MiB | 8.639167 ms |
| `+1` early | 100 MiB | 432.939417 ms |
| `+1` middle | 100 MiB | 432.324667 ms |

The `+1` ratios, 71.758588% early and 71.656695% middle, remain mandatory
diagnostics but are nonbinding under the prospective suffix-linear policy.
Count-changing edits are honestly `O(suffix)`, worst-case `Theta(N)`; no
logarithmic claim is made.

The formula-only 100-GiB analytical suffix bound is not a fixture, runtime
test, or latency projection. The middle model is 2,705,409 rebuilt references,
42,273 changed leaves, 673 changed branches, 42,947 mapping objects, and
186,891,342 canonical mapping bytes. Actual 100-GiB allocation was zero.

Evidence:

| Artifact | SHA-256 |
|---|---|
| raw JSONL | `b3596ff61b1314bad66f38675bc8acecccaa57d6a8686e30a0e224e91c8f72e1` |
| Python analysis | `d080f0f81346d0ec040801934129da94f04ef1e820b39adb97d733249e4024f5` |
| Ruby analysis | `86cd7018f849bfb605e351c99b47b7d5348dfd295a1e62ed9c8c96d49ead7114` |
| release executable | `7e91b90fecb9b314bfc2706c49184f09ff1e884db34804fc61772aabcf3dbb36` |
| runner | `965cc07fccc9f8aed8bea342b011d18e66bbf1e2d5680193cec6ea28b8e40c25` |

This completes CP-0006 measurement/policy selection only. WP4-P subsequently
deleted the losing profiles/selectors, froze and fingerprinted the selected-only
K64/F64 + DIR256K goldens, and passed both independent audits. WP4-P and WP4
are complete; WP5 is eligible/pending against the one compatibility-promoted
profile. Overall Phase 4 is not complete.

## Historical 216-row campaign — terminal NO-GO under its original contract

The earlier private 100/512-MiB file and 100,000-entry directory campaign
reported 216 rows and a 252-database read-only audit. Under its then-binding
contract, no challenger passed the primary performance gates and forced
`+1` ratios of 61.997–71.417% failed the then-binding 5% rejection rule.

That campaign never completed its required manifest, seal, external
attestation, or final audit. Its former approximately 65-GiB artifact root was
deleted at the user's direction. The surviving summaries are custody-lost,
historical directional evidence only; they are not relabeled, recovered, or
used to promote CP-0006.
