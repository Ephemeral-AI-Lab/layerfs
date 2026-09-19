# Stage 7 configuration audit — C1 and C2

> **Status:** Research; informative and not a product contract.

Read-only audit of frozen snapshot `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-stage7-audit-2mf09p4z`, manifest `c2f6034c534853919833c9f59758a17e74ea6331f41835b4267adf883bb74ff8`, based on HEAD `66bce8378` plus the captured working tree. Source references below are repository-relative and line numbers refer to that snapshot, not subsequent Stage 6 changes. No production files were changed; no compilation, tests or measurements were run, to avoid interference with the concurrent Stage 6 finalization.

## What is actually configurable

There are **four persisted policy knobs**, **five per-filesystem-operation resource fields**, plus caller-owned backing capacity and per-read limits. Most exported constants are supported-format or implementation limits, not configuration APIs. Neither C1 nor C2 product source reads environment variables. A harness flag or environment variable is not automatically a product knob.

### Persisted Store policy

| Value | Default | Accepted values | How to set / effect |
| --- | --- | --- | --- |
| Small-file construction cutoff | 131,072 bytes (128 KiB) | Exactly 128, 256, 512 or 1,024 KiB | `ConstructionPolicy::new(cutoff, whole_depth, chunk_depth)` for independent C1; `StoragePolicy::new(1, cutoff, whole_depth, chunk_depth)` for a Store. Nonempty files strictly below cutoff use one whole-file object; files at cutoff or above use chunked representation. Empty files use the defined empty file state. |
| Whole-file delta maximum depth | 8 | `0..=50` | Third argument to `StoragePolicy::new`; second to `ConstructionPolicy::new`. Zero disables new whole-file delta selection. C2 uses it for selection and reconstruction depth checks. |
| Chunk delta maximum depth | 4 | `0..=50` | Fourth argument to `StoragePolicy::new`; third to `ConstructionPolicy::new`. Zero disables new chunk delta selection. |
| Pooled metadata delta maximum depth | 8 | `0..=50` | `StoragePolicy::with_metadata_depth(depth)`. Independent of payload depths; zero disables new pooled-leaf deltas. |

`format_profile` is a constructor argument but **only `1` is accepted**. It is an explicit compatibility discriminator, not a choice of implementations. Schema version is fixed at **6** in this snapshot; earlier versions are refused, not migrated.

Sources: `core/crates/layerfs-content/src/policy.rs:14-101,119-128`; `core/crates/layerfs-storage/src/policy.rs:14-33,165-267`; depth selection at `core/crates/layerfs-storage/src/encoding/delta/select.rs:273`, pooled selection at `core/crates/layerfs-storage/src/cas/pool_lane.rs:315`, and read enforcement at `core/crates/layerfs-storage/src/encoding/delta/read.rs:159`.

Create-time and reopen behavior:

1. `Store::create(path, policy, scope)` validates policy before opening/creating the database, then records it. (`cas/store.rs:174-196`; `sqlite/schema.rs:84-105`.)
2. `Store::open(path, scope)` has **no override argument**. It reads the stored policy and derives capacities. (`cas/store.rs:201-218`.)
3. `Store::policy()` returns the accepted policy; `store.policy().construction()` is the matching C1 policy. Use that after reopening, rather than assuming defaults. (`cas/store.rs:228`; `policy.rs:260`.)
4. There is no public high-level setter to retune an existing Store. Low-level public `sqlite::schema::validate(connection, Some(expected))` can reject a mismatch, but this is not a high-level Store-open override. Do not suggest direct SQL updates as the tuning API.
5. Equality is strict when an expected policy is supplied; schema ID is also strict. (`sqlite/schema.rs:108-166`.)

The cutoff can change canonical structure and root IDs for the same logical content; it is therefore not a transparent tuning change across arbitrary revisions. Delta depths affect physical dependency selection and read admission, not canonical object bytes/IDs. Lowering a persisted depth by editing SQL can make existing chains unreadable. Raising a depth does not raise chain byte/work budgets and does not promise that chains of that depth are always achievable. A high cutoff increases the bounded whole-file object/probe size but not batch/transaction/chain ceilings; large objects use a singleton exception rather than silently raising those ceilings.

### Per-operation filesystem resources (ephemeral)

Set these directly on `FilesystemResources`, supplied as `FilesystemInput.resources`. They are not stored in the Store policy or canonical filesystem root.

| Field | Default | Validation in the high-level request | Real use |
| --- | ---: | --- | --- |
| `scratch_bytes` | 4,194,304 (4 MiB) | At least 1,024; **no maximum enforced** | Budget for unfinished sorted directory/inode pages. A shortage returns an error before tracked growth. It is not a total-process heap budget. |
| `maximum_pending_records` | 4,096 | Nonzero; no maximum | In-memory reference-reducer row count before spill; ordering-byte checks also apply. |
| `merge_buffer_bytes` | 16,384 (16 KiB) | At least one 96-byte ordering row; no maximum | Per merge-reader buffer, rounded down to a whole number of 96-byte rows. Multiple merge readers may coexist. |
| `base_read_batch` | 32 | Nonzero; no maximum | How many inode reference rows a reduction wave processes; also sizes its final run buffer as `base_batch * 96 * 4`. It is not negotiated with the provider's maximum demand. |
| `ordering_bytes` | 67,108,864 (64 MiB) | At least 96 | Aggregate declared ordering-row/run/output byte budget; also derives touched-serial capacity as `ordering_bytes / 16`. Backing capacity, when advertised, must cover it. |

Sources: `core/crates/layerfs-content/src/filesystem/input.rs:54-125`; defaults at `filesystem/references/reduce.rs:25-27`, `references/runs.rs:37-39`, `sorted/page.rs:26`; wiring at `filesystem/update.rs:170-176,246,317-355`; accounting at `references/runs.rs:95-108`; buffer allocation at `references/reduce.rs:375`; merge rounding at `references/merge.rs:71-75`.

Caller-owned spill storage is another real configuration boundary: `LocalFileBacking::new(directory)` defaults to **256 MiB**; `LocalFileBacking::with_capacity(directory, capacity_bytes)` chooses the directory and hard byte ceiling. `OrderingBacking`/`OrderingRun` allow another real owned backing. Passing no backing is supported until a spill is required; the first required spill fails explicitly. A declared 64 MiB operation ordering budget with a 32 MiB fixed backing is rejected up front. No silent fallback or automatic acquisition occurs. (`filesystem/references/backing.rs:26,32-74,178-183`; `references/runs.rs:68-108`.)

For reads, `read_all_bounded(reader, root, maximum, sink, scope)` sets a per-call logical-length maximum. `read_all` supplies `u64::MAX`; that means no whole-file logical-length cap, although traversal is streaming. `read_range` bounds output with the requested range. These are call arguments, not persisted knobs. (`file/read.rs:18-72`.)

## Values that are fixed or derived, not Store tuning knobs

| Area | Current fixed/derived values | Adjustment boundary |
| --- | --- | --- |
| CDC | Min/target/max = 8/16/32 KiB; normalization 2; seed 0; fixed masks and gear table; `FastCdc::new()` has no parameters | Canonical-profile change, not runtime tuning. `profile_id()` hashes those parameters. |
| Canonical grammar | 16 MiB canonical object ceiling; 8 MiB value field; mapping page max 128 entries; mapping depth 31 | Format/contract constraints. |
| Whole-file capacities | Raw cutoff minus 1; canonical raw plus 23; frame bound max(135,168, raw + raw/128 + 1,024); codec window log 18 through 256 KiB cutoff, otherwise 20 | Derived from the accepted policy. Do not independently mutate returned capacity fields as if they were supported knobs. |
| Pending save batch | 512 objects / 512 KiB canonical bytes; one oversized singleton may exceed byte budget in an otherwise empty batch | Fixed admission profile. `PendingBatch::push` explicitly documents the singleton exception. |
| Transaction | 8,191 submitted rows / 4 MiB minus 1 canonical byte | Fixed profile, not policy constructor fields. |
| Read and SQL waves | 4,096 objects per Store read wave; membership/locator page 128 IDs; cleanup page 128 rows | Fixed limits. `StoreProvider` inherits the Store limit. |
| Pack/group | Group target 48 KiB, group ceiling 64 KiB, ordinary/native pack max 256 KiB, 256 groups/pack, 8,191 records/group; singleton max 16 MiB + 4 KiB | Physical-format and placement profile. |
| Payload chain work | Canonical 512 KiB; encoded 256 KiB | Fixed, independent of allowed depth. |
| Metadata | 165 values/group, 16 KiB pooled group, 100 leaf rows, 8 KiB record; chain canonical 65,536 bytes; chain encoded 139,281 bytes; decoded work 32 MiB; match trial 128 KiB | Fixed profile. Metadata depth is the configurable exception above. |
| Caches/indexes | Dependency pack bodies 4 MiB per save; pooled values 512 KiB/read wave; ordinary decoded groups 512 KiB/wave; metadata value index 131,072 entries; content-signature index 8,192 slots and 704 KiB ceiling | Implementation bounds, with fixed ownership/lifetime. No `Store` cache-sizing API. |
| Codec | Zstandard payload level 3, group level 1; fixed checksums/content-size fields and zero workers; encode workspace 2 MiB/decode 1 MiB; chunk window log 20 | Fixed codec contract. Whole-file window derives from cutoff. |
| SQLite | MEMORY journal; synchronous OFF; temp_store MEMORY; foreign keys ON; busy timeout zero | Mandatory persistence profile; no WAL, sync or retry option. |
| SQLite cache observables | page_size/cache_size/cache_spill/mmap_size read from save connection by `SaveOperation::connection_profile()` | Measurements only. Product `configure` does not set these four values, so do not claim a fixed LayerFS cache size or treat this getter as a setter. |
| Filesystem grammar/work limits | 8 KiB pages; name 255 bytes/path 4,096 bytes/256 components; tree level 31; symlink 4,096 bytes; attribute domain 64/key 255/value 32 KiB; edits 4,096; walk/list limits 4,096 | Named profile or work ceilings, not user configuration. The source documents the consequences of the walk limit for initial builds and directory moves. |

Sources: `content/src/file/cdc/gear.rs:12-47`; `content/src/policy.rs:142-172,186-236`; `storage/src/policy.rs:50-164,325-351`; `storage/src/cas/batch.rs:45-68`; `storage/src/encoding/codec.rs:20-22,49-112,213-221`; `storage/src/encoding/delta/candidates.rs:54-59`; `storage/src/sqlite/connection.rs:34-52`; `storage/src/cas/store.rs:115-146,401-410`; `content/src/filesystem/limits.rs`; `content/src/file/edit/input.rs:22`.

`ConstructionCapacities` and `StorageCapacities` have public fields but are primarily **derived reports**, not uniformly effective tuning objects. `Store::capacities()` returns a copy, and changing the copy cannot configure that Store. In C1, only the supplied whole-file raw/canonical limits and stream-flush entry count are consulted by constructors; the chunk bounds and depth copies do not configure `FastCdc` or cause C1 to select physical deltas. The actual delta selector lives in C2. This distinction should be documented prominently for integration owners.

## Review findings relevant to independent replacement

### C-1 — C1's policy/capacity pair is not validated as one contract (P2)

Source-confirmed validation absence, **not a newly observed runtime test failure**. `construct_bytes_with_predecessor`, `construct_stream` and `apply_edits` call `policy.validated()` but do not verify the separately supplied `ConstructionCapacities`. Its fields are public. `ExtentBuilder::new` trusts `stream_flush_entries`, adds 1 without checked arithmetic, and allocates that size. Call path: public constructor -> `construct_chunked` -> `mapping::build_streaming_with_predecessor` -> `ExtentBuilder::new` (`file/content.rs:226-238,279-286,319-334`; `file/edit/apply.rs:39-47`; `file/mapping/build.rs:75-79`).

Consequences: a caller can accidentally pass capacities from a different policy, receive a late capacity refusal, or independently widen the frontier beyond the profile's declared bound. Setting `stream_flush_entries = usize::MAX` reaches unchecked `+ 1` for chunked construction; panic/overflow/allocation behavior is a source inference, not executed here. Changing the public `chunk_raw_limit` or delta-depth copy has no corresponding effect on the scanner or selector.

Smallest remedy: enforce equality against `policy.capacities()` at public construction/edit boundaries if capacities are intended to be immutable derivations; alternatively, explicitly define which fields are configurable and validate them. Do not introduce a generic configuration framework. Existing `file_complete.rs:274` covers unsupported policies, and `streaming.rs:198` covers normal derived frontier bounds; neither proves arbitrary forged capacities are rejected before work. Closure check: public API rejects mismatched/overflowing capacities before reading input or emitting objects, while default and each supported cutoff still produce their canonical results.

### C-2 — Filesystem resource acceptance has incomplete arithmetic and provider bounds (P2)

Source-confirmed missing upper validation, runtime reproducer **NOT_RUN**. `FilesystemResources::check()` accepts any nonzero `base_read_batch`; a run-backed `FinalRows::new` allocates `vec![0; base_batch.max(1) * 96 * 4]` using unchecked multiplication (`filesystem/input.rs:115-119`; `references/reduce.rs:375`). The high-level path wires this value into reducer finalization (`filesystem/update.rs:317-341`; `references/reduce.rs:282-290`). Arbitrarily large accepted values can overflow or request excessive memory rather than yield a typed capacity error. The five fields are also not one aggregate memory budget: merge-reader/run buffers are separate from sorted-page scratch and declared row/run accounting.

Further boundary gap: `AuthenticatedObjects` advertises no maximum batch capability (`object/access.rs:33-50`), whereas C2 refuses waves above 4,096; increasing a C1 batching knob cannot negotiate that limit. Do not impose C2's concrete constant inside C1; keep safe bounded calls or establish a narrow provider capability only if required by the integration design.

Smallest remedy: checked arithmetic/fallible reserve for derived buffer sizes, with usable upper resource validation; document ownership and batch semantics. Existing `filesystem_bounds.rs:630-674` checks insufficient backing capacity and `:1349` checks 512 KiB scratch; no searched test covers extreme `base_read_batch`. Closure check: a public run-backed request with an extreme batch returns a typed refusal without panic/partial publication, and sane batch variants preserve canonical results.

### C-3 — A named maximum scratch value is only a default in the actual API (P2 documentation/contract gap)

`MAXIMUM_OPERATION_SCRATCH_BYTES` is documented as the largest bytes one filesystem operation may hold (`filesystem/limits.rs:38-46`) and exported through `MAXIMUM_SCRATCH_BYTES`. But `FilesystemResources::check()` has only a lower bound and the sorted `Engine` creates `Budget::new(scratch_limit)` directly (`filesystem/input.rs:100`; `filesystem/sorted/page.rs:169-174`). Larger declared scratch is accepted by source. This matters when another worktree swaps an implementation under an integration that treats 4 MiB as a guaranteed ceiling.

Smallest remedy: decide whether 4 MiB is a hard profile maximum or the default caller budget; enforce the former or rename/document the latter. Do not silently infer an upper bound from a constant's name. Runtime reproduction NOT_RUN.

### C-4 — Separate physical-depth policy currently lives in C1's canonical-construction policy (P3 architectural friction)

C1 validates and carries `whole_file_delta_max_depth`/`chunk_delta_max_depth`, but its constructor algorithms do not select physical deltas; C2 consumes those values in its own policy/capacities. This is source-level policy coupling rather than a demonstrated correctness defect. It means a change to C2's accepted physical-depth range can require a C1 policy change even if canonical algorithms are identical. Integration should currently obtain the C1 policy from the Store to avoid duplication. A future minimal cleanup could separate canonical construction choice from physical delta policy, but changing public types during review is not automatically justified. First document that depth changes are physical and do not alter canonical IDs.

### Compatibility boundary that is intentional, but defeats arbitrary swapping

No runtime migration or persisted-policy retuning API exists. This snapshot's mandatory content-signature table bumps schema to 6; an older C2 whose exact schema is 5 cannot open it. That is explicit incompatibility, not broken loose coupling by itself. An algorithm-only replacement that needs schema changes requires a supported compatibility/migration decision; an unchanged integration call signature alone is insufficient evidence for switch-and-swap. Do not promise arbitrary combinations of optimized C1/C2 revisions.

## Concrete tuning example

This is a source-checked example using actual signatures; compilation/execution was deliberately not run. It creates a fresh Store at an absent path, uses a 256 KiB construction cutoff, payload depths 8/4 and metadata depth 12, derives the C1 settings from C2, and retains storage errors at the handoff. An existing Store should instead be opened and queried for its stored policy.

```rust
use std::{error::Error, path::Path};
use layerfs_content::{construct_bytes, ObjectId};
use layerfs_storage::{SaveHandoff, StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

fn save_new_store(path: &Path, bytes: &[u8]) -> Result<ObjectId, Box<dyn Error>> {
    Timing::disabled("save", |scope| -> Result<_, Box<dyn Error>> {
        let policy = StoragePolicy::new(1, 256 * 1024, 8, 4)
            .with_metadata_depth(12)
            .validated()?;
        let store = Store::create(path, policy, scope.child("create"))?;
        let construction = store.policy().construction();
        let mut save = store.begin_save(scope.child("begin"))?;
        let file = {
            let mut handoff = SaveHandoff::new(&mut save);
            let result = construct_bytes(
                construction,
                &construction.capacities(),
                bytes,
                &mut handoff,
                scope.child("construct"),
            );
            if let Some(error) = handoff.take_failure() {
                return Err(error.into());
            }
            result?
        };
        save.finish(scope.child("finish"))?;
        Ok(file.root)
    }).0
}
```

Filesystem tuning is separate, for example `FilesystemResources { scratch_bytes: 2 * 1024 * 1024, maximum_pending_records: 1024, base_read_batch: 32, ..FilesystemResources::default() }`. Attach it to the next `FilesystemInput`; supply backing capable of at least the still-default 64 MiB ordering budget if spilling can occur. Smaller scratch can explicitly refuse a workload; it is not a promise that all workloads will fit.

## Existing evidence and unrun checks

Reviewed external tests: `storage/tests/policy_capacity.rs` tests cutoff persistence/reopen, invalid values, singleton record storage, cutoff boundaries, independent metadata depth and unchanged work budgets; `storage/tests/delta_chains.rs` tests accepted/rejected depths and chain-work limits; `storage/tests/connection_profile.rs` verifies connection observables; `content/tests/file_complete.rs` rejects unsupported construction policy; `content/tests/streaming.rs` checks derived streaming bounds; `content/tests/filesystem_bounds.rs` checks ordering backing and scratch behavior. These test sources exist; this audit does not claim they passed against the captured dirty snapshot. The test named `every_supported_cutoff_is_persisted_and_reopened_unchanged` currently enumerates 128, 256 and 1024 KiB but omits the accepted 512 KiB value (`policy_capacity.rs:27`).

Suggested later targeted commands, only when Stage 6's resource-sensitive work is clear:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test policy_capacity --test delta_chains --test connection_profile
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test file_complete --test streaming --test filesystem_bounds
```

The missing malformed-capacity/extreme-resource checks must be added before these commands can prove closure of C-1/C-2. Substitution across real component revisions and cross-revision persisted-store compatibility remain NOT_RUN; policy inspection is not substitution proof.
