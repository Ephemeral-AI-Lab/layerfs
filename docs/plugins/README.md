> **Status:** Current general guide.

# LayerFS plugins

Plugins are tools built on LayerFS, each in its own repository. Their **listings** live in
[`plugins/`](../../plugins/) at the repository root: one folder per plugin,
`plugins/<plugin-id>/`, and nothing else, shown at [layerfs.ai/plugins](https://layerfs.ai/plugins/).
A listing describes and pins a plugin; the plugin's code is never copied there. This folder
holds what authors use to make one: this guide, the [`template/`](template/) to copy and the
checker in [`script/`](script/).

To list yours, open a pull request that adds `plugins/<your-plugin-id>/`. The pull request is
yours, from your fork and under your name: once it is merged, you are a LayerFS contributor.

## Build a plugin

A plugin builds on LayerFS's JavaScript packages. Their wiki,
[`api/README.md`](../../api/README.md), says which one a plugin needs and links each one's
guide. To have a coding agent design and build a plugin around them, paste this into one
running in your plugin's repository. On [layerfs.ai/plugins](https://layerfs.ai/plugins/)
`{{layerfs.commit}}` is filled in with the LayerFS commit plugins build on now.

<!-- The same text is on layerfs.ai/plugins/ (build-prompt.ts in the website); keep them in step. -->

```text
Build a LayerFS plugin with me: <what it should do>.

1. Clone https://github.com/Ephemeral-AI-Lab/layerfs beside this repository and check
   out commit {{layerfs.commit}}. Build against that commit only, and record it in this
   repository as layerfs.json: { "commit": "<the 40-character SHA>" }. The listing
   prompt reads it from there.
2. Read api/README.md in that checkout: it maps LayerFS's JavaScript packages and links
   each one's guide. Decide which kind of plugin this is, and tell me before writing
   code:
   - a tool or skill for agents: @layerfs/agent. Agents that only have Bash use its
     layerfs command.
   - something that shows a project: also @layerfs/backend.
   - a host that runs projects for its users: also @layerfs/runtime.
3. Read the guides for the packages you chose, and the skill in
   .agents/skills/layerfs-js-sdk/. If agents will call LayerFS through the plugin, give
   them that skill.
4. Design within these rules:
   - Use only the packages' public calls. Never import @layerfs/agent/internal or
     LayerFS source files.
   - Settings are the LAYERFS_* environment names. If the host can only set its own
     names, map them in the plugin.
   - Store calls run in the calling process on Linux only. On macOS and Windows, start
     the worker with @layerfs/runtime and pass its endpoint to agents
     (LAYERFS_WORKER_ENDPOINT).
   - Each call is attempted once. Handle a refusal (Conflict, Busy, NoSuchSandbox, …)
     the way the guides say; never retry or replay a call on your own.
   - Change nothing in the LayerFS checkout. If the plugin needs something LayerFS
     doesn't have, stop and tell me.
5. Write a short design: which packages and calls, where each part runs, and what
   agents see. Wait for my OK.
6. Build it in this repository, with tests that run without Docker by default, and a
   README with an Install section a coding agent can follow.
7. When it works and is pushed, list it with the prompt at
   https://layerfs.ai/plugins/submit/.

If a step fails, stop and give me the exact error; don't retry it on your own.
```

## List it with your coding agent

Paste this into a coding agent running in your plugin's repository. It needs `git`,
`python3` and the GitHub CLI (`gh`) signed in to your account. It writes the four files, checks
them and shows them to you; it opens the pull request only after you say yes.

<!-- The same text is on layerfs.ai/plugins/submit/ (submit-prompt.ts in the website); keep them in step. -->

```text
Submit this plugin to the LayerFS plugin listings for me: generate its listing files and
open a pull request to https://github.com/Ephemeral-AI-Lab/layerfs.

1. Read https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/plugins/README.md. It is the
   guide: every file, field and rule.
2. Gather the facts from this repository: name, a one-line summary, authors, license, the
   GitHub repository URL (git remote), the version (package.json, Cargo.toml, pyproject.toml
   or the latest tag), the platforms it runs on, which LayerFS parts it uses, and how to
   install it (its README). The commit to list is HEAD: check that it is pushed
   (git status, git log origin/HEAD..HEAD). The LayerFS commit is "commit" in this
   repository's layerfs.json, the one it was built and tested with; if there is no
   layerfs.json, ask me for it. Never guess a commit.
3. Fork and clone LayerFS: gh repo fork Ephemeral-AI-Lab/layerfs --clone, then in the clone
   create a branch named add-plugin-<id>.
4. Copy docs/plugins/template/ to plugins/<id>/ (a–z, 0–9 and dashes). If plugins/<id>/ already
   exists, this is a new version: add it at the top of versions in its plugin.json and
   update the other files only where they changed.
5. Fill in the four files from what you gathered, replacing every <…>:
   - plugin.json: the fields, 1 to 6 tags, and the version with both commits.
   - README.md: the display page, from ## headings down, from this repository's README.
     Image links must be https:// links to files in this repository at the listed commit.
   - install.md: the steps an agent follows to install it, using {{layerfs.commit}},
     {{commit}}, {{version}} and {{repository}}; it stops on any error.
   - logo.svg or logo.png: this repository's logo if it is square and under 64 KB.
     Otherwise ask me for one; never keep the template's logo.
6. Run python3 docs/plugins/script/plugin_check.py --online plugins/<id> in the clone and fix every
   problem it reports until it prints ok. Change nothing outside plugins/<id>/.
7. Show me the four files and the checker's output, and wait for my yes.
8. Then commit with the message "Add plugin: <name>", push the branch to my fork, and open
   the pull request with gh pr create --repo Ephemeral-AI-Lab/layerfs --title
   "Add plugin: <name>", its body saying what the plugin does and that plugin_check passes.
   Give me the pull request's link.

If a step fails, stop and give me the exact error; don't retry it on your own.
```

## List it by hand

1. Fork this repository.
2. Copy [`docs/plugins/template/`](template/) to `plugins/<your-plugin-id>/`. The id is the folder's name:
   a–z, 0–9 and dashes, at most 40 characters, not `submit` or `script`. It becomes the address,
   `layerfs.ai/plugins/<id>/`.
3. Fill in the four files below. Replace every `<…>` and delete what does not apply.
4. Check it: `python3 docs/plugins/script/plugin_check.py --online plugins/<your-plugin-id>` prints `ok` when
   every rule passes, or names each problem.
5. Open a pull request titled `Add plugin: <name>`. We review it, may ask for changes on the
   pull request, and merge it. The plugin appears on the site after its next build.

## The four files

Every listing has all four.

| File | What it is |
|---|---|
| `plugin.json` | About the plugin, its tags and its versions (below) |
| `README.md` | The display page: the plugin's page on the site, in Markdown |
| `install.md` | The install prompt: what people paste into a coding agent to install it |
| `logo.svg` or `logo.png` | The logo: square, under 64 KB, shown as supplied and never recolored |

### plugin.json

| Field | Required | What to put |
|---|---|---|
| `schema` | yes | `1` |
| `id` | yes | the folder's name |
| `name` | yes | as shown on the card and page |
| `summary` | yes | one line, at most 90 characters |
| `authors` | yes | `[{ "name": "…", "github": "…" }]`; `github` is optional |
| `license` | yes | an SPDX name, such as `MIT` |
| `repository` | yes | `https://github.com/<owner>/<repo>`, public |
| `tags` | yes | 1 to 6 lowercase tags: a–z, 0–9 and dashes |
| `platforms` | yes | any of `linux`, `macos`, `windows-wsl2` |
| `uses` | yes | what it builds on: any of `agent` (`@layerfs/agent` and the `layerfs` command), `backend` (`@layerfs/backend`), `runtime` (`@layerfs/runtime`), `daemon` (`layerfs-daemon` run directly) |
| `status` | yes | `preview` (works, partly checked) or `available` (used and checked) |
| `versions` | yes | at least one, **newest first** (below) |
| `host` | no | the tool it plugs into: `{ "name": "…", "url": "…" }` |

Each entry in `versions`:

| Field | Required | What to put |
|---|---|---|
| `version` | yes | the plugin's version, `x.y.z` |
| `commit` | yes | the full 40-character commit SHA of that version in your repository |
| `date` | yes | `YYYY-MM-DD` |
| `layerfs.version` | yes | the LayerFS release at or before the commit below, such as `v1.1.0` |
| `layerfs.commit` | yes | the full 40-character LayerFS commit SHA you tested it with: `commit` in your repository's `layerfs.json`, which the build prompt writes |
| `notes` | no | one line: what changed |

Pin exact commits, never branches: the install prompt installs exactly these. To release a
new version later, open a pull request that adds it to the top of `versions`.

### README.md

Start at `##` headings: the page already shows the name, summary and tags. Say what the plugin
does, what it adds, how it works, what it needs and its limits. Images must be links to files
in your repository at a pinned commit.

### install.md

The steps a coding agent follows, in order, stopping on any error. Name the pins with these
placeholders; the site fills them in for the version a reader picks:

| Placeholder | Becomes |
|---|---|
| `{{version}}` | the version, such as `0.2.0` |
| `{{commit}}` | that version's plugin commit |
| `{{layerfs.version}}` | its LayerFS release |
| `{{layerfs.commit}}` | its LayerFS commit (required: the prompt must install it) |
| `{{repository}}` | the repository URL |

Installing runs your build on someone else's machine, so keep the prompt to what is needed.
We read every install prompt before merging.

## Checking a listing

[`script/plugin_check.py`](script/plugin_check.py) checks every rule above:

```sh
python3 docs/plugins/script/plugin_check.py plugins/<id>            # the files and fields
python3 docs/plugins/script/plugin_check.py --online plugins/<id>   # also: repository public, commits exist,
                                                                    # LayerFS commits on main
python3 docs/plugins/script/plugin_check.py                         # every listing in plugins/
```

It prints `ok` or names each problem, and exits non-zero on any. It also catches template
blanks left in, unknown fields, unknown placeholders, a logo that is not square and the
template's own logo. The website refuses a listing that breaks a rule the same way
(`rules()` in the website's `src/components/plugins/schema.ts`; the two are kept in step).
`--online` makes one GitHub API request per fact, 60 an hour without a token; set
`GITHUB_TOKEN` for more.
