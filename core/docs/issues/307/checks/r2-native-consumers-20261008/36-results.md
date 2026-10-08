# R2 native consumers and directory callback checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This checkpoint fixes full-handoff SQL credit starvation and wires the native
directory consumer, READDIR/RELEASEDIR callbacks and file/directory GETATTR handle
association. Full R2 remains incomplete. The owner subsequently requested a stop
at this verified checkpoint and an actionable handoff; no mount/session/Ready
implementation checkpoint is started after that request.

The original [pressure test](02-pressure-test.txt) failed with16 admitted/parked
native requests,16 retained SQL completions, an empty SQL queue and an idle Store
reader. Each metadata consumer was waiting for another ordinary job slot while
retaining the metadata completion occupying its current slot. This was a real
dependency cycle, not slow I/O. READ/READLINK now consume every metadata answer
projection and its original Completion before requesting the local data window.
Independent FileRead/source capabilities remain retained. Owner limits, worker
count and request limits are unchanged. [The repaired suite](05-consumed-answer-tests.txt)
passes, including the full16-slot case. The failure was a bounded observation
failure, not a120second ceiling timeout; its exact command/wall/status are in
[receipt03](03-pressure-result.json).

Directory reads consume bounded original replies before subsequent SQL jobs.
Request-owned copies retain exact read/cursor and cookie-reservation values; a
shared Arc alone is not used to release original completion credit. Each offered
batch is consumed by one prefix publication. Zero/partial prefixes, old published
cookies, rejection of unused reservations and reads after RELEASEDIR are covered.
All16 offered batches can remain alive while publishing at the unchanged limits.
The callback bounds encoded reply data to128KiB; continuation cookies permit
further calls without a total directory-size limit.

GETATTR handle association uses one indexed union over native file and directory
associations, followed by atomic source acquisition. It does not retry a failed
file-kind lookup as a directory-kind lookup. Original read/data inputs, directory
offsets, failed page/cookie responses and requested cookie prefixes remain in
failure custody. Source tokens remain retained when release fails after payload
consumption. No destructor issues SQL or guesses cleanup success.

| Verification | Evidence and result |
| --- | --- |
| Host consumer suite | [4 PASS](11-directory-tests.txt); the expanded16-batch/zero-prefix directory case also [PASS](16-directory-pressure-test.txt) after its scoped test extension. |
| Linux consumer suite | [4 PASS](linux-port-tests.txt), real Owner/Store composition, full16 metadata pressure,128KiB mixed inherited/local data, exact original failure and directory-prefix/close cases. |
| Locked builds | Host build receipts and [final Linux Fuse](linux-final-fuse-build.txt)/[Daemon test](linux-final-port-build.txt) PASS. |
| Warning-denying Clippy | [Host](32-final-host-clippy.txt) and [Linux](linux-final-clippy.txt) PASS for Fuse/Daemon/Overlay all targets. |
| Format/guard/tooling | [Format](29-final-format.txt) PASS, [804 production files](30-final-boundary.txt) PASS and [49 tooling tests](31-tooling-tests.txt) PASS. |
| fuser provenance | [PASS](linux-final-fuser-integrity.txt);104 manifest/lock/config inputs, authorized bytes unchanged. |

[Identity18](18-source-identity.json) pins the semantic consumer source;
[final identity23](23-final-source-identity.json) adds only the native directory
reply-size bound. [Change scope24](24-final-change-scope.json) declares reuse of
unchanged semantic cases and final Linux compile/Clippy coverage for that callback
change. Binary hashes and explicit100second test ceilings are in the raw receipts.
Original compile error07 (borrowed iterator lifetime) is retained; the dot iterator
now owns its fixed offset rather than borrowing the stream. No runtime ceiling
expired. Unchanged dispatcher, attribute and lower-level engine-family evidence
from the preceding checkpoints is reused at its original scope.

All checks use natural caches and establish functional component behavior only.
Actual kernel replies, native Ready, permission isolation, reversible EBUSY and
complete normal drain remain NOT_RUN. No latency/storage/residency claim is made.
Global Store fixtures explicitly select Disposable/WAL/OFF; Overlay uses
MEMORY/OFF/EXCLUSIVE. Durable is NOT_RUN — disabled until explicit owner
reauthorization. Store fixture commands set `LAYERFS_CONSTRUCTION_WORKERS=1`.

[State receipt33](33-state-and-runtime.json) records Darwin25.4.0/ARM64,
Rust1.85.1, Docker29.5.2/ARM64, preserved owner notes and the4 unrelated containers.
Both owned Linux check containers are removed. The failed pressure fixture is
retained at `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`,
with file identities in33. Its component test exited; there is no live native
mount or process owning it. It is failure evidence, not an active Store to adopt.

Production LOC: **176088 -> 176905 (delta +817)**. Core110671→111488,
active67797→68614; excluded predecessors38878, excluded integration3996 and root
reference65417 unchanged. There is no relocation or retirement in this commit.
[Exact accounting34](34-exact-production-loc.json) and
[per-file classification35](35-per-file-production-loc.json) use the pinned
counter over independent parent/staged first-party Rust/runtime SQL snapshots,
excluding tests, docs, tooling, third-party and generated source.

Current implementation and its remaining native integration boundary are described
in [architecture75](../../../../architecture/75-native-request-service.md). The
forthcoming repository handoff records the exact committed identity and owner
pause boundary; this component result does not mark R2 complete.
