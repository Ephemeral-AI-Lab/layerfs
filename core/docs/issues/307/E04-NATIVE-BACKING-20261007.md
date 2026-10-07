# E04 native backing and original startup custody

> Status: implemented scoped compatibility refusal and retained functional
> diagnostic; whole E04 FAILED, S7/S9 incomplete. No release qualification.

Parent `2df73fd79cb82bb6bbc0357ea8bcf4a6d03d1e50` prospectively committed
[the v2 specification](../../../../docs/roadmap/0.1/0.1.7/cluster-two-e04-native-backing-v2.md).
This continues the [original E04 checkpoint](E04-ORIGINAL-RECEIPTS-20261007.md),
not a replacement of its failed runs or numerical gates. The
[requirement ledger](S7-S9-EXIT-RECONCILIATION-20261007.md) remains authoritative
only as a working checklist beneath owner instructions and primary contracts.

## Implemented prerequisite and actual refusal

Overlay startup creates one original database file, then on Linux makes one
unconditional read-only O_NOFOLLOW reopen. Two metadata calls verify both
handles identify the same regular file with one link. One fstatfs call on that
verified descriptor records its actual type before Allocation or SQLite. Known
incompatible `fakeowner` magic 0x6a656a63 returns UnsupportedFilesystem and
retains the empty artifact. Other I/O/identity failures retain their original
cause. No range primitive, database, schema, resource window, retry, fallback,
Store placement, or third-party source was changed.

The first guard was insufficient. [Receipt11](checks/e04-native-backing-20261007/11-linux-host-share-refusal.json)
failed and retained its 192512-byte database with 268435456 allocated bytes.
[Receipt13](checks/e04-native-backing-20261007/13-new-descriptor-filesystem-diagnosis.stdout)
is a finite zero-byte cause probe: the original create descriptor reports generic
FUSE 0x65735546; read-only and write-only reopenings of that same inode report
0x6a656a63. This is why the corrected guard verifies a reopened descriptor rather
than banning generic FUSE. [Receipt17](checks/e04-native-backing-20261007/17-corrected-host-share-refusal.json)
passes with create/reopen/identity/probe counts 1/1/2/1, zero SQL/allocation,
zero logical/allocated bytes and exact retained original identity.

CreationWork records the real attempted calls and optional type. The existing
shared startup receipt grows by these fixed fields; it is not a phase-memory
bound. E04 v2 serializes the new observations; shared E01 v1 remains unchanged.
The external validator preserves v1 and adds exact native-volume custody checks.

## One original native execution

| Field | Actual selection/evidence |
| --- | --- |
| Case and profile | E04-write-16m, Disposable global Store, local disposable overlay, seed1 |
| Source | Parent2df73 plus explicit current805-file inventory in [25](checks/e04-native-backing-20261007/25-final-build-inputs.json); source_sealed=false |
| Builds | Locked Rust1.85.1 debug host/Linux examples [26](checks/e04-native-backing-20261007/26-host-final-examples.json), [27](checks/e04-native-backing-20261007/27-linux-final-examples.json), ARM64 repository config retained |
| Host binary SHA256 | e310be3a71b4117fa369a4cc377c0d62844ccdf382d9432780877c4640db998c |
| Linux binary SHA256 | 31a04d50e19e7d09dacc1ea452662dbd7f416af4804715d9a48887c5a920c2f9 |
| Image | sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6 |
| Setup | clone of closed16777216-byte base and4096000-byte replacements; same original hashes as prior checkpoint; no regenerated/mutated master |
| Cache/resources | uncontrolled; no cold, exclusive ownership, calibrated resource, or numerical claim |
| Construction | ordinary1; separately supported actual Project Init4 |
| Budget | original product80s, internal disposal88s, complete external90s; no increase |
| Raw output | core/target/cluster2-307/e04-native-disposable-20261007/ |
| External result | [38](checks/e04-native-backing-20261007/38-disposable-native-diagnostic.json): FAILED,52116019208ns, no watchdog timeout |
| Separate retained validator | [39](checks/e04-native-backing-20261007/39-original-retained-validator.json):127819292ns/9s; write-window INCOMPLETE because native mapping unavailable |

The actual pre-start observer reports ext4 magic61267 on /dev/vda1, not an
inferred Docker storage-driver filesystem. It observed database absence in fresh
volume `layerfs-e04-disposable-eb14de6b6749461a-state`. The original command,
volume creation, exact image/CIDs, mounts, fixture/assignment/acquisition and
source/binary seals were retained before consumer startup. Guest facts are not
Darwin inode, allocation or device facts.

The Linux consumer completed1000 attempted writes and1000 complete write traces.
Its original oracle checked all16777216 final bytes and EOF, mode420/nlink1,
mtime(-7,42), exact root+dense.bin membership, immutable old file root and captured
Runtime Binding equality. Final and independently expected SHA256 both equal
`7bea926791fa6462c068c4980c7c2c6f9b12b9d7a2ae50141580db3abd1af617`.
The Binding check is not a fresh Branch-history query. Consumer recorded Gone
following its ordinary250ms idle period, native fence/reassembly release and
Stop, then exited0. It used no maintenance pump. Its original post-stop guest
stat is4853760 logical and273289216 allocated bytes, device65025/inode678/nlink1.
No independent post-exit guest observation ran, so that stat is not substituted
for the missing witness or for physical I/O/phase resources.

The whole invocation failed because the host retained
`CloseFailed { original: Io(UnexpectedEof), close: NotConnected(code57) }`.
Complete encrypted-record byte accounting, no partial/undelivered input and
joined output do not turn that original close failure into successful disposal.
Host exit1 remains FAILED; no final host release/result accounting is fabricated.
The consumer's known successful mutations and oracle are not terminal unknowns;
the unavailable whole host cleanup and original failed socket close stay distinct.

A secondary helper refusal required literal Mounts.Source==host ROOT. Actual
consumer Docker inspect instead reports `/host_mnt/Users/.../layerfs` in both
HostConfig.Binds and Mounts.Source, while its original command submitted
`/Users/.../layerfs:/work`. The preparer's same submitted bind retained the
literal spelling. Original collector input hashes match the independently sealed
inputs, but the original mapping guard refused and no post-exit witness exists.
A future logical bind witness must combine exact command/CID/inspect with original
input hashes; stripping a prefix or equating host/guest inodes is insufficient.
[Diagnosis40](checks/e04-native-backing-20261007/40-failed-native-diagnosis.json)
retains both causes and confirms all805 source inputs remained unchanged.

Consumer container d79ac40f17b8632f901a69ec67caaa1168e0ca56b81bac6d2f4b03c1493334e5
is observed exited0 without OOM in the original failed-inspect receipt. The
helper's validation failure left its summary inspect state unavailable; the raw
inspect remains authoritative for that observation, not guessed successful cleanup.
The pre-start helper, original volume, Store, closed inputs and all receipts remain.
[Receipt41](checks/e04-native-backing-20261007/41-retained-native-binaries.json)
retains matching post-exit byte copies of the two original binaries. The frozen
[helper copies](checks/e04-native-backing-20261007/helpers/) retain this failed
vehicle version before later changes.

## Scoped verification and remaining work

Host Overlay61 and Linux Overlay61 functional bodies passed at their recorded
build identities. After the reopened-descriptor correction, Linux61 passed again,
with the newly selected refusal proof as one additional body; ext4 device-pressure
remained ignored. Host reran only the three affected startup bodies after a fresh
no-run build. Receipt19's formatting failure is retained; only its three displayed
format changes were applied. No failed proof, artifact or unchanged control was
replayed. Original-source hashes and binary pre/post identities are in02–20/32;
pre-format test evidence is not relabeled as a later byte-identical source.

Final host/Linux Overlay+Daemon all-target Clippy passes in28/29. Formatting34,
boundary35(736 files), tool self-tests36(41) and harness syntax22 pass. Existing
88 plus11 E04 integration and21 native witness tests pass in23/24:120 distinct
bodies. These functional/static results close no numerical or whole-milestone gate.
The unused fuser patch warning reflects its exclusion from this selected graph;
no new dependency or lockfile change is made to hide that warning.

Next scoped correction: an explicit coordinated host-first application fence using
existing Supervisor/Upstream APIs, plus independently bound original input/probe
identities for Docker's actual source domain. Original EOF/CloseFailed and truncated
encrypted input remain refusals; no relaxed clean-EOF predicate. This is diagnostic
lifecycle composition, not a replacement for R1 application admission or S8 kernel
cancellation. The fixed write workload, profiles, limits and oracle stay unchanged.

E1 samples remain zero, all27 proposals NOT_RUN, qualification NOT_EVALUATED,
E05 NOT_RUN, Durable E04 unrun, and all eleven whole-observation gaps remain.
Per-job families/credit capacity, full SQL correlation, physical/resource phases,
sustained service/debt and exact S8 ownership still prevent E2–E4/S7 closure.
S9 R1–R4/Q1 remain incomplete; owner-selected500MB dense qualification and other
scale/topology/restart/application/Save proofs are not supplied by this16MiB case.
No S10 pipeline, S11 cleanup, S12 campaign or S13 retirement work is included.

Production LOC: 170068 -> 170108 (delta +40). Core104651 ->104691;
reference65417 unchanged; active Core61486 ->61526; excluded predecessors40321,
excluded integration2844 and Content18613 unchanged.
[Exact parent/staged comparison](checks/e04-native-backing-20261007/43-production-loc-comparison.json)
exports both product snapshots with git archive and runs the unchanged
production_loc.py counter (SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb).
No source relocation, reference retirement or algorithmic simplification is claimed.
