# R1b navigation cursor delivery draft

Status: implemented source, checks pending root serialized source freeze. Parent53b6bf741,2026-10-01. No performance/resource qualification PASS claimed.

Owned changes: mapping/read.rs, new mapping/navigation.rs, mapping/mod.rs declaration, tests/navigation_cursor.rs, architecture03-files.md, this delivery and R1B-NAVIGATION-CURSOR-FREEZE.md. No draft/namespace/storage/Bridge code changed by this worker. SC-01/02/04/06/07/08.

Replaced whole level/next frontier vectors and ascending per-segment root restart with32 bounded descriptor frames, a fixed32 leaf-demand array and a partial-leaf continuation carrying its next ordinal/absolute position. Range pruning retains out-of-range descriptors for future segments. Existing independent current-page owners/cache64 and leaf/payload32 waves remain. A provider/codec/sink failure terminalizes the cursor, preserves partial sink output and denies provider replay.

New independent in-memory bodies:66 acquisitions/4 calls/max32 for65-leaf full read; cache1 gapped ascending ranges with root exactly once and only63 needed leaves; independently framed level2 branches with shared leaves and nonzero-origin ranges; sink failure after100 bytes with no additional provider call on reuse. Bodies written but NOT_RUN. Existing real StoreProvider mapping_cache, file_read, mapping_cache, and edit owning proofs need root's serialized checks. No wider physical/native claim.

Direct rustfmt +1.85.1 --edition2021 ran successfully on the three owned Rust implementation/test files. No Cargo/build/tests/measurements ran. Product file counts: read533, navigation182, mod30 physical lines at this draft capture; final source checks/counts belong to root's commit. Production LOC not estimated; root counts parent versus exact final staged/committed scope.

Owning check suggestion: cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test navigation_cursor --test file_read --test mapping_cache; storage real-provider mapping_cache and affected edit checks; full owning examples/fmt/Clippy/boundary/selftests under the final coherent handoff. Root records exact exits and remaining source/provider gaps. There is no speed observation or admission result here.
