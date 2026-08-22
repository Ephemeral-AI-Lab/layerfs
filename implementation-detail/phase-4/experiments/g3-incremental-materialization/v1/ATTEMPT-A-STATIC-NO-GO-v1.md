# G3-v1 Attempt A — static NO-GO

Disposition: **NO-GO for an ordinary user-editable destination; pivot to Attempt B**

The repository has no production native materializer, no exclusive destination
mutation service, no persisted destination-authority state, and no gap-free
filesystem change journal. The available validated snapshot receipt authenticates
the SQLite store/head tuple; it says nothing about current ordinary destination
bytes. Inode, size, timestamps, mode, pathname, a sidecar MAC, kqueue/FSEvents,
and an earlier publication receipt are therefore invalidation hints only.

Exact qualification of a mutable ordinary destination as parent root `P` would
still require reading/authenticating all destination bytes: `Theta(S)`. Building
a watcher or broad OS integration would exceed G3 and would not close process
death, rollback, event loss, same-inode writes, mmap writes, or substitution.
Attempt A is rejected before source implementation because it cannot satisfy the
stated authority without either weakening correctness or doing the full-file work
G3 exists to avoid.

G3-v1 therefore evaluates Attempt B: a same-open protected verified native seed
held only by a read-only, unlinked file descriptor. The mutable destination is
never a payload source or byte authority.
