# Fixes from the freeze's exit review (2026-11-18)

A fresh reviewer checked the frozen spec against the code and found 23 defects.
Each is decided below. Owners: **A** = project/file/branch/commit rows, **B** =
sandbox/workspace/home rows, **C** = machine/store/backup rows, **F** = build.py,
status.html, pg.html, DECISIONS.md, calls.json top level, **M** = the mock
(`../index.html`). Edit only your own files, in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs/tmp/doctor-page/spec-src/` (this copy is
the working copy until the coordinator syncs it into the repository). Keep
`python3 -I build.py "$PWD"` passing for your files.

1. **Live reads and `generation`** (B home.get, C machine.get/list, store.get, F status.html).
   A `--with live` read writes the record only when the derived state word or its reason
   changes; that write moves `generation`. `observed_at` is kept beside the record in an
   observation file that `generation` does not cover. `generation` does not signal the death of
   an op's owner (a dead owner's op is ignored on read, with no write). Records-only reads use
   the last persisted state word and reason, so `engine_down`, `not_answering`, `missing` and
   `unreachable` can appear in a records-only `blocked_by` after a live read saw them.
2. **Commit ids** (A, F rules). Answers carry the full 66-hex id; examples are abbreviated and
   the rules say so. A prefix (8+ hex) that matches more than one Commit is refused
   `InvalidInput` with `matches` (a count). A `<ref>` or `--from R` is a Branch name first; if no
   such Branch exists it is a Commit id or prefix. `commit diff <ref>` for a Commit with no
   parent compares with its Branch's `base`, and with the empty tree when there is none.
3. **`project list --with stats`** (A): the section's keys are `["size"]`, from the head Layer's
   recorded length. No `file_count` there (it would need a format change). `project stats` keeps
   its counts from the bounded walk with `partial`.
4. **`workspace.status`** (B): `changes` is the first 20 entries of a bounded read (the daemon's
   existing bound of 256 overlay rows); `changes_count` is the number of entries read, with
   `partial:true` when the bound was hit. A renamed or removed folder is one entry (the folder's
   path, `type: "dir"`); the count counts entries, not files beneath them. `--path` filters
   within the same bound.
5. **Outdated worker with Sandboxes on it** (C machine.stop/get/doctor, F status.html, M).
   The finding's `fix` is `layerfs machine stop <m> && layerfs machine start <m>` when no Sandbox
   is recorded on the machine, and `layerfs machine stop <m> --force && layerfs machine start <m>`
   when some are (its `message` names the count). The page's button runs only the first form and
   is off while Sandboxes are on the machine, with the count in its tip (R2); the detail panel's
   Fix line shows the `--force` command to copy. The tip no longer says "Close them first"
   (calls through a mismatched worker are not sent, so they cannot be closed): it says
   "N Sandboxes are on it. Copy the fix to stop it anyway."
6. **`sandbox.get` when the machine does not answer** (B): `unknown` with `observed_at`,
   `workspaces` null. Not `blocked`.
7. **`workspaces` type** (B sandbox.list, sandbox.get, home.get tree; M): one type everywhere:
   a list of objects. List rows and tree nodes answer `[{workspace, branch}]`; `sandbox.get`
   adds its further keys per Workspace.
8. **Refusal fold map** (F): add `NoMatch` → `NotFound`; `AmbiguousMatch` → `InvalidInput`
   carrying `matches` and a `fix` adding `--replace-all`; `NoSpace` → `Capacity`; `Timeout` →
   `Unavailable`. (A: file.edit's note names the two edit refusals.)
9. **Call-log redaction** (A, B, C rows; F schema and rules). Each positional and option may
   carry `"redact": true`: the call log records its byte count, never its value. Set it on
   every argument today's `render.rs` list redacts (paths, commands, messages, edit strings,
   URLs, patterns, content) under its new name, and on the new `cwd`, `env`, `include`,
   `exclude`, `stdin`. `from` on `file mv/cp` (a path) is redacted; `from` on `branch fork` and
   `commit diff` (a Branch) is not. F: add `redact` to SCHEMA.md, pass it into calls.json, and
   state the rule. B: fix home.log's long example (`argv` shows byte counts for redacted
   arguments, as today).
10. **Secrets from Node and Python** (C store.save_auth, backup.connect; F rules). The secret is
    an option of kind `Content` with `"stdin": "secret"` semantics: `password` on
    `store save-auth`, `secret_key` on `backup connect`, each `required`, `redact`, with
    `"shell": "--password-stdin"` / `"--key-stdin"` (the shell reads the value from stdin and
    takes it no other way; Node and Python pass the value as the option). F: accept the `shell`
    field in build.py and say it in the rules.
11. **`mode`** (A file.write, file.mkdir): kind `Text`, octal (`"0644"`), the same spelling
    answers use.
12. **Default Workspace name** (B workspace.mount, sandbox.create): the default name is the
    Branch name made valid: lowercased, each character outside the Workspace name alphabet
    replaced by `-`. A collision is refused `Exists` with a `fix` naming `--name`.
    `sandbox create --mount B` uses the same default and has no name option of its own.
13. **Pages against rows** (F status.html, pg.html; rows as named):
    - Tree projects under a Store are bare names, as `store.get` answers them (F fixes status.html).
    - "Backup behind" is a `notice` reported by `store doctor` as check `store.backups` on the
      Store, naming the bucket in `message` and `fix` (`layerfs backup create <store> <bucket>`).
      No bucket check reports it. (C adds the check; F and M show it on the Store row.)
    - The per-entity state table in status.html lists exactly the reasons and `op` names the rows
      use. A, B, C: every row whose answer has `state` lists its possible `reason` codes and
      `op.name` values in a row field `states` (`{"reasons": [...], "ops": [...]}`), from what
      the code records today plus the row's `new_work`. F builds the table from those fields.
    - A project the Store does not list: hidden in `project list` (today's behaviour), a
      `notice` in `project doctor` and `store doctor`. Never "Missing".
    - C: check in the code whether removing a machine removes its PostgreSQL connection records;
      make `machine.remove`'s row and pg.html say what is true (F edits pg.html from C's finding;
      if they are not removed today, removing them is `new_work` of `machine.remove`).
14. **Mock against the frozen report** (M): use the frozen check ids (`home.*`, `bucket.*`,
    `store.connection` with `from`, no `workspace.record`). Row facts come only from tree nodes.
    B extends the tree nodes to what records hold: a machine node has `machine, platform, cpus,
    memory_bytes`; a Store node `store, engine, projects`; a PostgreSQL Store node also
    `connections` (machine names with their own address); a Sandbox node `sandbox, project,
    image, workspaces`; a bucket node `bucket, provider`; each with `state` and its companion
    when not ready. M drops facts no node has (file sizes, "2 Store files", "idle").
15. **Closed check lists** (A project.doctor, B home.doctor and sandbox.doctor, C machine.doctor,
    store.doctor, backup.doctor; F calls.json). Every doctor row carries `checks`: for each thing
    kind it checks, the complete ordered list of `{check, what, full_only}`. One list per kind,
    owned by that kind's doctor row, and a parent's shallow check of a child runs exactly that
    kind's list without the `full_only` entries: `project.record` and `project.listed` wherever a
    project is checked; the Store's own list wherever a Store is checked. Each of today's check
    ids is either in a list (possibly renamed) or named in the row's `off_table`/note as dropped.
    F: build.py collects them into `calls.json` `doctor.checks` (kind → list), asserts every
    check id in any doctor example is in that map, and asserts a kind has one owner row.
16. **Spellings**:
    - `change` is `added`, `modified` or `removed` everywhere (B workspace.status).
    - A machine's free disk is `disk_free_bytes` everywhere (C); a Store's reusable pages are
      `reusable_bytes` (C).
    - `home.log` rows name the call as `call` (not `op`), and the filter is `--call` (a family
      alone, or `family.call`, as today's `--op`); `--tag` matches exactly or by a prefix ending
      in `:`, as today (B). `op` is only ever the state companion object.
    - A list that could not include everything names what is missing in `not_listed`:
      `[{<thing>, blocked_by | reason, message}]`, in `workspace.list`, `backup.list` and
      `home.leftovers` alike (B, C).
    - `not_checked[].reason` (`timeout`, `busy`, `lost`) and a state's `reason` are separate
      enumerations; F says so beside both.
17. **`machine get --with live`** (C): the section does not repeat `observed_at` or `message`;
    add one example of a successful live read with its `live` section.
18. **Small rule breaks**: `file edit --old-string/--new-string` carry `required` (A);
    `backup connect <name> …` (C); `counts` in the doctor answer is the one plural that is a
    map, stated in the rules (F); `machine stats --with space` answers a list
    `[{part, bytes}]` (C); times inside `last`, `attempt`, `last_transfer` are named `*_at`
    (C); a no-op `machine start`/`stop` answers `{"unchanged":true}` (C); leftover kinds are
    snake_case (B).
19. **`log_tag`** (F global options; B home.log): `log_tag` stays a global option (an argument
    in Node and Python; the shell reads `LAYERFS_LOGS_TAG`, a setting, not call context).
20. **Cursors** (F rules; A): a cursor carries the call's filters and, where the call pins a
    Commit, the pin; `project list` and `branch list` cursors carry filters only. A cursor given
    with different filters or pin, or to another call, is refused `InvalidInput`. A cursor whose
    pinned Commit is gone is refused `NotFound`.
21. **`backup.list`** (C): drop `exported_from` and `binding_match` (reading them is extra I/O).
22. **`project doctor --full`** (A; F status.html): it writes nothing to the Store. It may keep
    its visited set in a scratch file outside the Store, as today's integrity scan does, removed
    when the call ends.
23. **Lost replies beyond C13** (C store rows; B workspace.exec): C: read the code for how an
    uncertain `compact`, `backup` or `restore` operation on a Store is cleared today and state it
    in the rows; if nothing clears it, `store get <store> --with live` settles it (new work),
    as `machine get --with live` does for a machine. B: `workspace.exec`'s `Unknown` refusal
    carries `exec`; `--env` is `redact`.
