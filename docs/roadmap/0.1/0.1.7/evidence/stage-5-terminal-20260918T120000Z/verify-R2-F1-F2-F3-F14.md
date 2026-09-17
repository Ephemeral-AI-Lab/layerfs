# Independent verification: R2-F1, R2-F2, R2-F3, R2-F14 (harness case selection)

Verifier: subagent, read-only on tracked files. Task stated the repository is frozen at
`99743b2cf3a869b7d8897a1f16b82d742aeedc40`; that full hash does not exist as a git object
(`git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` → `fatal: could not get object
info`, exit 1). The actual HEAD is `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (`git rev-parse
HEAD`), which shares the 9-character prefix `99743b2cf`; this verification ran against that
tree. The working tree was clean at session start (`git status --short` empty). During the
session sibling verification agents modified tracked files (`codec.rs` doc comment, three
docs) and added their own `verify-*.md` receipts; none of those changes are mine, and none
of them predate any of my measured runs (timeline and provenance below). All four verified
examples and the telemetry crate are byte-identical between the round-3 tree `2fe2a4642`,
HEAD, and the current worktree (`git diff` empty for each file), so every path:line citation
below is valid for all three. Claims fixed in round-3 commit `5e2a20a0c` ("fix(stage5): make
every harness case select the work its row names"), an ancestor of HEAD. No roadmap report,
suite result, commit message or receipt below is treated as evidence by itself; every row is
reproduced from source, tests or a fresh run.

## Provenance of the runs (why mid-session sibling edits do not taint them)

My example runs executed 02:50:31–03:03:20 local time against the `core/target/debug/examples`
binaries built at 02:43 from the then-clean tree: my own `cargo build … --examples` at ~02:50
reported `Finished … in 0.03s` (a fingerprint no-op, so no source in the dependency graph had
changed since 02:43), and `git status` was clean at that moment. The sibling edits landed
after my last run (`codec.rs` mtime 03:02:51; the examples were rebuilt by a sibling at 03:06).
To remove all doubt, I exported a pristine tree with `git archive HEAD` to `/tmp/vf-head`,
rebuilt there (`cargo +1.85.1 build --manifest-path /tmp/vf-head/core/Cargo.toml --workspace
--locked --examples`, exit 0), and re-ran every verification command from that build: all
roots, counters, labels, node counts and exit codes are identical to the runs recorded below.
The HEAD-build re-runs are the authoritative receipts; the `core/target` runs are corroborating.

## Verdicts

| Row | Claim | Verdict |
| --- | --- | --- |
| R2-F1 | `filesystem_timing_c1 --case attributes` applies a real attribute change inside the timed region; its printed counters differ from `--case empty` | **PASS** — with one documented nuance: the claim's parenthetical numbers ("6 emitted, 2 read") are an aggregate across three printed lines, not what the objects-counter line itself prints (see below) |
| R2-F2 | `measure_filesystem --mode c2` honours `--case`: six cases → six distinct case roots, banner matching what ran; c2 not special-cased | **PASS** |
| R2-F3 | `measure_edits --mode c2 --case small` builds its C1 objects in a labelled untimed preparation step, read-back in its own labelled region, printed exclusion line true | **PASS** — with one minor caveat: one `println!` reporting the save outcome executes inside the timed closure, so "and nothing else" is not literally exact (nothing the line says is excluded is timed) |
| R2-F14 | `measure_components` exits non-zero on a clipped (incomplete) timing report, like the Stage-5 pair | **PASS on wiring + telemetry contract** — a live clipped-run receipt is **unreachable with any legal input** (stated plainly below); the round-4 README contains no unreachability statement |

## Commands run (all from the repository root unless noted; exit codes)

| # | Command | Exit | Key result |
| --- | --- | --- | --- |
| C1 | `git rev-parse HEAD` | 0 | `99743b2cff2470e6634874d7ee14b9d37d0ba16e` |
| C2 | `git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` | 1 | object does not exist (claimed frozen hash is wrong beyond the 9-char prefix) |
| C3 | `git status --short` (session start) | 0 | clean |
| C4 | `git merge-base --is-ancestor 5e2a20a0c HEAD` | 0 | round-3 fix is an ancestor |
| C5 | `cargo +1.85.1 build --manifest-path core/Cargo.toml --workspace --locked --examples` | 0 | `Finished dev profile … in 0.03s` (cached; binaries from the clean tree) |
| C6 | `core/target/debug/examples/filesystem_timing_c1 --case empty --output /tmp/vf1-empty` | 0 | `objects read 0 … emitted 1 bytes 129`, `report_nodes 7`, `prepared_objects 3` |
| C7 | `core/target/debug/examples/filesystem_timing_c1 --case attributes --output /tmp/vf1-attr` | 0 | `objects read 0 … emitted 5 bytes 507` + attribute lines, `prepared_objects 11` |
| C8 | `for c in empty directory-update inode-update hardlink-move subtree-remove attributes; do core/target/debug/examples/measure_filesystem --mode c2 --case $c --output /tmp/vf2-$c; done` | 0 ×6 | six distinct `case_root` values (below) |
| C9 | `core/target/debug/examples/measure_filesystem --mode pipeline --case directory-update --output /tmp/vf2-pipe-dir` | 0 | same case root and readback as the c2 run of the same case |
| C10 | `core/target/debug/examples/measure_edits --mode c2 --case small --threshold-bytes 131072 --output /tmp/vf3-small` | 0 | labels + exclusion line below; `timings: 4 nodes at 2 levels` |
| C11 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-telemetry --test timer --locked` | 0 | `21 passed; 0 failed` |
| C12 | `head -c 8388608 /dev/urandom > /tmp/vf14-input.bin` | 0 | exactly 8 MiB (the maximum legal input) |
| C13 | `core/target/debug/examples/measure_components --mode c1 --input /tmp/vf14-input.bin --timings /tmp/vf14.json` | 0 | `timings: 5 nodes at 3 levels` |
| C14 | `core/target/debug/examples/measure_components --mode pipeline --input /tmp/vf14-input.bin --store /tmp/vf14-pipe.sqlite --timings /tmp/vf14-pipe.json` | 0 | `timings: 59 nodes at 5 levels` |
| C15 | `core/target/debug/examples/measure_components --mode c2 --input /tmp/vf14-input.bin --store /tmp/vf14-c2.sqlite --timings /tmp/vf14-c2.json` | 0 | `timings: 4 nodes at 2 levels` |
| C16 | `core/target/debug/examples/measure_components --mode c1 --input /tmp/vf14-input.bin --timings <pre-existing file>` | 1 | `Error: "--timings … already exists"` — proves `main`'s `Result` plumbing exits non-zero |
| C17 | `head -c 8388609 /dev/urandom > /tmp/vf14-toobig.bin` then C13 with it | 1 | `Error: "demo input is 8388609 bytes; this example refuses inputs above 8388608"` |
| C18 | `git archive HEAD \| tar -x -C /tmp/vf-head` | 0 | pristine HEAD export |
| C19 | `cargo +1.85.1 build --manifest-path /tmp/vf-head/core/Cargo.toml --workspace --locked --examples` | 0 | fresh build from the export |
| C20 | HEAD-build re-runs of C6, C7, C8 (all six cases), C9, C10, C13, C14, C15 (binaries under `/tmp/vf-head/core/target/debug/examples/`, fresh `--output`/`--timings` paths) | 0 each | **all outputs identical** to C6–C17: same roots, counters, labels, node counts |
| C21 | `cargo +1.85.1 test --manifest-path /tmp/vf-head/core/Cargo.toml -p layerfs-telemetry --test timer --locked` (workdir `/tmp/vf-head`) | 0 | `21 passed; 0 failed` |
| C22 | assorted read-only inspection: `git show 5e2a20a0c~1:<file>`, `git diff 2fe2a4642 HEAD -- <file>`, `git log`, `grep`/`sed`/`read` of the cited sources and receipts | 0 | citations below |

## Findings — R2-F1 (attribute change inside the timed region)

**The patch is inside the timed region.** `core/crates/layerfs-content/examples/filesystem_timing_c1.rs`:
- `:430-431` — `let started = Instant::now();` then `let (outcome, report) = layerfs_telemetry::timer::Timing::record("filesystem.update", |timing| …` opens the timed region; the closure body spans `:431-489` and `:490` takes `started.elapsed()`.
- Inside that closure: the update (`:441 update_filesystem_timed(…)`), the inode-table walk that finds the root inode's metadata root (`:458 let (value, inode_page) = root_inode(&staged, updated.0.value.inode_table())?;`), and the patch itself:
  - `:461-470` — `let patched = apply_patches(&staged, &mut objects, value.metadata_root, &[AttributePatch::Set { key: AttributeKey::new("user.example".to_owned(), b"note".to_vec())…, value: b"timed attribute value".to_vec(), }])?;`
- `:444-450` comment, verbatim: "`attributes` is the case named for an attribute change, so the change happens **inside** this timed region … Its work is charged to the same row counters as the update's".
- The report is printed only afterwards (`:592 print!("{text}");`), and the verification read-back `attribute_readback(&bag, attribute.root)` at `:567` is also after the timed region — it proves the timed work stored the value, it is not itself the change.
- The change is real, not a no-op: the base tree for `attributes` carries a real stored attribute tree (built in `base()` at `:218-258` with a real `portable`/`mode` entry), the patch sets a **new** key `user.example`/`note` to the 21-byte value `timed attribute value`, and the read-back through the public `visit_keys` + `read_value` paths returns exactly `readback 21 bytes "timed attribute value"`.

**The round-2 finding was real.** In `5e2a20a0c~1`'s version of the same file, the timed
`Timing::record` closure ended around old line 308, `print!("{text}")` sat at old line 376,
and `apply_patches` was called at old line ~423 — after the report was printed, outside the
timed region, under the comment "One attribute patch, timed separately and labelled as its
own operation". Round 3 moved it inside; that is the fix under test.

**The counters differ from `--case empty`** (C6 vs C7, both reproduced identically by C20 from the pristine HEAD build):
- empty: `objects read 0 waves 0 bytes 0 emitted 1 bytes 129`; `directories: … top-level 0`; `prepared_objects 3`.
- attributes: `objects read 0 waves 0 bytes 0 emitted 5 bytes 507`; `directories: … top-level 1`; `prepared_objects 11`; plus two lines only the attributes row prints:
  - `attribute reads: inode page lookup 20b4060746ac…, base attribute page 541c65f6a7cf…` (source `:553-562`)
  - `attribute_patch: base tree 541c65f6… set 1 removed 0 preserved 1 base entries 1 base pages 1 values emitted 1 | readback 21 bytes "timed attribute value"` (source `:568-577`)
- The empty row also matches the claim's own numbers exactly: "1 object emitted, 0 read".

**The numbers nuance (against the claim's parenthetical, and against the round-3 commit
message).** The task's paraphrase "(attributes: 6 emitted, 2 read)" — like commit `5e2a20a0c`'s
"the attributes row prints 2 objects read / 6 emitted" — does **not** match the objects-counter
line, which prints `read 0 … emitted 5`. The figures are only correct as an aggregate across
three lines: 5 objects emitted (objects line) + 1 value emitted (`attribute_patch … values
emitted 1`) = 6 emitted things; 2 canonical page reads named on the `attribute reads:` line
(the inode page lookup and the base attribute page — the same page the patch counters report
as `base pages 1`). Root cause, verified in source: `apply_patches` reads base pages through
its `reader` argument (`core/crates/layerfs-content/src/filesystem/attributes/patch.rs:66-83`,
`PageCursor::new(reader, base, &mut work)` at `:83`) and `root_inode` reads through
`Bag::read_canonical` (`filesystem_timing_c1.rs:362`), so both reads bypass the
`FilesystemObjects` boundary that feeds `objects_read`; the example's own comment at
`:553-557` says so: "Without that second boundary the row would print zero reads for work
that read a stored page." The round-3 evidence README's phrasing — "`--case attributes`
applies a real attribute patch inside the timed region: 5 objects emitted and 2 canonical
pages read" (`stage-5-terminal-20260918T020000Z/README.md`, "What this directory proves"
table) — is the accurate one; the commit message's phrasing overstates which counter prints
them. The row's substance (real change inside the timed region; counters differ from empty)
holds regardless, and the round-3 receipts (`case-selection/c1-attribute-case/{empty,attributes}.log`)
match my runs byte-for-byte on every counter line.

## Findings — R2-F2 (`measure_filesystem --mode c2` honours `--case`)

**Six distinct case roots** (C8, all exit 0; identical from the HEAD build in C20; identical
to the round-3 receipts `case-selection/c2-*.log` in the same case order):

| case | case_root |
| --- | --- |
| empty | `91d215210371a649b6d7616359e8d6d01249e3e909f71afba8cda763f11bc4e6` |
| directory-update | `681ab7e59a612a51c7ba5fe8699f7e65e54a18c42c744ddb0a1b230069558598` |
| inode-update | `3cc8eef1b962e53e7e16740f20b27067be60841e65a60b9a80bc8f311610f8c1` |
| hardlink-move | `2d81c8c734563197d3fea79b1d8f39f5b3c19d3b68a7e46a34ac8a36bc1494aa` |
| subtree-remove | `4a068563a4ced4da8f72428a7c42df4e3bad8be8bc9c1717e6969e140b88b2d0` |
| attributes | `622262df37bd1a4ce1da72df3548fca772851a998be7705aec3998c05ad2907f` |

`sort | uniq -c` over the six values: every value count 1 — all distinct. The empty root also
equals the c1 receipt's root (`case-selection/c1-empty.log`), consistent content addressing.

**The banner matches what ran.** Each banner (`core/crates/layerfs-storage/examples/measure_filesystem.rs:120-130`)
names the change set `changes()` builds for that case (`:138-210`), and the run prints the
applied work and a case-specific read-back:
- empty — "update: none; the fixture tree is re-emitted unchanged"; `case_work applied untimed: directories 0 bindings +0 -0 inode updates 0 fixture entries 0`; `d_first4=[]`.
- directory-update — "rename the first 10 names in /d"; `directories 1 bindings +10 -10`; read-back `d_first4=[f0010->13,…]` (the first ten `f` names are gone from the front).
- inode-update — "replace 10 inode values under unchanged bindings"; `inode updates 10`; read-back still `d_first4=[f0000->3,…]`.
- hardlink-move — "move 10 files from /d to /"; `directories 2 bindings +10 -10`; read-back `top_level=[d->2,m0000->3,m0001->4,…]`.
- subtree-remove — "remove 10 bindings from /d"; `bindings +0 -10`; read-back `d_first4=[f0010->13,…]`.
- attributes — "rename 4 names in /d and replace their inode values"; `directories 1 bindings +4 -4 inode updates 4`; read-back `d_first4=[a0000->3,…]`.

**c2 is not special-cased.** The case selection runs in `main` before the mode dispatch —
`:331-333`: `let entries = entries_for(&config.case); … let (directories, inodes) = changes(&config.case, entries);` — and the same case-selected `input` feeds all three arms (`:347-360`). The c2 arm (`:349-358`) applies the case's own change set **outside every timed region** via `apply_case` (doc at `:409-414`: "Applies the case's own change set, outside every timed region."; the update runs under `disabled(…)` = `Timing::disabled` at `:430`), prints `case_work applied untimed: …` (`:356`), and only then calls `run_c2`, whose timed region (`:486-497`, `Timing::record("storage.save", …)`) covers `store.begin_save` / `accept` / `finish` only; `fresh_store` (`:478`) and the `FinalizedObject` vector (`:480-484`) precede it, and `require_complete(&report)` gates the row (`:515`). A pipeline run of the same case (C9) prints the **same** case root `681ab7e5…` and the same read-back, confirming the selection is shared, not a c2-only code path.

**The round-2 finding was real.** `git show 5e2a20a0c~1:…/measure_filesystem.rs` shows the
pre-fix dispatch `"c2" => run_c2(&config, &bag)` — the case-selected `input` was never passed
to c2, so c2 admitted only the shared fixture (the commit message: "four cases printed
byte-identical admission rows while printing four different case banners").

## Findings — R2-F3 (`measure_edits --mode c2` preparation and read-back)

**What the run prints** (C10, exit 0; identical from the HEAD build in C20):
- `preparation untimed: two whole-file objects encoded in 2455500 ns; only their admission is timed below`
- `readback separately labelled elapsed_ns 5420459`
- `readback: verified 65559 canonical bytes through an independent read wave`
- `exclusions: no C1 file construction and no full-file object collection is timed; `storage.save` covers Store creation, the begin/accept/finish calls and nothing else`

**Source confirms every label** (`core/crates/layerfs-storage/examples/measure_edits.rs`, `run_c2` at `:410-517`):
- Both C1 encodes precede the timed region: `:435` `let expected = layerfs_content::file::encode_whole_file(&policy.capacities(), &changed)?;` and `:442` `let base_canonical = layerfs_content::file::encode_whole_file(&policy.capacities(), base_raw)?;`, with the `FinalizedObject` construction at `:443-455` and the wall clock `construction_started` at `:441`. Comment `:436-440`: "The two canonical objects are C1 construction, and this lane declares C1 construction excluded from `storage.save`. They are therefore built here, outside every timed region".
- The timed region `:470-485` (`timed(options, "storage.save", …)` → `Timing::record` when timing is on, `:110-123`) contains exactly `Store::create` (child `store.create`, `:471`), `store.begin_save` (child `storage.begin`, `:472`), `operation.accept(base)` / `accept(dependent)` (`:473-474`), `operation.finish` (child `storage.finish`, `:475`) — and the outcome `println!` at `:476-483`.
- The read-back is its own labelled region **after** the timed closure resolves: `:491` `let read_started = std::time::Instant::now();` (after `:486 let store = result?;`), `:492-499` `Timing::disabled("measure.readback", |scope| … store.read_batch(&[dependent_id], scope.child("storage.read")))`, printed at `:500-503`. Comment `:488-490`: "The authenticated read-back is its own labelled region, not part of the save".
- Machine check: the saved `c2-save.json` contains only `storage.save` → `{store.create, storage.begin, storage.finish}` (4 nodes, 2 levels) — no read span exists in the recorded report.

**Falsification attempt.** Is anything timed that the exclusion line says is excluded?
- "no C1 file construction … is timed" — true: both `encode_whole_file` calls sit outside the closure.
- "no full-file object collection is timed" — true: the timed region performs no collection; it admits two prepared objects.
- "`storage.save` covers Store creation, the begin/accept/finish calls and nothing else" — true of every category of work, with one literal caveat: the `println!` reporting the save outcome (`:476-483`) executes inside the timed closure, so the timed span also covers one stdout write (~80 bytes, the finish call's result). This is a reporting artifact, not excluded work being smuggled into the timer; it is the sole respect in which "and nothing else" is not exact. No C1 work and no read work is inside the timed region.

**The round-2 finding was real.** `git show 5e2a20a0c~1:…/measure_edits.rs` shows the pre-fix
`run_c2` calling `FinalizedObject::new(…, encode_whole_file(…))` twice **inside** the timed
`storage.save` closure and `store.read_batch(&[dependent_id], save.child("storage.read"))`
inside it too, while the row printed "exclusions: no C1 file construction and no full-file
object collection is timed" — the exclusion line was false before the fix and true after it.

## Findings — R2-F14 (`measure_components` fails a clipped report)

**The wiring** (`core/crates/layerfs-storage/examples/measure_components.rs`):
- `:145-158`, inside `save_timings`, after the JSON is written:
  ```rust
  if report.is_incomplete() {
      // A clipped tree is a hard failure for a measured row, exactly as it is
      // for the Stage-5 pair: … the process exits non-zero and says so …
      return Err(format!(
          "timings: INCOMPLETE - the node budget clipped this tree to {} nodes; {} must not be \
           quoted as this run's work",
          report.node_count(),
          path.display()
      ).into());
  }
  ```
- `save_timings` is the tail call of every mode: `run_c1` `:195`, `run_c2` `:268`, `run_pipeline` `:327`; each returns `Result<(), Failure>`; `main` (`:345-351`) returns `run()`'s result, so any `Err` exits non-zero through Rust's `Termination`. Demonstrated empirically through the identical plumbing (C16, C17): both error paths print `Error: …` and **exit 1**.
- Same shape as the Stage-5 pair: `measure_filesystem.rs:316-326` (`require_complete`: `if report.is_incomplete() { return Err("timings: INCOMPLETE - the node budget clipped this tree to {} nodes; …") }`, called at `:515` and `:580`) and `measure_edits.rs:299-310` (same gate, comment "D1: a clipped tree is a hard failure for a measured row, not a note"). `filesystem_timing_c1.rs:596-601` goes further and calls `std::process::exit(2)`.

**The telemetry contract, proven by the crate's own tests** (C11/C21: `21 passed; 0 failed`, exit 0):
- Budgets: `core/crates/layerfs-telemetry/src/timer/recording.rs:14` `pub const MAX_NODES: usize = 1_024;`, `:17` `MAX_DEPTH: u8 = 32`. A child that does not fit is not created and the parent and ancestors are marked incomplete (`recording.rs:139-156`). `TimingReport::is_incomplete` is true only for a measured report with missing detail (`report.rs:289-293`).
- `core/crates/layerfs-telemetry/tests/timer.rs:321-350` **`node_budget_clips_detail_and_keeps_running_the_operation`**: requests `MAX_NODES` children, asserts the last one is clipped, the operation kept running (`bodies == MAX_NODES`), `report.node_count() == MAX_NODES`, and `:345 assert!(root.is_incomplete());` — the crate's own proof that a clipped report has `is_incomplete()` true.
- `:367-380` `depth_budget_clips_detail_and_keeps_running_the_operation`: `:378-379 assert!(leaf.is_incomplete()); assert!(root.is_incomplete());`.
- `:217-247` `disabled_and_clipped_reports_are_distinguishable`: `:234-235` (clipped arm) `assert_eq!(clipped.completeness(), Completeness::Clipped); assert!(clipped.is_incomplete());`.

**Reachability of a clipped run through the CLI: NOT reachable with any legal input.**
- The input limit is 8 MiB: `measure_components.rs:28` `const DEMO_INPUT_LIMIT: u64 = 8 * 1024 * 1024;`, enforced in `read_input` at `:113-121` (C17: an 8 MiB + 1 byte input is refused, exit 1).
- With the maximum legal input (8 MiB of `/dev/urandom`, C12):
  - `--mode c1` (C13): **5 nodes at 3 levels**, exit 0. The tree is structurally fixed — `c1.only → content.construct → {content.probe, content.chunk, content.emit}` — because the construct path creates a fixed span set regardless of input size (`core/crates/layerfs-content/src/file/content.rs:240-263`: one `content.chunk`, one or two `content.emit`; the chunking loop itself creates no per-chunk spans).
  - `--mode c2` (C15): **4 nodes at 2 levels** (the saved save-tree; fixed).
  - `--mode pipeline` (C14), the deepest tree: **59 nodes at 5 levels**, exit 0. The read path adds ~3 nodes per payload wave (`mapping.payload → {storage.decode, storage.read}`), and waves are capped at 32 objects / 1 MiB (`core/crates/layerfs-content/src/file/mapping/read.rs:24` `READ_WAVE_OBJECTS: usize = 32`, `:33` `READ_WAVE_BYTES = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES`). Even the theoretical worst case — every chunk at the 8 KiB CDC minimum (`core/crates/layerfs-content/src/file/cdc/gear.rs:13`), i.e. 1,024 chunks → 32 waves → ~96 wave nodes — plus the fixed spans and a handful of `mapping.navigate` nodes stays far below the 1,024-node budget, and the observed depth (5) is far below 32.
- Exit code in every legal run: 0.

**Does the evidence satisfy the row?** The original round-2 row demanded "it exits non-zero
like the Stage-5 pair, **with a receipt of a clipped run**"
(`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-terminal-handoff-20260917.md:163`). The
round-3 claim's receipt is "source; `check-clippy.log`"
(`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:600`) — no clipped-run receipt
exists, and as demonstrated above **none can exist through this CLI with any legal input**.
The wiring (source, plus empirically proven error propagation) and the telemetry contract
(unit-tested clipping → `is_incomplete()` → the gate) together satisfy the substance of the
row — a clipped report would fail the run with a non-zero exit exactly like the Stage-5 pair —
but the row's literal "receipt of a clipped run" requirement is unmeetable, and neither the
round-3 report nor the round-4 README (`stage-5-terminal-20260918T120000Z/README.md`, which
contains no R2-F14 or unreachability statement — grep for
`clipped|unreachable|INCOMPLETE|F14|measure_components` returns nothing) records that
unreachability. That is a documentation gap, not a wiring defect.

## UNVERIFIED

1. **A live clipped-run receipt through `measure_components`** — impossible with any legal input (demonstrated above: 5/4/59 nodes for the maximum legal input against a 1,024-node budget), so the `is_incomplete` branch of `save_timings` was never actually executed. Its non-zero exit is established by source plus the empirically identical error-propagation path (C16/C17), not by triggering the branch itself.
2. **The claimed frozen-commit hash** `99743b2cf3a869b7d8897a1f16b82d742aeedc40` does not exist as an object; verification is against actual HEAD `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (9-char prefix match). If a different commit was intended, this evidence does not describe it.
3. **The full case × mode matrix (18 runs)** of the WP-A instruction was not run: my scope covered the six c2 cases plus one pipeline case for `measure_filesystem`; c1-mode case selection is verified by source (`run_c1` consumes the same case-selected `input`, `measure_filesystem.rs:348`) and by the six round-3 c1 receipts, not by fresh c1 runs.
4. **`measure_edits` modes other than c2**, and the other WP-A examples (`measure_pooled`, `edit_timing_c1`, `memory_ledger`, `filesystem_primitives_candidate`) — outside these four rows; their clipped-report gates were only cross-checked by the sibling verifier's grep (`measure_pooled.rs:193`), not by me.
5. **The round-3 diagnostics client `s5check`** was not re-run by me (the sibling `verify-R2-F12-F13.md` ran it; I used that file as context only, never as evidence).
6. **The `elapsed_ns` magnitudes** are not compared against any budget — these examples are wiring demonstrations, and no performance claim is made or checked here.
