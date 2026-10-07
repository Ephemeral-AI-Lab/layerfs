# R1b deepest-file production daemon composition

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Input `932abe80894e070d4767eba2c213dc5d38a64d8f`, 2026-10-08.

Main owner implements/verifies; agents review source/primary docs only. This
checkpoint creates the actual daemon application and its prerequisites, not a
second Commit driver or placeholder native/namespace constructor. R1 remains
open until actual Sandbox/runtime/access composition and proof.

| Deepest home | Reuse/change/new | Operation/custody/cost/proof |
| --- | --- | --- |
| daemon `control/operations.rs`, `registry.rs`, `serve.rs` | Change existing sole control owner | Split genuine constructor Commit from control-only dispatch. No-constructor Commit validates original token/custody then refuses before admit/capture/Save/epoch effects. One shared receive/decode/answer/fence path retains original Served/ServeFailure |
| daemon `install.rs` | Change existing sole installer | Add decoded-manifest seam for first-frame routing, same orchestration. Explicit Disposable refusal before claim/write/rename/open. Payload frames remain opaque until exact length/finish; preserve original failure/opened owner and no double fence |
| Bridge `daemon_types.rs`, `daemon_wire.rs`, `initial_record.rs`, `daemon_setup.rs` | New real application protocol/config | Add correlated Hello to existing Call/Answer and bounded observed daemon phase/incarnation/profile facts, distinct from FUSE Ready. Classify exact existing request/manifest magic once; malformed records retain original bytes, no decoder fallback. Protected config uses existing bounded codec and redacted keys |
| daemon `application/{config,owner,connection,serve,cli,failure}.rs`, `application/mod.rs`, `bin/layerfs-daemon.rs` | New actual application | Open protected config with safe existing nix/std, validate actual descriptor/dirs, disable dumpability before keys/Store/threads. One Overlay owner/start and direct installed/opened Store transfer into Service. Bounded connection slots/retained faults. No daemon process launcher/supervisor. Explicit install-pending/installing/retained/ControlReady phase; Hello startup wait observes one event/attempt, not replay |
| SDK `control/connection.rs` | Change validator only | Correlated Hello validates expected incarnation; existing exchange/quarantine remains sole owner |
| manifests/lock/guard | Change only justified locked edges | Existing nix0.31.3 safe process/fs/user capabilities; no third-party patch/new provider. Extend guard for actual paths/capabilities, keep provider-independent store/ free of engine paths/types |
| external daemon/Bridge tests, actual Linux binary proof | Change/new external proofs | Hello/codec/correlation, unavailable Commit no effects/original custody, installed transfer without reopen, actual binary listen/auth/install/control/readiness/protected config. Explicit100s test stops, narrower proof budget at registration; no Durable test/execution |
| source architecture, rollout/receipts | Change with actual implementation | Distinguish library/component/production application from native FUSE, Sandbox execution and live namespace Commit; preserve all old verdicts/unknowns and exact LOC |

Do not treat Installed acknowledgement as already published Service: establish
actual ControlReady separately before first control operation, without retrying a
prematurely refused Mount. An original final-ack loss may establish completed
validation only at phase Reply/published/opened/daemon_sqlite facts, retaining
that exact failure; an Open-phase partial owner does not qualify Ready. No later
read resolves original uncertainty. Shared Store/history survives daemon teardown;
no live seal/checkpoint/GC or automatic removal of partial artifacts.

Native runtime initial topology is same-container ordinary commands under explicit
nonroot identity, protected Store/Overlay/key paths and device controls. This
checkpoint contains no command supervision; Sandbox ordinary runtime owns true
Exec ID/status/stream/caller cancellation. Docker CLI exit alone is not remote
process completion. Standard runtime events/status research remains independent
while production filesystem application is implemented.

Each coherent subcheckpoint gets exact final source/build/binary identities,
locked build before bounded tests, affected Clippy/fmt/boundary/guard proof,
append-only outputs, and exact first-parent/staged/committed LOC with migration
subtotals. ARM64 inputs preserved, workers1. No timing, cache drop, disabled profile,
protected container/process/worktree action, remote edit/push or early retirement.
