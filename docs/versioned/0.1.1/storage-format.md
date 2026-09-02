# LayerFS 0.1.1 release-candidate storage format

> **Status:** Candidate compatibility record; `v0.1.1` is not published.

The 0.1.1 candidate preserves the five-table SQLite schema, canonical object
encodings, identity domains, content-defined chunking profile, immutable
Layer and Commit records, Branch publication ordering, and authenticated read
requirements defined by the
[0.1.0 storage-format contract](../0.1.0/storage-format.md).

Initialization may avoid persisting obsolete intermediate objects and may
admit final objects in bounded batches. Those changes do not alter reachable
canonical bytes, identifiers, or public Store compatibility. A released 0.1.0
Store must open on the candidate, and failed publication must not expose an
incomplete reachable closure.
