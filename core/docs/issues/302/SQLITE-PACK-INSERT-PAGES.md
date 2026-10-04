# Bounded set-based immutable pack insertion

> **Status:** Research; informative and not a product contract.

Based on `284bf5dda`. The qualified revision4 cause diagnostic counted 95 pack
INSERTs for 95 bodies /20,125,894 submitted body bytes. Locator and signature rows
were already set-based. Pack insertion still repeated prepare/bind/step/drop once
per immutable body. This treatment reduces that count; it does not claim to
remove WAL/main payload write amplification or sync cost.

The same publication input order is divided into pages bounded by the actual
SQLite variable and SQL-length limits and at most512 rows. Scalar/binding vectors
borrow digest/body slices from the existing immutable inputs; no payload clone.
All pages share the unchanged one-attempt publication transaction with locators,
value catalogue, signatures and policy. Known failures roll back the whole unit,
unknown outcomes retain quarantine; there is no retry/fallback/dependency/schema
change. Existing canonical/physical/row bounds, format/profile/worker/cache/
checkpoint settings remain. Pack counters still count actual packs, not pages.

Count/atomicity proof publishes513 small valid packs: BEGIN+two INSERT pages+
COMMIT (four statements), one write acknowledgement,513 sealed inserts and exact
body readback. A later publication inserts512 new bodies in its first page and
conflicts with an existing body in its second page; all512 earlier inserts roll
back, the existing body is unchanged, and the public absent-read reports Missing.
Initial fixture compilation used the wrong public output-vector signature and
was corrected; the first executable rollback assertion expected an empty result
instead of Missing and was corrected. The subsequent ownership review added the aggregate binding-byte bound described
below; final covering commands ran once at that final algorithm. Existing9publication tests,2ordinal and9transaction/cache/
closure tests plus two new page tests PASS (22 unique checks). Persistence
all-target Clippy, final focused fixture Clippy, Core fmt --all, boundary440files/
23guard self-tests and diff checks PASS. No aggregate/CI gate or broad workspace
claim. Product measurement binaries remain release/locked.

Prospective next selection: exactly one matched ordinary
`phase7-sqlite-init-1000-v2` child per arm at this committed source/harness identity.
Use runner.py run --case phase7-sqlite-init-1000-v2 --arm baseline --baseline-root
 target/phase7-baseline/layerfs --out <fresh output>, followed by --arm candidate.
Fresh outputs use issue302-sqlite-packpages1000-{baseline,candidate}-treatment1.
Both retain the15s complete command and9.5s independent proof, native cold-content
invalidation+whole-input zero-residency check, prepared fixture reuse outside the
timer, four Init constructors/envconstruction1, fresh SQLite creation/import/
checkpoint/allocation-release/close operation boundary. Keep actual Phase4.5
MEMORY/OFF and candidate WAL/FULL profiles declared. Time gate is
10*candidate<=11*reference and final allocation<=reference. Record every verdict
and receipt; no repeated unchanged arm. Prior FAILs remain authoritative for
those identities. Other3Init and3history selections remain required and NOT_RUN
at this identity; history reference budget limitation remains unresolved.

No speed benefit or all-seven success is claimed before the new receipts.
Production LOC comparison must cover exact parent/final staged/committed trees
with root/core/active/reference and migration subtotals before commit.

The safe rusqlite BLOB binding uses SQLITE_TRANSIENT, so Rust borrowing alone
does not bound SQLite-owned copies. The aggregate per-page binding charge is
also limited to the preceding single ordinary-pack statement's256KiB+56B. An
existing oversized singleton is isolated, with no companion bodies. Large packs
therefore keep separate INSERTs; batching targets small packs without increasing
payload binding ownership. A three192KiB-pack count/readback check confirms three
INSERTs within one transaction. This deliberately limits potential speed savings.


## Matched ordinary Init1000 at c55bfa2f3

One arm each, frozen source/tree/harness/fixture identities matched and clean.
Reference126,033,958ns /candidate229,371,791ns, ratio1.819920557, time FAIL:
10*candidate=2,293,717,910 >11*reference=1,386,373,538. Candidate Init-only
204,686,125ns; bootstrap22,935,625ns, checkpoint1,442,083ns, close274,458ns
are recorded separately. Reference Init119,510,209ns/bootstrap6,436,500ns/
close86,666ns. The compared product clock includes these lifecycle terms.

Roots match; separate proof, source-content cold0, cleanup and budgets PASS.
Complete commands248,184,916 /1,355,733,500ns (<15s), independent proofs
56,384,417 /594,714,917ns (<9.5s). Prepared source was reused outside timers,
fresh SQLite creation remained inside the product clock; release/locked
worktree-local sealed executables and exact dependency/profile flags recorded.
Final allocation23,101,440 /20,557,824B: candidate<=reference, storage PASS.
No phase-only memory claim from lifetime RSS or partial counters.

Candidate normal diagnostics:300statements,203,513VMsteps,32transactions,
17write commits,10publications,4reservations (3ordinal),95bodies/20,125,908B,
zero C2 pack/payload reads. COMMIT116,982,126ns is inclusive; its next phase
116,921,335ns still dominates. FULL5,976,273ns/group1,888,577ns; overlapping
stage totals are not added. The prior ordinary source had303statements, but
separate windows and worker-arrival pack variation prevent a causal time delta.
Small-pack page batching has a deterministic count benefit and retains existing
ownership, but this workload's mostly-large packs do not yield a competitive
speed result. It does not remedy the previously observed write/sync term.

[Comparison receipt](checks/sqlite-packpages1000-comparison-treatment1/comparison.json),
[reference receipt](checks/sqlite-packpages1000-baseline-treatment1/receipt.json),
[candidate receipt](checks/sqlite-packpages1000-candidate-treatment1/receipt.json)
retain raw identities, arithmetic, timings and every failure. Reproduction:
`python3 core/benchmark/fs-bench-pro/runner.py run --case phase7-sqlite-init-1000-v2 --arm baseline --baseline-root target/phase7-baseline/layerfs --out benchmark-results/fs-bench-pro/issue302-sqlite-packpages1000-baseline-treatment1`,
then the same case `--arm candidate --out benchmark-results/fs-bench-pro/issue302-sqlite-packpages1000-candidate-treatment1`.
These existing output paths are receipts, not instructions to overwrite/rerun.

Other Init100/10000/100000 and history stride10/3/1 are NOT_RUN at this identity.
The latest Init1000 is FAIL. All seven remain required; no terminal success,
unchanged-arm retry, durability relaxation or budget extension. Next cause work
must attribute actual publication batch byte/row occupancy before selecting a
safe publication-boundary change; existing aggregate VFS cost alone does not
prove which boundaries can be removed within the unchanged limits.

Implementation Production LOC137829->137867(+38), rootreference65417,
core72412->72450,active28368->28406/inactive44044; migrationold191/
new7958->7996/rest64263. Exact committed tree/parent confirmed. This evidence-only
follow-up retains137867/delta0, subject to exact staged/committed confirmation.
