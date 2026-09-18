# P2-6 receipt — one hash per resolved record

> **Status:** Item receipt. Written once, from [`after/`](after/) (product commit
> `57c4cf3bd`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p2-8/after/`](../p2-8/after/), collected on `8dc5b582e`, this item's parent
> (the docs commit `6587740b3` between them changes no product source).
> **Terminal state: landed** — the second hash is gone, the identity check is
> unchanged in strength, and the property is pinned by a guard that fails on the
> parent tree.

## 1. The item

The requested object of every read wave was hashed twice (register RT-08):

```rust
// encoding/delta/read.rs: the resolver's authentication
if ObjectId::for_bytes(&decoded) != location.object_id { Err(Integrity("dependency identity")) }
// cas/read.rs: the wave's re-check of the same bytes
if ObjectId::for_bytes(&canonical) != *id { Err(Integrity("read identity")) }
```

The resolver's hash is the one that authenticates; the wave's was a second full
pass over the same bytes. The safe half (the register's T0-11) is exactly this:
hand the verified identity back instead of recomputing it.

* `Resolver::resolve`, `resolve_dependency` and `resolve_at` return
  `(Vec<u8>, ObjectId)` — the bytes and the identity the resolution computed and
  checked against the locator;
* `cas/read.rs` compares `verified != *id`: the same check, one comparison instead
  of one hash;
* the chain path keeps **one** hash per record (now named `verified`);
* `cas/owner.rs::resolve_location` and the selector take the bytes; membership
  keeps its own re-hash by policy (parked, T2-21 — not this item's to change).

## 2. The gate — one hash per requested object, tamper detection unchanged

| Claim | How it is checked | Result |
| --- | --- | --- |
| the wave no longer hashes resolved bytes | `the_read_wave_does_not_hash_what_the_resolver_authenticated` reads `src/cas/read.rs` and rejects `for_bytes` there | **fails on the parent tree, passes here** |
| the wave still checks the identity | the same case requires `verified != *id` to be present | passes |
| tamper detection unchanged | `a_locator_whose_identity_does_not_match_its_bytes_is_refused`: save, read (control), rewrite the locator's `object_id` to another identity, read again | refused with an `Integrity` error containing "identity" — **passes on both trees**, which is the point |
| no other behaviour moved | `python3 compare_arms.py rounds/p2-8/after rounds/p2-6/after` | **37 steps, 0 differing** |
| no emitted bytes moved | `python3 pack_bytes_census.py rounds/p2-8/after rounds/p2-6/after` | 12 stores, packs/bytes/objects/pack-body sha identical |

**The magnitude is structural and the receipt says so.** No counter prices a hash:
`BLAKE3` is called inside `ObjectId::for_bytes` and nothing charges it, so
"one fewer full pass over each requested object per wave" is evidenced by the
removed call site and the fail-closed case above - not by a work counter and not
by wall time (`CONTRACT.md` §2.4). The case that fails on the parent tree is the
guard, and it is the honest form of falsification for a property with no counter.

## 3. The item's tests

`core/crates/layerfs-storage/tests/cas_reuse.rs` (extended, not replaced):

* `the_read_wave_does_not_hash_what_the_resolver_authenticated` — the fail-closed
  guard; fails on the parent tree, passes here;
* `a_locator_whose_identity_does_not_match_its_bytes_is_refused` — behavioural
  tamper detection with its own control; passes on both trees by design.

## 4. The eight checks (exit codes)

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 120 files |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | clean |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **464 passed, 0 failed** (462 + the two new) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | clean |
| 7 | `python3 tools/production_loc.py --files` | 0 | 19,368 core |
| 8 | `git diff --check` | 0 | clean |

Logs: [`after/checks/`](after/checks/). Parity 35/35 green and unchanged
(`git diff <parent>..57c4cf3bd -- '*tests*'` shows `cas_reuse.rs` only).

## 5. Production LOC

**19,368 → 19,368 (delta 0).** The returned identity replaces the caller's
recomputation line for line: one `let verified` added, one `.ok_or_else`-style
tail removed.

## 6. Architecture document (same commit)

`core/docs/architecture/05-storage.md` §6.10 (record reconstruction) gains the
paragraph: the identity check is one hash per resolved record, the resolver
returns what it authenticated, the wave compares identities, and what the change
does not touch (the check's kind and strength).

A correction to an earlier commit is also in this round's history, in its own
commit (`6587740b3`): `05-storage.md`'s `SaveOutcome` and `StoreReadCounters`
field lists were not updated by V5/V6, which carried their updates in
`10-counters.md` only. Recorded rather than folded into an item.

## 7. Clean-tree reproduction

```sh
git archive 57c4cf3bd | tar -x -C /tmp/verify-p2-6
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test cas_reuse
# running 11 tests ... test result: ok. 11 passed; 0 failed
# and on the parent tree, the guard alone:
git archive 8dc5b582e | tar -x -C /tmp/parent-p2-6   # + this round's cas_reuse.rs
cargo +1.85.1 test … --test cas_reuse the_read_wave_does_not
# test result: FAILED. 0 passed; 1 failed
```

Falsification answers: [`verify-p2-6.md`](verify-p2-6.md).
