# Group level diagnostic: correctness review

> Status: Research; informative and not a product contract.

Source reviewed: `605f6efc6` plus the prospective single constant change in
`core/crates/layerfs-storage/src/encoding/codec.rs`, `GROUP_LEVEL: 19 -> 1`.
This review is source analysis, not a measurement or a test execution receipt.
The root agent serializes all builds, tests, and measurements.

## Scope and invariants

`compress_group` obtains compression parameters from the level, then retains the
window-log cap of 16, content-size field, checksum, and no dictionary id
(`core/crates/layerfs-storage/src/encoding/codec.rs:386`). The exact level change
leaves the 16 MiB encode and 1 MiB decode workspace constants unchanged
(`codec.rs:78`). Payload compression uses its distinct constant at `codec.rs:94`
and is outside this experiment. Existing decoder checks remain unchanged:
nonempty and bounded input/output (`codec.rs:612`), frame type, content length,
window, dictionary id and checksum flag (`codec.rs:623`), exact decoded length
(`codec.rs:637`), and common frame-header/extent validation (`codec.rs:661`).

The shared level affects **both** ordinary physical groups
(`core/crates/layerfs-storage/src/pack/assemble.rs:163`) and pooled metadata value
groups (`core/crates/layerfs-storage/src/encoding/pool/value_group.rs:50`). The
existing compressed-versus-raw rule remains frame length plus 16 <= raw length
(`codec.rs:694`). A level change can change that decision, compressed sizes,
pack placement and transaction cadence. Equality of Store bytes, pack counts,
physical locators, or decode counters is not the semantic correctness criterion.
No canonical construction, identity, payload level, worker policy, validation,
or capacity change is part of this candidate.

## Existing coverage and focused command

Root can execute this existing test selection once, under the measurement lock:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage \
  --test codec_frames --test physical_formats --test storage_limits \
  --test pack_locator --test metadata_pool --test group_decodes
```

| Existing target and source location | What it actually covers |
| --- | --- |
| `tests/metadata_pool.rs:733` | Real pooled value-group compression, Zstandard directory tag, smaller physical body, reopened canonical identity and repeated read |
| `tests/metadata_pool.rs:363` | Exact canonical leaf bytes and identity after SQLite save/reopen |
| `tests/metadata_pool.rs:260` | Real compressed ordinary groups containing pooled leaves, reconstruction across the existing decoded-cache bound, decoded group accounting |
| `tests/metadata_pool.rs:537` and `:798` | Damaged pooled value digest and damaged pooled delta leaf are rejected |
| `tests/group_decodes.rs:37` and `:70` | Actual ordinary group decode accounting, bounded to the read operation |
| `tests/physical_formats.rs:75` | Physical pack/group declared bounds and framing rejection |
| `tests/physical_formats.rs:235` | Consuming and retained-tail pack assembly produce identical bytes within one codec configuration |
| `tests/physical_formats.rs:330` | Ordinary groups seal on framed raw-length arithmetic rather than guessed compression ratio |
| `tests/storage_limits.rs:23` | Exact raw group boundary and one-byte-over rejection |
| `tests/pack_locator.rs:202` and `:240` | Corrupt whole-file pack body and out-of-directory locator rejection |
| `tests/codec_frames.rs:72`, `:102`, `:120`, `:150`, `:169`, `:184` | **Payload** codec roundtrip, false lengths, truncations, checksum corruption, and window/content policy rejection |

All test paths in the table are under `core/crates/layerfs-storage/`.
The codec-frame target does **not** directly exercise hostile group-frame checksum
or group-window branches. Its window test explicitly documents that the isolated
window clause has no reachable witness from repository-produced single-segment
frames (`tests/codec_frames.rs:205`). Do not report more coverage than exists.
Existing successful compressed-group roundtrip coverage plus unchanged decoder
source is adequate for this single encoder-level experiment; no new test-only
product API, instrumentation, fixture framework, or duplicate suite is proposed.

## Physical expectations and history proof

No fixed compressed-byte golden was found in the focused targets. Tests derive
physical shapes from produced stores where relevant; the cache eviction fixture
pins decoded body sizes, which do not change with compression level. Some tests
assert that deliberately compressible groups actually compress; those assertions
must remain, and any failure must be investigated rather than rewritten to hide
lost coverage. The raw arithmetic and existing pack bounds remain mandatory.

For the measured history pair, compare all 17 (then 53 on stride3 if retained)
canonical state roots, oracle entry/file/byte checks, and each arm's independent
read-back verification. SQL quick_check complements those semantic checks but
does not replace them. Report storage allocation and apparent bytes separately.
Physical Store hashes are identity/custody evidence and are expected to differ
across levels. Save/provider counters can change as compressed physical layout
changes; report changes rather than asserting that every counter must match.

Exclusive codec CPU is not supplied by the existing phase receipt. Whole-process
user/system CPU and storage phase wall are valid separately labelled measurements;
neither is codec CPU. Historical offline codec CPU deltas are prior evidence,
not a measurement of this current candidate.

## Execution and patch status

Focused tests, sealed patch review, and matched history semantic results are
pending root execution. This agent has run no resource-sensitive command and has
changed only this document.
