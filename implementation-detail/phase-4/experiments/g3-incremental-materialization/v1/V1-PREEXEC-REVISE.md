# G3-v1 pre-execution revision

Disposition: **REVISE_BEFORE_MEASUREMENT**

No G3-v1 measured child ran. At classification time both
`target/phase4-g3-incremental-materialization-20260822-v1` and its `.lock`
were absent. The zero-row dry run remains historical evidence and no existing
v1 file was edited.

## Exact defect

The v1 runner froze only
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` as candidate
source custody. The actual candidate also depends directly on the newly added
`phase4_g3_materialization.rs`, the engine Cargo manifest that declares the
binary/dependency shape, and the workspace `Cargo.lock`. Because v1 omitted
those three inputs, its prospective `source_sha256` could not identify the
complete source set used to build the executable. Executing v1 would therefore
create an avoidable binary-to-source provenance gap.

The defect is orchestration/evidence-only. It does not reject the candidate
mechanism or alter the frozen nine-row schedule. v2 must freeze and copy all
four source inputs, derive a canonical `source_set_sha256`, build exactly once
inside the recorded campaign wall, freeze the resulting binary, and bind that
source-set digest to every row and terminal record.

## Frozen v1 file hashes

| File | SHA-256 |
|---|---|
| `ATTEMPT-A-STATIC-NO-GO-v1.md` | `7ad9413afa029e9e9f164c03722741a995efd3cbd0266fb3de7c670418deff9d` |
| `COUNTER-DICTIONARY-v1.md` | `2d388bc8ce65be8f0c9e877311368bdb86cc966479fd7332626a1dd9eb8412c8` |
| `CUSTODY-FREEZE-v1.md` | `cc11a1ad8088b89118a462d63a91150224c33f9b7335b6ccac319470cc95cd0b` |
| `DRY-RUN-v1.json` | `52d2d2c69f195be212f462ef4356be152fab43bb2879ae7b3109b57bb5f2be77` |
| `PREEXISTING-UNTRACKED-SHA256-v1.tsv` | `63b028fb4bff45ac8c7b46ad2ac073c95f4438a3cefa3fe3af34b583adba85c6` |
| `PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v1.md` | `2b49c77aa07eeaebc1e5889f4a648772241426194f1e20bdbe53a468b8969d81` |
| `analyze_g3_v1.py` | `58af2e737dcb198e7d7515d6a3755cd8b2db0c623bdac48c98fa9a9cb80353d2` |
| `recompute_g3_v1.py` | `79cf4284bee92d11debc6580c2c2a6204ea1c0617f1f16687fbb4d49ad4780c3` |
| `run_g3_v1.py` | `64543344f475f2df686f1da1169f67157df095053d388fc137595652c8cfd4b1` |

This report is additive and intentionally is not part of the earlier v1
dry-run hash table.
