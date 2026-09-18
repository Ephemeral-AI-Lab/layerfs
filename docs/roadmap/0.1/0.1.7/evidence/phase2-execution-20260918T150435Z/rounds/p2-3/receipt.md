# P2-3 receipt — `locking_mode = EXCLUSIVE`: measured and declined

> **Status:** Item receipt, **no product commit**. The item was implemented in its
> conservative form in a scratch tree, measured on the same save, and put in front
> of the multi-store test surface — which refuses it. **Declined**: it moves no work
> counter and breaks three pinned cases of the store's documented reader contract.
> Tree: parent `0a593084c`; candidate `/tmp/p23-lock` = that tree plus
> [`attempt/candidate.patch`](attempt/candidate.patch).

## 1. What was tried

Ruling 4's conservative form: `PRAGMA locking_mode = EXCLUSIVE` on the **save
owner's** connection only (`MutationOwner::acquire`, after the profile is
verified), never on a read connection, with the read-back verification the existing
pragmas use. `busy_timeout = 0` is untouched.

## 2. What the measurement says

**The same save, every counter identical** (`c2.ceiling`, 8,191 rows):

| counter | parent (`p2-2/after`) | candidate |
| --- | --- | --- |
| `inserted` / `reused` / `packs_created` / `pack_appends` | 8,191 / 0 / 33 / 8,169 | identical |
| `commits` / `statements` / `presence_queries` | 31 / 72 / 0 | identical |
| pool counters, `canonical_bytes`, profile read-back | — | identical |

So the gate's own first column ("commits / statements / wall on the same save") is
satisfied in the sense that nothing moves; the lock scope is not observable through
any product counter.

**The multi-store surface refuses it.** With the pragma on the save owner's
connection, every other connection to the same store is locked out while the save is
open, and three pinned cases of the documented contract fail:

```text
test a_pooled_read_refuses_a_value_group_above_the_captured_ceiling ... FAILED
test an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement ... FAILED
test the_watermark_survives_reopen_and_still_hides_a_later_open_save ... FAILED
test result: FAILED. 6 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out
```

Log: [`attempt/failing-tests.log`](attempt/failing-tests.log) (the full
`layerfs-storage` suite in the candidate tree: 180 passed, 3 failed — all three in
`tests/visibility.rs`).

Those cases are the store's published contract: an unrelated reader sees every pack
an acknowledged save published **while another save is open**, and a read refuses a
value group above its captured ceiling rather than blocking. EXCLUSIVE locking holds
the file lock for the connection's lifetime, so a second connection cannot even read
what is already published.

## 3. Why it is declined

1. **No work counter moves** — the profile item's own gate produces identity on the
   same save, so there is nothing to demonstrate.
2. **It contradicts a documented, pinned contract.** The three failures above are
   not test friction: they are the visibility watermark's reader guarantees, and
   `evidence/phase1`/`phase2` receipts rely on them (`an_unrelated_reader...` is the
   case that defines what a read wave may see).
3. **It cannot be scoped around the contract.** EXCLUSIVE is a property of the
   *connection*, and the save owner's connection is the only one that writes; a form
   that released the lock between transactions would not be EXCLUSIVE locking, and
   one that took it on read connections is excluded by ruling 4.
4. **The parallelism premise was already ruled out**: ruling 6 confirms every Phase 2
   measurement is single-worker, so the lock-scope saving has no parallel partner to
   pay off against.

## 4. What is left on the product today

`locking_mode` stays unset (SQLite's NORMAL), with one save owner and
`busy_timeout = 0`, exactly as `11-optimization-study.md` §16.6 records. If the owner
wants the pragma anyway, the patch is in
[`attempt/candidate.patch`](attempt/candidate.patch) and applies cleanly to
`0a593084c` — but the three failing cases would have to be re-specified, and this
receipt argues they should not be.

## 5. Checks

No product source changed, so no check was run for this item; the round that carries
this receipt runs the eight checks over its evidence-only change.

## 6. UNVERIFIED

* **No wall-time A/B was taken.** The candidate's `elapsed_ns` on the same save was
  not collected as a gate (`CONTRACT.md` §2.4), and no timing claim is made either
  way.
* **`SQLITE_BUSY` was not observed directly**; the three failing cases are the
  evidence that a second connection cannot proceed, and the specific error each one
  returned is in the log rather than summarised here.
* **A two-process deployment was not exercised** (the tests are in one process), so
  whether an out-of-process reader would behave differently is not measured.
