# Boundary: C1, C2 and the two traits

> **Status:** Research; informative and not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 1. Component architecture

### 1.1 The one-sentence separation

**C1 opens no database, pack or file.** It asks a caller-supplied provider for
already-authenticated canonical bytes, and it hands finalized canonical objects to
a caller-supplied consumer. C2 owns the path, the policy, the packs and the SQLite
file. `layerfs-telemetry` owns timing and depends on nothing.

That single rule is what makes the three components independently measurable, and
it is the reason the boundary is expressed as two traits rather than a facade.

```text
                          ┌────────────────────────────────────────────┐
                          │  caller / future application adapter        │
                          │  (Stage 7 / #172 — not part of core today)  │
                          └───────┬──────────────────────────┬─────────┘
                                  │                          │
                     drives C1    │                          │   opens C2
                                  ▼                          ▼
        ┌───────────────────────────────────┐   ┌────────────────────────────────┐
        │  C1  layerfs-content              │   │  C2  layerfs-storage           │
        │  ─────────────────────────────    │   │  ────────────────────────────  │
        │  pure with respect to I/O:        │   │  owns:                         │
        │    no database                    │   │    one filesystem path         │
        │    no pack                        │   │    one persisted policy row    │
        │    no file                        │   │    pack BLOBs (object_packs)   │
        │    no unsafe                      │   │    SQLite schema + indexes     │
        │                                   │   │                                │
        │  object identity + framing        │   │  exact CAS reuse               │
        │  complete-file construction       │   │  physical FULL/PREFIX encoding │
        │  localized known edits            │   │  pack framing + placement      │
        │  bounded logical reads            │   │  publication watermark         │
        │  filesystem trees (sorted COW)    │   │  dependency-chain resolution   │
        └───────────┬───────────────▲───────┘   └───────────▲───────────┬────────┘
                    │               │                       │           │
      AuthenticatedObjects          │        FinalizedConsumer          │
      (reads: bytes in)             │        (writes: objects out)     │
                    │               │                       │           │
                    └───────────────┴───────────────────────┴───────────┘
                          the entire C1 ↔ C2 boundary:
                          two traits, no shared concrete type

        ┌────────────────────────────────────────────────────────────────────┐
        │  layerfs-telemetry — std-only, no product dependency, no file I/O  │
        │  TimingScope threaded through every C1 entry point                 │
        └────────────────────────────────────────────────────────────────────┘
```

### 1.2 Dependency direction

```text
   layerfs-telemetry   ◄──── layerfs-content   ◄──── layerfs-storage
   (no deps)                 (telemetry only)       (content + telemetry
                                                     + rusqlite + zstd)

   layerfs-content  ──✗──►  layerfs-storage      NEVER
   either           ──✗──►  Workspace / history / FUSE / daemon / Monitor
```

C1 depends on telemetry and nothing else product-side. C2 depends on C1 — it
consumes C1's `FinalizedObject`, `ObjectId` and `ObjectRole` — and the reverse
edge does not exist. This is why C2 can be exercised with objects C1 never built,
and why C1 can be exercised with no storage at all.

### 1.3 The two boundary traits

**`AuthenticatedObjects`** — `core/crates/layerfs-content/src/object/access.rs`

```rust
fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>;
```

The contract, verbatim from the source documentation:

- Return the canonical bytes whose identity is `ids[index]`, **in that order**.
- Bytes that do not belong to the requested identity are a **contract violation**.
  A provider that cannot establish identity must return `IdentityMismatch` rather
  than bytes.
- **Absence is `MissingObject` and nothing else.**
- A provider that holds state for the request but cannot serve it — corrupt
  storage, a record outside this reader's visibility, a capacity refusal — reports
  **`ProviderFailure`**, "so the two classes stay distinguishable".

That last distinction is load-bearing for the no-fallback rule: "the object is not
here" and "the provider broke" must not collapse into one error, because only the
first is a legitimate reason for a caller to choose a different representation.

A batch is a **real grouped demand**, not a loop of point calls. It returns values
in demand order with exact cardinality.

There is one required method and one optional override:

```rust
// required
fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>;

// optional; default forwards to the above and records nothing
fn read_canonical_batch_scoped(
    &self,
    ids: &[ObjectId],
    scope: TimingScope<'_>,
) -> ContentResult<Vec<Vec<u8>>> { let _ = scope; self.read_canonical_batch(ids) }
```

The default is the honest answer for a provider that only hands back bytes it
already holds. A provider whose demand performs real work of its own overrides the
scoped form so that work becomes a **named child** of the caller's span instead of
being merged into the caller's own duration. C2's `StoreProvider` is exactly such
a provider.

**`FinalizedConsumer`** — `core/crates/layerfs-content/src/object/output.rs`

```rust
fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>;
```

`accept` **takes ownership**. Returning an error ends construction: the caller
receives that error once and no retry, resend or alternative path is taken. C1
keeps no payload copy after the hand-off.

`DiscardingConsumer` is the non-persisting implementation: it charges and counts
what construction emitted and performs no storage work at all. Its existence is
the proof that construction completes with no database, pack or file involved. Its
counters describe the **emitted object set, not a stored one** — the source is
explicit that this is not a durability or storage claim.

### 1.4 Timing seams

Every C1 entry point takes a `TimingScope`, so construction-only, read-only and
integrated callers measure the **same production functions**. Recording enabled or
disabled changes no product work: a disabled recording reads no clock and creates
no nodes.

```text
   Timing::start ──► TimingScope<'a, Pending> ──run()──► Active handle
                                                            │
                                          ┌─────────────────┴─────────────────┐
                                          │  children come ONLY from this      │
                                          │  handle ⇒ a child borrows its      │
                                          │  parent and cannot outlive the     │
                                          │  parent's measured region          │
                                          └────────────────────────────────────┘
```

Children created inside C1, by name and as they appear in the source:

| Entry point | Named children |
| --- | --- |
| `construct_bytes` | `content.encode`, `content.identify`, `content.emit` |
| `construct_stream` | `content.probe`, then the `construct_bytes` children |
| chunked construction | `content.chunk`, `content.emit` |
| `inspect` | `content.inspect` |
| `read_all_bounded` | `content.acquire`, then the traversal children |
| `FileView::open` | `edit.base_read` |
| `apply_edits` | `edit.base`, `edit.compare`, then the route's children |

For a whole filesystem operation, `FilesystemPhases`
(`core/crates/layerfs-content/src/filesystem/objects.rs`) records **coarse** phase
scopes under the caller's running scope: "a phase is recorded as a child of the
caller's running scope, so an enabled recording reports where a complete operation
spent its time **without one trace node per inode**". Disabled phases run the
identical body and read no clock.

The timer is three-valued about what it observed, which matters for honest
reporting:

| `NodeOutcome` | Meaning |
| --- | --- |
| `Ok` | the measured operation returned success |
| `Error` | the measured operation returned an error |
| `Unknown` | it **never returned** — panic, or the scope was dropped while running |

`Unknown` is explicitly **not** success: "a consumer that reads the outcome alone
must not treat a node that never completed as an operation that completed
successfully."

`Completeness` is similarly three-valued — `Disabled` / `Complete` / `Clipped` —
because a disabled run is legitimate and performs no measurement, while a clipped
run measured an operation and could not describe all of it. A caller checking only
`is_incomplete()` therefore never fails a disabled row.
