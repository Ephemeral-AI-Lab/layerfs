# R2 native directory ownership and cookie component

> **Status:** Verified component on parent1da897903. Full R2 Goal remains ACTIVE.
> Kernel dispatch/replies, mount Ready, permissions and complete drain are open.

Schema19 implements exact directory opens, independently retained read sources,
indexed cookie/name boundaries and bounded live cleanup on the existing Overlay
owner. Workspace reuses its64-key merge outside SQL; local kinds come from the
owner page and immutable kinds from canonical inode records. READDIR needs no
file lengths or portable attributes and acquires no kernel lookup references.
See [architecture](../../../../architecture/74-native-directory-custody.md).

Cookie preparation reserves disjoint IDs and reuses published names. Reservation
is allocation, not enumeration progress: only the prefix accepted by a reply
buffer publishes valid mappings. Concurrent alias cookies remain valid. Old
offsets still denote the original name boundary after removal/rename. RELEASEDIR
refuses new reads but preserves existing sources; the last source wakes automatic
64-row cookie cleanup during a live mount. Closed headers may outlast logical
revocation and fence physical namespace deletion. Directory moves update retained
parent identity in the existing namespace transaction, without an open-handle
scan or extra parent query on ordinary file/name operations.

| Selection | Evidence | Actual outcome |
| --- | --- | --- |
| Initial host component cases |07/08 | Native directory/lookup PASS; old64-page engine fixture FAIL during schema startup |
| Pressure fixture repair |09analysis;12/13 |96-page setup reaches unchanged real SQLITE_FULL mutation/rollback cases; PASS |
| Removed directory enumeration |12/13;17/18 | Original PathNotFound FAIL retained; owned proved-empty EOF repair PASS |
| Parent/read review |33–44 | Atomic retained-parent update and removal of redundant parent query PASS host/Linux |
| Kind demand |45–55 | Canonical inode-kind demand passes with the file-length provider unavailable |
| Host covering evidence |[summary60](60-host-summary.json) |91tests PASS across21selected binaries, reusing unaffected earlier cases at their declared scope |
| Linux covering evidence |[summary61](61-linux-summary.json) |92tests PASS across22selected binaries, with the same scoped reuse |
| Builds/examples and Clippy |02/04/10/15/21/33/45 builds;47host Clippy;29/41/52Linux | Locked all-target Overlay/Workspace/Daemon builds and warning-denying Clippy PASS |
| Formatting/source/tooling |56–59 | PASS;780product Rust/SQL files;48tool tests |

All test invocations had explicit100s wall limits, except the actual Linux daemon
application case's9s limit. No timeout occurred. The application initially passed
in200794709ns and its necessary later parent-field/schema-compatible check passed
in202159167ns; these are separate source identities, not resampled performance
arms. The final kind-only change affects no application-startup/control behavior,
so that application evidence is reused. Original failures, selections, exact
commands, binary hashes, limits and raw receipts remain unchanged.

Tests verify130cookie rows retiring in bounded live turns, stable published-name
reuse, concurrent aliases, partial/empty accepted prefixes, invalid unpublished
offsets, source access after RELEASEDIR, zero final namespace/owner resources,
foreign handles, and actual indexed plans paired with DatabaseWork. Canonical
Workspace tests cross two empty64-whiteout windows before visible names, use old
offsets after deletion, retain removed-directory metadata and observe a moved
parent through the existing atomic rename. The actual Daemon/installed-Store test
retains original Completion credits, performs no Store I/O on SQL-owner steps and
reaches automatic final cleanup. No private product source is included in tests.

The [final source identity](49-kind-source-identity.json) is unchanged after
verification. Initial and intermediate pins26/38 and their raw results retain
their narrower/wider exact behavior; [failure/review analysis](09-failure-analysis.md)
explains each follow-up. Natural caches and functional component scope only:
no numeric latency, storage-efficiency, cold eligibility or resident-memory
qualification. No new SDK benchmark or mounted native acceptance was run.

Global Store fixtures explicitly use Disposable/WAL/OFF; Overlay uses
MEMORY/OFF/EXCLUSIVE. Durable: NOT_RUN — disabled by owner until explicit
reauthorization. Linux construction workers1; reused pinned image and worktree
Cargo targets, fresh fixtures outside the repository bind, serialized worktree
lock. The application test remains native_fuse_ready=false. All three acknowledged
owned containers were removed: [32](32-linux-cleanup.json),
[44](44-final-linux-cleanup.json), [55](55-kind-linux-cleanup.json). Unrelated
resources and owner notes remain preserved.

Production LOC:172929 ->173827 (delta+898). Core107512→108410; active64638→65536;
excluded predecessors37431, excluded integration5443 and root reference65417
unchanged. [Exact parent/staged count](63-exact-production-loc.json) and
[per-file classification](64-per-file-production-loc.json) use the pinned counter
and the same Rust/runtime SQL scope, excluding tests/docs/tools/third-party code.
There was no relocation, reclassification or retirement.

Next R2 work is activation of the real replacement Fuse service and its shared
fixed K dispatcher, actual deferred Owner/Store ports, mount/profile/session
integration, Attach/Locate/Ready, Sandbox permissions, reversible Busy and complete
normal drain. Native receive/reply buffers must consume the exact cookie APIs and
preserve their original completion/source custody. This component's passed tests
do not complete or pause that full Goal; R3 kernel coherence and R4/R5 integration
remain separate unfinished work.
