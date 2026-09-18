# verify-p2-6 — one hash per resolved record

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `57c4cf3bd`, arm [`after/`](after/);
> before arm [`../p2-8/after/`](../p2-8/after/) on `8dc5b582e`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 57c4cf3bd \| tar -x -C /tmp/verify-p2-6` | 0 | clean P2-6 tree |
| R2 | `cargo test -p layerfs-storage --test cas_reuse` | 0 | `11 passed; 0 failed` |
| R3 | the same test file on the `8dc5b582e` archive, guard filtered | 101 | `the_read_wave_does_not_hash_what_the_resolver_authenticated` **FAILED** |
| R4 | `python3 compare_arms.py rounds/p2-8/after rounds/p2-6/after` | 0 | `steps compared: 37, differing: 0` |
| R5 | `python3 pack_bytes_census.py rounds/p2-8/after rounds/p2-6/after` | 0 | `stores compared across 2 arms: 12, identical: True` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes, the guard does:

```text
failures:
    the_read_wave_does_not_hash_what_the_resolver_authenticated
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 10 filtered out
```

The behavioural case (locator identity mismatch) passes on both trees, and that is
its purpose: it pins the detection the change must not weaken. Both answers are in
the receipt rather than only the convenient one.

**2b — did the counter move in the predicted direction and magnitude?** P2-6 has
no counter to move: a hash is not charged by any instrument. The claim is
structural (one fewer `ObjectId::for_bytes` per requested object per wave) and the
receipt states that instead of dressing wall time as evidence. What *is* measured
is that nothing else moved: 37/37 frozen steps bit-identical, 12/12 stores
byte-identical.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests. `git diff
8dc5b582e..57c4cf3bd -- '*tests*'` shows `cas_reuse.rs` only - two cases added, no
existing expectation touched.

**2d — single-variable?** `git show --stat 57c4cf3bd`: the resolver's return shape
and its one hash, the wave's comparison, two call sites taking the bytes, the two
new cases and §6.10 of the storage paper. One variable: where the requested
object's identity is computed.

**2e — the item's named risk (weakening authentication)?** The check is the same
hash against the same locator, performed one layer earlier; the wave's comparison
is an equality on the returned identity. The tamper case rewrites a locator to
claim different bytes and the read is refused, and `delta_chains.rs` /
`delta_payload.rs` / `metadata_pool.rs`'s corruption cases stay green.

**2f — is elapsed a gate anywhere here?** No, and none is quoted.

## 3. UNVERIFIED

* **No hash counter exists**, so "one hash per requested object" is enforced by a
  source guard plus the unchanged-equality structure, not by an instrument. If a
  later change reintroduces a hash through another function (not
  `ObjectId::for_bytes`), the guard would not see it.
* **The chain path still hashes every record** (that is its authentication), and
  the pooled-leaf path hashes once in its own arm; only the wave's duplicate over
  the requested object is removed. `cas/membership.rs`'s re-hash is deliberately
  left (parked T2-21) and is still paid per reuse occurrence.
* **The `Integrity` message for a rewritten locator can differ** between trees
  ("dependency identity" from the resolver rather than "read identity" from the
  wave). No test pins either string; the class is unchanged.
