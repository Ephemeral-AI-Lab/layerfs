# Architecture doc update policy

> **Status:** Current general guide.

When a change must update [`docs/architecture/`](../architecture/README.md),
what it updates, and how the update is checked. How a page is laid out is set
by [How these pages work](../architecture/README.md#how-these-pages-work) and
the [templates](../architecture/_templates/README.md); this policy does not
repeat those rules.

## Where a fact belongs

| Kind of fact | Home |
| --- | --- |
| How the current source works, for contributors | the owning topic under `docs/architecture/<layer>/<topic>/` |
| An operation followed across layers | `docs/architecture/flows/`: steps and links only |
| A term | [`docs/architecture/glossary.md`](../architecture/glossary.md): one row, linking the owning topic |
| How to use LayerFS | the user guide ([`docs/versioned/2.0.0/`](../versioned/2.0.0/README.md)) |
| What ships with a package | that package's README |
| Why a decision was made, fix records, plans, receipts, run output | the GitHub issue or pull request, never the tree |

Architecture pages describe the current source only. Earlier documents stay in
Git history; link them by a pinned commit URL, for example
`https://github.com/Ephemeral-AI-Lab/layerfs/tree/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs`
for the removed `core/docs/`.

## Which topic owns the code

Each topic's facts table names its crates and, in the **Source** row, the
folders it covers. That row is the ownership record:

- Every crate and every major `src/` folder has exactly one owning topic. A
  topic may span crates when one operation does (Commit spans
  `layerfs-workspace` and `layerfs-store-service`).
- To find the owner of a file, use the topic table in the
  [architecture README](../architecture/README.md#every-topic), then the
  topic's Source row.
- A new crate or major folder joins a topic's Source row in the commit that
  adds it. It becomes a new topic only when it has a code boundary of its own
  (a crate, a backend, or a folder of roughly 2.5k lines or more); a new topic
  is added to its layer README, the architecture README topic table and, for
  new terms, the glossary.

## When an update is required

A change updates the owning topic **in the same commit** when it alters any of
the following:

| The change alters | Update |
| --- | --- |
| A crate boundary or dependency | the facts table (Crates, Uses, Used by) and the layer README picture |
| An on-disk, database or wire format | **What it does** and the page that owns the format (for the wire, [Bridge](../architecture/engine/bridge/README.md)) |
| An algorithm or the order of steps | **Picture** and **What it does**; the flow, if a cross-layer step moved |
| An invariant, or what enforces it | **Invariants** |
| A named limit, constant or default | **Limits and failures** |
| An error kind or a refusal | **Limits and failures**, and [errors and events](../architecture/api/calls/errors-and-events.md) when callers see it |
| A call, argument, option or result | the call's `### family.call` heading in `api/sdk/<family>.md`, and the generated blocks below |
| A CLI flag, binding or MCP profile | the matching [front-ends](../architecture/api/front-ends/README.md) page |
| Platform or backend support | [Platforms](../architecture/control/machines/platforms.md) and `control/SUPPORT.json` |
| Where a call runs | the [Where each call runs](../architecture/control/README.md#where-each-call-runs) table |
| A test or example a **Demonstration** row names | that row's command and platform |
| A file path a page links | every Source link to it |
| A term's meaning | the glossary row |

No update is needed for a change that alters none of these, such as a
refactor that keeps behaviour and file paths, or a test-only change that no
Demonstration row names. The commit message or pull request then says
`Docs: no architecture impact (<reason>)`.

## How to update

- **Edit the owning topic.** Do not add a page for a change; extend the
  section that holds the fact.
- **Replace, don't append.** State the new behaviour as current fact and
  delete the old one. No "previously", "changed in", dates, issue history,
  receipts or "what was run" on architecture pages.
- **Cite source.** Each new behaviour row, invariant and limit links the
  source that implements it with a repository-relative link. Read that source
  before writing the row.
- **Link the owner, don't restate it.** The wire contract lives in Bridge,
  call placement in the control README, terms in the glossary.
- **Stay within the page budget.** A README stays near 300 lines and an extra
  page near 250, with at most 2 extra pages per topic (Store 3; the SDK has one
  page per call family). A page that outgrows its budget is cut, or split into
  an extra page that meets the code-boundary threshold.
- **Never hand-edit a generated block.** Regenerate it instead:
  `node api/scripts/gen-coverage.mjs` for
  [coverage](../architecture/api/calls/coverage.md) and
  `node api/mcp/scripts/gen-profiles.mjs` for the
  [MCP profiles](../architecture/api/front-ends/mcp.md).
- **Persistence profile.** Pages state once, where relevant, that Durable
  execution is `NOT_RUN — disabled by owner until explicit reauthorization`,
  and never describe Durable as a default.

## Checks before committing

Run the checks that cover the pages you changed and report the result, or
report that a check was not run and why:

| Check | Covers |
| --- | --- |
| Every relative link and anchor in the changed pages resolves | links, renamed headings, moved files |
| No link points into a removed folder such as `core/docs/` (except the GC design at `core/docs/issues/project-41/`, see the [documentation policy](../general/documentation-policy.md)) | removed material coming back by link |
| Every Source link names a file or folder that exists | pages describing moved code |
| Diagrams are `text` blocks of at most 80 columns | layout |
| `node --test api/node/test/branch-file/inventory.mjs` | one SDK heading per call, the count line (needs the built Node addon) |
| `node api/scripts/gen-coverage.mjs --check` | the coverage block (needs the built CLI and addon) |
| `node api/mcp/scripts/gen-profiles.mjs --check` | the MCP profile block |

Each command runs with a wall timeout of at most 120 seconds. CI reports and
never gates; a green run is not verification of a doc change.

## In the commit and pull request

- Name the topics updated, or give the `Docs: no architecture impact` line.
- Report production LOC as for every commit; documentation does not change it.
- When a change removes documentation, link what it removed by pinned commit
  URL in the pull request, and repoint every surviving link in the same
  commit.
