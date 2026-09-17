# N-11 / R2-F21 verification: the read wave's byte ceiling

| | |
| --- | --- |
| Claim | N-11 / R2-F21 (round-3 commit `2fe2a4642`): the read wave enforces a byte ceiling beside the object ceiling — every decoded payload refused above the chunk maximum (32,768); the wave's bytes checked against `READ_WAVE_BYTES` (32 payloads × 32,768 = 1,048,576); a crafted provider serving an oversized chunk under a legal leaf refused with no bytes emitted; the largest legal wave equals the declared figure; `stage-5-report.md` §6's "Read wave" row states all of this |
| Tree | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (frozen commit; `git status --porcelain` empty) |
| Method | read-only inspection + `cargo test` (artifacts under `core/target` only); this file is the only write under the repository |
| Verdict | **PASS** — all five bullets reproduced from code and a passing test; one framing qualification recorded in F6 |

## Commands run (from the repository root)

| # | Command | Exit |
| --- | --- | --- |
| 1 | `git rev-parse HEAD && git status --porcelain \| head -20` | 0 — HEAD `99743b2cff…`, working tree clean |
| 2 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test file_read the_payload_wave_enforces_both_its_object_and_its_byte_ceiling` | 0 — `1 passed; 0 failed; 14 filtered out`, test binary `file_read-e6b772fe520fe469` (identical to the round-3 receipt) |
| 3 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test file_read` | 0 — `15 passed; 0 failed` in 2.45 s |
| 4 | `git show 2fe2a4642 --stat --format=…` (message + diff stat) | 0 |
| 5 | read-only `sed` / `grep` / `wc` / `find` / `cat` over the files cited below | 0 each |

## Findings

### F1 — The constants (chunk maximum 32,768; wave 32 objects / 1,048,576 bytes)

- `core/crates/layerfs-content/src/file/cdc/gear.rs:17` — `pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;`
- `core/crates/layerfs-content/src/file/mapping/read.rs:24` — `pub const READ_WAVE_OBJECTS: usize = 32;`
- `read.rs:33` — `pub const READ_WAVE_BYTES: usize = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES;` → 32 × 32,768 = **1,048,576**.
- The doc at `read.rs:25-32` states: "Both halves of the wave are enforced, not just the count … A provider that hands back an oversized payload under a chunk role is therefore refused at the wave boundary instead of being copied through, which is what makes the object bound and the byte bound the same claim."

### F2 — The enforcement (both halves in the payload wave's flush)

`core/crates/layerfs-content/src/file/mapping/read.rs`, `Wave::flush`:

- `read.rs:104` — the object half: `if self.distinct.len() >= READ_WAVE_OBJECTS { self.flush()?; }` (a wave never holds more than 32 distinct payloads; the demand buffer also flushes at 4×32, `read.rs:114`).
- `read.rs:140-147` — the per-payload byte half, quoted:
  ```rust
  let inner = crate::object::decode_bytes_object(value)?;
  let payload = decode_chunk_payload(inner)?;
  if payload.len() > cdc::MAXIMUM_CHUNK_BYTES {
      return Err(ContentError::ObjectLimitExceeded {
          limit: cdc::MAXIMUM_CHUNK_BYTES,
          actual: payload.len(),
      });
  }
  ```
- `read.rs:148` — `wave_bytes = wave_bytes.saturating_add(payload.len());`
- `read.rs:151-156` — the wave byte half, quoted:
  ```rust
  if wave_bytes > READ_WAVE_BYTES {
      return Err(ContentError::ObjectLimitExceeded {
          limit: READ_WAVE_BYTES,
          actual: wave_bytes,
      });
  }
  ```
- The decode path refuses the same maximum before the wave's own check: `core/crates/layerfs-content/src/file/mapping/codec.rs:91-96` (`decode_chunk_payload`: `if bytes.len() > cdc::MAXIMUM_CHUNK_BYTES { return Err(ContentError::ObjectLimitExceeded { limit: cdc::MAXIMUM_CHUNK_BYTES, …` — so the chunk maximum is enforced twice on every decoded payload.
- Honest reachability note: with `distinct.len() ≤ 32` (read.rs:104) and every payload ≤ 32,768 (codec.rs:91, read.rs:142), `wave_bytes` can never exceed 1,048,576, so the `read.rs:151` branch cannot fire while the other invariants hold. It is a checked invariant (defense in depth that locks the byte figure), not an independently triggerable refusal; no test triggers it directly — the refusal test fires the decode/per-payload check, which raises the same error variant with `limit = 32_768`.

### F3 — The refusal test (crafted oversized chunk under a legal leaf; no bytes emitted)

`core/crates/layerfs-content/tests/file_read.rs:562` — `the_payload_wave_enforces_both_its_object_and_its_byte_ceiling`:

- `:572-590` — `CraftedChunk`, a provider that "Serves the real leaf and state, and one crafted payload under them".
- `:592` — `let oversize = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES + 1_024;` (= 33,792 bytes).
- `:593-604` — a hand-built canonical chunk ("exactly as the public encoder writes them, but with a payload the public encoder refuses to produce"); the doc at `:503-513` explains the object "is canonical and correctly identified - it is refused because of its **size**, not because its bytes do not hash to its id".
- `:607-643` — a real encoded `ExtentLeaf` and `FileState` over that payload, both accepted by the public encoders (`.expect("leaf")`, `.expect("canonical leaf")`, `.expect("state")`), so the leaf is legal and the refusal can only be about the payload's size.
- `:649-657` — the refusal assertion:
  ```rust
  matches!(outcome,
      Err(ContentError::ObjectLimitExceeded { limit, actual })
          if limit == layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES
              && actual == oversize),
  ```
  with the message "an object above the chunk maximum is refused at the wave boundary".
- `:658-662` — `assert!(out.is_empty(), "a refused wave emits no bytes: {}", out.len());`

Command 2 ran exactly this test to exit 0, reproducing the round-3 receipt `docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T020000Z/read-wave.log` (same binary hash, `1 passed … 14 filtered out`, EXIT=0).

### F4 — The largest legal wave equals the declared figure

Same test, `file_read.rs:664-734`:

- `:669-672` — `assert_eq!(READ_WAVE_BYTES, READ_WAVE_OBJECTS * layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES);` ("Moving either constant without the other would leave the byte figure describing a bound the object count already refuses, and this assertion fails instead.")
- `:673-684` — 32 chunks each exactly `MAXIMUM_CHUNK_BYTES` bytes (`encode_chunk_object(&bytes).expect("a maximum-sized chunk is legal")`), `total = READ_WAVE_OBJECTS * chunk` = 1,048,576.
- `:715-726` — the whole wave is served: `.expect("the largest legal wave is served")` and `assert_eq!(out.len(), total);`
- `:727-734` — `counters.max_payload_batch == READ_WAVE_OBJECTS` ("one wave holds exactly the object ceiling at the chunk maximum") and `counters.payload_bytes_read <= READ_WAVE_BYTES`.
- Supporting cases in the same target (command 3, 15/15 pass): `a_large_read_acquires_payloads_in_bounded_batches` (`:265-303` — a 24 MiB read fills the wave to exactly `READ_WAVE_OBJECTS` and re-asserts `READ_WAVE_BYTES == READ_WAVE_OBJECTS * 32_768` at `:290-298`) and `a_wave_is_released_before_the_next_one_is_read` (`:162-182` — `peak_live_payloads() <= READ_WAVE_OBJECTS`).

### F5 — stage-5-report.md §6's "Read wave" row matches the code

`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:409` in the frozen commit `99743b2cf` (§6 "Limits", heading at :395; a concurrent verification round's edit to the working tree, +10 lines near :188, shifts this row to :419 there — row text identical, verified by diff):

> | Read wave | enforced | ≤ 32 payload objects and ≤ 1,048,576 bytes (`READ_WAVE_OBJECTS × MAXIMUM_CHUNK_BYTES`): the count is enforced as payloads are demanded, and each decoded payload is refused above the chunk maximum, so the byte figure is a bound on what a wave can acquire rather than an estimate. The two constants are asserted equal to the product by the wave's own case |

Every clause matches the code: 32 objects (read.rs:24), 1,048,576 as the product (read.rs:33, gear.rs:17), "the count is enforced as payloads are demanded" (read.rs:104), "each decoded payload is refused above the chunk maximum" (codec.rs:91, read.rs:142), "a bound on what a wave can acquire rather than an estimate" (backed by the enforcement in F2 and the test in F3/F4), "asserted equal to the product by the wave's own case" (file_read.rs:669-672). §14's round-3 row (report :595) adds the enforcement summary and names the receipt `read-wave.log`; the evidence README (`…20260918T020000Z/README.md:25`) states the oversized-chunk/no-bytes-emitted detail. One placement nuance: that refusal-test detail is not verbatim inside the §6 row itself — it lives in §14's row, the receipt and the README — but the §6 row's "enforced" claim is exactly what the test proves, and nothing in the row contradicts the code.

### F6 — Falsification: C2's own wave, and the seam-table alternative

- **C2's own wave is count-only, with no byte ceiling of its own.** `core/crates/layerfs-storage/src/policy.rs:80` — `pub const READ_OBJECT_LIMIT: usize = 4_096;`, enforced by `check_read_demand` (`core/crates/layerfs-storage/src/cas/store.rs:259-268`), called in `Store::read_batch` (`store.rs:207`), the mutation-owner read (`store.rs:346`) and `Store::contains` (`store.rs:246`); it refuses only `ids.len() > limit` with `StorageError::CapacityExceeded`. `StoreProvider::read_wave` (`cas/provider.rs:70-76`) is that same `Store::read_batch`. C2's byte-related wave bounds are cache-release triggers, not refusals: `POOLED_VALUE_CACHE_BYTES = 512 * 1024` (`policy.rs:121`) and `DEPENDENCY_PACK_CACHE_BYTES = 4 * 1024 * 1024` (`policy.rs:113`) drop caches when crossed; per-object size is bounded at admission by `CANONICAL_LIMIT = 16 MiB` (`policy.rs:61`). The round-2 review's seam arithmetic — 4,096 × 16 MiB through the raw provider seam (`stages-1-5-review-20260917T230700Z.md:945`, seam row `:1428`) — therefore remains reachable for a hypothetical future adapter at that seam.
- **In the product read path, C2's wave is covered by the same ceiling, not its own.** Every provider call a logical read issues is one of C1's bounded waves: payload waves of ≤ 32 distinct chunk ids (`read.rs:104-108`, issued at `read.rs:126` via `read_canonical_batch_scoped(&self.distinct, …)`) whose served bytes are refused above the chunk maximum / 1 MiB at C1's wave boundary, and navigation waves of ≤ 32 page ids (`read.rs:257`) of ≤ 8,192 canonical bytes each (§6 "Page" row). The refusal test's crafted provider stands on exactly that seam and is refused with nothing emitted.
- **The row's wording covers it.** The §6 row describes the wave that acquires bytes (the payload wave, both sides of the provider call) and makes no byte claim about C2's standalone 4,096-object wave; the count-only demand ceilings are separately documented as counts (`filesystem-tree.md:572-576` "A read wave is bounded … both 4,096. A longer slice is refused"; `content-io-memory-audit.md:134` "Read-wave demand … both 4,096"). No document overclaims a byte ceiling where none exists.
- **The seam-table obligation route was NOT chosen — enforcement exists in code.** `content-io.md` contains no read-wave / byte-ceiling / adapter-obligation entry (grep for `read.wave|byte ceiling|adapter obligation` returns nothing). The enforcement is in `read.rs:140-156` with its test (F3/F4). The handoff's *preferred* placement (WP-G, `stage-5-terminal-handoff-20260917.md:270-276`: "a byte ceiling beside the object ceiling in `FilesystemObjects::read_batch` and in C2's wave") was **not** the implemented form either: `FilesystemObjects::read_batch` (`core/crates/layerfs-content/src/filesystem/objects.rs:89-95`, `MAXIMUM_READ_DEMANDS = 4_096` at `:20`) still refuses on count only, charging bytes to counters without a byte refusal. The implemented route satisfies the round-2 recommendation's first arm ("Give the read wave a byte ceiling (N-11)", review `:1598`) at the payload wave — where the bytes actually flow — and the round-3 commit message states exactly that (`2fe2a4642`: "The payload wave enforces both halves of its declared bound … (N-11, R2-F21)").

### Verdict reasoning

All five bullets of the claim are reproduced: the constants (F1), both enforcement halves (F2), the refusal test run to exit 0 with the no-bytes-emitted assertion (F3), the largest-legal-wave case asserting 1,048,576 (F4), and the §6 row matching the code (F5). The framing "in both C1's boundary and C2's wave" holds in the sense that C1's payload-wave boundary is where the wave C2 serves is checked and refused (F6); read as "a byte ceiling inside `FilesystemObjects::read_batch` and inside C2's `Store::read_batch`" it would be false — both remain count-only, exactly as WP-G's unimplemented preferred form and the review's seam-table row describe. The claim's own bullets, the §6 row and the test all state the implemented form, so the claim as evidenced is **PASS**, with the qualification above recorded rather than counted against it.

## UNVERIFIED

1. The full core suite (§14's "422 passed / 0 failed") was not re-run; only the `layerfs-content` `file_read` target (15 tests) and the single receipt test, both exit 0.
2. The `wave_bytes > READ_WAVE_BYTES` branch (read.rs:151) firing was not demonstrated by execution — it cannot fire while the 32-object and per-payload caps hold (F2 reachability note); that analysis is code reading, not a triggered refusal.
3. C2's count-ceiling test `a_read_wave_is_bounded_by_the_declared_ceiling` (`core/crates/layerfs-storage/tests/cas_roundtrip.rs:204`) was read but not executed; its subject is the count ceiling, not this claim's byte ceiling.
4. No end-to-end run through a real `Store`/`StoreProvider` serving an oversized chunk; the refusal test's provider is the crafted in-memory one the claim itself specifies.
5. Per-call-site id-count bounds for every non-payload `read_canonical_batch` caller (directory/attributes/inode level waves; `sorted/page.rs:241` page waves) were spot-checked (level fan-out at `directory/read.rs:125-126`; scratch-budgeted chunks at `sorted/page.rs:230-241`) but not exhaustively traced; they are outside the payload-wave scope of this claim.
