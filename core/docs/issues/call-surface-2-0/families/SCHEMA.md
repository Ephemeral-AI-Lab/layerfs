# Family data files: the frozen call table as data

One file per family: `families/<family>.json`. `build.py` loads these (in the
order project, file, sandbox, workspace, branch, commit, home, machine, store,
backup), renders the proposal page from them and writes `calls.json`, the
contract every implementation package and the generated contract test read.
Nothing about a call may live only in prose: if a package needs it to build
the call, it is a field here.

```json
{
  "family": "file",
  "who": "agents",              // or null
  "old_count": 15,              // calls in this family today (code's own table)
  "blurb": "html",
  "reasons": ["html", "..."],
  "calls": [ <call>, ... ]      // in help order
}
```

A `<call>`:

```json
{
  "call": "file.read",
  "badge": "reshaped",          // kept | extended | renamed | reshaped | merged | new | moved | paging
  "note": "html, short",
  "today": ["file.read"],       // today's call names this row replaces; [] when new
  "help": "one line of help, plain text, as the CLI prints it",
  "cli": "layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]",
  "positionals": [
    {"name": "project", "kind": "Project"},
    {"name": "path", "kind": "Text", "optional": true}
  ],
  "options": [
    {"name": "at", "kind": "Name", "help": "read as of this Commit"},
    {"name": "limit", "kind": "Int", "default": 2000}
  ],
  "stdin": null,                // "content" | "json" | "secret" | null
  "with": {                     // sections --with can add; {} when none
    "stats": {"where": "item", "keys": ["files", "size"], "help": "…"}
  },
  "paging": null,               // "cursor" | "line" | "byte" | null (unpaged because bounded: say why in note)
  "progress": false,            // true when the call reports {"phase","done","total"} events
  "quiet": false,               // true when the call writes no call-log line (a read a UI polls)
  "profile": "agent",           // "agent" (in the MCP agent profile) | "full"
  "short": {"always": ["text"], "sometimes": ["next_line", "binary", "size"]},
  "long": ["text", "next_line", "binary", "size", "commit", "root", "path", "mode", "lines"],
  "item": null,                 // for a call answering {"items":[…]}: {"short": {"always": [...], "sometimes": [...]}, "long": [...]}; a list of plain strings: {"scalar": "path"}
  "examples": {"short": ["{…}", "binary: {…}"], "long": "{…}"},
  "dropped": [                  // every option/positional that exists today and is not in this row
    {"today": "--offset", "now": "--line", "why": "1-based line window"}
  ],
  "new_work": null              // plain text when the row needs capability the code does not have today
}
```

Optional fields (added by the exit review; `build.py` checks them and passes them into `calls.json`):

- On a positional or an option:
  - `"required": true` on an option that must be given. `cli` then shows it without brackets.
  - `"redact": true`: the call log records the argument's byte count, never its value.
  - `"repeat": true` on an option that may be given more than once. A row may instead list such
    options by name: `"repeat": ["env"]`.
  - `"shell": "--password-stdin"` on a secret option (kind `Content`, `required`, `redact`): the
    shell reads the value from stdin under that flag and takes it no other way; Node and Python
    pass the value as the option. `cli` shows the shell flag, and the row's `stdin` is `"secret"`.
- On a row:
  - `"states": {"reasons": [...], "ops": [...]}` on every row whose `long` or `item.long` has
    `state`: the reason codes its thing can be broken with and the `op.name` values it can answer.
    The per-entity state table is built from these.
  - `"checks": {"<kind>": [{"check": "store.record", "what": "…", "full_only": false}, …]}` on
    every doctor row: for each kind of thing whose list the row owns, the complete ordered list.
    The list form `[{"kind": "store", "checks": [...]}]` is accepted too. Owners: `home.doctor`
    home; `machine.doctor` machine; `store.doctor` store; `project.doctor` project;
    `sandbox.doctor` sandbox and workspace; `backup.doctor` bucket.
  - `off_table`, `nested`, `refusals`, `kinds` and any other extra field pass through unchecked.

Rules the data must satisfy (`build.py` asserts them):

- `kind` is one of the code's kinds: `Name`, `Project`, `Text`, `Int`, `Bool`, `Json`,
  `Image`, `Content`, `Size`, `Function`. Names are snake_case; the shell option is the
  name with `-` for `_`.
- Every positional and option name appears in `cli`, and nothing else does.
- `long` is the complete, ordered key list of the long form, always present (nulls and
  empty lists included). It contains every key of `short.always` and `short.sometimes`.
  For a list call `long` holds the top-level keys (`items`, `next_cursor`, …) and
  `item.long` holds each row's keys. No "as write" shorthand: spell every key out.
- `short.always` keys are present in every short answer; `short.sometimes` only when
  they are news (non-null, true, non-empty). An action with nothing to return has
  `short.always == ["ok"]`.
- Every `examples.short` line parses as JSON after its optional `label: ` prefix
  and uses only keys from `short` (or `item.short`). `examples.long` parses and has
  exactly the keys of `long` (and each item exactly `item.long`).
- Every `with` section's keys are listed; a section answers under its own key
  (`"live": {...}`) unless `where` is `"item"` or `"top"` (keys added flat to each
  item, or to the top level).
- `today` across all families covers each of today's 84 calls exactly once; every option
  and positional of those calls is either in the new row or in `dropped`.
- `cli` conventions: `--with X` appears only when `with` is non-empty, and `with` is never in
  `options`; `-m msg` stands for the option `message`; a stdin input is `< content` or
  `< changes.json`, the last positional (kind `Content` or `Json`) of a row with `stdin` set;
  `on_progress` is not an option (`progress: true` carries it).
- A `dropped[].today` is one or more tokens joined by ` · `, each `--opt`, `<pos>` or `[pos]`, with
  an optional ` (old.call, …)` suffix on merged rows; every token exists on one of the row's
  `today` calls.
- `ok` appears in a short answer only alone: a call that never has news has
  `short.always == ["ok"]` and nothing in `sometimes`; a call that sometimes has news lists `ok` in
  `short.sometimes`. `long` keeps `ok`, true in every long form but a doctor's.
- Redaction: an argument today's call log redacts keeps `redact` under its new name, and so do
  `cwd`, `env`, `include`, `exclude` and `stdin`. `from` is redacted on `file.mv` and `file.cp`
  (a path) and not on `branch.fork` or `commit.diff` (a Branch). Family files do not mark
  `cursor`; `build.py` redacts every `cursor` option when it writes `calls.json`.
- State: every example object with `state` carries that state's companions and no other; a broken
  example's `reason` and an example's `op.name` are in the row's `states`. `op` is only ever the
  companion object.
- Lists: a `not_listed` entry is `{<thing>, blocked_by | reason, message}` with `reason` one of
  `timeout`, `busy`, `lost`, `refused`.
- Doctors: `short` is `{always: [], sometimes: [ok, findings, not_checked]}`; `long` is `ok,
  findings, not_checked, status, counts, observed_at, tree`. A `not_checked` entry's `reason` is
  `timeout`, `busy` or `lost` (a separate list from a state's reason codes). Every check id in an
  example is in its kind's `checks` list, and a `full_only` check appears only on a node whose
  `coverage` is `full`. `counts` is the one plural key that is a map.
- Field naming follows "Rules for every call" in `build.py` (thing's own word, `_bytes`,
  `size`, `*_at`, `*_ms`, plural = list, `<list>_count` only beside a cut list, lowercase
  enums, the state table).
