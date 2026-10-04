# Second-chance positive locator retention

The retained stride 10 key-order candidate increases locator calls by 60.29%.
A count-only public-reader churn fixture reproduces the failure: after filling
4,096 rows, alternate two low-key objects after pressure. Fixed-key selection
issues 34 post-fill singleton locate requests / evictions. The second-chance
policy issues 3 / 3, with 4,099 selection probes and 4,096 second chances. Exact
canonical outputs match. This is cause evidence, not a performance sample.

The same existing map holds 4,096 rows. Each row gets one reference flag; one
cursor advances through existing keys, protects current requested hits and clears
a reference flag once before selecting an unreferenced victim. At most two
map cycles per attempted removal; no extra index or larger transaction. Cache
lifetime, invalidation, negative-demand reset, oversized-frontier guard, row/
descriptor checks and all encoded/decoded/output limits stay unchanged.

Bookkeeping is charged: 8 bytes per live row plus 40 fixed bytes on arm64,
32,808 field bytes at capacity. This excludes BTree spare-node slots and heap
allocator overhead. The matching public-reader external allocator diagnostic
measures retained fill deltas 2,393,849 -> 2,453,777 bytes, +59,928 bytes for
its entire reader/decode/cache/mock-port scope. It is not a cache-only limit,
RSS/phase claim or universal total-heap bound. Three new u64 work fields add
24 bytes per Diagnostics value and count probes/second chances/live-field peak.

[Cause custody](cause.json) preserves all diagnostics, including an initial
wrong-library link and a package-only variant excluded from the heap comparison.
The before/after3 public-library profile/dependency fingerprints match; library,
probe and executable hashes identify the actual inputs. Probe imports external
test support and the normal public library; no private source is recompiled.
[Checks](checks.json) record 523 unique passing Core tests, locked all-target
Clippy, fmt and a 456-file boundary pass with explicit current-source proof reuse.
The unchanged guard's 23 selftests are reused. A new matched stride 10 pair
qualifies actual history counts before any promotion to stride 3 or 1.
