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
- Field naming follows "Rules for every call" in `build.py` (thing's own word, `_bytes`,
  `size`, `*_at`, `*_ms`, plural = list, `<list>_count` only beside a cut list, lowercase
  enums, the state table).
