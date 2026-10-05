# S6 non-file custody and exact name inheritance

> **Status:** Selected implementation contract after `cae3d43ed`, written before
> code. Not completion, release or measurement evidence.

Mint lookup owners independently of open/read/operation tokens. Each owner binds
an exact engine/namespace/serial and is nonrecycled; retained request observation
and exact release have the same lost-reply rules as existing capabilities. Lookup
references keep removed regular files, directories and symlinks available until
last independent custody. Root directory zero reference count is its canonical
encoding, not removal: only an explicitly removed non-root final inode from a
semantically checked unlink/rmdir/rename operation enters non-file orphan state.
Raw caller-ID lookup leases retain their explicit lifetime-uniqueness contract.

Removed non-file metadata uses the existing independent orphan domain and fixed
old root, with bounded lower-source migration. Symlink target bytes remain one
format-bounded window, but reads compose the effective layers rather than seek
its original creation generation. Known install/failure composition must not
substitute a newer immutable root or lose a target moved to another live layer.
Regular-file byte mutation still requires an actual writable open capability.
Native LOOKUP/FORGET/RELEASE and cache/invalidation wiring remain S8.

Every stored name row records whether the immediately lower view binds the name.
This boolean is relative to that lower view, as payload cutoffs are, so known
install needs no whole-name rewrite. On first mutation, an existing lower local
binding decides; otherwise the authenticated base fact decides. Rewrites in the
same generation retain the original lower-binding bit. Workspace obtains a
missing immutable fact outside SQL and revalidates mutable name/parent facts in
its publishing job. No absence is guessed from a removed name alone.

Definite-failure composition removes the lower name row in one bounded turn.
The upper final binding survives; its lower-binding bit advances to the removed
lower row's own bit. An upper whiteout whose new bit is false is deleted, with
exact dirty-name accounting. If no upper row exists, transfer the lower final
only when it still matters over the base. Old captures remain independently
owned; no rewrite or deletion happens while their exact reader holds the domain.
Each turn visits one name and indexed point rows, not unrelated namespace keys.

Work is bounded per job/window plus indexed key depth; cumulative migration and
reclamation follow actual touched rows/cells. These additions select no global
collection, total namespace cap, retry/replay or foreground payload fold. Required
physical reservation/headroom and resource/debt/device evidence remain a separate
unfinished S6 slice.
