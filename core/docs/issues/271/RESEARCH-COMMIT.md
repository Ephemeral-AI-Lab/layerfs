# Issue 271: Commit work after separated mounted writes

> **Status:** Research; informative and not a product contract.

This source audit uses the retained [four-hop 4,097-write diagnostic](FOURHOP-4097-EXTENDED-DIAGNOSTIC.md)
and its [raw receipt and log](evidence/fourhop-4097-extended-v1/run/).
The source under study is four-hop product commit
`933b3457c916519f458137e4c4f19662c8e9228e`, product seal
`7e1e36eed0920a717284c277e61ed983d12d04a3669450300e3402bdb2f144fa`.
The diagnostic has uncontrolled cache, so its times locate work in that run;
they are not qualified latency comparisons.

## Work and crossing model

Let `W` be acknowledged writes, `E` final ordered extents, `R` changed runs,
`S` replacement bytes, `P` distinct Local payload files, and `F` the bounded
Bridge body frame size. This fixture has `W=4,097`, `E=8,194`, `R=4,097`,
`S=4,097` and `P=4,097`; these facts follow from the independent verifier,
`LFS_PIECE_LOWER`, `LFS_FILE_INPUT` and one payload per accepted WRITE.

One SDK Commit call reaches the daemon's native
[`Workspace::commit`](../../../crates/layerfs-workspace/src/commit/operation.rs).
Its dirty walk selects the one file. `lower_file` checks the frozen final
sequence with one ordered cursor to establish exact `E`, `R` and `S`
([source](../../../crates/layerfs-workspace/src/commit/lower.rs)). The
[`FileUpload`](../../../crates/layerfs-workspace/src/commit/upload.rs) makes
another ordered descriptor walk and emits `24E` bytes; its
[`ReplacementSource`](../../../crates/layerfs-workspace/src/commit/source.rs)
walks the same frozen sequence for the `S` replacement bytes. Each Local
extent opens and authenticates its owned payload before reading one aligned
4 KiB page ([reader](../../../crates/layerfs-workspace/src/backing/reader.rs)).
The SaveFile Bridge request therefore has one request/reply, `24E+S =
200,753` body bytes and a source-derived roughly 13 full-or-tail 16 KiB body
frames. Its exact frame count is not exported. It has no per-edit Service RPC.

The host validates that stream *before* beginning the C2 save
([server](../../../crates/layerfs-server/src/service/save/content.rs)).
[`file_stream::read`](../../../crates/layerfs-server/src/service/save/file_stream.rs)
parses `E` descriptors and spools `R` 32-byte edit records, currently using
four eight-byte `write_all` calls per record. The C1 EditSequence then makes
record lookups/reads; the diagnostic counted 16,389 of each, about four per
changed run. The one file SaveFile is followed by portable metadata and one
History Commit; prepared result and successor keyed roots are also updated.
Commit does not replay the `W` individual extent publications made during
Exec.

This route is `O(E+R+S)` for ordered traversal, validation and transfer,
`O(P)` authenticated private-file opens and aligned reads, `O(R)` spool
records with four write calls each, and `O(ceil((24E+S)/F))` Bridge frames.
The Service request count is constant for this one-file case. The last two
costs are avoidable constants, while a representation of `R` separated changed
runs still needs `Ω(R)` description work. This model does **not** imply a
global `O(W²)` Commit. Multi-file and large-payload profiles have different
`P`, `E` and `S`; this fixture cannot prove their latency.

## Measured bounds and missing attribution

The raw SDK Commit took **0.434905 s**. LFT1 timed daemon `WorkspaceCommit`
at **0.433811 s** and its SaveFile at **0.380368 s**; the host's SaveFile
took **0.376547 s**. Host children `service.begin_save`, `service.save_file`
and `service.finish` took **0.002736**, **0.014798** and **0.001258 s**.
The remaining **approximately 0.358 s** is an *unattributed span*, not a
measured spool or transport phase. It includes the pre-save stream/parse and
any other work outside those children. The 16,389 edit record reads occur
inside the 0.014798 s C1 child, so changing those alone cannot explain the
larger span. The daemon reads `P` private files while generating the stream;
the host spools the records; existing timers cannot divide the span between
them.

Halving Commit itself saves about **0.217 s**. That is worthwhile for Commit
latency, but only **0.67%** of the 32.422 s diagnostic complete command.
Any proposal for a 2× combined command must also remove Exec work.

## Candidate changes, in order of proof cost

1. **One spool write per edit.** Encode four words into one 32-byte stack
   record and call `write_all` once in
   [`close`](../../../crates/layerfs-server/src/service/save/file_stream.rs).
   This removes 12,291 write calls at `R=4,097` with unchanged spool bytes,
   memory bound and wire format. Its wall saving is unknown; the pre-save
   attribution below must precede a speed claim.
2. **Share tiny-payload physical storage.** A new authenticated pack format
   could turn `P` private file opens and 4 KiB direct reads into fewer
   contiguous reads. It could also reduce Exec's per-write creation cost.
   Independent ownership, per-payload quota/refunds, partial-failure cleanup,
   old-root reads and bounded residency require a new proof and versioned
   format decision. Larger payloads may not benefit.
3. **Change the streaming shape only with a separate contract.** Descriptor,
   replacement and host validation are real Commit work. Increasing Bridge
   buffers, warming payload pages, keeping all records resident, or moving
   parsing before the Commit timer would change the measurement or resource
   contract. A bounded format reform needs its own source and scenario.

## Next causal diagnostic

Add a host LFT1 child around `file_stream::read` and its declared end-of-input
check, plus bounded cumulative counts/time for spool calls. On the daemon,
count and time descriptor cursor reads, private payload opens and aligned
reads, replacement bytes, and actual Bridge body frames within the one
SaveFile. Record sums with scope and clock domain; do not subtract clocks
across host and daemon as if synchronized. Run one prospectively named
count-driven diagnostic at a new source/harness identity, retaining the
original 25 s gate FAIL and uncontrolled-cache `INELIGIBLE` status.
