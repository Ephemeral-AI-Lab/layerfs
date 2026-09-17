# SA review: telemetry and timing (Stages 0-5 shared)

- Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main`, HEAD
  `f288d2af7ecdc7e00f7df153073398d333461aa3`.
- Scope: `core/crates/layerfs-telemetry/src/**`, every timing use inside
  `core/crates/layerfs-content/**` and `core/crates/layerfs-storage/**`, and the
  measurement examples under those packages' `examples/`.
- Method: source reading only. No cargo build, test, clippy or benchmark command was
  run for this review (a build could not add evidence for the questions asked, and the
  task forbids resource-heavy commands). Every claim below is a `path:line` quotation.
- Claim document read first, as required:
  `docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md` (538 lines). Statements
  from that document are labelled "the report claims"; they are never used as evidence
  for what the code does.
- All paths below are relative to the repository root.

## Verdict summary

| # | Required check | Verdict |
| --- | --- | --- |
| 1 | One real operation hierarchy reused by content/storage | PARTIAL - one timer in product source; the measured argument path (examples, and the qualified component-comparison arm) carries three independent clocks |
| 2 | Disabled path runs the same production body | PASS |
| 3 | Bounded retention and clipping | PARTIAL - bounds are real and enforced; one harness does not fail a clipped row, and `is_incomplete()` conflates "disabled" with "clipped" |
| 4 | Honest errors | PARTIAL - per-node outcomes exist, but a child that unwinds is reported `outcome: Ok`, `elapsed_ns: 0` |
| 5 | Construction vs persistence completion distinguished | PARTIAL - distinguished only where the harness creates two spans; `measure_filesystem --mode pipeline` prints one number for both |
| 6 | No sync in report output; JSON error handling; caller-supplied destination | PASS on no-sync and destination; PARTIAL on "keep the save result separate from the product result" |
| 7 | C1-only / C2-only / integrated bodies per example | PARTIAL - see the per-example table; three concrete mismatches |
| 8 | Overlap / containment / CPU-by-subtraction | PASS with caveats (no subtraction anywhere; containment is real and documented, but C2 work hides inside C1 spans) |
| 9 | Actual flags and defaults | PASS as a factual list; no example implements `--help` |

---

## 1. One real operation hierarchy (content/storage reuse of one timer)

**Verdict: PARTIAL.**

Both candidate packages depend on the single timer crate:

- `core/crates/layerfs-content/Cargo.toml:11` `layerfs-telemetry = { path = "../layerfs-telemetry" }`
- `core/crates/layerfs-storage/Cargo.toml:12` `layerfs-telemetry = { path = "../layerfs-telemetry" }`

Product source under `layerfs-content/src` and `layerfs-storage/src` contains **no
clock of its own**. A grep for `std::time|Instant|Duration::` over
`core/crates/layerfs-content/src` returns 0 hits; over `core/crates/layerfs-storage/src`
it returns exactly two, both the SQLite busy-timeout argument, not a timer:

- `core/crates/layerfs-storage/src/sqlite/connection.rs:10` `use std::time::Duration;`
- `core/crates/layerfs-storage/src/sqlite/connection.rs:43` `connection.busy_timeout(Duration::ZERO)?;`

The only clock reads in the whole timer crate are two `Instant::now()` calls:

- `core/crates/layerfs-telemetry/src/timer/recording.rs:110` `let started = Instant::now();`
- `core/crates/layerfs-telemetry/src/timer/recording.rs:155` `started: Instant::now(),`

Content and storage call the shared scopes directly, e.g.:

- `core/crates/layerfs-content/src/file/content.rs:126` `.child("content.inspect")`
- `core/crates/layerfs-content/src/file/edit/apply.rs:47` `let view = FileView::open(reader, request.root, edit.child("edit.base"))?;`
- `core/crates/layerfs-storage/src/cas/store.rs:215` `.child("storage.decode")`
- `core/crates/layerfs-storage/src/cas/store.rs:217` `let (values, counters) = read_scope.child("storage.read").run(|_| {`

**But the measurement path is not one hierarchy.** Three further clocks live in the
example/harness layer, and the *qualified* Stage-5 component-comparison arm uses one of
them instead of the timer:

- `core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs:27` `use std::time::Instant;`
  and `...:180` `let started = Instant::now();`, `...:224` `let elapsed = started.elapsed();`
  - this file has no `layerfs_telemetry` import at all;
  - it is the candidate arm of the qualified `component.primitives` rows:
    `tools/stage5_component_comparison.py:28-32` (`("small", 200, 20)`, `("wide", 2_000, 200)`,
    `("large-few-changes", 20_000, 20)`), `:110-129` builds the command with
    `"--samples", "1"`, and `:55-59` parses `elapsed_ns` out of the arm's `sample` line.
- `core/crates/layerfs-content/examples/edit_timing_c1.rs:116` `let started = Instant::now();`
  wrapping an operation that runs under `...:117` `let edited = Timing::disabled("edit", |scope| {`
  - the printed number is `...:140` `println!("elapsed_ns: {}", elapsed.as_nanos());`.
- `core/crates/layerfs-content/examples/filesystem_timing_c1.rs:296` `let started = Instant::now();`
  wrapping the telemetry recording started at `...:298`
  `layerfs_telemetry::timer::Timing::record("filesystem.update", |timing| {`.
- Storage examples add more: `core/crates/layerfs-storage/examples/measure_filesystem.rs:364`,
  `:407`, `:447`, `:462`, `:503`; `measure_edits.rs:330`; `measure_pooled.rs:208`;
  `memory_ledger.rs:224`; `measure_components.rs:336`.

So: for the product, one timer. For the evidence, the number a row quotes is sometimes a
harness `Instant` and sometimes the report's node (see §7 and §8); the two are never
reconciled in any example.

Second observation for this check: `memory_ledger.rs` installs a second *measurement*
subsystem (a counting global allocator plus `ps`-sampled RSS) —
`core/crates/layerfs-storage/examples/memory_ledger.rs:98` `#[global_allocator]` and
`...:144-148` `fn sample_rss()`. That is a memory ledger, not a second wall-clock
monitor, and all product calls in it are wrapped in `Timing::disabled`
(`...:178` `Timing::disabled("memory.phase", body).0`). The report claims future
`memory/` and `cpu/` modules are siblings of `timer` (`telemetry.md:87-93`); this
example is a third-party-of-the-crate implementation of that idea outside `src/`, which
is permitted for examples but means the memory ledger and the timing tree cannot be
joined today.

## 2. Disabled-path behaviour

**Verdict: PASS.**

Same body, no second code path:

- `core/crates/layerfs-telemetry/src/timer/scope.rs:194-205`:
  `pub fn disabled<T, E, F>(name: ..., operation: F)` then
  `:202` `let handle = TimingScope::<Active>::disabled();`,
  `:203` `let result = operation(&handle);`,
  `:204` `(result, TimingReport::disabled())`.
  The same `operation` closure is called with a disabled handle, exactly as
  `record` calls it with a running handle at `scope.rs:181-184`.
- `core/crates/layerfs-telemetry/src/timer/scope.rs:144-152`: a pending scope with no
  live target runs the body unmeasured -
  `let handle = TimingScope::<Active>::disabled(); return operation(&handle);` -
  and the same is true for a child that no longer fits the budget
  (`scope.rs:153-156`).
- Content phases: `core/crates/layerfs-content/src/filesystem/objects.rs:162-165`
  `match self.scope { Some(scope) => scope.child(name).run(|_| body()), None => body() }`.
- Content entry points: the untimed functions are the *same* function with disabled
  phases - `core/crates/layerfs-content/src/filesystem/update.rs:75`
  `run(objects, input, backing, &FilesystemPhases::disabled())` and
  `...:100` (update) versus `...:88`/`...:113` which forward the caller's `phases`
  into the same `run` at `...:116`.
- Examples: `core/crates/layerfs-storage/examples/measure_pooled.rs:161-165`
  `if options.timing { Timing::record(name, operation) } else { Timing::disabled(name, operation) }`
  and the identical helper at `core/crates/layerfs-storage/examples/measure_edits.rs:118-122`.
  The closure, not the mode, owns the product body.

Observable result with timing disabled:

- no root: `core/crates/layerfs-telemetry/src/timer/report.rs:202-204`
  `pub const fn disabled() -> Self { Self { root: None } }`;
- `is_recording()` is false everywhere: `scope.rs:61-69` (`Target::Disabled => false`);
- JSON prints `null`: `core/crates/layerfs-telemetry/src/timer/json.rs:19-20`
  `None => writer.write_all(b"null\n"),`;
- text prints a marker: `core/crates/layerfs-telemetry/src/timer/format.rs:15-17`
  `return writer.write_all(b"disabled: no timing recorded\n");`
- no clock is read, because a disabled run never constructs a `Recording`
  (`recording.rs:109-128` is reachable only from `Timing::record`, `scope.rs:179`).

## 3. Bounded retention and clipping

**Verdict: PARTIAL.**

Bounds are declared and enforced:

- `core/crates/layerfs-telemetry/src/timer/recording.rs:14` `pub const MAX_NODES: usize = 1_024;`
- `core/crates/layerfs-telemetry/src/timer/recording.rs:17` `pub const MAX_DEPTH: u8 = 32;`
- `core/crates/layerfs-telemetry/src/timer/recording.rs:20` `pub const MAX_LABEL_BYTES: usize = 128;`
- retention check: `recording.rs:203-206`
  `slot.running && slot.depth < MAX_DEPTH && self.slots.len() < MAX_NODES`;
- when the bound is hit the child is **not created**, the parent and all ancestors are
  marked incomplete, and the product body still runs:
  `recording.rs:147-150` `if !inner.child_fits(parent) { inner.mark_incomplete(parent); return None; }`,
  propagation at `recording.rs:226-233`, and the unmeasured fallback at `scope.rs:153-156`.
- attached subtrees consume the same budget: `recording.rs:239`
  `if depth > MAX_DEPTH || inner.slots.len() >= MAX_NODES { return true; }`, and the
  caller marks the node at `recording.rs:182-184`.
- labels are clipped on a UTF-8 boundary and remember it:
  `recording.rs:41-45` (`text: clip(text), clipped: true`), consumed at
  `recording.rs:112` (`incomplete: label.is_clipped()`) and `recording.rs:218`.
- public synthetic construction enforces the same limits:
  `core/crates/layerfs-telemetry/src/timer/report.rs:107-110`
  `if nodes > MAX_NODES as u32 || height > MAX_DEPTH { self.incomplete = true; return false; }`.
- the retained arena is one `Vec<Slot>` per recording (`recording.rs:96-99`), so the
  bound is per recording, and independent recordings share no state
  (`recording.rs:1-5` module doc).

Clipping cannot vanish **silently** from a saved report, but the harnesses differ:

- fail the run: `core/crates/layerfs-storage/examples/measure_filesystem.rs:315-322`
  `fn require_complete(report: &TimingReport) -> Result<(), String>` /
  `"timings: INCOMPLETE - the node budget clipped this tree to {} nodes; ..."`;
  `core/crates/layerfs-content/examples/filesystem_timing_c1.rs:380-385`
  `if report.is_incomplete() { ... std::process::exit(2); }`;
  `core/crates/layerfs-storage/examples/measure_edits.rs:299-311` and
  `measure_pooled.rs:193-202` return `Err`.
- only print a note and exit 0:
  `core/crates/layerfs-storage/examples/measure_components.rs:144-146`
  `if report.is_incomplete() { println!("timings: INCOMPLETE - detail was clipped, not zero"); }`.
  The saved JSON still carries `"incomplete": true`, so the fact is recorded, but the row
  itself is not marked failed.

Two further caveats:

1. `is_incomplete()` cannot distinguish "clipped" from "disabled" -
   `core/crates/layerfs-telemetry/src/timer/report.rs:243-248`
   `match &self.root { Some(root) => root.is_incomplete(), None => true }`.
   Every harness must test `has_root()` first to avoid calling a deliberately disabled
   run "incomplete" (`measure_edits.rs:285-288`, `measure_pooled.rs:180-182`,
   `measure_components.rs:128-134`, `measure_filesystem.rs:315` relies on being called
   with a recorded report only).
2. `core/crates/layerfs-telemetry/src/timer/recording.rs:176-178` drops an attached
   report without marking anything when the target node is not running:
   `if !inner.slots[id].running { return; }`. Reachability from the public API is
   UNVERIFIED (the only handle that can call `attach` is an `Active` handle for a node
   that is running by construction, `scope.rs:106-110`).

## 4. Honest errors

**Verdict: PARTIAL.**

The report does carry errors, per node:

- `core/crates/layerfs-telemetry/src/timer/report.rs:23-28`
  `match result { Ok(_) => Self::Ok, Err(_) => Self::Error }`;
- child: `core/crates/layerfs-telemetry/src/timer/scope.rs:160`
  `recording.finish(started, NodeOutcome::from_result(&result));`
- root: `scope.rs:185` `let report = recording.complete(NodeOutcome::from_result(&result));`
- rendered: `format.rs:26-28` `[error]` and `json.rs:70-73` `"outcome": "error",`.
  A recovering parent keeps the child's error, so a failed child is not laundered into a
  successful parent.

The gap is an unwound child. A panic escaping a child's `run` never reaches
`recording.finish` (`scope.rs:159-160`), and the crate deliberately keeps the recording
usable after a panic caught inside the operation - its own test asserts the resulting
state:

- `core/crates/layerfs-telemetry/tests/timer.rs:246-248`
  `let unstable = &root.children()[0];` / `assert!(unstable.is_incomplete());` /
  `assert_eq!(unstable.elapsed(), Duration::ZERO);`
  (the test does **not** check that child's outcome).

What that node actually reports is fixed by two lines:

- `core/crates/layerfs-telemetry/src/timer/recording.rs:218` `outcome: NodeOutcome::Ok,`
  (the value every new slot starts with), and
- `recording.rs:272-281`, which copies `slot.outcome` unchanged and substitutes
  `Duration::ZERO` for a still-running slot:
  `let incomplete = slot.incomplete || slot.running;` /
  `let elapsed = if slot.running { Duration::ZERO } else { slot.elapsed };`.

So a child that failed by unwinding is serialised as `{"outcome" absent == ok,
"elapsed_ns": 0, "incomplete": true}` - a reader must interpret `incomplete` as the
failure signal, and cannot distinguish it from "this child's detail was clipped".
No example or test exercises this node through the JSON writer, so the flag combination is
UNVERIFIED as an end-to-end receipt, though the code path is unambiguous.

## 5. Construction vs persistence completion

**Verdict: PARTIAL.**

There is no schema-level distinction. The complete node shape is:

- `core/crates/layerfs-telemetry/src/timer/report.rs:38-46`
  `name`, `elapsed: Duration`, `outcome: NodeOutcome`, `incomplete: bool`,
  `children: Vec<TimingNode>`;
- `core/crates/layerfs-telemetry/src/timer/json.rs:15-17`
  `A measured report is one object with \`name\`, \`elapsed_ns\` and \`children\`. The
  \`outcome\` and \`incomplete\` fields appear only when they are not the default.`

The only way to separate "construction finished" from "persistence acknowledged" is the
span the caller creates around each phase. In the storage measurement examples:

- separated: `core/crates/layerfs-storage/examples/measure_edits.rs:528`/ `:540`/ `:546`
  - `let mut operation = store.begin_save(scope.child("storage.begin"))?;`,
  `&mut handoff,`, `scope.child("file.edit"),`, `let outcome = operation.finish(scope.child("storage.finish"))?;`
  - the C1 construction is its own node `file.edit` between the two storage nodes.
- **not** separated: `core/crates/layerfs-storage/examples/measure_filesystem.rs:466-479` -
  `Timing::record("c1c2.pipeline", |timing| {` opens one root whose only children are
  `storage.begin` (`:467`) and `storage.finish` (`:478`); the whole C1 construction
  (`:469-473`, `update_filesystem(&mut objects, input, Some(backing_ref))`) has no span
  of its own. The row then prints one number for both phases:
  `core/crates/layerfs-storage/examples/measure_filesystem.rs:484-487`
  `"elapsed_ns {} (construction, bounded handoff and save acknowledgement)",`.
  A reader of that report cannot recover construction time at all.
- the same "one number" shape appears in `measure_components.rs:268-297`, which at least
  gives the construction a `content.construct` child (`:281`).

What "persistence acknowledged" means here is a COMMIT, not durability - the ack is the
node around `SaveOperation::finish`:

- `core/crates/layerfs-storage/src/cas/store.rs:313-321`
  `pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome> {` /
  `let counters = owner.finish()?;` / `Ok(SaveOutcome::from(counters))`.
- the accept half deliberately creates no node, so the report stays bounded by waves
  rather than by objects: `core/crates/layerfs-storage/src/cas/store.rs:283-290`
  (quote: `This call creates no timing node. The caller owns the tree: a caller accepting
  a wave plans one region for the wave and accepts every object of it inside that
  region, which is what keeps a save's report bounded by its waves instead of by its
  object count.`).

## 6. No sync/durability in report output; JSON failure; destination ownership

**Verdict: PASS on the sync and destination questions; PARTIAL on error separation.**

No `fsync`, `fdatasync`, `sync_all` or `sync_data` exists anywhere under
`core/crates` outside comments and the SQLite pragma:

- `core/crates/layerfs-storage/src/sqlite/connection.rs:37`
  `connection.execute_batch("PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;")?;`
- the remaining grep hits are documentation only, e.g.
  `core/crates/layerfs-storage/src/lib.rs:11-12`
  `no WAL, no added crash-durability work and no \`fsync\`/\`fdatasync\`/\`sync_all\`/\`sync_data\` anywhere in the product path.`

Report writing is buffered-write plus `flush` only, and the destination is always
caller-supplied:

- `core/crates/layerfs-telemetry/src/timer/json.rs:18`
  `pub fn write_json(&self, mut writer: impl Write) -> io::Result<()> {`;
- `core/crates/layerfs-telemetry/src/lib.rs:8-10`
  `performs no file I/O and reads no global configuration. Recording opens no file; output
  is written through caller-supplied [\`std::io::Write\`] implementations.`
- the writing examples add no sync: `measure_components.rs:135-138`
  `OpenOptions::new().write(true).create_new(true).open(path)?` / `let mut writer = BufWriter::new(file);` /
  `report.write_json(&mut writer)?;` / `writer.flush()?;`, identical at
  `measure_edits.rs:289-292` and `measure_pooled.rs:183-186`.
- writer errors propagate through every `?` in `json.rs`, and an out-of-range duration is
  an error rather than a saturated number: `json.rs:97-105`
  `io::Error::new(io::ErrorKind::InvalidData, "elapsed duration exceeds the u64 nanosecond range")`.
- no-clobber is real: `create_new(true)` plus pre-flight existence checks at
  `measure_components.rs:97-104`, `measure_edits.rs:160-162`, `measure_pooled.rs:76-78`.

Divergence from the documented pattern: the report-save error is folded into the
command's exit status, so a fully successful product run fails because a JSON file could
not be written.

- `core/crates/layerfs-storage/examples/measure_components.rs:184` `save_timings(&report, &options.timings)`
  is the tail expression of `run_c1`, whose `Err` reaches `run()` (`:347`) and `main`
  (`:334-340`) -> non-zero exit. Same shape at `measure_edits.rs:406`, `:489`, `:596-600`
  and `measure_pooled.rs:326-327`.
- the contract says otherwise: `core/crates/layerfs-telemetry/README.md:142-143`
  `The caller owns saving: choose the path, create without clobbering, flush explicitly and
  keep the save result separate from the product result.` and `telemetry.md:378`
  `// Keep the product result separate from save_result.`
- the crate's own example does it correctly: `core/crates/layerfs-telemetry/examples/timer_composition.rs:82-85`
  `Err(error) => println!("save failed, product result unchanged: {error}"),` followed by
  `Ok(())`.

Partial files are never removed by any example (they cannot be: the report is written to a
fresh `create_new` file and the failure path only returns). That matches
`telemetry.md:381-383` (`A failed write can leave a partial file: retain it as incomplete
and surface the error.`).

Also worth recording: the two Stage-5 harnesses write **no report file at all**.
`core/crates/layerfs-content/examples/filesystem_timing_c1.rs` never calls `write_json` or
`write_text` (it prints a hand-built `text` block at `:320-376`), and
`core/crates/layerfs-storage/examples/measure_filesystem.rs` prints only through
`println!`/`require_complete` (`:315-325`). Their report node counts and phase times exist
only in stdout, so a clipped or missing tree has no durable artifact.

## 7. C1-only, C2-only and integrated bodies per example

Documented contract for the Stage-5 pair (a claim, quoted for comparison):
`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-verification.md:80-88`
`A --mode pipeline row measures construction, the bounded handoff and the final save
acknowledgement in one region; read-back is reported separately.` /
`A --mode c2 row receives objects prepared outside the timed region and measures admission
only; it includes no filesystem construction.` /
`A --mode c1 row opens no database ...`.

### 7.1 `core/crates/layerfs-content/examples/filesystem_timing_c1.rs` (C1-only)

CLI: `:90-105`. `--case` default `"empty"` (`:91` `let mut case = "empty".to_owned();`),
`--output` required (`:103` `output: output.expect("--output FRESH_DIRECTORY")`),
any other argument panics (`:98` `other => panic!("unexpected argument {other}")`).

Case -> operation:

| `--case` | base (`base()`, `:118-211`) | timed update (`changes()`, `:233-279`) |
| --- | --- | --- |
| `empty` | 1 dir inode (`:119-129`) | none (`:235`) - base re-emitted |
| `directory-update` | /d with 200 files (`:130-157`) | 10 renames in /d (`:236-245`) |
| `subtree-remove` | same 200-file base (`:130`) | unbind `d` from / (`:268-275`) |
| `inode-update` | a/b hardlink fixture (`:158-192`) | replace inode 4 (`:246-253`) |
| `hardlink-move` | same fixture (`:158`) | move 4 from /d to / (`:254-267`) |
| `attributes` | 1 dir + 1 file (`:193-209`) | **none** (`:276` `"attributes" => (Vec::new(), Vec::new(), Vec::new()),`) |

- The timed region is one recording around the real C1 update:
  `:297-308` `Timing::record("filesystem.update", |timing| { ... update_filesystem_timed(&mut objects, &input, Some(backing_ref), &phases)`.
- The fixture build is untimed and disabled: `:225` `build_filesystem_timed(&mut objects, &input, None, &FilesystemPhases::disabled())`.
- **FINDING (case name does not select the claimed operation):** for `--case attributes`
  the timed update applies an empty change set (`:276`), so the row measures the same
  no-op body as `--case empty`; the attribute work the case name refers to runs *after*
  the report is printed and is not timed at all - `:388-440`, e.g. `:423` `apply_patches(`
  has no `Timing::record`/`Instant` around it, and the report was already emitted at
  `:376` `print!("{text}");`.
- C1-only is genuine in the dependency sense: `core/crates/layerfs-content/Cargo.toml:9-11`
  lists only `blake3` and `layerfs-telemetry`, so no storage crate can be reached.
- The C1 row does perform file I/O inside the timed span: `FileBacking` is handed into the
  timed call at `:304-305`, and `FileBacking::create_run` opens a real file
  (`core/crates/layerfs-content/src/filesystem/references/backing.rs:249-256`) while
  `FileRun::append` writes it (`...:317`). That is declared work (spill), but it means
  "C1-only" here is not "no file was opened".

### 7.2 `core/crates/layerfs-content/examples/edit_timing_c1.rs` (C1-only, matched pair)

- No CLI parser at all: `main()` at `:88` reads no arguments; the fixture is hard-coded
  (`:89-91` `let base = noise(3_300_000);` / `let start = 1_650_000_u64;` / `let replacement = noise(40_000);`).
- The operation's own tree is disabled: `:117` `let edited = Timing::disabled("edit", |scope| {`.
  The row's number is the harness clock (`:116`/`:133`), printed at `:140`.
- Construction is untimed and disabled: `:94` `let constructed = Timing::disabled("build", |scope| {`.
- So this row carries **no phase detail**; it is a single wall number with no tree.

### 7.3 `core/crates/layerfs-content/examples/filesystem_primitives_candidate.rs` (component arm)

CLI: `:86-106`. `--files` default `2_000` (`:88`), `--changes` default `200` (`:89`),
`--samples` default `1` with `.max(1)` (`:90`, `:101`). Note the parser bug: every flag
consumes the next argument and silently substitutes `0` when it does not parse -
`:94-97` `let value = arguments.next().and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);`
- so `--files abc` sets 0 rather than failing. Unknown flags panic (`:102`).
- No telemetry: the timed region `:180-224` is a bare `Instant`; there is no root, no
  phase, no `is_incomplete` check, and no `--output` (nothing is written to disk).
  This is the arm the qualified comparison rows come from (see §1).

### 7.4 `core/crates/layerfs-storage/examples/measure_components.rs`

CLI `:70-111`: `--mode` required (`:88`) and parsed at `:49-56` (`c1`|`c2`|`pipeline`),
`--input` required (`:89`), `--timings` required (`:90`) and refused if it exists (`:97-99`),
`--store` required exactly when `uses_storage()` (`:91-96`, `uses_storage` = not C1 at `:58-60`).

- `--mode c1` -> `run_c1` `:164-185`: `Timing::record("c1.only", ...)` with child
  `content.construct` (`:173`) over `construct_stream` and a `DiscardingConsumer`
  (`:166` `let mut consumer = DiscardingConsumer::new();`). Real C1 body, no store. PASS.
- `--mode c2` -> `run_c2` `:188-258`: the canonical fixture is built *before* the timed
  region and with clocks off (`:200-208` `let fixture = Timing::disabled("fixture", |root| { ... construct_bytes(...)`),
  and the timed region `:224` `Timing::record("c2.only.save", |root| {` contains only
  `Store::create` (`:225-229`), `begin_save` (`:230`), `accept` (`:231-233`) and
  `finish` (`:234`). No timed C1 construction. PASS.
  Caveat: the C2-only "save" number includes Store creation and schema initialisation
  (child `store.create`, `:228`), and the read-back is a separate recording
  (`:243` `Timing::record("c2.only.read", ...)`) whose report is rendered (`:256`) but
  never saved - only `save_report` is written (`:257`).
- `--mode pipeline` -> `run_pipeline` `:261-317`: real construction into `SaveHandoff`
  (`:275-282`), `operation.finish` (`:287`), and then a read-back **inside the same root**
  (`:288-296`, child `content.read`). This differs from `measure_filesystem`, which keeps
  read-back outside the operation timer (`measure_filesystem.rs:499-509`); the two
  integrated examples do not share a boundary rule.

### 7.5 `core/crates/layerfs-storage/examples/measure_edits.rs`

CLI `:125-169`: `--mode` (required), `--case` (required; `small`|`chunked`|`small-to-large`|`large-to-small`|`batch`, `:56-65`),
`--threshold-bytes` (required), `--output` (required, must not exist, `:160-162`),
`--timing on|off` default `on` (`:130`, `:143-149`). Policy is
`ConstructionPolicy::new(options.threshold, 8, 4)` (`:163`) and the fixture budget is
`FIXTURE_LIMIT: u64 = 4 * 1024 * 1024` (`:37`) enforced at `:191-194`.

- `--mode c1` -> `run_c1` `:369-407`: fixture built disabled (`:239`), timed
  `file.edit` (`:379`) over `apply_edits` with an in-memory `Provider` and
  `DiscardingConsumer`; no Store. PASS.
- `--mode c2` -> `run_c2` `:410-490`. Objects are supplied, and the run states its own
  limitation at `:418-422` (`--case selects the fixture only; ... it is not a per-case
  comparison`). **FINDING:** inside the timed `storage.save` scope (`:445-477`) the row
  calls content-side canonical encoding twice -
  `:449` `layerfs_content::file::encode_whole_file(&policy.capacities(), base_raw)?,` and
  `:459` `layerfs_content::file::encode_whole_file(&policy.capacities(), &changed)?,` -
  while printing at `:487` `"exclusions: no C1 file construction and no full-file object
  collection is timed"`. `encode_whole_file` is C1 code
  (`core/crates/layerfs-content/src/file/content.rs:80`) that allocates and frames the
  whole-file canonical object (`...:91-95`). It is not chunked construction, so the
  stronger claim ("no C1 file construction") holds for `construct_bytes`/`apply_edits`,
  but content-side encoding *is* timed.
  The same scope also performs a Store read - `:475` `let (values, _) = store.read_batch(&[dependent_id], save.child("storage.read"))?;` -
  so the row's `storage.save` node contains a read wave.
- `--mode pipeline` -> `run_pipeline` `:493-601`: base prepared untimed
  (`Timing::disabled("base.prepare", ...)`, `:504-515`), timed `edit.save` (`:524`) with
  `storage.begin` (`:528`), `file.edit` (`:540`) and `storage.finish` (`:546`), then a
  separate `verify.readback` recording (`:560-566`) saved to its own JSON (`:597-600`).
  The base is read through the real Store (`:527` `let reader = StoreReader::new(&store);`),
  i.e. C2 reads are nested inside the C1 `file.edit` span (see §8). PASS as an integrated body.

### 7.6 `core/crates/layerfs-storage/examples/measure_filesystem.rs`

CLI `:80-98`: `--mode` default `"c1"` (`:81`), `--case` default `"directory-update"` (`:82`),
`--output` required (`:96`), unknown argument panics (`:90`). Case -> fixture size
`:109-116` and case -> change set `:137-209`; the banner `:119-129` describes the change set.

- `--mode c1` -> `run_c1` `:358-397`. **PASS on "opens no database":** `fresh_store` (`:291`)
  is called only at `:401` (c2) and `:443` (pipeline); `main` (`:327-355`) creates no Store,
  and `run_c1` calls the untimed entry `:369` `update_filesystem(&mut objects, input, Some(backing_ref))`
  inside `disabled(...)` (`:366-370`). The number comes from the harness clock
  (`:364`/`:372`, printed twice as `elapsed_ns` `:376` and `end-to-end ns` `:391`).
  This repairs what the earlier review recorded as R36
  (`docs/roadmap/0.1/0.1.7/component-decoupling/stages-1-5-review-20260917T160000Z.md:180`).
- `--mode c2` -> `run_c2` `:400-432`: objects are re-materialised from the fixture before
  the timer (`:402-406`), then `Timing::record("storage.save", ...)` (`:408`) with
  `storage.begin`/`storage.accept`/`storage.finish` (`:409-418`). No timed C1 construction. PASS.
  **FINDING (case name does not select the row's claimed operation):** `main` prints
  `banner(&config.case)` for every mode at `:336`, but `run_c2` receives only
  `(&config, &bag)` (`:348`, signature `:400`) and never uses `directories`/`inodes`.
  A `--mode c2 --case subtree-remove` run therefore prints
  `:125` `"update: remove 10 bindings from /d (counts released)"` while performing no update
  at all; for c2 the case selects only `entries_for(case)` (`:109-116`).
- `--mode pipeline` -> `run_pipeline` `:436-511`: fixture persisted untimed (`:447-461`,
  labelled `setup untimed elapsed_ns`), then `Timing::record("c1c2.pipeline", ...)` (`:466`)
  with only `storage.begin` (`:467`) and `storage.finish` (`:478`) as children, and the
  read-back through a reopened Store outside the timer (`:502-509`). See §5 for the missing
  construction span. Note also that the timed update reads its base from the in-memory
  `Bag` (`:470` `let mut objects = FilesystemObjects::new(bag, &mut handoff);`), not from the
  Store that setup just wrote, so the integrated row's C1 half is served by process memory.

Documentation mismatch for the same family: the verification contract's case IDs
(`stage-5-verification.md:42-58`: `empty`, `small`, `wide`, `deep`, `noop`, `rename`,
`remove`, `create`, `move`, `subtree-remove`, `portable`, `generic`, `paged-read`,
`tiny-budget`, `save-reopen`, `patch-reopen`) are not the parser's case names
(`measure_filesystem.rs:110-127`: `empty`, `attributes`, `directory-update`,
`inode-update`, `hardlink-move`, `subtree-remove`). Only `empty` and `subtree-remove`
appear in both.

### 7.7 Other measurement examples

- `core/crates/layerfs-storage/examples/measure_pooled.rs` (C2-only): CLI `:51-92`;
  no C1 construction anywhere - the leaves are built by the harness itself
  (`:107-127` `fn leaf(rows: u64, last_seed: u64)`). All product work runs through
  `timed(...)` (`:233-270`, `:296-301`) so `--timing on|off` runs the same bodies. PASS.
- `core/crates/layerfs-storage/examples/memory_ledger.rs` (C2 memory ledger): CLI
  `:233-248`; every product call is `Timing::disabled` (`:178`, `:368`, `:377`, `:378`, `:380`).
  Not a timing row; its phasing is allocation-based (`:113-136`).
- `core/crates/layerfs-content/examples/fingerprint_collision_search.rs`: contains no timer
  at all; its interface is positional `:10` `cargo run --release --example fingerprint_collision_search [budget_bits]`.

## 8. Overlap and containment

**Verdict: PASS with caveats. No CPU time is derived by subtracting overlapping wall spans
anywhere; containment is real, intentional, and in one direction hides C2 work inside C1
spans.**

Containment is created deliberately by the scoped provider path:

- `core/crates/layerfs-content/src/file/mapping/read.rs:119`
  `.read_canonical_batch_scoped(&self.distinct, self.scope.child("mapping.payload"))?;`
- `core/crates/layerfs-content/src/file/mapping/read.rs:235`
  `reader.read_canonical_batch_scoped(&ids, scope.child("mapping.navigate"))?;`
- `core/crates/layerfs-storage/src/cas/provider.rs:65`
  `self.read_wave(ids, scope)` -> `core/crates/layerfs-storage/src/cas/store.rs:208-219`,
  which opens `storage.decode` and `storage.read` **inside** the caller's pending child.

So in `measure_edits --mode pipeline`, the `file.edit` node's inclusive elapsed contains
the Store's connection, ceiling read, decode and read waves. This is the model the report
claims (`telemetry.md:249-251` `Durations are inclusive wall time. Caller duration includes
remote work and instrumentation/transport costs. Do not add parent and child, infer CPU
time, or label caller-minus-callee as network latency.`) and the examples restate it in
every render call - `core/crates/layerfs-storage/examples/measure_edits.rs:316`
`println!("--- timing tree (inclusive; overlapping spans add to nothing) ---");`
(identical text at `measure_components.rs:152` and `measure_pooled.rs:169`).

Caveat 1 - C2 work inside a C1 span with no storage child. `SaveOperation::accept`
creates no node (`core/crates/layerfs-storage/src/cas/store.rs:283-290`), so the packing
and SQL work that admission performs (`core/crates/layerfs-storage/src/cas/save.rs:22`
`pub fn flush_batch(owner: &mut MutationOwner, objects: Vec<FinalizedObject>) -> StorageResult<()>`,
called from `store.rs:307`) is timed inside whichever span the caller was in. With a
`SaveHandoff` that span is the C1 edit
(`core/crates/layerfs-storage/src/cas/store.rs:539-548`;
call site `measure_edits.rs:529-541`), or the unspanned construction region of
`measure_filesystem.rs:469-473`. A reader comparing a C1-only `file.edit` row with a
pipeline `file.edit` row is comparing spans that include different C2 work; nothing in
either report says so.

Caveat 2 - disabled provider reads inside measured spans. When content reads through the
unscoped trait method (used by the filesystem paths, e.g.
`core/crates/layerfs-content/src/filesystem/inode/read.rs:108`,
`.../directory/read.rs:126`, `.../attributes/read.rs:61`,
`.../objects.rs:96`), the store runs with the clocks off:
`core/crates/layerfs-storage/src/cas/provider.rs:51-53`
`Timing::disabled("storage.read", |scope| { self.read_wave(ids, scope.child("storage.read")) })`.
Real store time then sits inside an enclosing measured span with no child detail and **no
`incomplete` flag** (the flag means "requested detail is missing", and the code never
requested it). The same pattern appears in product content source:
`core/crates/layerfs-content/src/filesystem/attributes/value.rs:77`
`layerfs_telemetry::timer::Timing::disabled("attributes.value", |scope| {`.

Caveat 3 - two clocks describe the same interval in the harnesses. In
`measure_filesystem.rs:462-481` and `filesystem_timing_c1.rs:296-309` an `Instant` span
wraps exactly the region of a telemetry root, so the two printed numbers cover the same
work with different overhead; neither is derived from the other, and no example prints the
difference. `measure_filesystem.rs:376` and `:391` print the *same* `elapsed` value under
two different labels (`elapsed_ns` and `end-to-end ns`).

No subtraction exists. A grep for `cpu|self-time|self_time` over both example directories
returns 0 hits, and the only `saturating_sub` in a storage example is unrelated to timing
(`measure_edits.rs:432`, sizing a byte patch). The phase lines printed by
`filesystem_timing_c1.rs:364-374` are each node's own inclusive `elapsed()`, not
differences.

## 9. Actual flags and defaults

No example implements `--help`. Three panic on an unknown argument; the rest return an
error. Nothing accepts a worker count or a cache-policy switch.

| Example | Flags (default) | Required | Unknown arg |
| --- | --- | --- | --- |
| `content/examples/filesystem_timing_c1.rs:90-105` | `--case` (`empty`), `--output` | `--output` (`:103`) | `panic!` (`:98`) |
| `content/examples/edit_timing_c1.rs:88` | none (no parser; sizes hard-coded `:89-113`) | - | - |
| `content/examples/filesystem_primitives_candidate.rs:86-106` | `--files` (2000), `--changes` (200), `--samples` (1, min 1) | none | `panic!` (`:102`); unparsable values become 0 (`:94-97`) |
| `content/examples/fingerprint_collision_search.rs:10` | positional `[budget_bits]` | none | - |
| `storage/examples/measure_components.rs:70-111` | `--mode` (none), `--input`, `--timings`, `--store` | mode, input, timings (`:88-90`); store for c2/pipeline (`:91-93`) | `Err` (`:85`) |
| `storage/examples/measure_edits.rs:125-169` | `--mode`, `--case`, `--threshold-bytes`, `--output`, `--timing` (`on`) | mode, case, threshold, output (`:154-157`) | `Err` (`:150`) |
| `storage/examples/measure_filesystem.rs:80-98` | `--mode` (`c1`), `--case` (`directory-update`), `--output` | `--output` (`:96`) | `panic!` (`:90`) |
| `storage/examples/measure_pooled.rs:51-92` | `--leaves` (128, 1..=4096), `--rows` (100, 2..=200), `--output`, `--timing` (`on`) | `--output` (`:75`) | `Err` (`:72`) |
| `storage/examples/memory_ledger.rs:233-248` | `--output`, `--pooled-leaves` (24), `--pooled-rows` (100) | `--output` (`:249`) | `Err` (`:246`) |
| `telemetry/examples/timer_nested.rs:74` | none | - | - |
| `telemetry/examples/timer_composition.rs:78-86` | positional output path (optional) | - | ignored |

Additional non-flag knobs: `measure_edits.rs:163` derives the construction policy from
`--threshold-bytes` (`ConstructionPolicy::new(threshold, 8, 4)`); `measure_pooled.rs:217`
hard-codes `StoragePolicy::new(1, 131_072, 8, 4)`; `memory_ledger.rs:395` records
`LAYERFS_CONSTRUCTION_WORKERS` from the environment but no example sets it.

## Timing/report buffers that grow with file, inode or object count

1. **`measure_pooled.rs` node budget vs `--leaves` range.** Two timing children are created
   per leaf - `:251-253` `let mut operation = store.begin_save(scope.child("storage.begin"))?;`
   / `operation.accept(object)?;` / `let outcome = operation.finish(scope.child("storage.finish"))?;` -
   inside a loop over `0..options.leaves` (`:239`). The CLI accepts up to
   `:40` `const LEAF_LIMIT: u64 = 4_096;` (``:79-81`), i.e. up to 8,193 nodes against
   `MAX_NODES = 1_024` (`recording.rs:14`). Any `--leaves` >= 512 therefore cannot produce a
   complete tree; the run fails loudly at `:193-202`, so the loss is not silent, but the
   accepted range is inconsistent with the recorder's declared cap.
2. **`edit_timing_c1.rs` demand log.** `:31` `demanded: RefCell<Vec<ObjectId>>,` extended on
   every read wave at `:52` `self.demanded.borrow_mut().extend_from_slice(ids);` - one
   `ObjectId` (32 bytes) per distinct object the edit reads, with no declared cap, printed
   as `nodes_read` at `:153`. It is measurement instrumentation, and it is bounded only by
   the hard-coded fixture, not by a declared limit.
3. **Object collectors in the examples.** `measure_edits.rs:273-282` (`struct Collector` /
   `self.objects.push(object)`), `edit_timing_c1.rs:65-74`, `filesystem_timing_c1.rs:32-51`
   (`BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>`), `memory_ledger.rs:161-171`. Each grows one
   entry (with the full canonical byte copy in three of them) per emitted or held object.
   `measure_edits` bounds this only indirectly through `FIXTURE_LIMIT` (`:37`) and the
   threshold guard (`:191-194`); `measure_components.rs:199-213` is the one that declares a
   byte cap (`:30` `const C2_FIXTURE_BYTES: usize = 131_071;`).
4. **Whole-tree duplication for read-back.** `filesystem_timing_c1.rs:311-313`
   `let mut merged = bag.clone();` / `merged.objects.extend(sink.objects.clone());` copies the
   entire object map per run, growing with fixture inode count (bounded per case at 202
   inodes, `:156` `(1..=202).collect(),`, but with no declared cap of its own).
5. **Telemetry itself is bounded and I found no uncapped telemetry buffer.**
   `recording.rs:96-99` is one `Vec<Slot>` per recording with the
   `slots.len() < MAX_NODES` gate at `recording.rs:205`; `Slot.children` (`recording.rs:88`) is
   bounded by the same total; labels are clipped to 128 bytes (`recording.rs:20`, `:33-46`).
   The two writers stream node by node (`format.rs:22-37`, `json.rs:29-58`) and allocate only
   a small unit string per node (`format.rs:67-82`) and 2 bytes of indent per level
   (`json.rs:131-136`), so report *rendering* does not buffer in proportion to object count.
   Reader-driven nodes scale with bounded waves, not objects:
   `core/crates/layerfs-content/src/file/mapping/read.rs:24` `pub const READ_WAVE_OBJECTS: usize = 32;`,
   `:33` `pub const READ_NAVIGATION_WAVE: usize = 32;`.

## Report claims that the code at HEAD does not support

- `docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md:478-506` claims the milestone is
  complete including "Preserve Results, child errors, early returns, recovery and panic
  propagation". The code does preserve results and ordinary errors, but a child lost to a
  panic inside the operation is reported `Ok` with `elapsed_ns: 0` (see §4); no example or
  test covers that node's serialised outcome.
- `telemetry.md:115-116` `On transport error, attach None before returning the original error so
  missing remote detail remains visible.` This is adapter guidance, not implemented
  behaviour; there are no adapters in the crate (`telemetry.md:310-323` keeps transport out
  of scope), so nothing at HEAD can be checked against it.
- `stage-5-verification.md:80-83` (c2 = "admission only", c1 = "opens no database") matches
  HEAD; the earlier review's R36 (`stages-1-5-review-20260917T160000Z.md:180`) and R35
  (`:179`, "never consult `is_incomplete`") no longer hold at HEAD -
  `measure_filesystem.rs:315-325` and `filesystem_timing_c1.rs:380-385` both fail the run.
- `stage-5-verification.md:52-54` `filesystem.attributes` cases are `portable`, `generic`,
  `wide`; neither harness's parser accepts those names, and `filesystem_timing_c1 --case
  attributes` times no attribute work at all (§7.1).
- `core/crates/layerfs-telemetry/README.md:108-110` `Clipping bounds both the live recording
  and the completed report; it never fails or shortens the measured operation.` This is
  accurate for the crate (`scope.rs:153-156` still runs the body) but the Stage-5/3-4
  harnesses turn clipping into a process failure, which is their own choice and is stated
  in each file.

## UNVERIFIED

- Reachability of the silent attach drop at
  `core/crates/layerfs-telemetry/src/timer/recording.rs:176-178` from the public API
  (no path found by reading; not proven impossible).
- The serialised JSON shape of a panicked child node (`outcome` absent, `elapsed_ns: 0`,
  `incomplete: true`). The state is fixed by `recording.rs:272-281` and the existing test
  `tests/timer.rs:246-248`, but no test writes that node through `write_json`, and no
  command was run to confirm it.
- Runtime behaviour of every example (no example was executed during this review); all
  statements about what a mode "measures" are statements about the code path selected by
  the parser, not about a recorded run.
- Whether any downstream report or ledger quotes the double `elapsed_ns`/`end-to-end ns`
  line from `measure_filesystem.rs:376,391` as if they were two different quantities; only
  the code in this scope was audited.
