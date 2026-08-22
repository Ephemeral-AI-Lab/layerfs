# G3-v6 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v6 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v6` and its `.lock` were
absent. All frozen v6 artifacts remain zero-row historical evidence.

## Exact finalizer audit defect

Mechanical v5→v6 versioning accidentally rewrote the immutable G2 dependency in
`finalize_g3_v6.py` to nonexistent G2-v6 paths and filenames. `verify_g2()`
looked for a v6 decomposition root, v6 payload/terminal/raw names, and v6
analysis files even though the only authoritative dependency is sealed G2-v5.
Consequently any completed v6 campaign would fail finalization regardless of
its own evidence.

The finalizer self-check did not invoke `verify_g2()`, so it failed to expose
the broken historical anchor. v7 pins every G2 path back to sealed v5 and makes
the no-write self-check execute the complete G2 hash and normalized-ledger
verification.

## Frozen v6 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v6.md` | `28126bc465d413667643d33fafa63f44dda13b996675ff14d3bab0ab9c627801` |
| `COUNTER-DICTIONARY-v6.md` | `8e1bdb1589392c78ef63f989aabf92c7529fffd393e42509c430975ff2cede5c` |
| `run_g3_v6.py` | `3f22e312a67ef6d7d89a1390ff41186c4fcc508a49ffe95bbdf6a8b2355d6c63` |
| `analyze_g3_v6.py` | `78ae6cfe7ad31b004ca78713c0238874227bcd2d753d5986304900b748955c74` |
| `recompute_g3_v6.py` | `93566bfa6c1576946d2cf13b5d5f28dc3940217507ba19226a0ce05e0c97ba2f` |
| `finalize_g3_v6.py` | `40326793b43cf3776cba364991ce706e22ec04237da9475a52050a33d6a631bf` |
| `DRY-RUN-v6.json` | `9dc644eaf507332379815f79aa45534a8813b0315b6adc6be420aaf0b2b56e88` |
| source-set digest | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology-set digest | `d035c67822c484ea1705571e00a4e235db4bda9946154e1dc576e65275f478e9` |
