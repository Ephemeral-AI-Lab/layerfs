# WP4-P terminal independent audit

- Date: 2026-08-21
- Starting checkpoint: `9def7af5ab2b408121b9dcbe40b6affa007626e5`
- Verdict: **PASS**
- Performance rerun: **not required and not performed**

## Independent lanes

### A — selected-only implementation and deletion

PASS. Live source contains one `SELECTED_PROFILE` with K64/F64 and DIR256K.
The losing file/directory constants, frozen identities, candidate arrays,
ordering, name resolver, 512-MiB fixture custody, multi-profile scheduler,
archival override, generic candidate CLI, and selector-based shell runner are
deleted. Runtime file and directory validation accept no capacity/page-ceiling
argument. The live selected regression scripts use the production profile ID.

The active-source deletion search returns zero matches for the removed names,
private ID domain, selector symbols, archival override, and retired CLI. The
exact search scope and historical-evidence exception are recorded in
`deletion-proof.md`.

### B — profile identity and independent selected goldens

PASS. A standalone recomputation hashes this exact 43-byte preimage:

```text
"layerfs/mapping-profile/v1\0"
|| 00000040
|| 00000040
|| 00040000
|| 00800000
```

The result is:

```text
b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1
```

The selected-golden test independently assembles Phase-1 envelopes, mapping
headers/tags, big-endian fields, file descriptors, directory pages/indexes,
delta pages/indexes, receipts, and `BLAKE3("layerfs/object\0" || bytes)` IDs.
Production encoders are comparison-side only. It regenerates the normative TSV
byte-for-byte.

The corpus freezes file boundaries at 1/64/65 and 4,096/4,097 references,
DIR256K one-entry and 897/898 greedy boundaries, delta genesis/add/remove/
replace/metadata, a 216-byte selected-profile receipt, and fourteen typed
malformed cases.

The audit initially found one missing boundary. The final test now proves,
without an 8-MiB allocation:

```text
maximum metadata entry = 4,173 bytes
2,010 entries = 8,387,745 <= 8,388,608
2,011 entries = 8,391,918 > 8,388,608
2,011 declared field -> CoreError::ObjectLimitExceeded
```

Both audit lanes rechecked the repair and returned PASS.

### C — authority, complexity, and historical evidence

PASS. CP-0006 remains unchanged and explicitly `qualification=false` /
`promotion=false`. The 216-row campaign remains historical
`NO-GO / custody_lost`. No document claims a 512-MiB or 100-GiB runtime, a
200/300-MiB/s qualification, logarithmic count-changing edits, native
materialization, a measured/global K64/F64 or DIR256K win, WP5 completion, or
overall Phase-4 completion.

The compatibility result remains:

```text
same-count edit:     path-local
count-changing edit: O(suffix), worst-case Theta(N)
DIR256K basis:       explicit unmeasured policy fallback
WP4-P:               complete
WP4:                 complete
WP5:                 eligible/pending
Phase 4:             not complete
```

## Verification

```text
layerfs-core unit tests:             44 PASS
selected golden tests:               2 PASS, 1 printer ignored
layerfs-engine owner tests:           4 PASS
selected benchmark correctness:      54 PASS
engine parity:                       14 PASS
layerfs-eval:                         5 PASS
workspace all-target/all-feature:    PASS
clippy --all-targets --all-features: PASS with -D warnings
cargo fmt --check:                   PASS
git diff --check:                    PASS
```

No profile-selection, 512-MiB, 100-GiB, or performance campaign was executed.

## Terminal disposition

The independent audits agree that the one selected compatibility profile is
K64/F64 + DIR256K with production profile ID
`b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1`.
WP4-P and WP4 may close; WP5 becomes eligible but remains unimplemented.
