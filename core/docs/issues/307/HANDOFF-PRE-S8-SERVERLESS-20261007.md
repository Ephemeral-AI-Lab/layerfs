# Handoff prompt: finish everything before S8 on the serverless Store

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Written 2026-10-07 at `main` `21ff451fa`. Paste the text below the line into
> a fresh session. It is an assignment only when the owner dispatches it.

---

You are the implementation owner for all LayerFS work that must be finished and
proven **before S8** (FUSE mount and Bash Exec). S8 is the most important stage.
Your job is to make sure that when S8 starts, any failure can only come from
FUSE or Exec, never from the global Store, the daemon engine or the host
handoff.

Work in the primary checkout `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local
`main`. Start at `21ff451fa`.

## 1. The design you are building

LayerFS has exactly two databases. Keep them separate in your head and in code.

| | Global Store | Overlay |
| --- | --- | --- |
| File | `store.sqlite` | `overlay.sqlite` |
| How many | One, shared by every sandbox | One per daemon |
| Where | A named Docker volume mounted into every daemon container | The daemon's own local disk |
| Holds | Immutable content objects and packs; mutable history (Branches, Commits, layers); id counters | All mutable local state: Workspace metadata, payload, scratch, custody, each under a Workspace prefix |
| Who opens it | Every daemon, directly, in-process | Only its own daemon |
| Writers | Many processes, one short write transaction at a time | One owner thread |
| Crates | `layerfs-storage`, `layerfs-history`, `layerfs-persistence` | `layerfs-overlay`, daemon `overlay/` |
| Lifetime | Outlives every Workspace and daemon | Dies with its daemon |

The Store is **serverless**: there is no host process, server, coordinator or
lock service between a daemon and the database. Coordination is the database
itself. Do not move the Store into the overlay or the overlay into the Store.

```text
 macOS host                                 Docker Linux VM (one kernel)
 ---------------------------------          ----------------------------------------------
 layerfs-sdk (thin)                         named volume, mounted in daemons only
   init:    create -> import -> seal          store.sqlite  (WAL; one writer at a time,
   install: stream the sealed file  ------>                  readers never blocked)
   control: mount / Exec / Commit /               ^   ^            ^   ^
            status / unmount                    write read        write read
                    ^                             |   |             |   |
                    |  authenticated         +----+---+-----+  +----+---+-----+
                    +--- control channel --> | sandbox A    |  | sandbox B    |  ...
                                             | = one daemon |
                                             | control      control commands
                                             | store/       1 write handle: Save, history, serials
                                             |              N read handles: objects, lengths
                                             | object cache immutable objects by id, shared
                                             | overlay/     overlay.sqlite, local mutable state
                                             | Workspace 1  one mount  -> many Bash   (S8)
                                             | Workspace 2  one mount  -> many Bash   (S8)
                                             +--------------+
```

- **Three levels:** many sandboxes share one Store; each sandbox (one daemon)
  serves many Workspaces; each Workspace is one mount serving many concurrent
  Exec commands.
- **Read path:** Workspace port → shared object cache → a read handle → SQLite
  snapshot. No transport.
- **Commit path, one thread:** capture → Content construction → Save batches on
  the write handle → stage and publish in one history transaction → local base
  install.
- **Host:** runs Project Init on macOS, seals one Store file, installs it once
  into the volume, then only sends control commands. It never opens the Store
  again and is never in the data path.
- **No retry.** A write takes the lock once with `BEGIN IMMEDIATE`. If another
  daemon holds it, the call returns a typed `Busy` before any effect. No busy
  handler, timeout, sleep, loop, readiness wait or writer gate anywhere.
- **Later databases.** SQLite on a mounted volume is the first provider. The
  daemon must stay unaware that the Store is SQLite (rules in §6).

Authority, in reading order:
[AGENTS.md](../../../../AGENTS.md), [core/AGENTS.md](../../../AGENTS.md), both
root handbooks, the [#303 index](../303/README.md), the
[integration contract](../303/06-cluster-one-integration.md), decisions
K28–K33 and rulings O-18–O-24 in
[08](../303/08-decisions-provenance.md), the
[deepest-file plan](SERVERLESS-STORE-PLAN-20261007.md) and the
[engine contract](../303/daemon-sqlite.md). Source establishes what is
implemented; these documents establish the target. Persistence still opens only
on macOS and the SDK runtime still exists in source.

## 2. Environment and database profiles

**Global Store, both profiles on WAL** (owner ruling O-21):

| Setting | Durable | Disposable |
| --- | --- | --- |
| `journal_mode` | `wal` | `wal` |
| `synchronous` | FULL (2) | OFF (0) |
| Survives | Process and kernel crash, as far as the VM disk honors a sync | Process crash only |
| `busy_timeout` | 0, set and checked | 0, set and checked |
| Unchanged | `page_size=4096`, `foreign_keys=1`, `wal_autocheckpoint=1000`, `journal_size_limit=4194304`, `cache_size=-2048`, `mmap_size=0`, `temp_store=2`, DEFENSIVE; `auto_vacuum=INCREMENTAL` only with acquisition tables | same |

- Open **verifies** `journal_mode = wal` and never sets or converts it. A Store
  in any other journal mode is refused. Existing Disposable Stores (memory
  journal) are regenerated, not converted.
- New profile identity strings replace the ones that say `macos`.
- **Disposable is the only profile you execute.** Durable must be implemented
  and must build at every checkpoint; its execution is
  `NOT_RUN — deferred by owner for Disposable-only development`.
- Linux links bundled SQLite (`rusqlite` feature `bundled` under
  `cfg(target_os = "linux")`, as `layerfs-overlay` already declares). The host
  uses the macOS system SQLite. Record both versions in the install manifest.

**Overlay database, unchanged:** `journal_mode=MEMORY`, `synchronous=OFF`,
`locking_mode=EXCLUSIVE`, `page_size=4096`, `auto_vacuum=NONE`, `mmap_size=0`,
one owner connection per daemon. Never `fsync` Workspace backing. Owner
defaults stay: 8 MiB credit, 64 KiB lifecycle reserve, 16 Workspaces, 16 jobs
and 2 lifecycle slots per Workspace.

**Placement rule that affects every test.** The Store needs shared memory and
POSIX locks on one kernel. It must sit on a named in-VM volume or the
container's own filesystem. **Never put a Store under the repository bind mount
(`/work`)**: that is a host share where WAL locking is not valid.

**Commands.** Run from the repository root so the ARM64 build inputs apply.

```bash
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p <package> --test <name> --no-run
```

```bash
docker run --rm --name <unique> -e CARGO_HOME=/work/core/target/cluster2-linux-cargo -e CARGO_TARGET_DIR=/work/core/target/cluster2-linux -e LAYERFS_CONSTRUCTION_WORKERS=1 -v /Users/yifanxu/Ephemeral-AI-Lab/layerfs:/work -v <named-volume>:/store -w /work sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6 <command>
```

- Export `LAYERFS_CONSTRUCTION_WORKERS=1` everywhere.
- The host has no `timeout`. Use `perl -e 'alarm shift; exec @ARGV' 100 <binary>`;
  inside the image use `timeout --kill-after=1s 100s`.
- Known traps: a source file rewritten in place can appear truncated inside the
  container until it is replaced by a new file (copy, then rename) — compare
  hashes before trusting a Linux build error; some Content tests read fixtures
  relative to their package directory; in zsh never name a variable `path`.

## 3. Target files and size

"Now" is the pinned counter on committed code. "After" is an estimate, not a
budget to hit by compressing code.

```text
core/crates/                                            now    after   change
  layerfs-api/sdk/src/                                 6,985    ~330   -6,655
    client/  runtime/               RETIRED            6,976       0
    init.rs      NEW  create, import, first Branch, seal,         ~90
                      manifest with a Store locator
    install.rs   NEW  stream the sealed file to a daemon          ~60
    control.rs   NEW  control commands, Exec output stream       ~170

  layerfs-api/core/src/             RETIRED              291       0     -291

  layerfs-bridge/src/                                  1,259    ~690     -569
    native/      channel kept; framing.rs RETIRED        746    ~540
    codec/  contract/               RETIRED              507       0
    control.rs   NEW  records for the control commands           ~145

  layerfs-daemon/src/                                  2,835  ~3,250     +415
    overlay/  service/              unchanged          2,328   2,328
    upstream/                       RETIRED              491       0
    store/       NEW, replaces upstream/                         ~430
      open.rs    one write handle + fixed read handles           ~105
      ports.rs   objects, lengths, serials over read handles      ~85
      bind.rs    mount: snapshot, root checks, Workspace bind    ~110
      commit.rs  Save scope, stage+publish, discard, install     ~122
    control.rs   NEW  serve the control commands                 ~200
    install.rs   NEW  write-then-rename the sealed Store          ~40
    exec.rs      S8, not yours                                   ~230

  layerfs-persistence/src/                             6,706  ~6,365     -341
    store/open.rs                   macOS gates removed
    store/seal.rs   NEW  checkpoint, close, verify one file       ~35
    backend/sqlite/allocation*.rs   DELETED              273       0
    backend/sqlite/{connection,transaction,profile}.rs   696    ~583
    history/{catalog,staging,commit}.rs  stage+publish   590    ~610

  layerfs-storage/src/              typed Busy; one    9,336  ~9,356      +20
                                    reservation per Save
  layerfs-history/src/              one catalog method   817    ~822       +5
  layerfs-workspace/src/            helper removal     3,861  ~3,830      -31
  layerfs-content/src/filesystem/qualify/  R2 qualifier, written, uncommitted
  layerfs-project/ layerfs-overlay/ layerfs-telemetry/   unchanged
```

**LOC rules, enforced on every commit:**

- New production files at most 999 physical lines; every `lib.rs`/`mod.rs` at
  most 200 lines and declaration-only.
- Each commit message carries
  `Production LOC: <before> -> <after> (delta <signed>)` from
  `tools/production_loc.py` (SHA-256
  `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`), first
  parent against the staged tree, with core, reference, active, excluded
  predecessor and excluded integration subtotals.
- Report the SDK, Bridge and upstream removal as **retirement of the
  host-mediated transport**, never as an algorithmic simplification.
- Expect a cut of about 7,000 production lines in the active workspace. Do not
  remove required validation or compress code to reach it.

## 4. Features expected before S8

Each row is closed only by its Disposable proof with a retained receipt.

| # | Feature | Proof |
| --- | --- | --- |
| F0 | R2 Content whole-root qualifier committed on its own | Its two test binaries pass in the Linux image (host already passes) |
| F1 | Persistence opens on Linux; both profiles WAL; allocation owner and public checkpoint removed | Linux create, publish, read, reopen |
| F2 | Typed `Busy`, before effect, session stays healthy | A real second **process** holds the write lock; the first gets `Busy`, changes nothing, then succeeds on a later explicit call |
| F3 | Readers are not blocked by a writer | A read completes while another process holds the write lock |
| F4 | Seal yields one file | No `-wal`, `-shm` or `-journal` remains; the file reopens; seal refuses while anything else holds the session |
| F5 | Host handoff | macOS Init → seal → install → a Linux daemon opens the installed Store and reads a complete root |
| F6 | Daemon `store/` adapter replaces `upstream/` | Bind, bounded root checks, object batches, file lengths and serial ranges over a real installed Store |
| F7 | Many Workspaces in one daemon | Several Workspaces bound at once over one Store and one overlay database, each reading independently |
| F8 | Store half of Commit, in-process | Committed, UpToDate, exact head-moved conflict, missing dependency; `Busy` leaves no stage row and no local change; a new Workspace binds the published root |
| F9 | Many sandboxes on one Store | Two daemon processes on one volume: concurrent Saves both land; a same-Branch race gives one winner and one exact conflict; a killed daemon leaves the Store openable and correct |
| F10 | Many callers on one Workspace, fair across Workspaces | S7 E4: finite arrivals per class and Workspace through the daemon engine, with runnable progress for every Workspace |
| F11 | Complete roots (Q1) on the installed Store | All path kinds, hard links, raw symlinks, metadata, a huge namespace, a sparse file and the 500,000,000-byte dense file, read through the daemon ports with full oracles. Actual files above 4 GiB: `NOT_RUN — waived by owner`; no file-size cap is introduced |
| F12 | Host-mediated transport retired | SDK `client/` and `runtime/`, Bridge `codec/`, `contract/` and `framing.rs`, API core, daemon `upstream/`, with their tests and examples removed or ported; everything still builds and Clippy is clean |
| F13 | Control channel without Exec | mount, Commit (Store half), status, unmount, fork and history read over the authenticated native channel |
| F14 | Remaining S7 accounting | E1 registration, E2 whole-operation accounting and the family-carrying receipt schema, E3 phase resources. Anything that measured the removed transport is withdrawn, not ported |
| F15 | Root qualification on an installed Store | The R2 qualifier runs in a daemon over overlay scratch as one explicit paid step; mount never walks the tree |

**Order.** F0 → F1–F4 → one Init measurement (§5) → F6, F7 → F8 → F12 → F5,
F13 → F9, F11, F15. F10 and F14 are independent of the Store and can be taken
whenever a Store step is blocked.

**Not yours.** S8: FUSE, Bash Exec, hiding the Store from Bash, real concurrent
Bash, kernel cache behavior. S10: capturing a live Workspace's changes and
normalizing its namespace into a Commit — before S8 you prove the Store half
with changes built directly through Content. S11–S13 and retirement of the root
`crates/` reference. Not built at all: remote Save, host application assembly,
restart custody.

## 5. Performance gate

Nothing passes on wall time alone. The gate has two parts.

**A. Count gates. Exact, deterministic, required for every feature above.**

| Gate | Requirement |
| --- | --- |
| Repeated mount | Store reads are a small constant (root object, root inode path, root listing page, portable metadata), independent of namespace size. No scan, copy or materialization |
| Warm base read | Zero Store calls for an object already in the shared cache |
| Cold base read | Batched by id; never one Store call per object when a batch is available |
| Reads under a writer | A base read completes while the write handle or another process holds the write lock |
| Commit write transactions | Exactly one id reservation per Save, plus one per publication batch, plus one for stage and publish. Report the count |
| Scaling | Store calls and rows grow linearly in changed objects and bytes. Reject anything quadratic or proportional to the whole namespace on an incremental path |
| `Busy` | Returns immediately; no sleep, loop or second attempt exists in source |
| Residency | No resident container grows with total file, namespace or Commit size; windows stay bounded |
| Lock hold | No write transaction spans a file, a Save or a Commit |

**B. Timing gates. Only against limits the owner has already set.**

- Project Init keeps its frozen limits (30 s and 19 s, with the 1.10×
  arithmetic) and its **eight recorded speed failures and eight strict-
  allocation failures stay exactly as recorded**. Strict-allocation selections
  become `NOT_RUN — mechanism removed`.
- Run **one** new Init measurement right after F1–F4. It decides one thing:
  if importing under WAL is slower than the retained memory-journal receipts,
  Init builds under a memory journal and converts to WAL at seal; otherwise WAL
  throughout stays.
- Do not invent a new numeric threshold. Where no owner limit exists, record
  the timing as diagnostic and ask.
- Budgets: a complete performance command at most 15 s (declared exceptions up
  to 25 s); an independent proof under 10 s.
- One sample per case and arm; no best-of; declared and enforced cache state;
  `--setup clone` for post-initialization cases; fresh append-only outputs;
  pinned source, build, binary, image and workload identities.
- Measure speed and storage together. Reject about 50% speed loss for about 5%
  storage benefit.
- The benchmark hosting rule still routes the global Store to the host. No new
  measurement is admitted until that text is aligned with the owner direction.
  Raise it; do not work around it.

Read the [measurement workflow](../../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md) and
[report template](../../../../benchmark_agent_report.md) before any timing run.

## 6. Rules that keep a later database swap cheap

1. Daemon `store/` uses only the Storage and History ports and opened handles.
   No SQLite type, file path, WAL or lock concept appears in it.
2. Every Store call is one of two shapes: an idempotent put or get of immutable
   content by id, or one conditional transition with exact expected state.
3. `Busy` is a typed outcome of every write.
4. Reads are batched by id.
5. Publication means "bytes are stored before anything references them".
6. The install manifest carries a provider kind and a Store locator; today the
   locator is a path on the volume.
7. Nothing is collected or rewritten in place. No GC under several writers.

## 7. Fast iteration rule

The loop for every change:

1. **Plan the files first.** Before each checkpoint, write its deepest-file
   plan: files reused, files changed, new files, the requirement behind each.
2. **Build before running.** `--no-run` first, so compilation is never mistaken
   for a slow test.
3. **Run the smallest thing.** One package, one test binary, one invocation.
   Host first; then only the affected binaries in the Linux image.
4. **Every test invocation has a wall stop of at most 120 s; use 100 s.** A test
   that reaches it has FAILED as a hang. Diagnose from source and its bounded
   output. Never background a test, never repeat it unchanged, never loop it.
5. **Diagnose before rerunning.** Read the failure, find the cause, change
   something, then run once. A failed run is a receipt you keep.
6. **Receipts are append-only.** Each run writes a new numbered file under
   `core/docs/issues/307/checks/<checkpoint>/` with timestamp, binary hash, wall
   stop and exit code. Keep failed, ineligible and unrun selections.
7. **Disposable only.** No new Durable test, proof, diagnostic or measurement.
8. **Whole-crate checks once per checkpoint,** at the final identity: Clippy
   with `-D warnings`, `fmt --check`, `python3 -B core/tools/check_product_boundary.py`.
9. **One reviewable local commit per checkpoint,** with docs updated in the same
   commit and the exact LOC line. Small commits beat large ones.
10. **No timing during iteration.** Exploratory timing is diagnostic. A
    measurement is a separate, registered step (§5).
11. **You alone run Cargo, tests and measurements,** one at a time. Subagents
    may read and review; they do not build, test or edit.
12. **Tests cannot wait forever.** Bounded waits; a spawned thread or process
    must exit even if its peer panics. Multi-process tests clean up their
    children.

## 8. Constraints

- Authorized: local implementation, local commits, scoped verification.
  **Not authorized:** remote push, release, deployment, new worktrees, checkout
  reset, early retirement of the root `crates/` reference.
- Leave untouched and unstaged, and do not treat as authority:
  `core/docs/issues/307/HANDOFF-S7-S9-RESUME-20261006.md` and
  `core/docs/issues/307/S7-S9-SPEED-TEST-PLAN.md`.
- Do not interrupt or prune unrelated processes, worktrees or containers.
  Preserve containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59`,
  `d2550144998b`.
- E04 is functionally closed on its recorded host-mediated topology. Do not
  reopen or rerun it. Preserve its trace and every historical receipt and
  verdict; never relabel one.
- No new dependency when an existing crate supplies the capability. No patch,
  fork or vendoring of third-party code beyond the authorized fuser 0.18.0
  signed-timestamp patch. Locked builds only.
- No test-only branches, hooks or benchmark-selected behavior in product source.
- One attempted operation: no retry, replay, refresh or guessed cleanup.
  Preserve exact failures and unknown outcomes.
- No CI and no preflight wrapper exists; do not create one.
- Commit messages end with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## 9. State you inherit

- `21ff451fa` and `c6553e417` are documentation commits recording the direction,
  the rulings and the plan. `af963a718` is the last code commit (S7 checkpoint 4).
- **Uncommitted, to commit first (F0):** the R2 Content qualifier —
  `core/crates/layerfs-content/src/filesystem/qualify/`, a sequential reader in
  `filesystem/inode/read.rs`, small visibility changes in `filesystem/state/`,
  `core/crates/layerfs-content/tests/filesystem_qualify.rs` and
  `core/crates/layerfs-daemon/tests/root_qualification.rs`. Host: 10 Content
  bodies and 1 daemon body pass over real overlay scratch; Clippy, formatting
  and the boundary guard are clean. Linux: **NOT_RUN** — two builds failed on
  the stale-mount trap, the third was interrupted. Receipts 01–11 are under
  `core/docs/issues/307/checks/s9-root-qualification-20261007/`; keep them and
  continue the numbering.
- Persistence has **no** uncommitted change. An earlier start on it was reverted.
- Three read-only source reviews fed the plan. Their findings are estimates and
  source reading, not builds or measurements; verify each against source as you
  reach it.

## 10. Final handoff

When you stop, state plainly:

- each feature F0–F15 closed, with its exact proof and receipt path;
- every failed, incomplete, ineligible, unrun, owner-deferred or
  dependency-gated item, including all Durable execution;
- source, build, binary, image, cache and profile identities;
- original custody and cleanup outcomes and any terminal unknown;
- exact production LOC for every commit, with the retirement labelled;
- what S8 and S10 still need from this work;
- the actual working-tree state and everything preserved.

Persist through useful independent work. Do not stop at component counts,
compiled examples, source-only APIs or diagnostic consistency. Ask the owner
only for a decision that is genuinely theirs.

Start with F0: refresh the container's view of the changed Content sources,
build the two R2 test binaries in the Linux image, run each once with a 100 s
stop, then commit the qualifier on its own.
