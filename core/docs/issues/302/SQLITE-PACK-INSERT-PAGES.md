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
