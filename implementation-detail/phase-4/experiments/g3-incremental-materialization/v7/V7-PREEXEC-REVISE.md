# G3-v7 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No v7 build, measured row, analyzer campaign child, or finalizer ran. At
classification time both
`target/phase4-g3-incremental-materialization-20260822-v7` and its `.lock` were
absent. All frozen v7 artifacts remain zero-row historical evidence.

## Exact sole defect

`COUNTER-DICTIONARY-v7.md` line 183 says the finalizer verifies sealed G2-v7
anchors. That contradicts the same file's lines 159–164 and the corrected
finalizer, both of which bind the only authoritative dependency: sealed G2-v5.
Because the contradictory dictionary was part of the hashed method set, v7's
methodology identity cannot be accepted even though its executable code points
to the right G2 evidence.

v8 changes only G3 namespace/versioned methodology and removes every
version-matched G2 reference. All G2 dependency language and code consistently
bind sealed G2-v5, and a method-set-wide self-check enforces that rule.

## Frozen v7 hashes

| File | SHA-256 |
|---|---|
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v7.md` | `b2336a48a41ca91f71f50cf67cfb93a9c9ea4c012bbb84531be79892e2e399f6` |
| `COUNTER-DICTIONARY-v7.md` | `c40f1fe36823610abd300e95903a26f17f9e1dd8aabe3c0099349d9cda5e1e29` |
| `run_g3_v7.py` | `9c635e20a2d2e27c82e01e1934be5ee06a3493135603a624a712746638ac331d` |
| `analyze_g3_v7.py` | `4a46ec34a4591126859e069fb713402c26fe459321d7e82e185de8bd7b949a7a` |
| `recompute_g3_v7.py` | `cf7bc29b19906a98070c8757b08765162994cb32fa92d5a8dcfc58a44b02b738` |
| `finalize_g3_v7.py` | `7844cb91e4f43833f8720ce0727d7738b3134f02eee35d2c7333357a4d6f803d` |
| `DRY-RUN-v7.json` | `ce20fa0508c192569d51f2c9a2667659b97a1bc2c2cb8b864f0ab9566aca726e` |
| source-set digest | `70ef2606389813ebd980bf2e5fe9f4585333717fd7dabf21fb69cb4e4c140c9f` |
| methodology-set digest | `fbfd3a253fe6f9cfd0f59dc2b240c6560dcd78e5f897820478d7f154efb47361` |
