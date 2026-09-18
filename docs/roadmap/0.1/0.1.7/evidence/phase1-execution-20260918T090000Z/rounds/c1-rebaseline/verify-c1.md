# verify-c1 — C1 counter correctness

> **author-verified.** Written by the same agent that wrote C1's product change
> (the Phase 1 driver is single-agent by owner instruction, so no independent
> reviewer exists). The evidence is the reproducibility of the commands below on
> the named trees, not this file's word. Trees: `4a86107fc` (C1's parent) and
> `7447f87d9` (C1), both exported with `git archive` into `/tmp/verify-c1/`.
> Round: [`receipt.md`](receipt.md) (with the §9 correction this file's check 2b
> produced).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 7447f87d9 \| tar -x -C /tmp/verify-c1/commit` | 0 | clean tree at C1 |
| R2 | `cd /tmp/verify-c1/commit && CARGO_TARGET_DIR=/tmp/verify-c1/target-commit cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_bounds a_grouped_demand` | 0 | `test result: ok. 1 passed; 0 failed; … 11 filtered out` |
| R3 | the seven sealed-oracle targets on the same clean tree | 0 ×7 | `fixture_seal` 2, `filesystem_reference` 2, `edit_reference` 2, `object_identity` 11, `filesystem_codec` 9, `filesystem_updates` 6, `filesystem_profile` 2 — **34 passed, 0 failed** |
| R4 | `order`/`c2` rows on the same clean tree with a client built from that tree (`order-rows-correction-20260918/after/`) | 0 ×5 | `dir_pages_read 17`, `ino_pages_read 81`, `read_waves 7`; `c2` unchanged |

R4 is the reproduction that mattered: the round's own `after/` arm had run a
**stale** probe client (correction §9), so the after-counter for the `order` rows
was re-established from a clean archive with a client built from that archive.

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes.
`git archive 4a86107fc` → `/tmp/verify-c1/parent`, the C1 test file copied in
unchanged, the **product left at the parent's state**:

```
cd /tmp/verify-c1/parent && CARGO_TARGET_DIR=/tmp/verify-c1/target-parent \
  cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml \
  -p layerfs-content --test filesystem_bounds a_grouped_demand      # exit 101
test a_grouped_demand_is_one_wave_and_every_page_it_decoded ... FAILED
panicked at crates/layerfs-content/tests/filesystem_bounds.rs:1183:5:
assertion `left == right` failed: 5 point reads and 1 grouped demand are 6 waves, not 9
  left: 10
 right: 6
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out
```

The failure is the counter defect itself, at the exact assertion the test pins;
the same file passes on C1's tree (R2).

**2b — did the counter move in the predicted direction, and did other counters
move unexplained?** Direction yes; the *round's own* `order` rows did not move at
all, and that was the finding. C1's two statements predict `read_waves` **down**
(one grouped demand is one wave) and `pages_read` **up** (batched decodes were
invisible). Measured with clients built from each tree (§9.2):

| counter | before (`4a86107fc`) | after (`7447f87d9`) |
| --- | ---: | ---: |
| `order.default` / `order.forced64` `read_waves` | 11 | **7** |
| `order.default` / `order.forced64` `directories.pages_read` | 2 | **17** |
| `order.default` / `order.forced64` `inodes.pages_read` | 1 | **81** |
| `c1.directory-update`/`c1.subtree-remove` `objects.read_waves` | 4 | **3** |
| `c1.directory-update`/`c1.subtree-remove` `inodes.pages_read` | 1 | **5** |

Every other counter on those rows is identical between the corrected arms
(enumerated in receipt §9.2/§9.3), and the `c2` rows — the one family C1's
statements do not reach — are unchanged in every work counter when measured with
correctly built clients. No counter moved unexplained.

**2c — is the parity set green and is the test diff limited to the new test?**
Yes: 34/34 green on the clean C1 tree (R3), and
`git diff --stat 4a86107fc..7447f87d9 -- '*tests*'` is exactly one entry,
`layerfs-content/tests/filesystem_bounds.rs | 158 +++++` (a new test appended to
an existing suite). No pinned test was edited, which is what the plan requires:
C1's pre-authorization is a **re-baseline**, not a re-pin.

**2d — is the commit single-variable?** Product files touched:
`filesystem/objects.rs` −1, `filesystem/sorted/page.rs` +5, plus
`core/docs/architecture/10-counters.md` (+7/−2, the same-commit architecture-doc
update) and the round's own evidence directory. Nothing else rides along; the
third counter defect found while building the test is reported on #178 rather
than fixed here.

**2e — error paths.** `c1.empty` (the empty case) reads 0 objects / 0 waves and
emits its root unchanged; `c1.attributes` (the route that charges a second
boundary) reads 0/0 and emits 5 objects; the `batch_children` point-read fallback
is the branch where the grouped fetch returns `Ok((1, Vec::new(), None))` — it
stays uncounted because its caller point-reads and charges, and the fixture that
exercises the fallback (`filesystem_sorted`, 6 tests) is green. A malformed-input
probe is not applicable to C1: neither statement adds validation or a refusal
path, and no error identity changed (the `edits.*` rows keep their roots).

**2f — is elapsed quoted as a gate anywhere?** No. Every elapsed figure in the
receipt, the commit message and this file is labelled diagnostic; the gates are
`read_waves`, `pages_read`, `entries_examined` and the root identities.

## 3. UNVERIFIED

* **`SortedWork.pages_read` is still not a complete total.** C1 makes batched
  decodes visible, but a second engine instantiated inside
  `Engine::apply_root` returns its work to nobody (receipt §8). No receipt may
  quote an absolute `pages_read` total as complete until the owner rules on it.
* **The `order`/`c2` rows of the round's own `after/` arm remain as collected**
  (stale client) and are **superseded**, not deleted, by
  `order-rows-correction-20260918/`. I did not re-run that arm in place: the
  driver refuses to reuse an output path, and rewriting it would violate the
  append-only rule.
* **Phase 0's D25/D26 values were not re-measured on the Phase 0 tree.** They are
  cited as published for that tree; the correction re-measures the *C1* pair.
* **The `--offline` builds in `/tmp` were not run under the measurement lock**
  (no other resource-sensitive work was running, but the lock is not
  instrumented); the round's gate samples in `after/` were collected with the
  lock held and nothing else running.
