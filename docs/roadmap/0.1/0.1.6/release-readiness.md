# v0.1.6 release readiness

> **Status:** preparation record, 2026-09-16. This page states what is complete,
> what is missing and which decisions are the owner's. It is **not** a release
> contract, a release note or a tag, and nothing here claims the release is cut.

## Where the line stands

The **benchmark line is complete**: [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122)
and [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) are closed with
all 33 regular cases and 3 extensions terminal at seed 1 on one identity chain
(performance 28 PASS + 8 declared `N/A`; verification 36 PASS), evidence committed
under [`evidence/issue154/`](evidence/issue154/) and recorded in ledger
[L11/L12](evidence/issue154-rollout-ledger.md).

The **product line is not closed**: [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151)
("execute #149 + #150 and qualify create-500, bulk-create-500, and 25k") is still
open. Its three gates are recorded as owner-accepted with limitations attached in
the [roadmap README](README.md) (L18–L20), but that text also states plainly that
the adoption recommendation "is not a release decision: no merge, issue closure or
tag follows from it", and its own limitations list is unfinished work — sandbox
*process* memory is not emitted by the frozen harness, dirty shared-mmap visibility
is unsolved, and the breadth families were not run.

## `docs/general/release-policy.md` requirements

| requirement | state | evidence / gap |
| --- | --- | --- |
| workspace builds, all tests pass | **met** | `tools/preflight.sh` all steps passed on `3e6f2dac5`; workspace fast suite green |
| formatting, `-D warnings` clippy, `git diff --check` | **met** | preflight steps 1/4; `git diff --check` clean |
| public SDK and CLI reference match their exported surface | **partial** | the workspace suite passes; the *versioned manual* that states the surface for this release does not exist yet (`docs/versioned/0.1.6/` missing) |
| SQLite schema and static SQL manifest structural tests | **met** | part of the workspace fast suite; every benchmark receipt observed `schema_version 10` |
| supported FUSE and container-runtime gates on a capable host | **met for the benchmark scope** | 36/36 verification rows PASS on macOS + Docker Desktop (2 CPU / 2 GiB container); no Linux-host gate is claimed |
| versioned manual and limitations complete | **missing** | `docs/versioned/` stops at 0.1.5; the 0.1.6 manual, its CLI/SDK/container/storage pages and `limitations.md` must be written from the released source |
| every published benchmark links to reproducible raw evidence and identifies the exact source | **prepared** | `release-notes/0.1.6/benchmark-{performance,verification}.csv`, derived by `release-notes/0.1.6/derive_tables.py` from the committed matrices; every row carries its receipt path, source seal `86f14b2d…` and product seal `970964e9…` |
| release tag resolves to the reviewed source tree | **missing** | no `v0.1.6` tag exists; the workspace version is still `0.1.5` |
| compatibility boundary stated explicitly for this release | **owner decision** | the benchmark side observed schema 10 and an unchanged product seal `970964e9…`; whether v0.1.6 changes the public API, daemon protocol or Store/canonical contract must be stated by the owner before the manual and contract are written |

## Prepared now (no new claims)

* `release-notes/0.1.6/derive_tables.py` and the two CSVs it writes: 36 rows each,
  every receipt path verified present on disk, one source seal and one product seal
  across all rows, the declared target (15 s) and declared exception (25 s, or 30 s
  for the one owner-declared verification row) carried next to the measured wall.
* The audit above, the [roadmap README](README.md) status and the ledger, so the
  release notes can cite committed evidence rather than paraphrase it.

## Remaining steps, in order

1. **#151 disposition** — the owner closes it (or records what remains) so the
   product-side scope of the release is settled.
2. **Compatibility boundary** — one owner statement: public API, CLI, daemon
   protocol, canonical identity and Store format for v0.1.6, with the migration /
   downgrade boundary if any.
3. **Versioned manual** — `docs/versioned/0.1.6/{README,quickstart,specification,cli,sdk,container-runtime,storage-format,limitations}.md`
   from the released source.
4. **Release notes** — `release-notes/0.1.6/{release-contract,README,acceptance,verification,benchmark-closeout,waivers,github-release}.md`
   plus `release-evidence.json`; `waivers.md` lists the declared benchmark
   exceptions (three ≤25 s rows, one ≤30 s verification row) and any product-side
   waiver the owner grants for #151.
5. **Version bump and gates** — `Cargo.toml` `0.1.5` → `0.1.6`, `cargo +1.96.0 fmt --all --check`,
   `tools/preflight.sh`, `check_plan.py`; note that the bump changes the *source*
   seal (root `Cargo.toml`/`Cargo.lock`) while the **product seal stays
   `970964e9…`**, so the benchmark claim remains "released source or the exact
   recorded source seal `86f14b2d…`".
6. **Tag and publish** — `git tag -s v0.1.6` on the reviewed tree, push the tag,
   publish the source-only GitHub release with its checksums (v0.1.5 published no
   package, prebuilt executable or runtime image; nothing indicates a change here).

Steps 1–2 and 6 need the owner; 3–5 are authoring and mechanical work that can be
executed on the word.
