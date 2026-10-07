# R0 deepest-file reconciliation plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Input HEAD `1a6bb53ef14e1860d8f222df11394e5a654bb34d`, local `main`, 2026-10-08.

R0 changes current documentation and dispatch only. No product, SQL, manifest,
lockfile, kernel profile, acceptance threshold or historical receipt changes.
Root owns edits and verification. Three read-only agents review native ownership,
captured construction and public API/rollout/evidence scope; they run no builds,
tests, measurements or cleanup.

| Deepest home | Reuse/change/new | Required reconciliation |
| --- | --- | --- |
| `AGENTS.md`, `core/AGENTS.md` | Change existing modified guides, after original snapshots | Runtime owns commands; daemon owns filesystem only. Preserve all unrelated guide edits; only R0 hunks enter the commit |
| `303/workspace-api/{mount,exec,commit,unmount,status}.md` | Change current primary proposals | Ordinary external filesystem access, optional SDK exec delegating to Sandbox, exact control producer and filesystem drain; direct daemon Store |
| `303/{README,01-architecture,daemon-sqlite,fuse,06-cluster-one-integration,07-implementation-validation,08-decisions-provenance,09-implementation-handoff}.md` | Change current routing/owner proposals | Remove live routing to daemon execution/host Store; identify superseded layout/slices; retain historical baseline pins |
| `307/S8-SPECIFICATION-20261008.md` | Change current prospective specification | Remove custom Exec wire/supervisor/launcher/cgroups/tracking; move stream/process responsibility to runtime; preserve all six filesystem corrections, profile/coherence and failure custody |
| `307/S8-IMPLEMENTATION-PLAN-20261008.md` | Change deepest-file matrix | SDK Project/Workspace/Sandbox and actual Sandbox backend/access, daemon filesystem executable only; align with full destination manifest |
| `307/S8-PROOF-PLAN-20261008.md` | Change prospective selections, retain IDs | Withdraw old daemon Exec selections before execution. Register replacement ownership rows with distinct suffixes; no historical verdict changes |
| `307/S8-MECHANISM-EVIDENCE-20261008.md` | Change current mechanism ownership | Runtime stream correctness and access setup retain proof obligations, with no daemon process owner |
| `307/HANDOFF-S8-IMPLEMENTATION-20261008.md` | Replace narrow unexecuted dispatch with current routing | R0–R9 authorization, primary checkout, exact proof/evidence/LOC rules; no obsolete C1-only limitation |
| `307/FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md` and destination manifest | Reuse existing latest-owner layout | Already no daemon Exec; verify all proposed ownership and forbidden destinations, do not rewrite original inventory receipt |
| `307/ROLLOUT-LEDGER-20261008.md` and `checks/r0-owner-reconciliation-20261008/` | New | Persistent R0–R9 status, original guide snapshots, proof withdrawal/correction ledger, hashes, independent findings, documentation checks, exact LOC |

Cost/custody: no total command/file/edit/Commit cap; no command identity in FUSE
admission or capture; shell exit is no filesystem drain. Forced filesystem stop
never implicitly signals external processes. Normal probe continues ordinary
service; active control producers refuse force before effects. Indexed lookup
custody, one connection-specific abort and one plain detach, R+N callback-entry
accounting and complete daemon-work disposal all remain requirements.

Independent R0 proof: cross-document ownership/forbidden live destinations,
retained genuine corrections/owner rulings/proof identifiers, local links and
anchors (record inherited broken links separately), whitespace, protected hashes,
unchanged historical evidence and product trees, first-parent/staged LOC. Rust and
runtime checks are inapplicable to this documentation-only change. R0 completion
is contract alignment, never native implementation or qualification.
