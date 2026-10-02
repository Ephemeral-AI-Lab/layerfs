# Strict-v2 independent fixture acquisition

Status: Dated planning checkpoint; not release evidence or a product contract.

All product files were archived from `285dd3f4a54a896344e389b51a8495300b98b4ca`
inside the owned worktree `target/sp1-oracle-source`; the archive uses its own
`core/target`. The source inventory proves 360 product, shipped SQL and build
input files match their exact Git blobs. No candidate product source supplied
any expected canonical identity or selection. `manifest.json` pins the complete
acquisition source, binary, outputs and independent old C2 codec inventories;
`dual-use/manifest.json` separately pins the additional one-record FULL body.
Current opt-in acquisition test style can differ from archived `acquisition.rs`
after Clippy corrections; the archived source hash remains authoritative for
these bytes. These are fixture acquisitions and count diagnostics, not speed
samples, release SDK Init measurements or strict runtime PASS evidence.

The old `81f2cf2` fixture/evidence was restored without its checklist/log. Its
producer profile and hashes retain their historical scope. This strict-v2 source
adds the actual regular-file CDC origin, portable metadata graph, threshold and
history content roots, fixed work/depth boundaries, exact same-save stream and
pure physical Metadata/FilePayload streams. `domain-packs` encode presealed
canonical bodies using the unchanged old producer, after independent producer
emission inventory classification. Original `vectors/filesystem-producer.sqlite`
has mixed Native records and cannot qualify as strict physical placement; use
`domain-packs/metadata.sqlite` and `file-payload.sqlite` instead. Pool leaves are
requested and hashed independently; acquisition does not claim intermediate
pooled-leaf canonical hashing inside one dependent read.

Initial acquisition compilation failed because the external harness called
nonexistent `AdvisoryPredecessors::iter` and borrowed a temporary Provider. The
harness was corrected to `ids()` and a named Provider. A subsequent shell edit
used an archive-relative path from the archive directory, failed to modify the
file, and redundantly reproduced that same compile failure; no product source
or expected output changed. Initial acquisition then passed. The harness was
extended to acquire the missing physical/full and same-save outputs, preserving
all initial output directories. Final frozen harness acquisition executed all
three ignored targets once (`3 passed`, 0.10s diagnostic wall); existing semantic
canonical/raw/TSV outputs were compared byte-for-byte to prior independently
acquired outputs. No performance number was selected or measured.

The additional dual-Chunk physical target first failed an external assertion
about ObjectId Debug formatting (quotes were omitted); its failed output directory
remains under the owned target. Corrected assertion and fresh output passed in
0.01s diagnostic wall. It consumes the already pinned canonical object and
encodes one FULL; it does not regenerate any canonical expected bytes.

The first actual CDC range is `[0,16396)`, canonical length 16417, ID
`a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399`.
Ordinary `construct_stream(N(200000))` and attribute `emit_value` for `sp1/opaque`
emit the same Chunk canonical identity/bytes. Their default mapping bodies
are different (multiple regular-file extents versus one attribute extent).
A same-FileState/Extent-ID reference-use witness therefore remains separately
required; it is not implied by this equal Chunk proof.

Full filesystem roots here require scope `[0x53;32]`, root serial1 and fixed
file serials2/3/4/5, deterministic mode0640/directory0750/mtime1700000000000000000ns
and supported ordinary C1 graph construction inputs. Public FUSE delivery must
independently establish those metadata inputs before claiming equal complete
filesystem roots. Expected content bytes/partitions are independently pinned
regardless. Provider authenticity, actual SQL-only metadata read GET delta0,
writer/private eligibility, Unknown custody, resource attribution and the
Linux/FUSE public route remain separate runtime gates.
