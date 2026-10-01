# Full deepseek-harness backend import experiment

Status: Research; informative and not a product contract.
Owner2026-10-01 explicitly includes everything under deepseek-harness: hidden,
.git, ignored/generated artifacts and all symlinks. No filtering or sampling.
This is standalone MinIO+SQLite backend import, not SDK init_namespace, certified
filesystem-root/Commit history or a mounted Workspace. No#288 campaign.

## Qualified closed corpus

Source /Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness copied once to private
benchmark-results/storage-probes/deepseek-full-master-v2/tree. Regular files103108,
directories16868 including root, symlinks10070, logical file bytes3475776149,
hardlink aliases0, xattrs130046. Closed manifest75501568 bytes, SHA256
541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af;
ordered entry/xattr transcript SHA256
c6da088ccba1875d34e5d2a512896d162a7812466c9a57f9396c5aa3798e961f.
Acquisition51.100994s, independent byte copies, no APFS clone/source hardlinks;
source untouched, symlinks retained without following. Native Darwin read-only
xattr APIs capture opaque names/values; per-file stable stat during read checked.
No atomic multi-file live-source snapshot or ACL/ownership-restoration claim.
Initial acquisition failed before file content on Python's missing listxattr;
retained rootv1, demonstrated correction uses native API, fresh rootv2 qualified.

## Stages and exact operation

One C1/C2 preparation diagnostic constructs every regular file with public
construct_stream/default policy (whole-file below128KiB, real CDC8–32KiB for
large files) and FULL encoding. Exact CAS membership is an indexed SQLite table,
with paged metadata, bounded lane tails and existing groups/pack formats; no
population vector or recursive history. No deltas/predecessors for fresh import.
Record source-read/construct/encode/metadata/pack output wall/counts, actual SQLite
version, buffer/window arithmetic and full prepared artifact hashes. All entries
and opaque metadata copied into private catalog, file content roots appended.
One producer, C2 group target48KiB/ceiling64KiB and packs256KiB, independent
physical locators. Private staging uses MEMORY/OFF,512KiB cache, no product edits.
Preparation limit180s; labeled diagnostic, not speed admission or credited as an
upload-arm construction cost. Required source work remains visible in this term.

Selection I-deepseek-full-prepared-v1 uploads every sealed pack from that prepared
state through one owned native MinIO server and four bounded upload connections.
Outstanding requests capped8, bodies <=256KiB each. This is one construction
producer plus network overlap, not four construction workers. Record exact PUTs,
encoded/logical bytes, ACK counts/walls and catalog finalization. No retries or
query-based guessed adoption. Global catalog switches through Python's sealed
SQLite3.51.3 to WAL/FULL/fullfsyncON; one ready/publication marker commits only
after all expected uploads acknowledge. Final checkpoint/close work is included.
Catalog is actual local metadata plus file content roots/locators, with no
canonical namespace root or real Branch/LayerStack claim. All source references
remain private until this publication marker; unacknowledged outcome remains
PARTIAL/Unknown, never guessed success or automatic resend.

Complete upload+catalog performance command <=25s, prospectively declared large
corpus exception to15s. Do not omit files, enlarge deadline or repeat unchanged
arm for better speed. Source clone/setup is not a cold claim. Numerical cache
INELIGIBLE/performance_claim=false; no physical cache/peak/I/O or product speed
admission. Report preparation plus upload walls separately; sum is informative
pipeline composition, not a measured SDK Init/full cold bootstrap PASS.

## Proof, custody and retention

Separate10s proof downloads all expected packs (four bounded connections), checks
original sealed bytes/hashes, then public C2 decode/C1 read reconstructs every
file against independent original fixture bytes with bounded comparator. Catalog
entries/xattrs compare exactly with closed source manifest; symlink targets,
portable mode/mtime, uid/gid/flags and aliases recorded. No file-body oracle from
candidate output. No mounted syscall/OS bootstrap execution proof is claimed.
If exhaustive proof exceeds its bound, retain observed coverage/status and do not
claim PASS. A necessary narrower diagnostic is separate, source-pinned/count-driven.

All imported pack objects/catalog are user-requested deliverables and retained;
cleanup means stop only owned MinIO process and retire transient workers/handles,
not deleting imported content. Keep credentials/private startup logs and source
manifest/content out of public evidence. Publish only hashes/counts/seals/status.
Temporary disk admission16GiB for qualified3.48GB source plus private stage,
proof download and catalogs; before effects check available capacity, count actual
sizes and fail if stage/proof pack bodies exceed4GiB each. Native/OS page cache is
unavailable for bounds; no heap-only memory claim. This is an explicit larger
backend corpus profile, not activation of deferred product Workspace counts.

One sample, fresh output, exact source/build/provider/corpus seals, all failures/
unsupported observations retained. Product source and source repository remain
untouched. Commit notes/source/evidence with exact productionLOC and update#291.
