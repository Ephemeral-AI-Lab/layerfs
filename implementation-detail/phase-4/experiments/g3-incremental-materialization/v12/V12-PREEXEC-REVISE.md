# G3-v12 pre-execution REVISE

Date: 2026-08-22

Disposition: **REVISE before execution; zero v12 measured rows; G4 remains
unstarted**

The seven frozen v12 methodology/dry-run files remain byte-for-byte unchanged.
No v12 result root or lock was created. Independent pre-execution review found
five evidence-protocol defects, so v12 is preserved and must not be executed:

1. `finalize_g3_v12.py` verified the sealed G2-v5 manifest, terminal,
   terminal verification, raw rows, and normalized-ledger agreement, but did
   not pin and rehash the exact primary analysis
   `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803`
   or independent recomputation
   `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e`.
   A different analysis pair with the same normalized ledger could therefore
   satisfy the historical dependency gate.
2. The static finalizer accepted the six expected labels with arbitrary
   nonempty argv and accepted only the summary substring `15 passed; 0 failed`.
   It did not require the six prospectively exact command sequences/argv or
   prove that the output named exactly the intended 15 focused G3 tests.
3. The v12 execute token was self-contained in the methodology. There was no
   external premeasurement freeze binding the exact source set, methodology
   set, and dry-run hash to an independently supplied exact anchor-file SHA.
   Source, method, dry-run, or anchor changes therefore did not require a new
   independent execution authorization.
4. Primary analysis, independent recomputation, and finalization checked only
   the length of each raw row's executable, methodology, and environment
   digests and did not equate every row's command to the authoritative frozen
   nine-row plan. A consistently shaped but wrong operand, method, environment,
   or argv could pass those custody checks.
5. `ENVIRONMENT-v12.json` captured selected values from the parent process
   before the runner constructed the actual child environment. It therefore
   did not record or enforce the selected build/row/analyzer values, source and
   methodology identities, or the executable identity used by children when
   applicable.

Frozen v12 identities at rejection:

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v12.md` | `39a081a185aa4560e60f5d6a862c47e0f13d9ac2d67ac769f6676a1238f8ecf8` |
| `COUNTER-DICTIONARY-v12.md` | `ae02f03113aee72686cacbc6535fd933d3e01d1632a647ab9acffa6303f16cb6` |
| `run_g3_v12.py` | `173927974edfdad4aad6c7b9f13235f42d07ad53945a804d51b513c1dada51e5` |
| `analyze_g3_v12.py` | `46c90041f462105aea1d03de112d695b0dd99aa2a94eac5bf49a7df1cd83ba3b` |
| `recompute_g3_v12.py` | `1922bdb89df0867f621ba0af183c774fec017e300ff3dad98d6d02e378c9eddb` |
| `finalize_g3_v12.py` | `d5f66aac0acb5c17dea21983abd749fe6854aaf237bd48c943c5bd1fae036671` |
| `DRY-RUN-v12.json` | `f68396efb3e07a5750277e62956c5ef7783d82cdd65ec346d56e2b54af6fce1a` |

v13 must close all five findings while retaining the v12 repaired source,
full authenticated fallback, direct changed-range counters, old-or-new
publication, exact nine-row/no-rerun schedule, and stop-before-G4 boundary.
