# Durable100 queued reservation cause

The public observer runs public Project Init over a fresh opt-in acquisition
Store using Durable WAL/FULL/fullfsync. It separates creation, acquisition units,
Storage ports, finalization and close. Runtime SQL counters correlate with the
actual provider-linked SQLite EXPLAIN in `12-reservation-plans.stdout`; both
reservation statements search the singleton policy row by INTEGER PRIMARY KEY.
There is no missing index or whole-table reservation scan.

The original diagnostic has three Storage reservations: pack33, pack105 and
ordinal102. Queued encoded groups were counted as individual packs by
`finish_pack_bound`, although the bounded queue combines several groups per pack.
After the source correction, reservations are pack33 and ordinal102. Init write
commits fall18→17; reserve calls3→2, statements12→8 and VM steps162→108.
Five publication commits and eight acquisition commits remain. Including schema
creation, complete product writes fall19→18. Canonical root and publication
contents/counts remain equivalent. Pack IDs may use an earlier acknowledged tail;
their actual SQL VM paths therefore need not have identical totals.

These are count/functional diagnostics, with uncontrolled caches and enabled
timers. They did not acquire an explicit measurement run lock. The old observer
binary has only a pre-run hash and was overwritten by the changed build; no
post-run old-binary seal is available. Their clocks/RSS/rates are INELIGIBLE and
cannot replace the separately registered locked runner checkpoint. Neither the
creation commit nor the five publication commits is removed from measured work.
The full diagnostic stores remain in primary ignored benchmark-results paths
recorded by the selections, with compact stdout/stderr and source/binary identities
here. The changed binary can still be checked against its recorded hash.

All attempts are retained. `01` failed observer compilation (`Timing::enabled`
did not exist), corrected to the public `Timing::record` API in `02`. `04`
failed regression compilation at an incorrect mapping import, corrected in `05`.
`06` used a fixture that did not exhaust the reservation tail and failed its pack
count assertion; source diagnosis selected128 compressible32-KiB canonical chunks.
`08` then reproduces the intended baseline failure: reservation count1→2. The
corrected source passes that same regression inside the covering host run `13`.
No test reached a wall ceiling. Failed commands are not erased or relabeled.

Locked host `09` builds all targets without running tests, then `13` passes260
bodies across Persistence, Storage, Project and SDK, under110s.58 recorded
covering executable hashes match before/after; the subsequently added EXPLAIN
example has no test bodies and is outside that earlier58-binary seal. Host `14`
and Linux `17` Clippy pass with warnings denied. Linux `15` builds all four
packages with `--no-run`; portable `16` passes Storage/Project bodies under an
inner110s deadline plus1s kill grace and outer120s ceiling. Selecting two packages
changed Cargo feature unification, so `16` also compiled its affected variants;
that compilation is included in its recorded wall. Native global-Persistence
tests remain macOS-only. Docker's declared inner target differs from the host
wrapper's launcher environment; the command records the actual container target.

Boundary guard `18` passes652 production Rust/SQL files, followed by Python compile
`19`,40 tooling tests `20`, and formatting `21`. Existing unused-fuser-patch Cargo
warnings are retained. No dependencies, vendor source, profile, SQL schema or
harness were changed. Source contracts and prospectively registered checkpoint
are updated separately; these checks do not establish S7/S9 or speed completion.
