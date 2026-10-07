# Authenticated sealed Store installation

> **Status:** Implemented source after `2f4668b73`; functional evidence is tracked
> in [F5](../issues/307/PRE-S8-F5-20261007.md). No FUSE, Exec or timing admission.

SDK `install(&SealedProject, &mut Connection)` streams the sealed file without
opening a database or reading the native source directory. The host sends one
checked StoreManifest, receives the original temporary-file admission, streams
at most65519 bytes per native record, checks exact length and EOF, then sends a
final marker. Only that marker permits publication. No container grows with total
file size. Completed local sends and acknowledged daemon writes are separate
counts; channel diagnostics own cryptographic/socket work.

Daemon `receive_install` accepts an authenticated connection and a destination
provisioned by its caller. The manifest cannot choose another destination. It
checks metadata, typed history identities and startup settings before creating
anything. An existing canonical parent resolves aliases to one destination and
one deterministic `.installing` name. Exclusive create_new owns that temporary
name once. A second destination check detects an earlier installer that published
between the first check and exclusive creation. No cooperating installer can
publish while another owns that same temporary name; there is no wait or retry.
This is one-time file installation, not coordination for ongoing Store writes.

The daemon writes positive-progress windows, never repeats a failed write,
checks the final marker and renames once. Durable synchronizes the global file
and containing directory; Disposable does neither. No overlay backing is synced.
The parent directory belongs to the deployment and must not be modified by another
actor during installation. This ownership rule is also required for provisioning
SQLite's Store placement on a single-kernel local filesystem/named volume.

After a known rename, concrete bootstrap opens one writer first and a fixed read
set. It reports the actual SQLite version. The original first Branch, stack,
headless genesis root, filesystem profile/scope, root inode/listing page and
portable metadata are checked using the same bounded root validator as bind.
Install does not qualify the whole namespace. It returns the opened Store for
normal daemon use and acknowledges an exact manifest with both SQLite versions.
The host then has only native control authority, never a remote data service.

A failed attempt retains the original manifest, exact destination/temporary name,
acknowledged claim, write counts, known rename, opened Store if any, original typed
cause, reply failure and socket-fence result. A failed rename is uncertain; no
subsequent stat/read guesses its outcome. No partial/output file is automatically
removed. A client preserves whether request send was attempted, admission arrived
and an original installed acknowledgement arrived; a malformed acknowledgement
remains retained. Failed active protocol attempts fence the owned connection.
Before-effect local validation leaves an unused connection alone.

The metadata format has an8192-byte control-record window, explicit versions,
checked field lengths/booleans and no trailing bytes. Store byte length remains
u64; the >4GiB metadata test is not an actual >4GiB file proof. Data bytes are
plain native records during the declared stream; this does not restore object,
length, serial or Save RPCs. The prior data framing remains retired.

Native open errors, acknowledgement loss and partial transfer are distinct from
publication. An acknowledged installation does not by itself prove a complete
large-root oracle, FUSE readiness, Exec confinement or crash/restart recovery.
All current execution is Disposable; Durable remains built and owner-deferred.
