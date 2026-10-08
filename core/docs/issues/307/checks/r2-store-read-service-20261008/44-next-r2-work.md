# R2 remains active after the Store reader checkpoint

The user explicitly requested a Goal to finish full R2. Its active objective is
full native Ready/read/permissions/ownership/normal drain, not these components.
No approval is pending; do not mark complete or pause at this checkpoint.

Next implementation should establish consistent native read decisions and indexed
mount/serial lookup aggregates in Workspace/Overlay, then activate the replacement
Fuse connection/request service and connect these actual ports. Existing source
seams: Overlay namespace/compound.rs SourceRows; database/connection.rs atomic
transaction; lifetime/lookup.rs, file_owners.rs and source.rs exact leases; runtime
schema.sql/accounting.sql; Workspace mutation/eval.rs base fact Need decisions and
base/view.rs with_client; Daemon typed commands and original completion futures.
Keep the lookup answer and native count acquisition in the same deciding atomic
owner job. Use checked native mount/namespace/serial counts, backed directory
handles/cookies and bounded live teardown; no resident filesystem ownership map.

Store read tickets now grant idle sessions fairly and pair read-only History.
Native ports must await these tickets and run original bounded demand work through
objects_on/file_lengths_on, never synchronous StorePorts waits. Per-demand grouping,
semantic read plans, native status/drain observers and provider custody still need
actual integration; granting a reader does not by itself qualify that integration.

The earlier fuser lifecycle extension remains authorized and verified. Its public
void-return reply methods intentionally expose only ReplyAttempt; selected S8
spec §5.2/§10 preserves unavailable exact send/delivery results. Do not manufacture
another dependency approval for that already documented limitation. No new fuser
source has changed in this checkpoint. Run its provenance guard before native
builds and retain its original signed-minimum timestamp limitation.

Replacement Fuse remains excluded and its predecessor must be preserved/accounted
before activation. One shared K=read_handles+2 worker pool, R16+N2 per connection,
full selected profile, SDK bind/Attach/Locate/Ready, deployment security, kernel
Busy usability and complete normal drain plus owning native proofs remain. All
other R2 authorities remain binding. R3/R4/R5 are separate unfinished work.
