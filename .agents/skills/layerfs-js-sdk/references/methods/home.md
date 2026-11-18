# home calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (the home's path is
> `/tmp/demo/home`); the rest is from the table, the spec rows and the
> manual. 1.x `logs.list` and `logs.reset` are `home.log` and
> `home.clear_log`; `doctor.fix` is `home.prune`; the `logs` and `doctor`
> families are gone, with no alias. An agent working on a project rarely
> needs this family; a folder scope refuses it.

```js
home.init()
home.get({ with: ["platforms", "tree"] })
home.log({ call, project, client, tag, outcome, since, limit, cursor })
home.clear_log()
home.doctor({ on_progress })
home.leftovers({ machine })
home.prune(leftover)
```

The home is `LAYERFS_HOME` (default `~/.layerfs`), or the `home` of
`layerfs.client({ home })`. Its layout and settings are in the manual's
[home page](../../../../../docs/versioned/2.0.0/home.md).

## `home.init` and `home.get`

`init` creates the home, converts an older layout, and leaves a current home
as it is. Short: `{ home }`, with `unchanged` or `converted`.

`get` answers the home's state and build. Short: `{ home, state, version }`,
with `generation` when ready (a counter that moves each time a record of the
home is written or removed; a caller that kept a tree reads it again only
when the number differs), and `reason` and `fix` when broken. Long adds
`message`, `layout`, `mode`, `bundle`, `daemon` and `cache`.

```bash
$ layerfs home get                                                             # seen, before init
{"home":"/tmp/demo/home","state":"broken","version":"2.0.0","reason":"missing","fix":"layerfs home init"}
$ layerfs home init                                                            # seen
{"home":"/tmp/demo/home"}
$ layerfs home get                                                             # seen
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":2}
```

Sections:

- `with: ["platforms"]` adds `platforms: { host, items }`, one row per
  platform this build knows (short `{ platform, ready }`, with `missing`).
  It was 1.x `machine.platforms`.
- `with: ["tree"]` adds `tree: { machines, stores, buckets }` from the
  home's records only: each machine with its sqlite `stores` and its
  `sandboxes`, the PostgreSQL Stores (on no machine) and the buckets; a node's
  state shows only when it is not `ready`.

```bash
$ layerfs home get --with tree                                                 # seen
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"tree":{"machines":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native","stores":[{"store":"shop","engine":"sqlite","projects":["api"]}]}],"stores":[],"buckets":[]}}
$ layerfs home get --with platforms                                            # seen
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"platforms":{"host":"macos","items":[{"platform":"native","ready":true},{"platform":"docker","ready":true},{"platform":"apple-container","ready":true},{"platform":"lima","ready":true}]}}
```

## `home.log`

The calls made against this home, newest first, from its call log
(`trace/calls.jsonl`, then the one file it rotates to). Filters, each of
which must match: `call` (such as `file.write`), `project`, `client` (`cli`,
`node`, `python`, `rust`), `tag` (exact, or a prefix ending in `:`),
`outcome` (a row's `outcome` or its `error`) and `since` (milliseconds since
1970). `limit` is 1 to 1000, default 200.

A row's short form is `{ call, outcome, logged_at }`, with `project` when
the call named one and `error` (the public code) when it was refused; the
long form adds `duration_ms`, `client`, `argv`, `commit` and `tag`, and the
answer's long form `skipped` (lines that could not be read). `outcome` is
`refused` for a refused call, else the result's word in snake_case, else
`ok`. A redacted argument (a path, a command, a message, edit text, a URL, a
pattern, content, a cursor) is in `argv` as its byte count. The next page is
`cursor: next_cursor`; a cursor from before a rotation or a `clear_log` is
stale (`InvalidInput`). `get`, `log` and `clear_log` write no line of their
own.

```bash
$ layerfs home log --limit 3                                                   # seen
{"items":[{"call":"home.leftovers","outcome":"ok","logged_at":"2026-11-18T22:05:33Z"},{"call":"home.doctor","outcome":"ok","logged_at":"2026-11-18T22:05:33Z"},{"call":"home.doctor","outcome":"ok","logged_at":"2026-11-18T22:05:32Z"}],"next_cursor":"eyJmb3Ii…"}
```

```js
// every refused call of one project, newest first
let cursor;
do {
  const page = await layerfs.home.log({ project: "shop/api", outcome: "refused", limit: 1000, cursor, detail: true });
  for (const row of page.items) console.log(row.logged_at, row.call, row.error);
  cursor = page.next_cursor;
} while (cursor);
```

A caller tags its lines with the `log_tag` option on any call (shell: the
`LAYERFS_LOGS_TAG` setting).

## `home.clear_log`

Truncates the call log in place and removes the rotated file. Short:
`{"ok":true}`; long: `{ ok, outcome: "cleared", line_count }`.

## `home.doctor`

Checks the home, then each machine, PostgreSQL Store and bucket, shallowly.
The report is the [doctors'](doctor.md) shared shape.

```bash
$ layerfs home doctor                                                          # seen
{"ok":true}
```

## `home.leftovers` and `home.prune`

`leftovers` lists what interrupted operations left behind: one bounded walk
of the home's own folders on `native`, and one through the worker of each
running machine (`machine` walks one). Nothing is started: a machine that is
stopped, busy or does not answer is named in `not_listed`. An item's short
form is `{ leftover, modified_at }`, with `size` and `report_only`; long adds
`removable_at`, `kind`, `machine` and `path`. `leftover` is its id,
`<machine>:<kind>:<name>`; the kinds are `run_folder`, `empty_root`,
`init_copy`, `backup_staging`, `image_tmp` and `export_container`.

```bash
$ layerfs home leftovers                                                       # seen
{"items":[],"not_listed":[{"machine":"docker","blocked_by":{"machine":"docker","reason":"stopped"},"message":"machine docker is stopped"}]}
```

`prune(leftover)` removes the one leftover whose id it is given, once, under
that kind's guard. Short: `{ freed_bytes }`, or `{"ok":true}` when the size
was not measured; long adds `outcome ("removed"), leftover, kind, machine,
path`. Refusals: `InUse` (the kind's guard does not hold; nothing removed),
`NotFound` (no leftover has that id now), `InvalidInput` (an `init_copy`
that is only reported), `Unavailable` (the machine is stopped), `Unknown` (the worker's
answer was lost; nothing is sent again).
