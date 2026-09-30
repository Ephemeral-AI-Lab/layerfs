# Issue287 independent canonical reference

> **Status: Current planning checklist; no release candidate exists.**
> R0 proof source, 2026-09-30; audited product parent
> `7edddbdb8e8512627aed0ed42533ef099d802384`.

The [canonical contract](../R0-CANONICAL-CONTRACT.md) freezes the prospective
v2 grammar. contract.json retains exact descriptor/table bytes, source hashes
and separate pre-existing v1 pins. reference.py and verify.py are development
tools outside product source; no LayerFS code or candidate output provides their
expected roots. Hashing alone uses the existing published blake3 dependency.

From repository root:

```sh
cargo +1.85.1 build --release --locked --offline --manifest-path core/docs/issues/287/oracle/hash/Cargo.toml
python3 core/docs/issues/287/oracle/verify.py --check
```

The helper's target is inside this owned worktree at oracle/hash/target. Compile
once and reuse it; do not use a foreign target. The repository-root ARMv8 AEAD
configuration remains a recorded compilation input. These are finite reference
checks, not a benchmark or product-provider proof.

The frozen ledger is vectors.json. `--check` regenerates expected objects from
the reference recipes, decodes them, checks independent final byte ledgers and
complete finite namespace correspondence, preserves v1 fixture pins, exercises
rejection vectors, then compares every ledger field. `--write PATH` creates a
new ledger and refuses an existing path; it cannot overwrite vectors.json.
After publication, new cases/method changes require a new prospective ledger
version with their own source/method pin. Never reseal expectations from candidate
output. The deliberately ineligible earlier draft is retained under evidence/.

The executed scope is8 file vectors (empty/small/cutoff, exact v1 parent, v1
no-op preservation and a successive v2 parent),6 finite namespace roots,9 parent
fill/split/collapse transitions and10 negative cases. Wider mapping partitions,
incremental capability authority and candidate comparisons remain R3 gates.
The [validation record](evidence/VALIDATION.md) states exact results and gaps.
