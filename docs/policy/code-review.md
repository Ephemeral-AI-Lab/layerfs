# Code review and triage policy

> **Status:** Current general guide (owner-approved 2026-11-13).

How a pull request gets a tier, who approves it, what evidence it carries, and
how an issue gets a priority. The tier follows the most important component a
change touches, ranked by the owner: **core** (P0), **engine** (P1),
**control** (P2), **api** (P3). The tier sets how carefully a PR is read and
what evidence it needs. A shorter list of **gate** risks decides when the
owner must approve. This policy points to the rules it relies on and does not
repeat them:

| Rule | Owner |
| --- | --- |
| Measurement: one sample, declared cache state, speed and storage, 5 ms floor | [measurement workflow](../general/agent-measurement-policy.md), [benchmark rules](../general/benchmark_rules.md) |
| One attempted operation, no retries; Disposable only, Durable `NOT_RUN` | root [`AGENTS.md`](../../AGENTS.md) §Active persistence profile and §4; [engine guide](../../engine/AGENTS.md#failure-persistence-and-dependencies) |
| Tests built with `--no-run` first, then each run under a 120-second wall timeout | root `AGENTS.md` §4 |
| Production LOC in every commit | root `AGENTS.md` §Production LOC comparison |
| 999/200-line ceilings, no tests in `src/`, unsafe allowlist | [engine guide](../../engine/AGENTS.md#production-file-size-and-responsibility) |
| fuser patch authority and provenance | [engine guide](../../engine/AGENTS.md#fuser-provenance-and-authorized-patch-checks) |
| When architecture docs must change | [architecture doc update policy](architecture-doc-update.md) |
| Release waivers | [release policy](../general/release-policy.md) |
| CI reports and never gates | root `AGENTS.md` §4 |

The policy applies to PRs into a release branch (`main`, `layerfs_v2_0_0`).
A PR into a feature branch can be reviewed the same way. The PR that merges a
feature branch links the reviewed PRs and is reviewed only for what they did
not cover.

## The tier of a pull request

A PR's tier is the **highest tier of any path it changes or renames from**.
Risk sets (listed under [Path sets](#path-sets)) can raise a path's tier
whatever layer it sits in. When in doubt, use the higher tier.

| Tier | Paths | What review protects |
| --- | --- | --- |
| **P0** | `core/crates/**` except `layerfs-telemetry`; the `risk:format` set in core and engine | canonical identity, content correctness, encoding and packing, history, the cost of the algorithms, the Store bytes a later release must read |
| **P1** | `engine/crates/**`, `engine/vendor/**`, `engine/patches/**`, `core/crates/layerfs-telemetry/**` (its call-log encoding is in `risk:format`); the `risk:format` set in control and tools (records, migration); the `risk:store`, `risk:wire`, `risk:kernel`, `risk:delete`, `risk:security` and `risk:deps` sets; the guard tools | Store integrity and custody, Commit capture, overlay custody, FUSE/NFS ownership and caches, record and wire compatibility, deletion, secrets and isolation |
| **P2** | `control/crates/**`, `control/SUPPORT.json`, `.github/**`, `.config/**`, release tools (`tools/linux_bundle.py`, `tools/machine_image*`); owner-only documents; the contract docs set | machines and workers, Sandbox launch, CI, the rules agents follow and documents they treat as instructions |
| **P3** | `api/**`, `plugins/**`, `.agents/**` (except skills), other `docs/**` and `tools/**`, `README.md` | the call table, argument checks, front ends, packaging, text |

Manifests (`Cargo.toml`, `package.json`, `pyproject.toml`) take the tier of
their crate. A manifest change to dependencies, features or build flags also
falls into the `risk:deps` set.

### Gate risks: when the owner approves

The owner approves a PR, at any tier, when it:

- changes production source under `core/crates/*/src/`, except
  `layerfs-telemetry` outside its `risk:format` file;
- falls in the `risk:format`, `risk:store`, `risk:delete` or `risk:security` set;
- changes an owner-only path: every `AGENTS.md`, `docs/policy/**`,
  `docs/general/{agent-measurement-policy,benchmark_rules,release-policy,documentation-policy}.md`,
  `engine/vendor/**`, `engine/patches/**`, `.config/nextest.toml`, or a guard
  tool (`tools/{check_product_boundary,check_fuser_integrity,production_loc,ci_tests}.py`);
- adds a dependency;
- adds a loop around a fallible call, a busy handler, a resend or an
  error-driven fallback (normally refused as a retry);
- weakens a guard, an allowlist, a pin or a test lane;
- removes or renames a released call, argument, result, refusal kind, CLI
  flag or MCP profile.

Path sets find most gate risks. A gate risk that the paths miss, such as a
control change that alters who holds a Store, is added by hand by the author
or the reviewer, with the `owner-review` label.

### De-escalation

De-escalation lowers the evidence a PR needs. Only D1 also lowers its tier.
The author records the code on the `Tier:` line. The reviewer checks that the
diff contains only what the code claims.

| Code | Applies when | Effect | Never applies to |
| --- | --- | --- | --- |
| **D1 editorial** | only Markdown text changes: wording, links, formatting | tier P3; the author may self-merge with a review record | the contract docs set; owner-only paths; a doc that states a new invariant, limit, format or design (that is reviewed at the tier it describes) |
| **D2 comment-only** | only comments or rustdoc change | P3 evidence | a removed code line (commenting code out is a code change); a `SAFETY` comment |
| **D3 test-only** | only `tests/`, `examples/`, `benches/` change | no measurement or mount proof unless the test is one; the same reviewer | golden and fixture files in the `risk:format` set; a changed expected value or weakened assertion (a contract change); `engine/vendor/**`; a file that names Durable |
| **D4 generated** | blocks from `gen-coverage.mjs` or `gen-profiles.mjs` | the block is not read; the generator's `--check` passes | |
| **D5 relocation-only** | files moved or renamed with no behaviour change | LOC labelled relocation; tests run | a move mixed with a behaviour change (land the move as its own commit) |

### Recording the tier

The first line of the PR body records the tier the author claims:

```text
Tier: P1 (engine/crates/layerfs-fuse) ; risks: risk:kernel ; gate: no ; de-escalated: none
```

The `tier:P0`–`tier:P3` label is the **path floor**: the tier the paths give,
before any de-escalation. It only ever rises as the PR changes. When the `Tier:` line
claims a lower tier through a de-escalation code, the reviewer confirms it in
the review record. When the author and the reviewer disagree, the higher tier
applies.

## Who approves

| | P0 | P1 | P2 | P3 |
| --- | --- | --- | --- | --- |
| Approver | the owner | any maintainer other than the author; the owner for a gate risk | any maintainer other than the author; the owner for a gate risk | one maintainer; D1 may be self-merged |
| Reading depth | every changed production line; every changed assertion, fixture and golden; callers of every changed public port | every changed line in gate-risk and lock code, read end to end; the rest by diff; tests read for weakened or deleted assertions | every line of custody, secret, worker-protocol, isolation and deletion code; the rest by diff | the diff; call-table entries and refusal text in full |
| The reviewer runs again | the boundary guard; the covering tests on Linux | the boundary guard; one covering test of the reviewer's choice; the fuser check when the vendored fuser changed | the boundary guard | nothing |

- **The review record** is a PR comment or review, by any account, that starts
  `Review record` and gives the reviewed head commit, the tier, what was read,
  what was run again with the result, and what was not checked. Merge when the
  record names the current head; a later push needs a new record, at least for
  what changed. With no branch protection this record is the only proof of
  review, and it works where GitHub's Approve button does not (an author cannot
  approve their own PR).
- **The owner's own PRs.** Agents push from the owner's account, so most
  owner PRs have no second person. Their reviewer is an
  **independent-context AI review**: a fresh session given only the diff and
  this policy, not the author's description, recorded with the model and the
  head commit. Where possible, a different model from the authoring agent
  writes it. That review is enough to merge. For `risk:format` and
  `risk:store` hunks, a contributor also reads only those hunks within a week
  after merge and files an issue for each finding.
- **Agent-written PRs from contributors.** The person who launched the agent
  answers for the evidence. An AI review supplements the human reviewer and
  never replaces them.
- **Measurements are checked, never sampled again.** The measurement workflow
  allows one sample per arm. The reviewer checks the receipt (identities, cache
  state, timer scope, speed and storage, arithmetic) and diagnoses a doubtful
  number with a labelled count, not a second run.
- CODEOWNERS, once added, requests reviewers for gate-risk paths only and
  requires nobody.

## Evidence the author reports

R = required, C = required when the condition holds, — = not needed.
"Report" means the exact command, its timeout and its outcome. List failures
one by one, and list tests that already fail on the base against the base.
Nothing runs on PRs into `layerfs_v2_0_0` automatically, so this evidence is
the author's word plus whatever the reviewer runs again.

| Evidence | P0 | P1 | P2 | P3 |
| --- | --- | --- | --- | --- |
| `Tier:` line | R | R | R | R |
| `Production LOC: <before> -> <after> (delta <±n>)` per commit, with scope and method | R | R | R | R |
| Tests: `cargo +1.85.1 test --locked … --no-run`, then each selection under a ≤120 s wall timeout | R | R | R | C: Rust changed |
| `clippy --locked --all-targets -- -D warnings` and `fmt --all -- --check` on the changed packages (not `layerfs-node`, which the owner keeps out) | R | R | R | C: Rust changed |
| `python3 -B tools/check_product_boundary.py` and the tool tests | R | R | R | C: Rust or tools changed |
| Linux mount proof: the affected `ci_tests.py` mount targets in a fresh container with `/dev/fuse` | C: reachable from a mount | R for fuse, nfs, daemon, mount-ports, overlay, workspace | C: Sandbox backend, daemon channel | — |
| Performance: speed **and** storage under the measurement workflow, or a count-driven diagnostic | C: `perf` set | C: `perf` set | — | — |
| Compatibility (below) | C: `risk:format` | C: `risk:format`, `risk:wire` | C: same | — |
| `Docs:` topics updated, or `Docs: no architecture impact (<reason>)` | R | R | R | R |
| `Durable: NOT_RUN — disabled by owner until explicit reauthorization`; new Stores select Disposable explicitly | C: `risk:store` | C: `risk:store` | C: `risk:store` | — |
| A test that protected data survives | C: `risk:delete` | C: `risk:delete` | C: `risk:delete` | C: `risk:delete` |
| Lock order and hold described; nothing in the publication lane grows with tree, history or Branch count | C: locks or lane changed | C: same | C: same | — |
| fuser: `python3 -B tools/check_fuser_integrity.py` | — | C: vendored fuser changed | — | — |
| Deleted tests listed, with surviving behaviour covered again | C | C | C | C |
| Dependency: why no existing crate covers it | C | C | C | C |

**The `perf` set** (paths under [Path sets](#path-sets)) is the code whose
speed or storage the owner measures: construction, encoding, saving and
packing, Commit, the publication lane, FUSE dispatch and overlay payload. A
change there reports a receipt or a count-driven diagnostic. Saying that the
change is not performance-relevant needs the reviewer's agreement in the
review record. A measurement that cannot fit the budget is reported `NOT_RUN`
with its wall time and reason; at P0 it needs the owner's waiver.

**A mount proof that cannot run** (a macOS host) is reported
`NOT_RUN (host: macOS)`. At P1 the PR does not merge until the reviewer or
another maintainer with a Linux host runs it, unless the owner waives it in
the PR.

**Risky lines.** The reviewer scans the added lines for `Durable`, `unsafe`,
retry or busy-handler patterns, `remove_dir_all`, `rmtree`, `DELETE FROM` and
`#[ignore]`, for removed `assert` lines, for changed fixtures and goldens, for
renames with large edits, and for dependency hunks. A report-only bot comment
may list these later (see [Labels](#labels)); until then the reviewer greps
the diff. The author does not write "no retry" or "no unsafe" lines.

### Compatibility evidence

A change in the `risk:format` or `risk:wire` set:

- names the format change and the version it bumps (SQLite or PostgreSQL
  schema, record version, bridge or worker protocol), and updates the page
  that owns the format;
- shows that a Store, home or record written by every earlier 2.x release is
  read correctly or refused with a documented refusal, never misread
  (`engine/crates/layerfs-persistence/tests/old_format_open.rs` is the shape);
- for core encodings, shows that the identity of unchanged content does not
  change, or links the owner's approval of an identity break;
- for the wire, says whether an older and a newer peer refuse each other
  cleanly (a macOS or Windows host talks to a Linux worker of its own build);
- for a change that refuses old Stores without a migration, links the owner's
  decision.

### Reviewable size

Each commit does one kind of thing: a move or rename is its own commit,
labelled relocation, before the behaviour change; a format change and a
dependency each land apart from unrelated code. As a guide, gate-risk hunks
beyond about 800 production lines are hard to read at the required depth in
one sitting; split such a PR where it can be split, or say in one line why it
cannot (an atomic format migration).

### Merging

PRs are squash-merged. The author writes the squash commit's message,
including its `Production LOC:` line computed against the base branch's head
at merge time.

## Issue priority

An issue's priority comes from **impact first** (severity), then exposure
(urgency). The layer only sets the starting guess before triage: core and
engine start at S2, control and api at S3. Impact beats layer: corruption is
S1 in any layer, a typo is S4 in any layer.

| Severity | Impact | Examples |
| --- | --- | --- |
| **S1** | data lost, corrupted, or wrong content read or committed; history lost or wrong; success reported for something that did not happen; a secret leaked, a Sandbox escape or a worker auth bypass; a Store left unopenable | a read returns wrong bytes; Commit captures a tree that is not the frontier; a CLI call deletes a Branch it was not asked to |
| **S2** | crash, hang, deadlock or unbounded growth; a failure with no workaround; an owner rule broken (a retry added, Durable executed); a speed or storage regression past its band and the 5 ms floor on a hot path | daemon deadlock on unmount; FUSE stale cache after rename |
| **S3** | wrong result or refusal with a workaround; a misleading error; a regression off the hot path | a short id reported as a missing Branch |
| **S4** | text, formatting, help, docs | a misspelled help line; a stale link |

| Urgency | Exposure |
| --- | --- |
| **U1** | in a released version or the active release branch, on a default path or a supported platform, or happening now |
| **U2** | a non-default path, a less common platform, or unreleased code only |
| **U3** | unusual conditions, an experimental platform, or a frozen component |

| | U1 | U2 | U3 |
| --- | --- | --- | --- |
| **S1** | P0 | P0 | P0 |
| **S2** | P1 | P1 | P2 |
| **S3** | P2 | P2 | P3 |
| **S4** | P3 | P3 | P3 |

| Priority | Expectation |
| --- | --- |
| **P0** | assigned at triage. The path is disabled or refused at once, with an exact refusal and no silent fallback. The affected Store or home is kept unchanged as evidence. The fix, with a regression test, is the next change merged in the area. An S1 blocks a release and is never waived ([release policy](../general/release-policy.md#release-requirements)). |
| **P1** | assigned at triage; regression test; fixed for the next release unless the owner waives it under the release policy |
| **P2** | scheduled at triage; does not block a release |
| **P3** | backlog; `good first issue` candidates |

A fix takes its **PR tier from its paths**, not from the issue's priority,
plus every gate risk it hits. The owner reviews every fix for a P0 issue. An
S1 found in an unmerged PR is a review finding that blocks that PR, not an
issue. Security reports go through GitHub private vulnerability reporting,
never a public issue.

## Labels

| Label | Colour | Description | Set by |
| --- | --- | --- | --- |
| `tier:P0` | `#B31D28` | PR path floor: core source or a Store format | author, from paths |
| `tier:P1` | `#E36209` | PR path floor: engine, records, Store custody, wire, kernel, deletion, security, deps | author |
| `tier:P2` | `#F9C513` | PR path floor: control, CI, policy and contract docs | author |
| `tier:P3` | `#BFD4F2` | PR path floor: api, docs, plugins, local tools | author |
| `prio:P0` | `#B60205` | Issue: S1 data loss, corruption or security | triage (issues) |
| `prio:P1` | `#D93F0B` | Issue: major, S2 on an exposed path | triage |
| `prio:P2` | `#FBCA04` | Issue: minor, or major off the common path | triage |
| `prio:P3` | `#C5DEF5` | Issue: cosmetic or rare | triage |
| `layer:core` | `#0B3D91` | core/crates | author (PRs); issue form |
| `layer:engine` | `#1F6FEB` | engine/crates, vendor or patches | author; issue form |
| `layer:control` | `#54AEFF` | control/crates or SUPPORT.json | author; issue form |
| `layer:api` | `#B6E3FF` | api/: Rust crates, Node, Python, MCP, CLI | author; issue form |
| `risk:format` | `#8B0000` | Persistent format: codecs, packs, SQL schema, records, fixtures | author; reviewer |
| `risk:store` | `#A0522D` | Store custody: open, lease, publish, install, backup, restore | author; reviewer |
| `risk:wire` | `#BF8700` | Bridge, worker, host or CLI framing and handshakes | author; reviewer |
| `risk:kernel` | `#7D4E00` | FUSE, NFS, mount ports or the patched fuser | author |
| `risk:delete` | `#6F42C1` | Code that deletes data: GC, cleanup, removal, retention | author; reviewer |
| `risk:security` | `#D1242F` | Secrets, credentials, worker auth, isolation, unsafe and FFI | author; reviewer |
| `risk:deps` | `#CC317C` | Lockfiles, build flags, vendor, patches, dependency hunks | author |
| `owner-review` | `#000000` | A gate risk: the owner approves | author; reviewer |

Labels are set by hand from the [path sets](#path-sets) at first: the author
sets `tier:`, `layer:` and `risk:` labels and `owner-review`, and the reviewer
corrects them. A report-only workflow that sets them, and lists the risky
lines, may follow once the sets have settled; it needs the owner's approval
because nothing else runs on PRs into `layerfs_v2_0_0`.

Existing labels stay as they are, including `bug`, `enhancement`,
`new feature`, `hold` (do not merge), `release`, `documentation`, `plugin`,
the version labels and the topic labels such as `postgresql` and
`garbage collection`. Severity and urgency stay in the issue form and the
triage comment; they are not labels.

## Path sets

Globs are relative to the repository root. Renames count from both their old
and their new path.

| Set | Globs |
| --- | --- |
| `risk:format` | `core/crates/layerfs-content/src/object/**`, `core/crates/layerfs-content/src/**/*{codec,wire}.rs`, `core/crates/layerfs-storage/src/{encoding,pack}/**`, `core/crates/layerfs-telemetry/src/output/encode.rs`, `engine/crates/layerfs-persistence/sql/**`, `engine/crates/layerfs-persistence/src/backend/{records.rs,*/schema*.rs}`, `engine/crates/layerfs-persistence/src/blocks/**`, `control/crates/layerfs-home/src/{json.rs,home/records.rs,home/fields.rs}`, `control/crates/layerfs-machine/src/record.rs`, `control/crates/layerfs-sandbox/src/{record/**,exec/record.rs}`, `control/crates/layerfs-store/src/{record.rs,project/record.rs,backup/record.rs}`, `control/crates/layerfs-control/src/migrate/**`, `tools/store_blocks.py`; fixtures and goldens: `**/tests/fixtures/**`, `**/*.sqlite`, `**/*.bin`, `engine/crates/layerfs-persistence/tests/old_format_open.rs`, `core/crates/layerfs-content/tests/{object_identity,filesystem_codec,fixture_seal}.rs`, `core/crates/layerfs-storage/tests/{object_key,physical_formats,codec_frames}.rs` |
| `risk:store` | `engine/crates/layerfs-persistence/src/**`, `engine/crates/layerfs-store-service/src/{store,write,history}/**`, `engine/crates/layerfs-workspace/src/construction/**`, `engine/crates/layerfs-project/src/store/**`, `engine/crates/layerfs-daemon/src/install.rs`, `engine/crates/layerfs-overlay/{sql,src/database}/**`, `control/crates/layerfs-store/src/{open,open_args,lease,identity,place}.rs`, `control/crates/layerfs-store/src/{sqlite,postgresql,backup}/**`, `control/crates/layerfs-store/src/project/{init,postgres_init,add}.rs`, `control/crates/layerfs-bucket/src/backup/**`, `control/crates/layerfs-sandbox/src/daemon/**` |
| `risk:wire` | `engine/crates/layerfs-bridge/src/{wire,daemon_wire,provision_wire,initial_record}.rs`, `engine/crates/layerfs-bridge/src/{control_,sdk_}*.rs`, `engine/crates/layerfs-bridge/src/duplex/**`, `control/crates/layerfs-machine/src/worker/{frame,hello,client}.rs`, `control/crates/layerfs-sandbox/src/host/frame.rs`, `control/crates/layerfs-store/src/project/frame.rs`, `api/crates/layerfs-cli/src/framed.rs` |
| `risk:kernel` | `engine/crates/layerfs-{fuse,nfs,mount-ports}/**`, `engine/vendor/**`, `engine/patches/**` |
| `risk:delete` | `tools/cleanup*`, `control/crates/layerfs-store/src/project/remove.rs`, `control/crates/layerfs-machine/src/remove.rs`, `control/crates/layerfs-bucket/src/bucket/**`, `engine/crates/layerfs-overlay/src/maintenance/{garbage,orphan}.rs`, `engine/crates/layerfs-daemon/src/control/cleanup.rs`, `core/crates/layerfs-telemetry/src/output/retention.rs` |
| `risk:security` | `control/crates/layerfs-home/src/secrets.rs`, `control/crates/layerfs-bucket/src/bucket/{credentials.rs,s3_api/sign.rs}`, `control/crates/layerfs-sandbox-backend/src/postgres_url.rs`, `control/crates/layerfs-sandbox-backend/src/{backend/namespaces,local/os/linux}/**`, `control/crates/layerfs-control/src/worker_server.rs`, `control/crates/layerfs-machine/src/worker/{hello,client}.rs`, `engine/crates/layerfs-bridge/src/{duplex/handshake.rs,process/**}`, `engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/file_control.rs`, `core/crates/layerfs-storage/src/encoding/codec.rs`; new `unsafe` or FFI anywhere (found by the boundary guard and the risky-lines scan, not by path) |
| `risk:deps` | `Cargo.lock`, `**/package-lock.json`, `.cargo/**`, `engine/{vendor,patches}/**`; and dependency, feature or build-flag hunks in any `Cargo.toml`, `package.json` or `pyproject.toml` |
| `perf` | `core/crates/layerfs-content/src/file/construction/**`, `core/crates/layerfs-storage/src/{encoding,save,pack}/**`, `engine/crates/layerfs-store-service/src/store/commit*.rs`, `engine/crates/layerfs-persistence/src/backend/*/lane/**`, `engine/crates/layerfs-workspace/src/construction/**`, `engine/crates/layerfs-fuse/src/dispatch/**`, `engine/crates/layerfs-overlay/src/payload/**` |
| contract docs | `docs/architecture/**/README.md`, `docs/general/{cluster_one_handbook,cas_cdc_deltaencoding_handbook}.md`, `.agents/skills/**` |

A path set changes with the code: a PR that adds, moves or removes a file that
belongs in a set updates this table in the same commit.

## What this asks of each role

- **Author:** fill in the `Tier:` line and the evidence; run the tests and
  guards yourself; say what you did not run and why.
- **Reviewer:** confirm the tier; read at the tier's depth; run the boundary
  guard again; check measurements, never sample them again; write the review
  record against the head commit.
- **Owner:** approve P0 and gate risks; grant waivers in the PR or the
  release record.

Nothing here is a required check or branch protection. Labels, CODEOWNERS and
templates report; the review record is the PR.
