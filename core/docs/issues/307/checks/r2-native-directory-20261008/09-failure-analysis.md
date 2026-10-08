# Directory component failure diagnosis

Initial host selection07/08 passes native_directory and native_lookup, then the
existing engine SQLITE_FULL mutation test fails during Overlay::create. Actual
schema19 startup is69pages on host SQLite3.51.0; the old64-page fixture cannot
initialize. This is a failed setup at the original explicit quota, retained in07.

Select96pages for the engine and costs mutation-failure fixtures so they can
reach their intended DML boundary. Their unchanged64-cell sequence and128KiB
atomic write must still produce real SQLITE_FULL, preserve earlier publication
and roll back logical state, respectively. No runtime bound, workload, product
quota or performance gate is changed. The separate32-page schema-failure tests
remain unchanged. This does not relabel the64-page setup failure as a pass.
Only the repaired engine test and previously unrun host selections need execution;
reuse the already passing native component cases at unchanged product identity.

Repaired selection12/13 passes the pressure fixtures and existing Overlay cases,
then native Workspace enumeration fails after FORGET/rmdir/RELEASEDIR. The exact
directory source still retains metadata, but ordinary SourceView::inherits
correctly rejects an unowned removed namespace serial. Native directory listing
needs the owned case: validate its retained directory metadata and zero visible
entry count, then return EOF without falling through to immutable names. Read
that parent metadata through source_inode so the independent removed domain also
survives a later install. Ordinary unowned listing keeps its existing refusal.
This is a product fix, not a test relaxation. Recheck native directory paths,
the previously unrun cases and the existing namespace-view merge suite.

Source review adds the partial native_directory_open index to the actual
revocation predicate. Without it, an absent live handle could require inspecting
closed headers awaiting bounded cookie cleanup. The index makes open discovery
independent of that closed population; the owning EXPLAIN check requires its
selection. This schema/index change needs final scoped host/Linux checks. It adds
one startup page beyond the earlier69-page observation; the old receipt remains
at its original identity.

Final source inspection establishes that source_directory_entry_window already
calls inode_at, which includes the independent orphan domain. Remove the duplicate
parent source_inode query added during the removed-directory repair; retain the
owned proved-empty EOF behavior. The first Linux run remains valid at its own
earlier source identity, with that redundant read counted honestly.

Parent identity must also remain current after a directory move while its handle
is open. Add a semantic moved_directory value to the existing Changes object,
validated against the same final name binding, and update the one retained parent
row in that publication transaction. Workspace emits it only for a directory
changing parents; ordinary file/name operations pay no extra parent query. This
is native directory-read correctness through existing Workspace mutation ports;
it does not wire or qualify R3 kernel mutation/coherence. A new public test checks
old read custody and a later read's new parent after rename.

The final read-page review narrows immutable kind demand from BaseView::stat to
BaseView::inode. A directory entry needs its authenticated kind, not portable
attributes or file length. The canonical fixture now makes lengths unavailable
during READDIR composition, proving that no unnecessary length dependency remains.
This changes only that Workspace composition path; prior Linux source/binaries
retain their exact wider-demand identity. Recheck the affected native cases at
the final source, without rerunning unaffected suites.
