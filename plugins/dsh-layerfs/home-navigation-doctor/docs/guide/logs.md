# Logs

> **Where:** `LayerFS › Logs` · **Opens from:** the Logs button (list icon) in the [Center nav bar](components.md#nav-bar); a call detail's **Calls of this session** / **Calls of this checkpoint** reopens it filtered · **Source:** see the end

The detail views below the current entry point come from the earlier demo setup.

![1 Doctor, 2 Logs and 3 Settings (isolated fixture data)](img/home-navigation.png)

*Isolated fixture data.*

Logs lists every LayerFS call made on this LayerFS home, newest first: one row per call,
with the exact CLI command, its outcome, what it saved and, for calls DSH made, the session,
turn and checkpoint they belong to. It is how you check which CLI call a click ran: press a
button anywhere in the app, then look at the top of this page.

![The Logs page, numbered](img/logs-overview.png)

| # | Component | What it is |
|---|---|---|
| 1 | **Refresh** | Reads the log again from the top |
| 2 | **Reset…** | Clears the call history (asks first) |
| 3 | Project search | Only calls naming a project (`store/project`) |
| 4 | **Operation** | Only one family or call (`workspace`, `workspace.commit`…) |
| 5 | **Client** | **All** · **cli** · **node** · **python** · **rust** |
| 6 | **Outcome** | **All** · **OK** · **Refused** |
| 7 | **Writes only** | Hides reads (on by default) |
| 8 | Command | The call as typed: `layerfs workspace commit dsh-…:main --message '<10 bytes>' --detail` |
| 9 | Saved | What the call saved: **committed** / **forked** / **merged** / **published** / **unchanged** and a short id |
| 10 | Session | The session it belongs to and where: **turn 3 · turn checkpoint**, **turn 1 · tool call**, **host save**, **turn 0 · base · references** |

Above the page is the [nav bar](components.md#nav-bar): `Home › Logs`.

## Layout

A head row (**Logs**, **Refresh**, **Reset…**), the filters, then the table of calls with
**Load older calls** at its foot. A row opens the call's detail.

## Body

The table answers "what did LayerFS do, for whom, and did it work". The page reads LayerFS's
own call log for this home (`home log`), which every client of the home writes to (DSH,
LayerFS Connect, terminals, scripts). With a runtime that has no `home log`, it reads the
plugin's stand-in (`trace/calls.jsonl`), which holds only DSH's calls. A row is one call;
DSH tags each of its calls `dsh:<session>:<receipt>`, and the host labels the row with that
session's title, turn and checkpoint.

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| Table | 100 calls a page, newest first: **Time** (relative, exact on hover), **Client**, **Command** (family and call in bold; contents as `'<N bytes>'`), **Outcome**, **Duration**, **Saved**, **Session** | Click or Enter on a row → the call detail. ↑/↓ move between rows. Hover or focus the command → the full command | `logs.list` → `home log --detail` (read), filters passed as `--call`, `--client`, `--tag`, `--outcome` | **Loading calls…**; **Could not read the call log** with **Retry** |
| **Refresh** | | Press → reads the first page again | `home log` (read) | Disabled while reading. The page also rereads on each new call (the host's `logs` event), on reconnect and when the window gets focus, keeping pages already loaded |
| Project search | | Type `store/project` → only calls naming it (after a short pause); Escape clears | `home log` (read; matched by substring here) | |
| **Operation** | Families and calls seen so far | Pick → only those | `home log --call` (read) | |
| **Client** | | Pick → only that client | `home log --client` (read) | |
| **Outcome** | | **OK** → only successes; **Refused** → every other outcome | `home log --outcome ok` (read); **Refused** filters here | |
| **Writes only** | On by default | Off → reads show too | — (filtered by the host) | **No writes yet** · *Read calls are hidden.* **Show reads** |
| Outcome cell | **OK**, or LayerFS's code in red (`NotFound`, `Busy`…) | — | — | |
| Saved cell | **committed** / **forked** / **merged** / **published** / **unchanged** and the version's short id (full id on hover) | — | — | **—** when the call saved nothing |
| Session cell | The session's title, and **turn *n* · tool call**, **turn *n* · turn checkpoint**, **host save**, **references** | Click the title → opens that conversation | — | **—** for calls DSH did not make |
| **Load older calls** | | Press → the next 100 | `home log --cursor` (read) | **All calls loaded.**; ***n* unreadable lines were skipped.** |
| Tag chip | **Calls of session *ef98…*** (the session id without DSH's `session-` prefix) or **Calls of call *id*** | **×** (**Show all calls**) → the filter goes | — | Shows when the page was opened for one session, checkpoint or call. **These calls are not in this machine’s log** when none are |

![The page filtered to one session](img/logs-session.png)

Opened for one session (1, from **Calls of this session**), the page clears every other
filter, so **Writes only** (2) is off and reads and refusals show too. This fixture's
session made 10 calls: the Branch fork (**turn 0 · base**), DSH's own `sandbox get` (refused
**NotFound**, before it creates the Sandbox), `sandbox create` and `workspace mount` (**host
save**), then per turn one `workspace exec` (the agent's tool call) and one `workspace commit`
(the turn checkpoint).

## Call detail

![A call's detail, numbered](img/logs-detail.png)

| Component | What it shows | Interaction → result | LayerFS call | States |
|---|---|---|---|---|
| Head | The call (`workspace commit`) and its outcome; ***i* of *n*** | ↑ / ↓ (1) or the arrow keys → previous / next call in the table. **×** or Escape → back to the row | — | |
| **Command** | The full command | **Copy** (2) → the command on the clipboard | — | **Copied** |
| **Details** (3) | **Started**, **Duration**, **Client**, **Outcome**, **Result** (`committed`), **Saved version** (**created** / **unchanged** / **references**, the full id), **Tag**, **Session** (title, turn and checkpoint) | **Copy** beside the version and the tag | — | |
| **Open session** (4) | | Press → opens the conversation | — | Only for calls DSH made |
| **Calls of this checkpoint** / **Calls with this tag** (5) | | Press → the table shows only calls with this tag | `home log --tag` (read) | |
| **Calls of this session** (6) | | Press → the table shows only this session's calls | `home log --tag dsh:<session>:` (read) | |
| **Output** | **Exit code**, **stdout** and **stderr** sizes, each stream with **Copy**, JSON marked; a refusal's code, message, **Fix:** and **Refusal details** | **Retry** when the output could not be read | `logs.output` (no LayerFS call: the outputs DSH keeps for its own calls) | **Loading output…**; **The call printed nothing.**; **Truncated: …** |

![No output kept for a call](img/logs-output.png)

**No output kept for this call** appears for calls other clients made, for older calls, and
after a Reset: *Outputs are kept only for calls made through DSH on this machine, and only
recent ones; Reset clears them. LayerFS’s own call log never stores output.*

## Variants

- **LayerFS's call log** (`home log`, this server): every client of the home is listed, and
  Reset clears it for all of them.
- **The plugin's stand-in** (a runtime without `home log`): only calls made through DSH, and
  Reset clears only those.
- **Imported sessions:** for a session tag, calls saved with the session in LayerFS (its
  receipts) follow this machine's lines.

## Modals opened here

| Modal | Opened by | What it does | LayerFS call | Undo |
|---|---|---|---|---|
| **Reset the call log?** | **Reset…** | *The call history of this LayerFS home is cleared for every client that uses it (DSH, LayerFS Connect, terminals and scripts). Checkpoints and saved versions are kept.* (with the stand-in: *Every call recorded through DSH on this machine is removed…*). **Cancel** · **Reset** → ***n* calls removed.** | `logs.reset` → `home clear-log` (or the stand-in file), and the kept outputs | None: the history is gone |
| Call detail | A row | Above | — | |

![Reset the call log?](img/logs-reset.png)

**Reset…** is disabled while the log is empty. A [confirm dialog](components.md#confirm-dialog).

## States and signals

- A red code in **Outcome**: LayerFS refused the call. Open the row for the refusal's
  message and **Fix:**.
- **These calls are not in this machine’s log**: the session was imported from another
  machine, or the log was reset since.
- **No calls match these filters**: change or clear them.

## Where next

- [Conversation](conversation.md): a session's own view of these calls (Trajectory, and the
  Sandbox map's Activity).
- [Doctor](health.md): check what a refused call points at.
- [cli-map.md](cli-map.md): which click runs which call.

## Source

`src/client/logs/`: `LogsPage.tsx`, `CallDetail.tsx`, `Command.tsx`, `format.ts`,
`request.ts` (opening the page for a tag), `session-words.ts`, `locale.ts`. Host:
`src/host/logs/index.ts` (`logs.list`, `logs.reset`, `logs.output`, the `logs` event),
`src/host/logs/source.ts` (`home log` or the stand-in), `src/host/layerfs/call-log.ts`
(the stand-in), `src/host/layerfs/call-output.ts` (kept outputs).
