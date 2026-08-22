# G3-v9 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v9 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v9` and its `.lock` were
absent. All frozen v9 artifacts remain zero-row historical evidence.

## Exact analyzer self-check defect

The v9 runner invokes each analyzer's frozen copied `--self-check` before any
row. Primary lines 317–320 and independent lines 265–268 construct the expected
results root from global `REPO`, but a copied analyzer's `HERE.parents[4]`
resolves one level above the repository. Thus both actual-analysis paths were
repaired, but the mandatory copied self-check would fail before measurement.

v10 makes each analyzer self-check execution-location aware. It validates either
the exact source-tree method location or the exact relocated
`<repo>/target/<v10-target>/results-v10/methodology-v10` location, derives the
campaign repository through `repo_from_results`, and rejects malformed or
one-level-high layouts. Runner and finalizer remain source-executed and retain
their existing derivation checks.

## Frozen v9 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v9.md` | `8343b05316bd10d1963003cb2b1b4b9e48695432c11ac726bfd7a03dc5410ee6` |
| `COUNTER-DICTIONARY-v9.md` | `37e3e53b5bcf7dc8f4a7ddcfd631768264ecc3883ddcf63fbea84eaa1f0b118f` |
| `run_g3_v9.py` | `b6d43155e4700499a6f4c8ad66d14ef99be2c1fe5adc6694278b5c9a005822d2` |
| `analyze_g3_v9.py` | `d9565881badb712c7ce0691033e5ce3cafff2700e9161d40f15bac3eba8d0e18` |
| `recompute_g3_v9.py` | `ef6e84131cde7d7b1a1e16bcdc137f01a60fb5d9b9130b3eec4206f1b76d0aac` |
| `finalize_g3_v9.py` | `335812ddf295536ee08a25f3fb4726d07546c8ac84f1ed9ce432514c34c703c2` |
| `DRY-RUN-v9.json` | `06a6bbcb8c0811d8a81dad06061910a38c822a04b74d0dac7d7767317a8ab87c` |
| source-set digest | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology-set digest | `2937869aa433d182596c2a4a528a8d67b4d9893d48f468e7e76cdbfbfaee5769` |
