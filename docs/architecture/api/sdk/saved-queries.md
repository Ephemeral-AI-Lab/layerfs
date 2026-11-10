# Saved branch and file queries

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The implemented branch/file extension is tracked by
[layerfs-project#128](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/128).
These shared decisions govern its branch/file call additions and extensions.
Connect's existing renderer simulation informed the requirements; integration with
Connect is outside this work.

## Identities and relationships

`main` is project core content. Its saved versions are publication Layers; a side
branch's saved versions are Commits. A typed reference carries the full stored
ID, its `Commit` or `Layer` kind, root, and recorded originating branch when known.
Display prefixes never serve as immutable query identities. A Commit parent,
base Layer, merge parent and a Layer's source Commit are distinct relationships.
Main walks publication parents, while file provenance can follow a publication's
source Commit. Publication does not itself change the ownership of equal lines.

The current Layer record stores no publication timestamp. Layer details therefore
return `time: null` with `time_status: "unavailable"`; `source_time` is separately
available when a source Commit records it. Do not substitute the source time or
an observation time for a nonexistent publication time. This extension does not
change Store format to add that timestamp.

## Numbered pages

Public numbered queries use `page` (one-based, default 1) and `page_size` (default
50, 1..1000). They count **matching results**, not candidates examined. A filter
miss cannot consume a returned-result slot. No caller cursor is required.
Explicit `page` cannot be combined with legacy `page_pos` or `cursor`. Existing
legacy paging remains supported on calls that already accepted it.

The first saved-history response returns a full resolved anchor. Reuse its ID as
`at_commit` on later pages, holding filters, traversal, page size and scope fixed.
Comparison returns both resolved endpoints; all subsequent path/file reads use
those exact IDs. A refresh of mutable heads is a new query. Branch inventory is
live unless a separate inventory snapshot is implemented; one saved Commit
cannot freeze additions/removals of branch names.

A numbered request admits at most 10,000 candidate visits, including seeking,
and a 48 KiB read-reply budget. These are request budgets, not a total history,
file-count or Store-size limit. Mandatory page records that do not fit, or a scan
that cannot determine its result page within its budget, produce `Capacity`.
The refusal marks incomplete work; it does not return an empty successful page,
advance past unreturned matches, or cause an automatic retry. Optional large
hunks/details can be omitted only with their explicit omission/coverage fields.
An indexed seek or bounded immutable-query page-boundary cache avoids repeating
completed prefix walks where the underlying catalogue supplies no direct index.
Cache eviction never changes query results; a cold request can explicitly refuse
work that exceeds its replay budget.

Successful pages retain boolean `has_more`; false is justified by observed
exhaustion, not by a short or empty filtered scan. No exact total/page count is
promised. A page boundary cache is scoped by Store/project, pinned endpoint(s),
filters, traversal and page size, and cannot authorize access or substitute for
saved-record validation.

## Differences and attribution

Keep Added/Removed/Modified change kinds. Content bytes, persisted metadata,
inode identity and line attribution are independent change dimensions.
`include_attribution` is opt-in. It must discover paths whose bytes are equal but
whose aligned, unchanged lines have different known owners. Content-root equality
cannot prune attribution comparisons. Added/removed text is a content change;
unknown or coarse ownership is not a confident attribution-only change.

File comparison preserves its existing left/right arguments. Missing-file sides
require the documented explicit opt-in; legacy missing-path refusals remain.
Binary, too-large, clipped, coarse, unrequested and unknown attribution states
remain distinct from an exact comparison with no owner changes. Blame and diff
are computed at saved immutable roots, never at live Workspace overlays.

## Scope and compatibility

`branch.commits`, `branch.get_commit`, `branch.diff` and `file.revision` remain
unavailable to folder-scoped clients: full history records expose project-wide
metadata. Existing permitted file calls continue to check every input path.
Project identity validation applies to saved references as well as branch names.
Read calls do not start machines, publish or acquire a write lane. Worker routes
forward the same checked options and preserve structured refusals without replay.

`commit.*`, existing write calls and legacy option/result fields remain supported.
The call table owns JS/Python/CLI names/options; generated declarations and per-call
pages must agree with the implementation. Nullable/partial additions must not
silently widen legacy boolean fields or replace established refusal behavior.

## Stateless matching-result continuation

Pinned `branch.diff` and `file.revision` return a `continuation` when another
page exists. A new process repeats the pin/query and passes the token to seek
from its authenticated immutable boundary. Page cache eviction is irrelevant
to this route. Query/pin mismatches and tampering are `InvalidInput`; all
existing per-page Capacity refusals remain. Commit comparisons now also
accept full Layer endpoints through the existing scalable plan and path cursor.
