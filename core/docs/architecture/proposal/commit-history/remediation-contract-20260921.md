# Repaired history boundary

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Frozen before dependent implementation on `codex/pair2-history-remediation`,
parent `35740836f84b9ef687da2f70d785c9e6b2ed2fd2`. This note implements P0 of
[the remediation specification](remediation-20260921.md). C5 schema 1 (seven
tables), C2 schema 7 and legacy profile 1/opcodes 1–5 remain unchanged.

## Failure grammar

Legacy Failure remains exactly `code:u8, unknown:bool-u8, cleanup:code-or-0:u8`.
For a validated profile-2 request, Failure is those three bytes followed by
`version:u8=1, conflict-tag:u8, conflict-fields, stage-tag:u8, stage-fields`.
The generic frame admits legacy length 3 or history lengths 6–482; profile-aware
decoding retains the exact legacy three-byte bound. All integers are big endian.
No trailing bytes or obsolete shapes are accepted.
The request profile selects the decoder; there is no response-driven downgrade.

Conflict tags: 0 none; 1 BranchMoved (expected and actual optional Commit IDs,
then expected and actual Layer base IDs); 2 StackMoved (expected/actual Layer
IDs); 3 StageChanged (expected token:u64, optional actual token); 4 BaseMismatch
(selected Commit base and Branch base Layer IDs). Optional values use 0 or 1
followed by the fixed-width value. Typed identity tags and token ranges are checked.

Stage tags: 0 unobserved; 1 absent (Workspace ID); 2 retained (complete StageWire);
3 acknowledged with unknown final disposition (complete StageWire). A retained
stage is the row observed in the deciding transaction after definite refusal and
successful cleanup. Unknown transaction outcome cannot claim retention. Composite
Commit carries its acknowledged stage even when catalog admission fails, using
tag 3 unless the deciding transaction provides a more precise observation. No
later mutable reread supplies context. Failure context is boxed in the bridge DTO;
legacy bytes remain identical although the Rust Failure value is no longer Copy.

## Root descriptors

BranchSnapshotWire appends an optional root serial (0 or 1 + u64). GetBranch
requires a present serial validated by opening the captured effective root through
C1 after releasing C5. The existing root/profile/scope fields form its descriptor.
Fork returns the coherent metadata snapshot with absent serial; it does not turn
an acknowledged metadata publication into failure by performing a later C1 read.
StackCreated (history result tag 12) is StackWire followed by the constructed
root:32 bytes and root serial:u64. Both come from successful construction and its
consumed reservation; no assumed serial 1. Other record widths remain unchanged.

## Cursor v2 and capability

Cursor v2 keeps the existing 160-byte layout: version/range (2), catalog ID (32),
incarnation (8), three length-prefixed zero-padded 33-byte fields (102), MAC (16).
The fields are subject, immutable anchor, last delivered identity/position.
Names use immutable Stack/Branch IDs and a checked name lookup when resuming;
no legal 1–63-byte name is stored in a 33-byte field. Ancestry resumes after the
last delivered record under its authenticated immutable anchor. A provided start
must equal that anchor; absent start uses it, never the moving live head. A new
explicit Commit anchor is membership-checked with the existing 4,096-row ceiling.
The keyed MAC attests to the anchor/position relationship established by the
traversal that issued it. Resume checks typed IDs and ownership without rewalking
ancestry. The 4,096-row ceiling applies to new explicit membership proofs; it
does not cap a history traversed through authenticated bounded pages.

The MAC is the first 16 bytes of keyed BLAKE3 over domain
`layerfs/history/cursor/v2\0` and bytes 0..144. An explicit nonzero 32-byte
`cursor_key` capability is supplied by the authority to create and read-only open;
no public binding/catalog-derived key, random fallback, clock, PID or registry.
Native configuration requires `LAYERFS_HISTORY_CURSOR_KEY` (64 hex digits).
The authority retains the secret outside the catalog and supplies the same key
on read-only reopen. Missing/zero capability refuses open; wrong capability
refuses existing continuations at MAC validation. Open alone cannot authenticate
a supplied key against schema 1, which stores no secret or key commitment.
Keys are redacted from Debug output and never included in replies/receipts.

## Bounds and precedence

History terminal result bytes, including tags/prefixes/cursor, are at most 16 KiB;
request metadata retains 32 KiB, page count 128 and cursor 160. Record minimum /
maximum widths remain Stack 117/179, Branch 71/166 (head adds 33), Commit 116/149,
Layer 85/168 (genesis/child), Stage 309/342. Encoders check bounds before growing
buffers and decoders before record allocation. Reservation exclusive end must be
representable within i64::MAX before high-water publication.

For add-layer: source validation, exact-source UpToDate, stale Branch/Commit/base/
stack refusal, NoChanges, immutable identity/provenance verification, insertion.
Thus an old-base sibling publication is a typed stale-base refusal before derived
identity collision checking; existing immutable rows remain untouched. Provenance
validation still applies to admissible insertion and exact-source records.
