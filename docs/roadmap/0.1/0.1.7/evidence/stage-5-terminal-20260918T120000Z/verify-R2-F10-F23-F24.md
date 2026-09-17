# Verification: R2-F10 / N-7, R2-F23 / N-17, R2-F24 (round-4 commit 6c00e0f53)

Verifier: independent reproduction round, 2026-09-18. Repository at
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, working tree clean.

**Identity note.** The task stated the tree was frozen at full SHA
`99743b2cf3a869b7d8897a1f16b82d742aeedc40`. Actual `git rev-parse HEAD` is
`99743b2cff2470e6634874d7ee14b9d37d0ba16e` — the two agree on the 9-character
prefix `99743b2cf` and diverge after it, so the stated full SHA appears to be a
transcription artifact. All verification below was performed on the actual HEAD
`99743b2cff…`, whose history contains the round-4 commit
`6c00e0f53172ef9ff7645e745a7f2d045461d0ac` (`git log --oneline -1 6c00e0f53`
→ `6c00e0f53 fix(core): distinguish provider failures, close the pragma set,
verify profiles`, at HEAD~3).

## Verdicts

| Row | Verdict |
| --- | --- |
| R2-F10 / N-7 (provider failure classes distinguishable from absence) | **PASS** — with two scope caveats stated below (C1, C2), neither of which breaks the tested contract |
| R2-F23 / N-17 (closed `Pragma` enum, no caller string in SQL) | **PASS** — with one literal-phrasing nuance stated below (N1) |
| R2-F24 (profile re-verification before first write) | **PASS** — with one public-surface caveat stated below (C3) |

## Commands run (all read-only on the repo; build artifacts under `core/target`; scratch under `/tmp`)

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` |
| 2 | `git status --porcelain` | 0 | empty (clean tree) |
| 3 | `git log --oneline -5`; `git log --oneline -1 6c00e0f53` | 0 | round-4 commit confirmed at HEAD~3 |
| 4 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-storage --test provider_errors --test connection_profile --locked` | 0 | `provider_errors`: 3 passed, 0 failed; `connection_profile`: 4 passed, 0 failed |
| 5 | `git show 6c00e0f53 --stat` / `-- core/crates/layerfs-storage/src/sqlite/connection.rs` | 0 | diff quoted under R2-F23 |
| 6 | ripgrep searches (ObjectMissing, MissingObject, PRAGMA, `format!`, `begin_immediate`, `verify_profile`, `Pragma::`, SQL-keyword inventory, `rusqlite` in manifests) across `core/crates` | 0 | cited inline below |
| 7 | `cargo +1.85.1 run` in `/tmp/layerfs-falsify` (standalone probe, repo untouched; see C1) | 0 | output quoted under R2-F10 |

Test run detail (command 4):

```
running 4 tests
test an_unconfigured_connection_is_refused_by_profile_verification ... ok
test a_degraded_connection_is_refused_by_profile_verification ... ok
test a_connection_with_a_busy_timeout_is_refused_by_profile_verification ... ok
test a_configured_connection_passes_profile_verification ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 3 tests
test an_object_the_store_does_not_hold_is_absence ... ok
test a_corrupt_pack_is_not_reported_as_absence ... ok
test a_record_above_the_publication_watermark_is_not_reported_as_absence ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## R2-F10 / N-7 — PASS

### The mapping function (full quote)

`core/crates/layerfs-storage/src/cas/provider.rs:28-51`:

```rust
fn provider_error(error: StorageError) -> ContentError {
    match error {
        StorageError::ObjectMissing(_) => ContentError::MissingObject,
        StorageError::Content(content) => content,
        StorageError::Integrity(what) => ContentError::ProviderFailure { what },
        StorageError::CapacityExceeded { what, .. } => ContentError::ProviderFailure { what },
        StorageError::UnsupportedPolicy { field } => ContentError::ProviderFailure { what: field },
        StorageError::VisibilityCeiling { .. } => ContentError::ProviderFailure {
            what: "record above the visibility ceiling",
        },
        StorageError::Collision(_) => ContentError::ProviderFailure {
            what: "identity collision",
        },
        StorageError::MissingDependency { .. } => ContentError::ProviderFailure {
            what: "missing dependency",
        },
        StorageError::Engine(_) => ContentError::ProviderFailure {
            what: "engine failure",
        },
        _ => ContentError::ProviderFailure {
            what: "store read failure",
        },
    }
}
```

Coverage of every `StorageError` variant (`core/crates/layerfs-storage/src/error.rs:14-80`;
15 variants): `ObjectMissing` → `MissingObject` (the only absence arm, provider.rs:30);
`Content` → passthrough (provider.rs:31); `Integrity`, `CapacityExceeded`,
`UnsupportedPolicy`, `VisibilityCeiling`, `Collision`, `MissingDependency`,
`Engine` → explicit `ProviderFailure` arms (provider.rs:32-46); the remaining
five (`OwnershipUnavailable`, `UninspectedState`, `UnknownOutcome`,
`CleanupFailed`, `Aborted`) are covered by the catch-all `_` arm
(provider.rs:47-49) → `ProviderFailure`. The mapping is exhaustive: no variant
other than `ObjectMissing` can produce `MissingObject`.

### The read path the provider drives

`StoreProvider::read_wave` calls `Store::read_batch`
(provider.rs:70-76 → store.rs:202-233), which:
- refuses an over-ceiling demand before opening a connection:
  `check_read_demand` → `StorageError::CapacityExceeded` (store.rs:259-268) →
  `ProviderFailure` (the claim's "capacity refusal");
- derives the ceiling from the publication watermark, not `MAX(pack_id)`
  (store.rs:210-213);
- runs `read::read_objects`, where locators are deliberately collected with an
  unbounded lookup so an unpublished record is an explicit visibility refusal,
  never a missing object — `core/crates/layerfs-storage/src/cas/read.rs:49-66`:

```rust
// Locators are collected above the ceiling on purpose: a record that exists
// but is not yet published must be reported as a visibility refusal, never
// mistaken for a missing object. Dependency reads inside the resolver do use
// the captured ceiling.
let locations = lookup::locations(connection, ids, i64::MAX)?;
...
if location.pack_id > ceiling {
    return Err(StorageError::VisibilityCeiling { pack_id: location.pack_id, ceiling });
}
```

- reports `ObjectMissing` only when no locator row exists for the demanded id
  (read.rs:73-77), and re-authenticates every reconstruction
  (`ObjectId::for_bytes(&canonical) != *id` → `Integrity("read identity")`,
  read.rs:91-93).

Corrupt pack bytes are an integrity failure, not absence: the pack row itself
missing is `StorageError::Integrity("pack row is missing")`
(`core/crates/layerfs-storage/src/sqlite/lookup.rs:159-162`), and pack header /
group framing is validated by `parse_header` / `group_view`
(`core/crates/layerfs-storage/src/encoding/delta/read.rs:282-297`). All map to
`ProviderFailure`.

### The trait contract documents the distinction

`core/crates/layerfs-content/src/object/access.rs:26-32`:

```rust
/// ... Absence is
/// reported as [`ContentError::MissingObject`] and nothing else: a provider
/// that holds state for the request but cannot serve it - corrupt storage, a
/// record outside this reader's visibility, a capacity refusal - reports
/// [`ContentError::ProviderFailure`] so the two classes stay distinguishable.
```

`ContentError` itself carries the same contract
(`core/crates/layerfs-content/src/error.rs:48-61`): `MissingObject` — "The
provider does not hold the requested object"; `ProviderFailure` — "The provider
holds state for the request but cannot serve it ... never the answer for an
object the provider simply does not hold".

### Tests (quoted assertions, all passing)

`core/crates/layerfs-storage/tests/provider_errors.rs`:
- corrupted pack → `ProviderFailure` (lines 79-88):

```rust
match provider.read_canonical_batch(&[root]) {
    Err(ContentError::ProviderFailure { what }) => {
        assert!(!what.is_empty(), "the refusing class is named");
    }
    Err(ContentError::MissingObject) => {
        panic!("a corrupt pack reached C1 as absence")
    }
    ...
```

- unsaved object → `MissingObject` (lines 104-111):

```rust
match provider.read_canonical_batch(&[absent]) {
    Err(ContentError::MissingObject) => {}
    Err(ContentError::ProviderFailure { .. }) => {
        panic!("absence was reported as a provider failure")
    }
    ...
```

- unpublished record → `ProviderFailure` with the named class (lines 147-156):

```rust
match provider.read_canonical_batch(&[unpublished]) {
    Err(ContentError::ProviderFailure { what }) => {
        assert_eq!(what, "record above the visibility ceiling");
    }
    Err(ContentError::MissingObject) => {
        panic!("an unpublished record reached C1 as absence")
    }
    ...
```

### Falsification attempts and result

1. **Is any error class still collapsed to `MissingObject`?** No
   `StorageError` variant other than `ObjectMissing` maps to it (the match is
   exhaustive; see above). The `Content` passthrough (provider.rs:31) cannot
   smuggle absence either: grep shows `ContentError::MissingObject` is
   constructed in `layerfs-storage/src` only at provider.rs:30 (the mapping
   itself), and in `layerfs-content/src` only at `filesystem/merge.rs:186,259`
   and `filesystem/read.rs:127,273` — C1's logical layer above the provider,
   never the storage read path.
2. **Caveat C1 (empirically reproduced): an above-watermark record reached as a
   *dependency* collapses to absence.** The resolver follows
   `base_object_id` under the read ceiling
   (`core/crates/layerfs-storage/src/encoding/delta/read.rs:150-151`;
   identically `core/crates/layerfs-storage/src/encoding/pool/read.rs:226-227`):

   ```rust
   let location = lookup::location(self.connection, base, self.ceiling)?
       .ok_or(StorageError::ObjectMissing(base))?;
   ```

   `lookup::location` filters `AND pack_id <= ?ceiling` in SQL
   (`core/crates/layerfs-storage/src/sqlite/lookup.rs:59-65`), so a base whose
   row exists above the watermark returns `Ok(None)` → `ObjectMissing` →
   `MissingObject` at the provider boundary. A standalone probe
   (`/tmp/layerfs-falsify`, a path-dependency binary; the repo was not touched)
   built a real store, saved a constructed file, then — using the same
   external-SQL tamper class the round-4 tests themselves use — inserted a
   locator row for a new object `U` into a pack above the watermark and
   retargeted the *published* root's `base_object_id` to `U`. Output (exit 0):

   ```
   baseline root read: Ok
   pre-tamper: watermark ceiling = 4, highest pack = 4
   published root with above-watermark base: MissingObject  <-- collapses to absence
   direct demand of above-watermark record U: ProviderFailure(record above the visibility ceiling)
   absent object: MissingObject (value-root absence contract)
   ```

   So the claim's "a record above the publication watermark → ProviderFailure"
   holds for *directly demanded* records (the tested case) but not for records
   reached as dependencies, which degrade to `MissingObject` under tampering.
   This state is unreachable in a product-written store: chronology requires
   the base's locator key to be strictly below the dependent's
   (delta/read.rs:155-157, pool/read.rs:231-238), and the watermark advances to
   the save's highest created pack (owner.rs:838, 917-920), so a published
   dependent's base is always published. The caveat is a tampered-state edge,
   not a product-path collapse; it does not contradict the tested contract.
3. **Caveat C2 (source-verified): corrupt locator bytes of the wrong identity
   width or unknown role code surface as passthrough `ContentError`s.**
   `ObjectId::from_bytes` returns `ContentError::InvalidIdentityLength`
   (`core/crates/layerfs-content/src/object/id.rs:32-38`, reached from
   lookup.rs:108/112) and `ObjectRole::from_code` returns
   `ContentError::InvalidRecord`
   (`core/crates/layerfs-content/src/object/output.rs:66`, reached from
   lookup.rs:113). These pass through provider.rs:31 unchanged — distinguishable
   from absence (not `MissingObject`), but not `ProviderFailure` either. The
   `Integrity`-class locator corruption (lookup.rs:98 "locator identity",
   lookup.rs:106-120 role/length/group/record decode checks, read.rs:92 "read
   identity") does map to `ProviderFailure`. The commit message itself states
   the passthrough design ("a wrapped content error passes through"), so this
   is a phrasing caveat on "every other failure class", not a contract break.

## R2-F23 / N-17 — PASS

### The closed enum and its const SQL text

`core/crates/layerfs-storage/src/sqlite/connection.rs:71-117`:

```rust
/// The integer pragmas the product reads.
///
/// The set is closed on purpose: a pragma is never addressed through a
/// caller-supplied string, so no caller input can reach SQL text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pragma {
    /// `PRAGMA application_id`: the schema's application identity.
    ApplicationId,
    /// `PRAGMA user_version`: the schema's version.
    UserVersion,
    /// `PRAGMA synchronous`: the durability level; the profile is OFF (0).
    Synchronous,
    /// `PRAGMA temp_store`: the temporary storage; the profile is MEMORY (2).
    TempStore,
    /// `PRAGMA foreign_keys`: constraint enforcement; the profile is ON (1).
    ForeignKeys,
    /// `PRAGMA busy_timeout`: the wait before a locked Store fails; 0.
    BusyTimeout,
    /// `PRAGMA page_size`: the engine's page size, read for evidence only.
    PageSize,
    /// `PRAGMA cache_size`: the engine's page cache, read for evidence only.
    CacheSize,
    /// `PRAGMA mmap_size`: the memory-mapped I/O limit, read for evidence only.
    MmapSize,
}

impl Pragma {
    /// The pragma's literal SQL text.
    const fn sql(self) -> &'static str {
        match self {
            Self::ApplicationId => "PRAGMA application_id",
            Self::UserVersion => "PRAGMA user_version",
            Self::Synchronous => "PRAGMA synchronous",
            Self::TempStore => "PRAGMA temp_store",
            Self::ForeignKeys => "PRAGMA foreign_keys",
            Self::BusyTimeout => "PRAGMA busy_timeout",
            Self::PageSize => "PRAGMA page_size",
            Self::CacheSize => "PRAGMA cache_size",
            Self::MmapSize => "PRAGMA mmap_size",
        }
    }
}

/// Reads one declared integer pragma.
pub fn pragma_i64(connection: &Connection, pragma: Pragma) -> StorageResult<i64> {
    Ok(connection.query_row(pragma.sql(), [], |row| row.get(0))?)
}
```

The integer-pragma helper takes the enum, never a string. In-crate readers:
`schema.rs:139-140` (`ApplicationId`, `UserVersion`) and `connection.rs:59-65`
(`Synchronous`, `ForeignKeys`, `BusyTimeout`); the evidence-only variants are
read by `tests/policy_capacity.rs:342-349`.

### `format!("PRAGMA {name}")` is gone

`git show 6c00e0f53 -- core/crates/layerfs-storage/src/sqlite/connection.rs`
shows the removal:

```diff
-/// Reads one integer pragma.
-pub fn pragma_i64(connection: &Connection, name: &str) -> StorageResult<i64> {
-    let sql = format!("PRAGMA {name}");
-    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
-}
```

A grep for `PRAGMA` across `core/crates` finds product-source hits only in
`sqlite/connection.rs` (the enum and fixed literal statements in
`configure`/`verify_profile`), `sqlite/schema.rs:195`, and tests.

### Remaining `format!`-built SQL — no caller string reaches SQL text

- `core/crates/layerfs-storage/src/sqlite/schema.rs:195`:
  `.prepare(&format!("PRAGMA table_info({table})"))?` — `table` is bound only
  to the compile-time constant `REQUIRED_TABLES` (schema.rs:22-58: exactly
  `store_policy`, `object_packs`, `metadata_value_groups`, `objects`) via
  `validate` (schema.rs:94-96). It is a closed const set, not caller input.
- `core/crates/layerfs-storage/src/sqlite/lookup.rs:40-45,59-65,133-137` and
  `core/crates/layerfs-storage/src/sqlite/cleanup.rs:61-65,116-120`: the only
  interpolated values are bind-placeholder ordinals —
  `format!("?{}", first + index)` / `format!("?{index}")` — joined into
  `IN (...)` lists; every identifier and value is passed as a bound parameter
  (`rusqlite::params_from_iter`), never as SQL text.
- SQL exists only in `layerfs-storage`: `rusqlite` appears only in
  `core/crates/layerfs-storage/Cargo.toml`, and no SQL statements exist in
  `layerfs-content` or `layerfs-telemetry` src. Within `layerfs-storage`, all
  `prepare`/`execute`/`query_row` SQL lives in the `sqlite` module (matches
  elsewhere in `src` are comments/identifiers only).

### Nuance N1

"the only pragma construction is the enum's const SQL text" is not literally
complete: `configure` and `verify_profile` also issue pragma statements as
fixed string literals — `connection.rs:33` `"PRAGMA journal_mode = MEMORY"`,
`:37` `"PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;"`, `:38`
`"PRAGMA foreign_keys = ON;"`, `:39`/`:55` `"PRAGMA foreign_keys"` /
`"PRAGMA journal_mode"` — plus schema.rs:195's `format!`-built
`PRAGMA table_info({table})` over the const table set. None of these
interpolates a caller string; the substantive claim (closed set, no caller
input in SQL text) holds.

## R2-F24 — PASS

### The acquire path: verify strictly before the first write

`core/crates/layerfs-storage/src/cas/owner.rs:169-181`:

```rust
pub fn acquire(
    connection: Connection,
    capacities: StorageCapacities,
    pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
) -> StorageResult<Self> {
    // The connection is caller-supplied at this seam, so its profile is
    // re-verified rather than trusted: a write must never run on a
    // connection with another journal mode, synchronous setting,
    // foreign-key enforcement or busy timeout than the declared one.
    crate::sqlite::connection::verify_profile(&connection)?;
    // Ownership first: the baseline and cursor are only meaningful when no
    // other writer can publish a pack between reading them and using them.
    write::begin_immediate(&connection)?;
```

`verify_profile` (line 178) precedes `begin_immediate` (line 181) — the first
write-attempting statement — and precedes every other use of the connection.

`core/crates/layerfs-storage/src/sqlite/connection.rs:47-69`:

```rust
pub fn verify_profile(connection: &Connection) -> StorageResult<()> {
    let journal: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(StorageError::Integrity("journal mode"));
    }
    if pragma_i64(connection, Pragma::Synchronous)? != 0 {
        return Err(StorageError::Integrity("synchronous mode"));
    }
    if pragma_i64(connection, Pragma::ForeignKeys)? != 1 {
        return Err(StorageError::Integrity("foreign key enforcement"));
    }
    if pragma_i64(connection, Pragma::BusyTimeout)? != 0 {
        return Err(StorageError::Integrity("busy timeout"));
    }
    Ok(())
}
```

All four declared profile properties are checked (MEMORY journal, synchronous
OFF, foreign keys ON, zero busy timeout); each refusal names the failed
property, and the doc (connection.rs:47-53) states verification "reads the
pragmas back and never rewrites them".

### Tests (quoted assertions, all passing)

`core/crates/layerfs-storage/tests/connection_profile.rs` — unconfigured
refused (lines 25-29):

```rust
match verify_profile(&connection) {
    Err(StorageError::Integrity(what)) => assert_eq!(what, "journal mode"),
    Err(other) => panic!("expected a journal-mode refusal, got {other}"),
    Ok(()) => panic!("an unconfigured connection passed profile verification"),
}
```

degraded refused (lines 54-58): `assert_eq!(what, "foreign key enforcement")`
after `PRAGMA foreign_keys = OFF;`; busy-waiting refused (lines 72-76):
`assert_eq!(what, "busy timeout")` after a 1 s `busy_timeout`; configured
passes (line 40): `verify_profile(&connection).expect("the declared profile
verifies")`. Note the tests exercise the public `verify_profile` directly;
`MutationOwner` is crate-private (`core/crates/layerfs-storage/src/cas/mod.rs:9`
`mod owner;`), so the acquire wiring is verified by source (above) and by the
fact that `Store::begin_save` is its only product caller.

### Other paths to a write

- `Store::begin_save` (store.rs:185-199) opens **its own** connection —
  `connection::open(&self.path, false)` at store.rs:187, which applies
  `configure` (connection.rs:23-25) — and passes it to
  `MutationOwner::acquire` (store.rs:189), which re-verifies. This path both
  configures and verifies.
- Every write-helper call site in product code runs on the owner's
  acquire-verified connection (`owner.rs:662` `insert_group`, `:809`
  `insert_object`, `:832/:835` `insert_pack`/`append_pack`, `:850/:902/:913/:921`
  commit, `:935` rollback, `:920` `advance_retained_pack_ceiling`, `:938`
  `cleanup::abandon`) or on a Store-opened+configured connection
  (`store.rs:118-119` `schema::create`). The mid-operation re-acquires
  (`owner.rs:856`, `:914`) reuse the same verified connection.
- **Caveat C3:** `pub mod sqlite` (`core/crates/layerfs-storage/src/lib.rs:35`)
  publicly exposes low-level write helpers — `sqlite::write::{begin_immediate,
  insert_pack, append_pack, insert_object}`, `sqlite::cleanup::abandon`,
  `sqlite::schema::{create, advance_retained_pack_ceiling}`,
  `sqlite::pool::insert_group` — each accepting an arbitrary `&Connection`
  with no profile verification. An external caller bypassing the `Store` API
  could write with an unconfigured connection. No product path does, and the
  claim concerns `acquire`'s behavior at its seam, which holds.

## UNVERIFIED

- The full `layerfs-storage` suite was not run; only the two named test files
  were executed (per the verification instructions). Other tests (e.g.
  `policy_capacity.rs`, which also asserts the pragma profile) were read but
  not executed.
- `cargo clippy`, `cargo fmt`, and `core/tools/check_product_boundary.py` were
  not run — not required for these three rows.
- Caveat C1's dependency-above-watermark behavior was reproduced with a
  `/tmp`-only standalone binary against the crates at HEAD, not as a repo test
  (the repo is read-only for this round); the probe source is
  `/tmp/layerfs-falsify/src/main.rs`.
- Caveat C2 (wrong-width identity / unknown role code in a locator surfacing as
  passthrough `ContentError`) was verified by source reading only, not
  empirically exercised.
- Whether any *future* adapter interpolates caller strings into SQL is out of
  scope; the check covers `core/crates/*/src` at HEAD as instructed.
