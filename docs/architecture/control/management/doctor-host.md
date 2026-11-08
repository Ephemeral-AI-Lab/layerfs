# Host diagnostics

> **Status:** Current LayerFS 2.0.0 backend architecture guide.

[Doctor overview](doctor.md).

Record-race fingerprints inspect only management records, selected management
directories, lease stamps and local incoming-file metadata. They do not descend
into daemon runtime trees, mounted Workspaces, logs, traces or SQLite payloads.
Directory identity/mode and selected child membership are compared separately
from volatile runtime/SQLite directory bookkeeping. This detects observed record
changes and rejects incomplete observations; it is not a transactional or
ABA-proof snapshot. Filesystem calls check budgets at defined local boundaries.
