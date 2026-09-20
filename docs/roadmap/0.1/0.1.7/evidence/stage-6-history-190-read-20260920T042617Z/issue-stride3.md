## #190 stride3 confirmation, verification and disposition

One sample per arm on stride3, same identities and protocol as stride10.

| quantity | baseline2 | candidate2 | delta |
|---|---:|---:|---:|
| **Operation ns** | 56,552,396,089 | **49,458,300,790** | **−7,094,095,299 (−12.544288%)** |
| Filesystem ns | 32,619,895,790 | 25,336,046,585 | −7,283,849,205 |
| Provider ns (nested) | 31,150,817,381 | 23,865,118,886 | −7,285,698,495 |
| Store begin+accept+finish ns | 20,149,867,049 | 20,329,594,046 | +179,726,997 |
| Content ns | 3,021,191,127 | 3,033,043,252 | +11,852,125 |
| Complete command ns | 80,483,422,875 | 73,127,405,000 | −7,356,017,875 |
| catalogue statements | 1,334,277 | **368,074** | — |
| catalogue interval ns | 10,443,893,588 | 3,315,711,491 | −7,128,182,097 |

The stride3 content and Store intervals are very slightly slower; the filesystem interval dominates by an order of magnitude, and both are reported as measured rather than netted away. The rejected span shape regressed here by **+1,885,811,742 ns** and was therefore not retained despite its larger stride10 win.

**Equivalence.** All 17 / 53 state roots match; canonical and value-group inventories match; the saved Stores are **byte-identical between the arms** in both selections (stride10 `4af37932…`, 49,324,032 B apparent / 50,249,728 B allocated; stride3 `f5c7ff5a…`, 62,152,704 / 62,152,704) and equal the retained L40 level-1 candidate Stores exactly. The historical allocated targets 49,344,512 / 64,024,576 B are unchanged: stride10 misses by 905,216 B, **inherited from L40 because this treatment changes no stored byte**, and is not relabelled.

**Separate identity-matched verification**, one run per performance identity:

| selection | baseline ns | candidate ns | target ns | gates |
|---|---:|---:|---:|---|
| history-stride10 | 5,593,199,541 | 4,606,757,792 | 10,000,000,000 | PASS / PASS |
| history-stride3 | 19,864,253,708 | 16,126,274,334 | 20,000,000,000 | PASS / PASS |

1,083 / 3,377 sampled paths per arm, zero mismatch, zero missing, zero unexpected, identical 2,415 / 8,739 group decodes. Coverage is **sampled, not exhaustive**.

**Validation.** 494 core tests, 117 harness tests, 12 example targets, core Clippy/fmt, 122-file boundary guard and six guard self-tests **PASS**. Inherited harness Clippy/format failures are unchanged, not rerun, and not claimed as passes. One stride3 preflight was deferred at 66.73% CPU idle with no named competitor and consumed no sample.

**Disposition: retain.** PR [#202](https://github.com/Ephemeral-AI-Lab/layerfs/pull/202), commit `4049e28b6`, then `3cec2f8da` for the LOC record. Production LOC **85,722 → 85,725 (delta +3)**; reference 65,417 unchanged, core 20,305 → 20,308. Ledger entry L42.

**Not established, and left open.** Cold/performance admission **INELIGIBLE**; every O3 pinned-counter row **INCOMPLETE**; the unmatched historical v0.1.6 comparison unresolved; single samples do not establish repeatability; the diagnostic 120/240 s caps are not ordinary 15/25 s admission rows; the provider's 1.34 s stride10 residual is unnamed; the 7,817 ns per-statement cost is not decomposed. **#190 stays open.** Proposals not implemented: selected-group BLOB range reads, operation-scoped pooled pack cache, pipeline ownership/streaming.
