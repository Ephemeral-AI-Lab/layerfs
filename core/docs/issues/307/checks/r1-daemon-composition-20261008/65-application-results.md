# R1b direct-Store daemon application checkpoint

> **Status:** Implemented application checkpoint; R1 remains IN_PROGRESS.
> Input `932abe80894e070d4767eba2c213dc5d38a64d8f`; source identity [44](44-session-fence-source-identity.json).
> No release, performance, native FUSE Ready or complete R0–R9 acceptance claim.

The actual Linux daemon binary loads protected config, starts one Overlay owner,
admits a single original installer, transfers the direct opened Store into the
existing Service, and handles correlated startup/session/ordinary filesystem
controls. The SDK/Bridge add Hello and EndSession while preserving existing
request/reply tags. Control-only Commit validates original token/custody then
refuses before capture/Save/epoch effects. No placeholder namespace producer,
host data server, daemon command supervisor or caller-process cancellation appears.

| Final/reused check | Outcome and scope |
| --- | --- |
| macOS/Linux locked all-target builds, selected Bridge/Daemon/SDK packages | PASS [53](53-final-host-build.txt), [46](46-session-fence-linux-build.txt); compilation first, tests/examples covered by build |
| macOS/Linux warning-denying all-target Clippy, selected packages | PASS [54](54-final-host-clippy.txt), [52](52-final-linux-clippy.txt) |
| Formatting / product boundary / tooling self-tests | PASS [55](55-final-format-check.txt), [56](56-final-boundary.txt):730 production Rust/SQL files, [57](57-boundary-tests.txt):46 tests. Guard remains textual, not semantic/native proof |
| New private config/startup/session records | PASS2 [14](14-daemon-records-tests.txt); codec behavior unchanged afterwards, comment/admission/application/fence changes separately covered |
| Existing control/install wire records, native channels, actual traditional installer | PASS2+2+6+3 [58](58-control_records-tests.txt), [59](59-install_records-tests.txt), [60](60-native-tests.txt), [61](61-native_install-tests.txt); commands/binary identities [62](62-functional-test-identities.json) |
| Control constructor unavailable / original unknown precedence | PASS1+1 [25](25-control-no-producer-tests.txt), [26](26-control-unknown-tests.txt). No Store SQL/reservation/demand or registry/engine state change for refusal. Earlier6 passing cases in [15](15-control-tests.txt) retained/reused for unchanged operations; overall15 remains FAILED6/7 |
| Actual final Linux daemon binary proof | PASS1 [49](49-final-native-application-proof.txt), source hashes verified at runtime; protected config, actual install/Hello/bind/status/history/logical unmount/session end and underlying-owner quarantine |
| Owned proof artifact cleanup | PASS [63](63-owned-native-cleanup.json):only two exited successful owned containers and their fresh named volumes removed once; inspections [38](38-native-container-inspect.json), [50](50-final-native-container-inspect.json) retained |

All Store executions explicitly select Disposable/WAL/OFF. Overlay is separately
MEMORY/OFF/EXCLUSIVE. Durable execution is `NOT_RUN — disabled by owner until
explicit reauthorization`. One construction worker, ARM64 build flags from the
unchanged root Cargo config, locked dependencies; only the existing nix0.31.3 edge
is activated, with no new package version or third-party source edit. Fuser is
inactive, so its unused-patch warning is retained and no native fuser build occurs.

Final Linux test binary SHA256:
`287bc3026303f040421b855a0287ccdfaa2e105480a38c17f761ede14bffe005`.
Daemon binary SHA256:
`ef5d18523a9f30da6197e7c1590140a4b00055b1c7c767e7be8bf6cfe701f518`.
Image SHA256:
`378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Linux controller Init and the separate daemon both report SQLite3.53.2. This is
Linux-controller application proof, **not** macOS-to-Linux provider qualification.
Traditional macOS installer/root oracle reports SQLite3.51.0 and covers every
regular byte (300048B), six paths, hardlinks, raw symlink and absent original source.
It is a separate library/component scope, not mounted filesystem coverage.

Test invocations have explicit100s host stops; the actual Linux application test
has a9s complete functional bound. Its57,503,083ns recorded wall is diagnostic
only, with natural functional caches and no cold/residency/timing/storage gate.
No performance campaign/resampling, numerical PASS or speed/storage claim is
made. Builds are not test execution. One early host build and Linux builder
overlapped on independent platform targets; no timing comparison uses them.

Original failures remain append-only: first private visibility errors [02](02-first-build.txt),
a generated syntax delimiter error [03](03-repaired-build.txt), test fields absent
from StoreWork [07](07-host-build.txt)/[09](09-host-build.txt), the retained test
completion refusing terminal Close [15](15-control-tests.txt), and Linux-only test
Installed wrapper field errors [24](24-linux-build.txt). Source-directed repairs,
not timeout/retry/profile changes, produced later scoped checks. The earlier
successful binary proof [37](37-native-application-proof.txt) is preserved;
[49](49-final-native-application-proof.txt) reruns only because SDK session fencing
and its underlying-owner test changed. No test reached a wall ceiling.

Remaining scope: actual SandboxApi/backend lifecycle, true ordinary command
streams/status/cancellation/access proof, native mounted Ready/mutations/coherence,
live namespace normalization/Commit, multiple native Workspaces/processes and
application stop/fence/join. Retained malformed/product faults can fill declared
finite slots until explicit custody transfer/lifecycle termination; adversarial
admission/diagnostic-transfer qualification is open. Root reference and excluded
predecessors remain intact. R7/R8/R9 acceptance/retirement stays unrun.

Exact production LOC:166022→167277 (delta+1255), core100605→101860,
active57731→58986; reference65417/excluded predecessors36325/excluded
integration6549 unchanged. The [comparison](67-exact-production-loc.json)
uses the pinned counter over exact parent/staged product snapshots; documentation,
tests and receipts do not contribute. New assembly/custody implementation explains
the increase; no legacy code was relocated, deleted or relabeled as simplification.

Full staged whitespace check is FAILED(exit2) for original raw test stdout only:
eight receipts have trailing blank lines;15 additionally has trailing whitespace
on its original failure line. [Full output](68-full-staged-whitespace.txt) is
preserved exactly. [Scoped source/docs check](69-source-doc-whitespace.txt) is PASS
with only this campaign's raw.txt receipts excluded. This is not a full-check PASS.
