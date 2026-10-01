# R1e authenticated native purposes and dispatch freeze

Source parent: published control-target checkpointb98bc780d. SC-07/SC-08.
Root alone owns Bridge/native client/server/acceptor grammar and composition.
No native protected/global/physical capability is enabled by this freeze.

HELLO v1 remains exactu16(1), unprotected General. HELLO v2 is the existing R0
four-byte grammar u16(2), purposeu8 General1/Catalog2/Control3, reserved0.
It is the first encrypted frame after authenticated NoiseKK, id0/KindHello;
response is the exact accepted bytes. Unknown purpose/reserved/version refuses.
No raw selector or unauthenticated hint selects a privileged executor.

General carries the existing ordinary surface. Catalog permits only HistoryQuery
and the current Operation.metadata_mutation() set, with zero body. Control permits
existing finite FileSaveCapabilities, SandboxHello, WorkspaceStatus,
WorkspaceViewStatus and WorkspaceReleaseView, with zero body; unsupported future
cancel/certified-parent/v2/FileSet commands remain unavailable. Other purposes
refuse before handler/SQL/body effects. Existing grants/scopes/request counters and
absolute deadlines still apply. A selected purpose cannot be changed or replayed.

The explicitly selected v2 native topology has4 General persistent owners,1 Catalog,1 Control,
1 preauthentication/refusal owner; closing/terminal work remains in its original
class until worker join and descriptor close. The preauthentication owner becomes
its authenticated class without starting another ordinary worker. General cannot
consume either protected class. The7 accepted-session slots and each2MiB worker
stack fit14MiB plus2MiB acceptor/runtime allowance inside the existing16MiB stack
input. No ordinary worker/Save/read ceiling grows. Configured ordinary capacity
above4 refuses the selected v2 topology before listener effects;
it cannot silently borrow additional stack/FD owners. Accepted socket plus receiver,
sender and shutdown clones have bounded declared FD slots, all retained through
terminal/join. Kernel queues/buffers are observations, not byte admission.

Default Server.listen()/native configuration retain the original v1 topology,
including its4 bounded handshaking/active/closing workers. Explicit
Server.listen_with_purposes() or LAYERFS_NATIVE_PURPOSES=2 selects the reviewed
v2 topology before listener effects. An existing v1 listener cannot be adopted
as v2. Host callers use Server.connect_purpose; no key/selector bypass is exposed.
The acceptor's synchronous accepted/refusal descriptor is separately counted in
addition to7 worker slots; total accepted descriptors8, receiver/sender/shutdown
clones at most21, listener1 and transientrefusal1, before OS/diagnostic descriptors.
No helper worker is started by classification.

HELLO record is bounded to4 payload bytes before receiver allocation. General
retains the existing32KiB metadata/16KiB body frame limits. Catalog frames remain
32KiB, no body/data, with1MiB work/terminal ceiling still requiring actual aggregate
allocation proof. Control uses8KiB frame payload ceiling inside its64KiB pending
input, with no bulk batch. Existing Sender/Receiver backing capacities are charged
by their actual allocation/last-owner proof; this grammar alone is not that proof.

A fixed class ledger owns admission and a continuously retained per-session slot;
class counters are released only after joined worker/socket closure. Handshake or
terminal failure never opens a replacement route. Slow handshake/input/output keeps
its selected owner/deadline. A pre-existing authorized Catalog connection remains
independent of saturated General/hostile preauthentication. Protection against an
unbounded stream of hostile new handshakes is not claimed.

Independent vectors pin1/2 General/Catalog/Control bytes, malformed reserved/id/
version, wrong-purpose operations before effects, actual General saturation,
held sources/real scratch and catalog refill before either source resumes,
hostile handshake/output, exact terminal/reap counts and exhausted class refusal.
MacOS native main still refuses Apple32MiB readback0. Direct composed/library
native tests only prove explicit logical dispatch. Strict engine/whole-process/
physical resource qualification stays CAPABILITY-LIMITED until eligible provider,
allocator/FD/engine and kernel-barrier observations pass; capability bits stayfalse.
