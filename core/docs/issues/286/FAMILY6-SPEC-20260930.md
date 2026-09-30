# Family6 mixed Workspace mutations — frozen v1 selection

Functional/diagnostic registration,2026-09-30; no numeric/release admission.
Owner directed reporting the270-level chain costs to
[#276 comment5903016828](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828),
deferring that optimization and moving to Family6. No production/codec,
Budget/quota/worker/deadline or architecture change is part of this family.

Canonical module `families/workspace_mutations.py` owns exactly3 new native
profiles then4 public SDK/FUSE profiles, in registry insertion order. Reuse
existing native fixture, sealed small master, SDK shell driver, namespace
runner/proof helpers and read-only checkpoint verifier. No duplicate framework,
second Init benchmark, direct edit API, ioctl or command-specific product path.

| Ordered native selection | Oracle/custody | Functional child command |
| --- | --- | ---: |
| mixed live G1/G2 | Full mixture, old/G1 pins; held canonical response while later write/mtime/unlink/symlink move occurs; known G1 then known G2, parentage; all paths/modes/bytes/targets/aliases and exact selected mtimes; clean close | <=15s |
| mixed known/unknown failure | Two independent clones; metadata denial before canonical Commit, and lost canonical response after server Commit; held mixed view remains readable, no replay, correct actual C5 branch via public query, allocated/reserved custody stays accounted | <=15s |
| mixed known canonical/local-C5 failure/resume | Frozen Stage/mixed pin, later G2, actual allocation denial on saving thread after canonical response; same selector resumes once, no second canonical call; later G2 Commit, full oracles and checked refunds | <=15s |

Native mixture uses Workspace public APIs: create work directory/file; write
abcdefghij; overwrite offset2 with XYZ; shrink to6 then extend to10, yielding
`abXYZf`+four zero bytes; mode0600 and mtime(-17s,123ns); hardlink/rename alias
to moved; symlink target file/rename to link2; create/remove empty directory and
temporary file; overwrite inherited sibling to sibling-new. Known rmdir-nonempty,
descendant-cycle rename and directory-hardlink refusals preserve revision. G2
writes QQ at0, mtime(-19s,456ns), removes moved and renames link2→link3. Old
and mixed G1 pin trees retain full selected attributes/bytes/opaque targets.

Only external tests inject failures through the existing production-Service
fixture controller, never a product test hook. Canonical C5 branch inspection
is a read-only public HistoryQuery; it does not authorize replay after the
Workspace's Unknown result. The known-before-Commit failure may precede the
first complete file save in this mixture; its scope is checked selector/view/
physical custody. The unchanged r054 saved-root-specific proof is separately
reused, not silently replaced by that narrower assertion. Known/unknown case
intentionally retains custody, which is copied before owned external teardown;
it is not labelled checked product clean-close.

Native topology is the established Linux component functional control with
production in-process Server, not a public SDK/host-SQLite speed arm. Closed
clones, hashes/owner preconditioning, build/container acquisition stay outside
the child; actual Workspace work, in-child oracles and product cleanup/refusal
remain inside. Container/volume cleanup is separately recorded and checked.
Four independent small-master clones cover the three native profiles.

| Ordered public SDK selection | Ordinary POSIX-FUSE command/result | Complete SDK command | Separate verifier |
| --- | --- | ---: | ---: |
| mixed ordinary | mkdir/create/write/offset overwrite/shrink/extend/chmod/hardlink/rename/symlink/remove/rmdir, inherited sibling overwrite and fixed UTC mtime | <=15s | <9s |
| retained G1/live G2 | Same mounted Workspace: full mixture prelude, Pin G1, known prelude Commit, later overwrite/alias removal/symlink move/mtime, final known Commit; full10-byte held G1 file | <=15s | <9s |
| known POSIX refusals | rmdir nonempty, move ancestor below descendant, directory hardlink all refuse; assert old live names/bytes and no forbidden bindings, then known Commit of accepted creates | <=15s | <9s |
| shell exit7/no Commit | Create private uncommitted directory/file then explicit exit7; exact known exit7, no Commit; canonical head/tree unchanged; checked unmount/delete | <=15s | <9s |

SDK ordinary mixture creates work/file=abcdefghij, writes XYZ at2 with ordinary
dd, shrinks6 then extends10 (zero fill), chmod0600, creates hardlink alias,
renames it moved, creates/moves symlink target file to link2, creates/removes
work/empty and work/remove, writes sibling-new, and sets work/file UTC
2020-01-02T03:04:05 (1577934245s,0ns). Symlink traversal and alias bytes are
checked in Exec. Retained G2 writes QQ at0, removes moved, moves link2→link3
and sets UTC2020-01-03T03:04:05 (1578020645s,0ns). Fixed independent byte
oracle is `QQXYZf`+four zero bytes; no candidate trace generates the oracle.

Full old/new manifests encode directories/files/symlinks, exact modes, regular
file bytes and complete opaque target digests. Read-only independent verifier
checks complete listings, every mode and byte/target, known head and parentage,
unchanged inherited inode IDs, hardlink same-serial/exact reference counts2→1,
and exact old/new selected mtimes. Symlink support is permitted only by explicit
expected-kind manifests; unexpected symlinks still fail a type mismatch. SDK
failed-shell proof checks the retained canonical head and old complete tree
without requiring advancement. Accepted private shell operations are not rolled
back by shell exit; they are discarded by checked unmount, never implicitly
committed. Verification and raw performance are separate invocations; no replay.

Performance headline is SDK driver launch-to-exit: host Server/SQLite,
Sandbox/Mount, arbitrary Exec, all declared Pin/Commit, Status, lease release,
unmount/delete/logs/shutdown. Record Exec/Commit/prelude intervals separately.
SDK runs host Server/SQLite and Linux daemon/FUSE/workload. The four-byte and
ten-byte bodies do not supply a throughput claim. No comparative baseline or
ratio is frozen; one sample per case/arm. All cache domains are uncontrolled,
`numeric_latency_status=INELIGIBLE`; clones are setup reuse, never a cold claim.
Sampled shared-process metrics and lifetime cgroup data are not phase-memory gates.

Reuse the already closed byte-sealed native small master and existing once-
prepared SDK small master/.marker starting Commit; independent writable copies
for each selection, no fixture regeneration or mutated sample reuse. Locked
release binaries only; worktree-local target/ARMv8 flags/dependencies unchanged.
Daemon/image reuse requires exact hashes. New SDK receipt records exact exit
status, and optional verifier properties add aliases/mtime without changing
the product operation or earlier profile IDs/bounds.

Explicit relevant proof reuse: r058 live SDK stopping/held-FD known Busy/Io and
checked release/unmount/delete; r054 known C1 saved-root and32-lease forged/
stale/unissued authority. Registry carries source commit, raw manifest and
compact-receipt hashes; retain old source/artifact labels. These paths did not
change in production. All earlier families1–5 retain their existing relevant
proofs, including the entire F2 group; zero earlier benchmark reruns.

Frozen commands (fresh append-only outputs):

```sh
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-mutations-native --out benchmark-results/fs-bench-pro/issue286-workspace-mutations-native-r067
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-mutations-sdk --out benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-r068
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-r068 --out benchmark-results/fs-bench-pro/issue286-workspace-mutations-sdk-proof-r068
```

Record every failure/unrun/ineligible line and all seals/commands/bounds with
`benchmark_agent_report.md`, append log and report, commit/push/comment#286.
Product LOC uses exact first-parent/final staged source snapshots; expected
Family6 production delta0 (tests/examples/harness/docs excluded). No package-
scale or10240 scalability qualification; those architecture issues and the
270-level optimization remain deferred under#276. Family7 shell/package follows.
