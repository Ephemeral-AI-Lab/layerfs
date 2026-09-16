# LayerFS v0.1.6 — sandbox-local Workspace state, unchanged Store format

> **Status:** LayerFS 0.1.6 release announcement draft. Published as the GitHub
> release for tag [`v0.1.6`](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6).

v0.1.6 moves the **live mutable state of a Workspace into the sandbox**. The
container-side owner holds the mutable namespace, the file data and a private
packed payload backing created for that mount; the host keeps canonical
construction, the SQLite Store and publication, and learns the workspace's
mutable state only when a Commit transfers it. There is **no pause or quiesce
step** in the Commit path — `FREEZE`/`RESUME` remain wire constants with no
dispatch handler, and the measured `commit_pause_fence_ns` is `0` in all 56
SDK-edit rows — and canonical construction runs with **one worker**.

**The Store format did not change.** `SCHEMA_VERSION` stays 10, no schema or
static SQL changed since v0.1.5, and `layerfs-content` and
`layerfs-layerstack-store` are byte-identical to the v0.1.5 release, so ObjectId
domains, chunking, content roots and Commit derivation are untouched. Existing
schema-6/7/8/9 Stores connect without promotion; there is no in-place promotion,
no downgrade and no retained-history transfer command. Explicit compaction stays
removed, and previously compacted Stores remain readable through the retained
authenticated LFCNT1 read path.

**What changed at the boundary:**

- the sandbox owner keeps live mutable state and a private snapshot backing
  directory per mount (created and removed per mount; a stale directory from a
  crashed mount is refused, never reused);
- the daemon's mount request carries that backing root, so a v0.1.6 sandbox owner
  and a v0.1.5 host do not share a live workspace — match SDK, CLI, daemon and
  runtime components across a session;
- the public CLI is unchanged (`layerfs --version` prints `layerfs 0.1.6`), and
  the only public-SDK additions sit behind the `test-instrumentation` feature.

**What it costs, measured:**

- **One construction worker**, so construction-heavy work is slower: six
  dedup/CDC construction cells are **1.50–1.65×** v0.1.5 and are
  **owner-accepted as recorded**. The direct diagnostic is 2 290.09 ms with one
  worker versus 1 458.89 ms with the variable unset, against a 1 426.04 ms
  comparator. The cold `namespace-100000` Init target is **owner-waived**
  (4.986 s against the 2.7 s target; v0.1.5 control 4.398 s, 1.13×).
- **A bounded sandbox spool replaces page-cache residency.** The sandbox's own
  residency is ≤ 2.6 MiB in the B2 control, and the Commit-time transfer now pays
  a storage read (**≈2.1 GiB/s**) instead of the cache-served ≈19 GB/s that a
  warm run appeared to show. Sandbox *process* memory is not measurable on this
  harness: only container-scoped numbers are comparable, and a lifetime cgroup
  peak is not a phase number.
- Kernel-dirty shared `mmap` is still not captured by a Commit — the
  specification's own open obligation, isolated to exact byte level, with no
  registered selection affected.

**Validation of this release's own selections:** 36 rows (33 regular + 3
declared extensions), seed 1, one sample per case and mode — **28 performance
receipts and 36 independent verifications, every verification `PASS`, every
cleanup `PASS`, no `FAIL`, `TIMEOUT` or `NOT_RUN`**. Five mode-level results are
above the 15 s family target and are published as such: three performance rows
(15.760 s, 16.492 s, 18.312 s) under the declared 60 s invocation allowance
(gate `EXCEPTION`), one verification row at 23.10 s (24.17 s standalone) under
the owner-declared **30 s** ceiling, and two verify-only exhaustive rows at
15.014 s and 59.326 s under their frozen 120 s and 300 s watchdogs.

The wider #152 sandbox-local campaign behind this change is 196 collected cells
over three candidate identities: **189 PASS**, six owner-accepted single-worker
material regressions, one owner-waived 2.7 s cold-Init target, three
owner-banked controls cited rather than re-run, nine optional rows not run, and
the inherited 11 + 11 `historical_access` rows `NOT_RUN` because their sealed v2
Store is not recoverable. Its six `workspace_reliability` fault-injection proofs
were repaired after the campaign and are now **27/27 PASS** on the frozen
candidate. No number was re-labelled, and no failing cell was dropped.

**Read more:**
[release record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/release-notes/0.1.6/README.md) ·
[every measured selection](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/release-notes/0.1.6/benchmark-closeout.md) ·
[acceptance and waivers](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/release-notes/0.1.6/acceptance.md) ·
[versioned manual](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/docs/versioned/0.1.6/README.md) ·
[#152 final report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.6/docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md)

## v0.1.6 selections at a glance

Complete-command wall times of one sample per case and mode, seed 1 (`perf` =
performance invocation, `verify` = separate identity-pinned verification). These
are whole-invocation walls, not product timers, and no row was re-run.

| Family | Cases | Perf (s) | Verify (s) | Notes |
|---|---:|---|---|---|
| `file_size_transition` | 7 | 1.774–2.127 | 1.758–1.996 | the 128 KiB boundary controls, above/below/exact and round trips |
| `branch_development` | 6 | 1.979–16.492 | 2.086–23.100 | incl. the F4 compact controls and the declared 30 s verification ceiling |
| `dedup_branch_history` | 6 | 2.045–2.821 | 3.670–9.448 | the F5/F6 history profiles at K10 and K100, incl. namespace-inode |
| `mixed_load_bearing` | 6 | 3.042–18.312 | 3.029–59.326 | four regular rows plus two verify-only exhaustive replays |
| `multi_workspace_development` | 5 | 3.304–15.760 | 3.824–20.360 | four concurrent workspaces plus the four-workspace control |
| `historical_access` | 6 | verify-only | 1.593–1.964 | boundary before/after, inode before/after, fork point, divergent head |

This is a **source-only Developer Preview**, not production storage. Crash and
power-loss durability are not promised, Workspace backing is not fsynced, and no
crates.io package, prebuilt executable or public runtime image is part of this
release.
