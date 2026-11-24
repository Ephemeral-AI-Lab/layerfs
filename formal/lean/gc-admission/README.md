# GC admission / liveness — Lean 4 model (shipped scope)

A Level-2, Mathlib-free Lean 4 model of the **shipped** LayerFS GC
admission/liveness scope: ordered identity (`machine_incarnation`,
`sandbox_generation`) compared by **milliseconds only**, the Sandbox
tombstone, the `gc_intent` done-row / audit path
(`release`/`preview`/`settle`, `Copy::Done`), the compact barrier, the
operator `preview`/`settle`/`release` escape, and the authority wiring.
`list` and `audit` are read-only and are not state transitions; `doctor` is a
product check the model does not describe (see Limits).

The model is deliberately narrower than the superset: lease/fence, the
cross-home `PlaceView` merge and `archive --prune` are deferred under
Ephemeral-AI-Lab/layerfs#775 and are **not** part of this formalization.  It is
a *design model*: the Rust/DB transaction, the filesystem home protocol and
concurrency are refinements, not proved here.

## Build

```sh
export ELAN_HOME=/path/to/elan
export PATH="$ELAN_HOME/bin:$PATH"
lake build
```

`lean-toolchain` pins `leanprover/lean4:v4.36.0`.  There is no Mathlib, no
`lake exe cache get`, and no network step after the toolchain install.

## Policy

No `sorry`, no `admit`, no `axiom`, no `native_decide`.  `GcAdmission/Axioms.lean`
prints the axiom receipts for the main theorem, the §9.4 corollaries, the
Inv3/Inv4 witness preservation and the base/force-remove lemmas; the allowed
axioms are only `propext`, `Classical.choice` and `Quot.sound`.
`tools/check_lean_axioms.py` requires every listed receipt (23) and fails on
any other axiom, `sorryAx` or a missing receipt.  `#guard`s are elaborated by
`lake build`, so a false example fails the build.

## Module map

| Module | Contents |
| --- | --- |
| `GcAdmission/State.lean` | `Ordered` (raw token included)/`PlaceOrder`/`PlaceValue`/`comparePlace`/`parseToken`/`parseMs`; `Liveness`, `RecordedPlace`, `PlaceRecord`, `Witness`, `Intent`, `Machine`/`Home`, `Store`; the single readable-home lookups (`homeFor`, `homeIncarnation`, `tombstone`); `placeOf`/`verdict`/`witnessFor`, `ownerVerdict` with the Store kind |
| `GcAdmission/Transitions.lean` | `Op` (13 transitions), executable guards, `step`, `settlePlace`, `reconcileDead`, `closeHome`, the release/mark-uncertain end |
| `GcAdmission/Invariants.lean` | `HoldsBranch`, `MountedOn`, `LiveHeldOn`, `OrderedDeath`, `OrderedRule`, `CompactBarrier`, the `Inv1`/`Inv2`/`Inv7`/`Inv8_no_automatic_release` invariants and `Inv`; hypotheses H2/H3/H4/H7 (H0, H2d, H-done-op-id and H-no-prune are the README's explicit assumptions) |
| `GcAdmission/Proofs.lean` | pure ordered lemmas, per-transition preservation, `step_preserves`, the §9.4 corollaries |
| `GcAdmission/Examples.lean` | executable `#guard` examples |
| `GcAdmission/Axioms.lean` | `#print axioms` receipts |

Dependency order: `State → Transitions → Invariants → Proofs → Examples →
GcAdmission`.

## Model notes

* **Millisecond-only ordering (review round 1) and raw identity (round 2).**
  `Ordered.lt` is `a.ms < b.ms`; the nonce is identity material, not tie-break
  evidence.  `compareOrdered` compares the **raw** token strings first
  (`.equal` only when the raw strings are equal), then the label guard
  (admission never orders across kinds), then the milliseconds.  A
  same-label, same-millisecond pair with different raw/nonce is **`.unordered`**,
  never an order, and two spellings of the same `(label, ms, nonce)` -- a
  different zero padding, say -- are `.unordered`, not `.equal`.  This is the
  product's `compare_place` shape (`recorded == current` first, then
  milliseconds only); `Ordered` carries the raw text so the model can state it.
  A same-ms worker token is therefore `Uncertain`, never `Dead`; a same-ms
  retired generation is `Uncertain` only where `compareSandbox` reaches the
  unordered tombstone branch.
* **Token parsing (round 2).** `parseToken` accepts only the real
  `p1:<label>:<ms>:<nonce>` shape: the milliseconds field must be non-empty
  ASCII decimal digits and `< 2^64` (the product parses a `u64`), so `_`, signs,
  whitespace, an empty field and an overflow are all `none` (absent evidence,
  never death).  `parseMs` is deliberately stricter than `u64::from_str`, which
  also accepts a leading `+`; no minted token carries one.  The nonce must be
  non-empty; the raw text is kept in `Ordered.raw`.
* **Lists instead of `Finset`.** Lean 4 core/`Std` does not ship `Finset` and
  the task forbids Mathlib, so the enumerated collections are `List`s with
  deduplicated updates.  §7.6 (fence/lease) is deferred with the lease
  feature.
* **One readable home.** `State.home : Option Home` is the single machine home
  admission can read (its ordered incarnation, its current Sandbox records and
  its `sandbox-retired` tombstones).  There is no cross-home `PlaceView` merge:
  a missing home is `none` (no proof, never death).
* **Authority-gated place lookup (review round 1).** `placeOf` reads the place
  path only when the intent recorded one of the two `gc_intent` authority
  columns (`machine_incarnation` or `sandbox_generation`,
  `Intent.hasPlaceEvidenceB`).  A **place-null** row returns `none` and goes to
  the owner evidence -- naming a machine alone is not authority.  When the
  home is readable, each field falls back to the intent's recorded value
  independently, like `PlaceView::record`
  (`machine := h.incarnation.orElse i.recorded.machine`, sandbox likewise,
  `retired` from the home).  The machine *name* is not an authority column.
* **Own-place fallback (F2).** With no readable home (or no machine name on a
  placed intent), `placeOf` falls back to the intent's own recorded
  machine/Sandbox generation.  A placed row whose home this admission cannot
  read is therefore Live, not owner-killed -- deliberately more conservative
  than the p4 code for that one case (plan §14).  `place_fallback_conservative`
  states the fallback-only consequence.
* **The owner fallback reads the Store kind (round 2).** `ownerVerdict` takes
  `sharedStore` and `State` carries it (default `false`, a local SQLite Store).
  A place-null `oldBoot` row is `Dead` only on a local SQLite Store; on a
  shared PostgreSQL Store it is `Live`, because another machine on that boot
  may still run the owner -- exactly `OwnerView::owner_dead`'s `!shared_store`
  rule (`engine/.../history/admission.rs`).  A `Witness` snapshots the kind it
  was read with (`Witness.sharedStore`), so `WitnessConsistent` stays a
  self-contained row property; `reconcileDead` uses the same rule.
* **Total intent map and the support invariant (review round 1).**
  `Store.intents` is the map value (total), not a filtered view;
  `Store.entries` is the finite key list and `EntriesSound` is the real
  invariant `(∃ i, intents op = some i) ↔ op ∈ entries`.  `Store.removeIntent`,
  `Store.removeWhere` and `Store.updateWhere` update the map as well as the
  support, so an unlisted live row is impossible in an `Inv` state.  The DB
  read that builds `entries` must return **every map row** -- retained done
  rows included, which is what `EntriesSound` asserts of a loaded Store -- so
  H4 is stated over the whole map, not only the non-done rows (an
  implementation obligation; `EntriesSound` proves the model's support is
  exact, not that the database read was complete).
* **Inv7 is monotonicity over the one intent map.** `doneRows` is derived
  (`Store.intents op = some i` with `i.state = done`), but `Inv7` does not
  compare it to an audit projection: it says no transition drops a retained
  done row (`Inv7_no_done_removed`) and that compact copies every one
  (`Inv7_compact_copies`).  There is no `exported` set and no archive
  transition.  **Dependency:** the model's Inv7 is the design statement; the
  shipped SQLite compact keeps a done row only once the open SQLite done-row
  fix (product PR **#782**) is merged.  The formal result assumes that fix, it
  does not prove it.
* **Compact barrier is defined once.** `CompactBarrier` is Inv1's third
  conjunct; there is no separate `Inv5` field.  `compact_barrier` is the
  guard-soundness theorem for the compact transition, whose SQL copy is
  `Copy::Done` (`op_id = current op OR state = 'done'`) and whose own-intent
  exclusion (`op_id <> $1`) is an abstraction of the model's derived audit set
  (the caller's transient row is not a modelled holder).
* **Force remove / close end the Sandbox's mount intents.**
  `.sandboxForceRemove` is not `.sandboxClose`.  Both end the non-done rows
  naming the Sandbox around the tombstone write and the record deletion;
  `refused = false` is `release_or_mark`'s release (`gc_release` deletes the
  row), `refused = true` is the Store refusing that delete, so the row is
  marked uncertain (`gc_mark_uncertain`) and stays `Unresolved`, never
  silently dead.  Done rows never match the end, so `Copy::Done` survives;
  `force_remove_ends_mounts` records the effect (done, or uncertain with
  `uncertain = true`).
* **`sandboxClose` (review round 1).** A non-forced close refuses
  `MountsActive` (`mounted = true`) while a Workspace is mounted; force is the
  separate `sandboxForceRemove`.  On a successful close it writes the durable
  tombstone, deletes the record, and runs `mount_intents::end`: the Sandbox's
  leftover mount intents are released, or marked uncertain when the Store
  refuses (`refused`).  The tombstone generation is derived from the home
  record, **not** a caller argument; a record without a generation closes with
  no tombstone.  The old `.legacy raw`-any-generation close path is gone: a
  legacy generation is written to the tombstone as-is (the product writes the
  record's string), and no caller-supplied generation is accepted.
* **`sandboxUnmount` (review round 1).** The daemon's own unmount is
  `release_or_mark`: `refused = false` deletes the row; `refused = true` marks
  it uncertain and records the admission witness instead of deleting it.
* **`branchRemove` (review round 1).** The guard requires the Branch to exist
  (`branches b = some .live`) as well as every dead-certain holder settled; an
  absent or already-removed Branch is not a successful removal.
* **N1 -- release deletes an old uncertain row too.**  `endsOnSandboxB` includes
  a row already marked uncertain by an earlier refused release, so a later
  successful release deletes it as well.  The audit then relies on the retained
  done rows, not on the deleted uncertain row; the `#guard` net in
  `Examples.lean` covers this case.
* **N2/N3 -- the refused-release witness is post-tombstone.**  A refused release
  runs on the state after the tombstone write, so `markSandboxMountsUncertain`
  records `witnessFor` of that state: a row recorded at the closed generation
  can get `witness = dead` while `state = uncertain`.  That is intentional:
  no `.live` row remains; uncertain rows are retained, report `Unresolved`, and
  block compact.  The row's earlier witness is not preserved.
* **No lease/fence in the verdict.** `placeVerdict` is machine incarnation ->
  Sandbox generation/retired tombstone -> Live.  `comparePlace` refuses to
  order tokens of different labels (`.unordered`, no proof), matching H2's side
  condition; the Rust comparator ignores the label but admission never compares
  across kinds.

## Operator-transition abstractions

* **Preview evidence.** The product's preview mints a **fresh random op id**
  and writes `evidence = "action=preview;operator=<label>"` with **no `op=`**.
  The model's `previewIntent` uses the caller's `row` as that fresh id and
  `previewEvidence` writes exactly that text; the caller-supplied row is the
  abstraction of the random mint.
* **Settle scope.** The product's `settle`/`release` act on one explicit op
  that **must be a Mount holder of the Branch** (`gc_intents` filtered to
  `GcScopeKind::Branch` + `GcIntentKind::Mount`), and `--legacy` restricts the
  action to rows without owner or place evidence.  The model's
  `operatorSettle` acts on any non-done intent and has no `--legacy` flag; this
  is a documented abstraction, not per-holder fidelity.  Successful release is
  retained as a `done` row with `action=release;operator=<label>;op=<id>` and
  cleared owner columns, which is the model's `operatorRelease`.
* **`operatorPreview` freshness** is `intents row = none`; the audit lookup is
  derived from the map and adds no second condition.

* **Inv2's fields are abstractions.** The intent's `witness` is the model's
  stored admission snapshot; the DB keeps the row's authority/evidence
  columns, not a witness object.  `action` is rendered into the retained
  row's `evidence` (`action=...;operator=...;op=...`), and `settled` records
  the explicit operator step; neither is a separate `gc_intent` column.
  Inv2 therefore constrains the model's retained done/uncertain rows, not a
  second table.  Inv3/Inv4 are constructor corollaries of
  `WitnessConsistent` over a witness's own fields, not independent state
  invariants.

## Invariant / transition matrix

`✔ guard` = the transition may change the invariant's subject and needs the
guard; `–` = unchanged by construction.

`Inv` contains Inv1 (whose third conjunct is `CompactBarrier`), Inv2, Inv7,
Inv8 and `entriesSound`.  There is no `inv5` field.  Inv3 and Inv4 are not
fields of `Inv`: they constrain a stored witness's own fields, which
`WitnessConsistent` already fixes (`Inv3_preserves` / `Inv4_preserves` are the
witness-level lemmas, now in the 23 receipts).  §7.6 (fence) is not a column
because lease/fence is deferred.  The columns keep the §7 numbering; column 5
is Inv1's third conjunct.

| Transition | 1 | 2 | 3 | 4 | 5 | 7 | 8 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `sandboxCreate` | – | – | – | – | – | – | – |
| `sandboxMount` | ✔ guard | – | ✔ | ✔ | ✔ | – | – |
| `sandboxUnmount` | – | ✔ refused | ✔ refused | ✔ refused | – | – | – |
| `sandboxClose` | – | ✔ end | ✔ refused | ✔ refused | – | – | – |
| `sandboxForceRemove` | – | ✔ end | ✔ refused | ✔ refused | – | – | – |
| `machineStart` | – | – | – | – | – | – | – |
| `machineRestore` | – | – | – | – | – | – | – |
| `admissionRecheck` | – | ✔ | ✔ | ✔ | – | – | – |
| `branchRemove` | ✔ guard | – | – | – | – | – | – |
| `storeCompact` | ✔ guard | – | – | – | ✔ | ✔ | – |
| `operatorPreview` | – | – | ✔ | ✔ | – | ✔ | ✔ |
| `operatorSettle` | – | ✔ | ✔ | ✔ | – | – | – |
| `operatorRelease` | – | ✔ | – | – | – | ✔ | ✔ |

`sandboxForceRemove` carries a `refused : Bool` beside the ordered-generation
guard; `sandboxUnmount` and `sandboxClose` carry the same `release_or_mark`
choice, and `sandboxClose` carries a `mounted : Bool` for the `MountsActive`
refusal.  The retained done rows are untouched in every branch.

Columns 3–4 follow `WitnessGood_step`: `sandboxUnmount`, `sandboxClose` and
`sandboxForceRemove` rewrite the stored witness (`witnessFor` of the
post-tombstone state) only on the **refused** branch; the release branch
deletes the row.  `operatorRelease` **does not** rewrite the stored witness --
it keeps the admission snapshot (the model's `operatorRelease` leaves
`witness` unchanged), unlike `operatorSettle`/`admissionRecheck`, which
refresh it.  `operatorPreview` stores the audit witness (`auditWitness`).

## What each invariant says

1. A live mount is never reclaimed: a Branch-scoped non-done row sits on a
   live Branch, a removed Branch has no non-done holder, and a compacted
   Store has no non-done row (`CompactBarrier`).
2. A retained operator done row and every uncertain row carries a consistent
   admission witness.
3. (witness-level corollary, not an `Inv` field) A stored place witness that
   says Dead names an explicit ordered cause (`OrderedDeath`).
4. (witness-level corollary, not an `Inv` field) `OrderedRule` decomposes the
   witness's verdict in the `compareMachine` → `compareSandbox` order; the
   final fallback is Live.  The state-level enforcement is the guards
   (`allDeadCertainB_sound`, `reconcile_only_dead`, `operatorReleaseOk`'s
   `uncertain` requirement).
5. (Inv1's third conjunct) A live/uncertain intent blocks `store.compact`; the
   transition-level form is `compact_barrier`.
6. (deferred) fence/lease.
7. `Copy::Done` as non-removal over the single intent map: no transition drops
   a retained done row, and compact copies every done row
   (`Inv7_compact_copies`).  `done_survives_compact` is the compact-specific
   corollary.
8. No automatic release: every done row has an operator principal.

## Hypotheses (explicit assumptions, not decoration)

H2, H3 and H4 are **assumptions on the implementation**, stated so a reader
can see what is not proved; H7 is a schema well-formedness assumption.  H0 is
the real start premise of `step_preserves` (`h : Inv s`: the loaded state
already satisfies `Inv`); H2d, H-done-op-id and H-no-prune are explicit
assumptions of the shipped behaviour that the model does not prove.  Each is
marked here as an obligation rather than silently used.

* **H0 (initial state)** -- the loaded state satisfies `Inv`.  This *is* the
  `h : Inv s` premise of `step_preserves`; nothing is assumed about earlier
  history.  `empty_inv` only proves the degenerate `emptyState`: no transition
  creates a home, so a home-bearing Store cannot start from `emptyState`, and
  H0 is the real obligation that the first loaded snapshot satisfies `Inv`
  (`EntriesSound` included).
* **H2 (`H2_stop_old`)** -- the product's mint-and-stop-old premise: the new
  machine incarnation is strictly greater in **milliseconds** than the one it
  replaces (same label), and the old kernel is stopped before the new one
  serves.  Only the token-order half is expressible over the abstract State.
  A failed H2 (a clock rollback) can only lose reclamation, never cause a
  false death: an older current value is `Uncertain`.
* **H2d (mount records the current place)** -- a successful mount writes the
  home's *current* ordered incarnation/generation onto the new intent
  (`i.recorded`).  The model's `sandboxMount` takes the intent's `recorded`
  values as given; the equality with the home's current values is a product
  obligation, not a proof.  It is what makes the stored witness auditable
  after the home moves on.
* **H3 (`H3_retention`)** -- home-record **existence** retention: a
  previously recorded tombstone/incarnation is never silently dropped by a
  transition.  The value may be **replaced**: a `machineStart`/`machineRestore`
  rewrites the incarnation, and `closeHome`/`forceRemoveHome` replace the
  tombstone by name (`SandboxTombstone::write`).  The value-identity form is
  false; H3 is the "do not lose the evidence" obligation the home protocol
  owns (the ordered direction of a write is H2's job).
* **H4 (`H4_read`)** -- one bounded read returns **every** map row, retained
  done rows included: the DB-to-`entries` enumeration obligation.
  `EntriesSound` is the state-level support property the transition proofs
  maintain; H4 is the requirement that the initial read populates it
  completely.
* **H7 (`H7_owner_key_unique`)** -- owner-key uniqueness for non-done rows
  (the DDL `UNIQUE(scope_kind, scope_id, owner_boot)`; multiple NULL owners
  are permitted).  It says nothing about done-row durability.  The DDL
  constraint itself is Level 3/4.
* **H-done-op-id (no done-row rewrite)** -- no insert/upsert rewrites a
  retained done row's `op_id`.  The risk is `gc_intent_insert ON CONFLICT`;
  the model's `addIntent`/`updateIntent` and the invariant proofs assume a
  done row's key is stable.  This is a schema/write-path assumption, not a
  proof.
* **H-no-prune (no shipped pruning)** -- no shipped caller prunes or archives
  retained done rows; `prune_done` is an engine path exercised by tests only,
  and `archive --prune` is deferred (#775).  Inv7's non-removal statement is
  about the modelled transitions; a pruning caller is outside the model.

## Limits

* Level 2 proves the **design**, not the database: H1 (one atomic
  transaction), H4 (one bounded read enumerating every map row, done rows
  included), H7 (the owner-key schema) and the SQLite/PG details are
  obligations of the implementation.
* **F2 own-place fallback.** `placeOf`'s own-place fallback is a deliberate
  conservatism; the p4 code can call such a row Dead where this model says
  Live.  This is a design choice, not a proof of the Rust code.
* **F5 batch-Bool fidelity.** The force remove's `refused : Bool` covers the
  whole batch; the product's refusal is **per intent**, and a partial failure
  (some rows released, some marked uncertain) is not modelled.  The product
  also selects by Workspace and Branch (which intents a stopped Sandbox's
  Workspaces hold), not by a caller-named Sandbox; the model selects every
  non-done row naming `(machine, sandbox)`.  `force_remove_ends_mounts` is the
  batch abstraction, not per-intent fidelity.
* **Close selection.** `sandboxClose`'s `mounted` is a model parameter; the
  product derives it from the Sandbox's Workspace mount table plus the
  caller's `held` list, and `mount_intents::end` selects by the Workspaces
  recorded in the Sandbox state.  The model ends every leftover row naming the
  Sandbox, so the selection is an abstraction.
* **Code-only paths not modelled.** `reclaim`/`remove` paths that end a
  Sandbox without a tombstone, `settle_legacy` bulk, `prune_done` (no shipped
  CLI caller; the engine path is exercised by tests only) and `store.remove`
  are not transitions.  `prune_done` in particular is an engine path, not a
  shipped CLI command, and is out of this model (H-no-prune).
* **N1.** A successful release may delete a row an earlier refused release had
  marked uncertain; the audit then keeps the retained done rows only.
* **N2/N3.** The refused-release fallback records the post-tombstone witness,
  which can say `dead` while the row state is `uncertain`; uncertain rows are
  retained and still block compact.
* **Doctor is unmodeled.**  The shipped product has a `doctor` consistency
  check.  This model does not define `doctor : State → Bool` or a
  `doctor_sound` theorem; its row-level consistency expectations correspond to
  Inv2/Inv3/Inv8, but the computable check and its soundness are left as a
  Level-2/implementation obligation.
* H5/H6/H8 and the sticky cross-home merge/4096-place cap are deferred with
  the cross-home `PlaceView` work (#775).
* Concurrency, crash recovery, the JSONL audit durability and the retained
  done-row read path are operational/Level-3 concerns.
* Lease/fence, cross-home `PlaceView`, and `archive --prune` / the
  `exported` set (G2 export-before-prune) are deferred (#775); there is no
  archive transition and no fence text in this model.

## Main theorem and receipt

```lean
theorem step_preserves (op : Op) (s : State) (h : Inv s) : Inv (step op s)
```

`GcAdmission/Axioms.lean` prints one closure per declaration: the main
theorem, the §9.4 corollaries of the shipped scope, `Inv3_preserves` /
`Inv4_preserves`, `Inv7_holds` / `Inv7_preserves`, `EntriesSound_preserves`,
`empty_inv` and `force_remove_ends_mounts`.  Each line is of the form

```
'GcAdmission.step_preserves' depends on axioms: [propext, Classical.choice, Quot.sound]
```

There is no `sorryAx` and no user axiom.  `tools/check_lean_axioms.py`
requires the 23 shipped receipts, accepts exactly the three standard axioms,
and rejects `sorryAx` or a missing receipt.

## Corollary / example map

| §9.4 corollary | Statement in this model | Example |
| --- | --- | --- |
| `no_reclaim_live` | a successful `branchRemove` leaves no `LiveHeldOn` | mount on a removed Branch is refused |
| `compact_barrier` | a successful `storeCompact` leaves no `AnyLiveHeld` | a live intent blocks compact |
| `reconcile_only_dead` | a row the worker-start reconcile deletes satisfies `reconcileDead` | (covered by the guards) |
| `done_is_operator` | every done row has `doneBy = some .operator` | preview/release retain an operator done row |
| `place_fallback_conservative` | `placeOf = none` ⇒ the verdict is the owner fallback with the Store kind | owner-evidence witness |
| `conservative_no_death` | `placeOf = none` ⇒ `¬ PlaceDead` | missing place is live |
| `unordered_never_dead` | a machine `some .uncertain` never yields `Dead` | (pure authority) |
| `restore_is_uncertain` | a machine `some .uncertain` is `Uncertain` | pure authority |
| `ordered_rule` | machine → Sandbox → Live (both Sandbox cases) | ordered compare cases |
| `ordered_rule_sequential` | the full sequential rule, including the Live fallback | pure authority |
| `compareMachine_uncertain_cases` | a machine `uncertain` is exactly `recordedNewer` or `unordered` | pure authority |
| `unordered_place_never_dead` / `recorded_newer_is_uncertain` | raw `comparePlace` forms | pure authority |
| `done_survives_compact` | retained done rows survive `storeCompact` (`Copy::Done`) | compact keeps a done row |
| `preview_is_done` | preview adds a `done` row (G4) with the stored `auditWitness` | preview example |
| `force_remove_ends_mounts` | a successful force remove leaves no `.live` row naming the Sandbox: every such row is `done` or (`uncertain` and `uncertain = true`) | release / refused-release force-remove examples |

The illustrative §9.4 statements are recorded with their real premises.
`unordered_never_dead` and `restore_is_uncertain` are machine-level
(`compareMachine ... = some .uncertain`), because the plan's
`comparePlace = unordered`/`recordedNewer` forms are about one value pair; the
raw place-level forms are the separate `unordered_place_never_dead` and
`recorded_newer_is_uncertain` lemmas, and `compareMachine_uncertain_cases`
records the exact case split.  `ordered_rule` includes both Sandbox cases and
no lease.  `preview_is_done`'s freshness is `s.store.intents row = none`; there
is no `audit` conjunct (the audit is derived) and no `exported` premise.
The millisecond-only `#guard` net covers the same-ms both nonce orders
(`.unordered`, never Dead), the record-first and tombstone cases, the
authority-gated place-null row, the per-field sandbox/retired fallback, the
`oldBoot` Store-kind split, the raw-token spellings, the strict `parseToken`
rejections (sign/whitespace/`_`/overflow), the stored `auditWitness` of a
preview, and an `iBind` mount that records both authority columns.

## CI and product LOC

`.github/workflows/lean.yml` is a **report-only** job (never a required
check).  It runs `leanprover/lean-action@v1` on `formal/lean/gc-admission`,
greps for `sorry`/`admit`/`axiom`/`constant`/`native_decide`, checks the axiom
receipt with `tools/check_lean_axioms.py`, and runs the LOC guard that
`formal/` contributes **0** product LOC:

```sh
python3 -B -m unittest discover -s tools -p 'test_production_loc.py'
```

The `#guard`s in `Examples.lean` are evaluated by `lake build`, so a false
example fails the job; no separate test runner is needed.  There is no Mathlib
and no cache fetch.

`tools/check_lean_axioms.py`, `tools/test_production_loc.py` **and**
`tools/production_loc.py` are in the workflow's `paths` filter (a change to the
scanner alone re-runs the guard); the no-sorry grep also rejects
`native_decide`.  `formal/` is outside every root that
`tools/production_loc.py` counts, so it contributes **0** product LOC.

**Dependency on PR #782.**  The model's Inv7 (`Copy::Done` retention) assumes
the open SQLite done-row fix in product PR **#782**.  Until that fix is merged,
the shipped SQLite compact can drop a retained done row; the formal Inv7 is the
design statement plus that fix, not a proof of the pre-fix code.  The Lean job
is report-only and does not gate #782.

## §14 conformance record

The shipped scope is anchored to **product PR #770 rebased onto
`layerfs_v2_0_0`** and the formalization in **PR #780** (base
`layerfs_v2_0_0`); the release head is `2f9cb974438c8b7f6fbbbd9dfb831693102340e5`
at the time of writing, and the PR/release pair is the anchor rather than a
fixed hash.  The earlier `a0ef644d`/`87890d785c` SHAs described the superset
branch and are historical only (they are not the current anchor).

The confirmed shipped shapes are: the two `gc_intent` authority columns
(machine incarnation, Sandbox generation) plus the durable Sandbox tombstone;
the sequential `liveness` order machine → Sandbox generation/tombstone → Live
(no lease step), with **millisecond-only ordering** and **raw-token identity**
(`Ordered.raw`; equal only when the raw strings are equal); the derived done
set and `Copy::Done` retention (with the PR #782 SQLite fix as a dependency,
not a proof); G4 preview that retains a `done` row (fresh random op id,
`action=preview;operator=<label>`, no `op=`) with the stored `auditWitness`;
the compact barrier; and the operator `settle`/`release` escape (release only
on an explicitly uncertain row, branch-scoped Mount holder, `--legacy`
restricted).

**Shipped-model conformance (round 2).**  The tree has **13 `Op`
constructors** and **23 axiom receipts**.  `Inv` is Inv1, Inv2, Inv7,
Inv8 and `EntriesSound`; the **compact barrier is Inv1's third conjunct**
(`CompactBarrier`), not a separate `Inv5` field.  Inv3/Inv4 are
witness-level corollaries (`Inv3_preserves`/`Inv4_preserves`), not `Inv`
fields.  `Store.intents` is the total map and `EntriesSound` is its exact
support; `placeOf` is **authority-gated** by `Intent.hasPlaceEvidenceB` (a
place-null row takes the owner fallback), and the owner fallback is
Store-kind aware (`sharedStore`: `oldBoot` is Dead only on a local SQLite
Store).  The home value may be replaced (H3 existence retention), and the
model's `entries` includes retained done rows (H4 over the whole map).

A non-forced `sandboxClose` refuses `MountsActive` while a Workspace is
mounted, writes the record-derived tombstone, deletes the record, and runs
`mount_intents::end` (release, or mark uncertain); the model's `mounted` and
`refused` Bools are that abstraction.  A forced `sandbox.remove` is modelled
after `end_mount` / `release_or_mark`: the Sandbox's non-done mount intents are
released or, when the Store refuses the delete, marked uncertain; done rows
are retained, and `force_remove_ends_mounts` records that no `.live` row
remains.  A `sandboxUnmount` refused delete marks the row uncertain rather
than deleting it.  A release deletes an old uncertain row too; the audit keeps
the retained done rows.  `branchRemove` requires a live Branch.

The one deliberate conservatism is `placeOf`'s own-place fallback (plan §14:
the model may leave a row Live that the p4 code would call Dead for a placed
row whose home this admission cannot read).  `OwnerEvidence::oldBoot` is modelled
exactly: `ownerVerdict` kills a place-null other-boot row only for a local
SQLite Store (`sharedStore = false`) and keeps it Live for a shared PostgreSQL
Store.  Lease/fence, the cross-home `PlaceView` merge (with its sticky
conflict/newest rule and 4096 cap), `archive --prune` / the exported set, the
`reclaim`/`remove`-without-tombstone code paths, `settle_legacy`, `prune_done`
(tests only, no shipped CLI caller) and `store.remove` are out of this round
and are deferred or documented in Limits (#775 for the superset work).  The
SQLite done-row fix is product PR **#782**; Inv7 assumes it.
