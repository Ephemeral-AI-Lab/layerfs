# LayerFS 0.1.1 Developer Preview — draft

> **Publication status:** Blocked. Do not publish until every item in
> [verification.md](verification.md) passes against the tagged source.

LayerFS 0.1.1 preserves the 0.1.0 public and storage contracts while improving
large existing-directory initialization, localized small-edit Commit,
Workspace Create, and bounded reads.

Candidate highlights:

- bounded initialization with eight existing producers, four fixed slabs, and
  the calling thread as the sole SQLite admission owner;
- no canonical object-segment spool or parent payload copy on the admitted
  direct path;
- exact operation-local metadata reuse without a persistent cache;
- localized Commit planning for ordinary content-only edits; and
- demand-loaded authenticated Workspace bootstrap objects and bounded
  read-ahead.

Correctness, resource, cleanup, FUSE/materialization equality, managed Docker,
native quality, namespace performance, and registered payload gates pass in
the terminal retained evidence. The workspace version is `0.1.1`.
Publication remains blocked because the final clean commit and CI identity
have not been recorded, and release artifacts, checksums, and the annotated
tag do not exist.

When those gates pass, replace this warning with the exact tag, commit,
benchmark source seal, artifact links, and checksums. Until then, LayerFS 0.1.0
remains the latest published Developer Preview.
