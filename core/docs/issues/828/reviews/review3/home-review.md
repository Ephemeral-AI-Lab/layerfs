# layerfs-home layout review (#828 plan): organization and naming

Scope: every planned path under `control/crates/layerfs-home/` in
`nav/site/layout.json`. I read each file's `//!` header, `use` lines and public
items in the old source (`v2tree/control/crates/layerfs-home/src`, plus
`layerfs-sandbox-backend/src/support/json/`). I grepped `layerfs_home::<module>`
across the whole `v2tree` and mapped each user to its planned path. This is
planning only; nothing in the repository was touched.

## 1. Verdict

`json/`, `lock/`, `log/` and the settled `reply/` hold up. Two folders do not:

- **`op/` is a theme word, not a role.** It puts the `op` record field together
  with a process-wide SIGINT flag that never touches it and with
  process-identity helpers. Rule 20 already says where op intent goes: `record/`.
- **`disk/` (11 files) mixes two roles.** One is the home as a folder: what
  lives where, `layerfs.json`, a worker's root, secrets and filesystem capacity.
  The other is the record codec that every entity's `record/` folder builds on:
  the record file, typed fields, `observed.json` and the generation counter.

The fix is one new `record/` folder that takes the record codec, the op intent
and the owner identity, and `op/` goes away. With it, `reply/values.rs` stops
being `helpers.rs` under a new name, and `outcome.rs` joins `reply/`. The root
drops to 2 loose files. The crate goes from `disk json lock log op reply` to
`disk json lock log record reply`. That is six clearly different roles, the same
number of folders as before.

## 2. Findings (ranked)

**1. Dissolve `op/`: `intent.rs` → `record/op.rs`, `process.rs` → `record/process.rs`, `cancel.rs` → root.** (organization, naming)
- `cancel.rs` is "the process-wide cancel request … during `store upgrade`". It
  uses only `std::sync::atomic`. Its only users are `layerfs-api/src/lib.rs`
  (`pub use layerfs_home::cancel`, for the CLI signal thread) and
  `layerfs-control/src/store.rs` (→ `layerfs-store/src/admin/with_users.rs`).
  `op.rs` never uses it. It is not "an operation in flight".
- `op.rs` uses `crate::{home, process}`. Its header calls it "the one in-flight
  intent … its entity's record holds one `op`". Rule 20 says `record/` holds "its
  record, identity and op intent", and the sibling crates already follow it:
  sandbox `record/`, store `admin/record/intent.rs`, project
  `data/project/record/op.rs`.
- `process.rs` describes itself as "This process as a record names it (an
  `op`'s owner, a Store lease, a forward)". Its users are records: the Store
  lease and compaction, sandbox `record/daemon.rs` and `record/exec.rs`, and
  inspect's process view. It is owner identity, so it belongs in `record/`.
- Name the file `op.rs`, not `intent.rs`. That is the field name (`op`), the
  type (`Op`) and today's public path (`layerfs_home::op`). See finding 6 for
  the matching word in other crates.

**2. Split the record codec out of `disk/` into `record/`.** (organization)
- `disk/records.rs` → `record/file.rs`. It holds `read_record`, `write_record`,
  `write_entity`, `versioned`, `with_field`, the private folder modes and
  `now_ms`. The new name is needed because rule 14 forbids
  `record/records.rs`.
- `disk/fields.rs` → `record/fields.rs`. Its header says it is "the record
  codec's typed half"; sandbox `record/{daemon,sandbox,workspace}.rs` use it.
- `disk/generation.rs` → `record/generation.rs`. It "moves whenever a record of
  the home changes", and record writers call it.
- `disk/observed.rs` → `record/observed.rs`. It is `observed.json` "beside its
  record", written through `read_record`/`write_record`.
- What stays in `disk/` is the home as a folder: `paths.rs` (the path table,
  including `log/` and `trace/`), `layout.rs` (`layerfs.json`), `guest.rs` (a
  worker's root on a machine data disk is not a home), `secrets.rs` (`secret/`),
  `keys.rs` (keys kept in `worker.json`/`control.json`) and `capacity.rs`
  (statvfs). That is 6 files, all answering "what is in this folder on disk and
  how much room it has". `capacity.rs` is the one file the name `disk/`
  predicts best.

**3. `reply/values.rs` is `helpers.rs` renamed. Split out the two parts that are not values.** (naming, rule 6/12)
- `helpers::lock`: its doc says "The lock of a table each holder changes in one
  step". It is a poison-tolerant `Mutex` lock, used by sandbox registry,
  lifecycle, output and unmount_guard and by store service. Its test is already
  named `table_lock.rs`. Move it to **`lock/table.rs` (new)**, so `lock/`
  holds every lock.
- `helpers::ordered_token` and `helpers::random` mint place values
  (`p1:<home>.<kind>:<ms>:<nonce>`) and fresh random bytes for record
  incarnations, ids and keys. Users include machine `lifecycle/create.rs` and
  `boot.rs`, sandbox `record/sandbox.rs`, api `worker/serve.rs` and store
  init/backup. `ordered_token` already calls `crate::home::now_ms`. Move both to
  **`record/token.rs` (new)**.
- `values.rs` then holds only call-argument and answer values: `text`,
  `required`, `SHOWN`, `shown`, `shown_names` and `map_of`. That matches the
  plan's own description ("call argument/answer values").

**4. Move `outcome.rs` from the root into `reply/`.** (organization)
Its header: "What one host operation answers: a value, or a refusal with a stable
code … what a refusal carries for the caller's next step". The plan defines
`reply/` as "what every answer is built from". `Outcome`, `refused`, `with_fix`
and `with_next` are the core of that, more than `time.rs` is. Lower modules in the
crate (`lock/`, `record/`) also return `SdkRefusal`, which is fine inside one
crate. This does not reopen `reply/`; it puts the file that defines a reply
inside it. `reply/` grows to 6 files.

**5. Move the home's `log/*.jsonl` writers from `records.rs` to `log/lines.rs` (new).** (organization)
`records.rs` lines ~170–237 hold `UNCERTAIN`, `log_line`, `SWEPT` and
`log_swept`. They already depend on `crate::logs::{ROTATE_AT, rotate}`, and the
`logs.rs` header says "The lines of `log/*.jsonl` rotate the same way". Moving
them out leaves `record/file.rs` as a pure record codec and gives `log/` every
log file the home writes. `log/` grows to 4 files.

**6. Use one word for op intent across crates.** (naming, rule 19; outside this crate)
With finding 1, home has `record/op.rs` and project has `record/op.rs`, but store
has `admin/record/intent.rs` for the same concept. Its header reads "The one
operation a Store's record holds (`op`…)". Rename it to `admin/record/op.rs`.

**7. Test areas: `tests/call/` uses a rejected word; `log_rotation.rs` and `tail.rs` stay loose.** (naming, rules 7, 12, 18)
`call/` is on the owner's rejected list, and rule 7 covers `tests/`.
`tests/records/` is plural where the `src` folder is `record/`. Two binaries
also stay at the top level, though neither needs its own process; neither sets
any process-global state. Mirror the `src` folders instead:
- `tests/record/main.rs`: `generation`, `names` (typed_names), `op` (op_intent;
  it already exercises the entity lock), `token` (ordered_token), `lock`
  (table_lock).
- `tests/reply/main.rs`: `cursor`, `json` (json_codec), `outcome` (shell_word),
  `shown`.
- `tests/log/main.rs`: `rotation` (log_rotation), `tail`.

**8. `json/read.rs` → `json/stream.rs`.** (naming, low)
Both `value.rs` (`parse`) and `read.rs` read JSON, so "read" does not tell them
apart; the owner already rejected `read/` as too broad. The difference is the
purpose. `value.rs` is the whole call-value codec (shell, worker frames, front
ends). `read.rs` is the streamed shallow reader `Json<R>` for other tools' JSON
(image manifests, platform facts). `stream.rs` says that.

**9. Move `euid()`/`egid()` from `lock/flock.rs` to `record/process.rs`.** (naming, low)
`flock.rs` groups them with the locks only because both go through `nix` with no
unsafe code. They are "this process's ids". Their users are store and sandbox
record discovery and machine `table/probe.rs`. `lock/flock.rs` then holds only
locks.

## 3. Proposed layout

Scope: `control/crates/layerfs-home/src/` only; tests are covered in finding 7.
The listing is relative to the #828 plan:
- A space means the path the plan has, or a move this review makes, noted in the
  annotation.
- `+` means a file no plan row creates.
- `-` means a plan path this review drops.

LOC is production LOC of the old file from `prod_loc_files.txt`
(`tools/production_loc.py --files`, same snapshot as the plan). Split files show
their pre-split LOC, and new split targets show `—`. The crate total is
unchanged at 3,637.

```diff
  control/crates/layerfs-home/src/      [3,637]
    lib.rs                                  20
    cancel.rs                               14   (plan: op/cancel.rs; process-wide SIGINT flag, not an op)
    names.rs                               144
-   outcome.rs                             113   (→ reply/outcome.rs)
    disk/                                [447]   (the home as a folder on disk)
      capacity.rs                           60
      guest.rs                              51
      keys.rs                               40
      layout.rs                            132
      mod.rs                                18   (loses the records re-exports)
      paths.rs                              94
      secrets.rs                            52
    json/                                [789]
      mod.rs                                 4
      strings.rs                           162
      stream.rs                            232   (plan: json/read.rs; renamed)
      value.rs                             391
    lock/                                [175]
      entity.rs                             37
      flock.rs                              31   (euid/egid → record/process.rs)
      home.rs                              107
      mod.rs                                 —
+     table.rs                               —   new (helpers::lock)
    log/                                 [931]
      calls.rs                             377
+     lines.rs                               —   new (log_line, log_swept from record/file.rs)
      mod.rs                                 —
      tail.rs                              434
      trace.rs                             120
-   op/                                      —   (dissolved: → record/op.rs, record/process.rs, cancel.rs)
+   record/                              [656]   (record codec, op intent, owner, tokens; plan files moved in)
      fields.rs                            104   (plan: disk/fields.rs)
      file.rs                              185   (plan: disk/records.rs; loses the log/*.jsonl writers)
      generation.rs                         65   (plan: disk/generation.rs)
+     mod.rs                                 —   new
      observed.rs                           30   (plan: disk/observed.rs)
      op.rs                                188   (plan: op/intent.rs)
      process.rs                            84   (plan: op/process.rs; gains euid/egid)
+     token.rs                               —   new (helpers::ordered_token, random)
    reply/                               [461]
      cursor.rs                             67
      events.rs                             43
      mod.rs                                 —
      outcome.rs                           113   (plan: root outcome.rs)
      state.rs                             151
      time.rs                               24
      values.rs                             63   (loses lock, random, ordered_token)
```

The crate root has 2 loose files (rule 3 allows up to 4), and every leaf folder
has 3 to 8 files. Public paths change from `layerfs_home::disk::{read_record,…}`,
`::op` and `::outcome` to `::record::…` and `::reply::outcome`. The plan already
renames `home` → `disk`, so call sites change in this refactor anyway.

## 4. Left alone, and why

- **`json/` name.** It is named for a format, but it is one concern: the JSON
  text codecs (call values, streamed shallow reader, string quoting). The name
  predicts the contents exactly, and it is not a catch-all of the rule 6 kind.
  Moving the shallow reader down is proven by its users: machine
  `setup/facts.rs` (`Json`), `table/support.rs` and lima `template.rs` (`quoted`), and the sandbox
  docker archive. Machine sits below sandbox, so home is the lowest common crate.
- **`disk/` name.** Once the record codec leaves, it fits: capacity is disk
  space, `guest.rs` is a machine data-disk root, and the rest is files on disk.
  No other planned folder is named `disk`. `layout/` was the alternative, but
  rule 14 would force renaming `layout.rs`, with no clearer result.
- **`log/` and its file names.** `calls.rs` is the call log `calls.jsonl`, and
  it is a file, not the rejected `calls/` folder. `trace.rs` is the
  `LAYERFS_TRACE` route. `tail.rs` is the cursor reader that both log calls use.
- **Moving `keys.rs` up.** `use` lines would allow it: `worker_secret` users are
  machine `setup/`, store `admin/route/place.rs` and inspect, and `daemon_keys`
  users are sandbox and inspect. But `paths.rs` already keeps every entity's path
  in home on purpose, and splitting 40 LOC across two crates is not clearly
  better.
- **Moving `cancel.rs` up to layerfs-store.** Its users (api, store) would allow
  it, but api re-exports it as the process-wide flag for the CLI signal thread,
  so it stays generic.
- **`now_ms` (record) vs `reply/time.rs::iso`.** One makes record timestamps and
  the other formats answers. Different roles, so they are not merged.
- **`reply/` membership beyond findings 3–4, and `names.rs` loose.** `reply/` is
  settled. `names.rs` serves both argument parsing and records, so the root is
  the honest place for it.
- **Two cursors** (`reply/cursor.rs` page cursor, `log/tail.rs::Cursor` byte
  position). They sit in different folders and their headers say which is which.
