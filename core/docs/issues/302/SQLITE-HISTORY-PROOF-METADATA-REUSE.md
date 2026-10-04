# Bounded metadata reuse inside one independent proof

Prospective changed verifier; no original receipt promotion. Product unchanged.

History53 reference at16ca22090 finishes performance83.107392291s within170s but
proof times out9.502048166s. The shared verifier re-requests immutable metadata
for every root and looks up all file metadata even when `Reuse.lengths` already
contains that authenticated root. The latter rows are immediately skipped.

Add a verifier-only metadata reader, empty at one proof invocation start, with
2MiB canonical bytes/512entries and LRU eviction. Only namespace metadata walk
uses it; file content sampling and all source oracle comparisons use the original
reader. Miss bytes must come from the actual authenticated source and are hashed
against their requested ObjectId before admission. Bodies above the existing
8KiB inode-node cap are not retained. No oracle bytes or expected producer data
prefill this cache. Each state still walks the complete namespace and compares
all path/kind/size and every declared sampled digest, and C5 custody plus original
owner preservation remain. File metadata lookup excludes lengths already checked
under the same immutable ID, matching the existing skip in the validation loop.

This is an additional bounded verifier work buffer, not an increase in product
body/decoded/cache/queue/worker limits. Lifetime one entire proof command; no reuse
from prior command, setup, another arm or other phase. First acquisition is paid
inside that same proof command. Separate proof9.5s and product cold contracts are
unchanged. Digest/length reuse already exists in this proof; this adds metadata
reuse without skipping any oracle comparison. Sealed reference/candidate vehicles
share the exact module; new harness identity requires new matched qualification.

External helper tests cover mixed hits/misses/order/duplicates, identity/missing
refusal, fixed bounds/eviction and fresh invocation empty state. Compilation-only
argument/constant/trait import defects corrected from diagnostics; no performance
arm repeated. Focused owning checks and diagnostic measurements are recorded below.

Prospective one native-only53reference count diagnostic on retained original
closed Store/producer/census/metadata, source/database cold equally attested,
9.5s native/60s complete. No performance rerun, qualification, or old proof
promotion; per-state memo counters and read work attribute the mechanism. If it
fits, new ordinary paired selections still need the whole combined proof.


Focused checks:4external helper tests PASS (3contracts then one new short-batch
refusal); verifier example/helper test Clippy-Dwarnings PASS after removing
unnecessary non-Drop lifetime calls;8history facade/runner/provenance tests PASS.
Package formatting applied. Runtime product source is unchanged; no full-workspace
or new product-boundary verification claim. Freeze and execute diagnostic next.
