# #237 pack-space C1: exact first write only when already closed

The [prospective plan](pack-space-treatment-plan-20260924.md) froze this as the first, narrow treatment. Source `bbb0281bcd34c0bd6983c9d1a298a5646584eef6` changed only Core pack placement: a new pack displaced within its first `select_many` call now inserts `zeroblob(used)` rather than `zeroblob(262144)`. An open pack still reserves room for later in-place append. The [placement boundary test](../../../crates/layerfs-storage/tests/pack_locator.rs) was red on the control (262,144 versus expected 200,728), then the focused ten-test `pack_locator` suite passed on C1. The [physical-writing architecture](../../architecture/13-physical-writing.md) was updated in the product commit. Production LOC `117421 → 117425` (+4), core `52004 → 52008`, reference unchanged `65417`.

Exactly one candidate used locked Cargo release examples from the clean committed C1 source, the same runner harness seal `96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3` as the retained [Core release control](sdk-100k-release-result-20260924.md), seed-1 SHAKE manifest `23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`, an independently hashed source copy, and a fresh result directory. Its final check saw **0/126,206 resident payload pages**. Namespace metadata remained unqualified and no result is performance admission. The [candidate receipt](evidence/packspace-c1-20260924/receipt.json), [build](evidence/packspace-c1-20260924/build.json), [cold checks](evidence/packspace-c1-20260924/cold-launch.json), and [source-copy custody](evidence/packspace-c1-20260924/source-copy.json) retain identities. The full raw hash manifest is worktree-local at `benchmark-results/fs-bench-pro/sdk-100k-packspace-c1-20260924-01/manifest.json` and `runner.py verify` returned PASS.

| One raw 100k release row | Control | C1 candidate | Difference |
| --- | ---: | ---: | ---: |
| Public `Client::init_project` | 5.077702667 s | **5.512661042 s** | +0.434958375 s raw; not a causal speed ratio |
| Complete driver command | 6.408342167 s | **6.065425084 s** | −0.342917083 s raw; outside-call setup varied |
| Full reopened 101,001-path / 500-MB oracle | PASS, 12.603483166 s | **PASS, 12.956341584 s** | both miss general under-10-s verifier expectation |
| Pack rows | 2,082 | **2,082** | 0 |
| Exact-length pack rows | 0 | **17** | +17 |
| Pack BLOB capacity | 545,783,808 B | **545,160,862 B** | −622,946 B |
| Pack declared used | 510,615,731 B | **510,615,715 B** | −16 B from fresh authority identity |
| **Unused pack tail** | **35,168,077 B** | **34,545,147 B** | **−622,930 B (−1.77%)** |
| Combined Store + History apparent | 554,098,688 B | **553,472,000 B** | −626,688 B |
| Combined Store + History allocated | 558,145,536 B | **554,713,088 B** | −3,432,448 B |
| Driver lifecycle peak RSS | 155,631,616 B | **157,237,248 B** | +1,605,632 B raw, different run |

The [control](evidence/packspace-c1-20260924/control-pack-geometry.json) and [candidate pack geometry](evidence/packspace-c1-20260924/candidate-pack-geometry.json) were queried read-only from the closed databases. Both have 4,096-byte SQLite pages, zero freelist pages and 2,082 packs. C1 removes only **622,930 B of 35,168,077 B** reserved tail; **98.23% remains**. The larger allocated-file reduction also includes a **2,805,760-B reduction in filesystem allocated-minus-apparent accounting**, so it must not be credited to the changed pack capacity alone.

**Decision: partial mechanism proof, not the database-space fix.** The full readback passed, but C1 missed the material space objective: only 17 packs were born and displaced in the same placement call. Retain this row; do not repeat it. Proceed to the separately frozen C2 treatment that closes every pack at each placement flush, then measure its pack count, tail, Store pages, CPU/RSS and full oracle before deciding whether to adopt it. The historical v0.1.6 100k Store remains a different-fixture context, not a matched compactness gate.
