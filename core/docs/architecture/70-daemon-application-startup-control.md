# Direct-Store daemon application startup and control

> **Status:** Implemented source during R1b after `932abe8089`; verification
> receipts are retained under [R1b checks](../issues/307/checks/r1-daemon-composition-20261008/).
> R1 ordinary Sandbox/runtime/access composition remains open. Native FUSE Ready,
> mounted live Commit and complete application drain are not established here.

The real `layerfs-daemon --config <protected-file>` binary delegates to
`application/`. It loads one explicitly encoded Bridge DaemonSetup, initializes
one Overlay owner, and listens for authenticated native installation/control.
The process disables Linux dumpability and sets umask077 before reading keys or
starting workers. Non-Linux production startup returns Unsupported. File and
backing-directory checks use actual opened descriptors, O_NOFOLLOW/O_CLOEXEC,
effective uid and restrictive modes; deployment must keep their ancestors stable
and daemon-controlled. The config and backing files are outside the mount root.
The controller's key and daemon key do not appear in environment/arguments.

DaemonSetup carries explicit connection/read/cache/job windows, namespace
admission, command identity and backing/mount placement. Command uid differs from
the daemon and root; it describes Sandbox policy, not an implemented command
launcher. Linux process/mount/capability access must still be proved through the
ordinary Sandbox/runtime route. Existing locked nix0.31.3 supplies safe process
and descriptor controls; provider-independent `store/` receives no path/engine
transport type. No daemon Exec registration or supervisor is introduced.

Startup is InstallPending, Installing, Retained or ControlReady. Explicit
existing-store configuration opens Disposable once with retained authority; an
installation failure never selects it as fallback. Otherwise one authenticated
StoreManifest enters the existing sole installer after the exact original record
has been decoded. Payload records stay in that installer. It refuses a profile
other than explicit Disposable before claim or file/database effects. Existing
retained Durable implementation can compile; execution remains
`NOT_RUN — disabled by owner until explicit reauthorization`.

The original InstalledStore transfers into Service without reopening the file.
The installer acknowledgment precedes application publication, so a correlated
Hello with an explicit startup wait observes ControlReady before control use.
Hello observes actual daemon incarnation and actual Overlay/Store SQLite versions;
it does not claim a mounted location or FUSE Ready. A stale supplied incarnation
receives Invalid. A lost final installer acknowledgment can leave ControlReady
only from the original Reply phase, published file, validated manifest/version
and opened owner together; the original InstallFailure remains retained. An
Open-phase partial owner alone cannot establish readiness. Startup failure
retains observed Overlay work and any already initialized Overlay owner through
later failures; application publication failure retains the entire original
install result.

The application admits a fixed number of ordinary connection workers and one
additional startup slot. Before Store readiness the additional slot accepts the
installer; an ordinary control gets a before-effect Capacity refusal and ends
that conversation. Authentication and first-record blocking I/O use explicit
configured waits, cleared before operations; these are I/O wait bounds, not a
whole-handshake wall bound or command runtime timeout. Subsequent controls and
installation payloads have no implicit runtime timer. Completed workers are
joined before their slot is reused. Fixed slots retain malformed request bytes,
unresolved product results, failed replies/fences and failed diagnostic transfer.
Their Debug representation redacts raw input and authority. Original
no-decoded-product Io/Channel termination diagnostics transfer once to the daemon
stderr owner before capacity is released; a failed transfer retains both errors.
An explicit operator custody-transfer method changes no filesystem state and
performs no replay/artifact removal; the current CLI does not expose that method.

EndSession/SessionEnded terminates a healthy authenticated control conversation.
SDK Control validates its original correlation and fences the underlying channel
once, so transferring that channel cannot reset the ended session. A failed local
fence retains the original SessionEnded answer at ControlPhase::Fence and does
not attempt another close. Session end is independent of terminal Workspace unmount;
Workspace ownership remains in Service. Unresolved control results retain the
actual Served value, without fabricating a transport/protocol failure. Delegated
installer failure ends its conversation without another receive, reply or fence.

The control-only Service path reuses ordinary bind/status/fork/history/terminal
logical unmount. Commit without a namespace producer validates token and original
custody read-only, then refuses before lifecycle admission, capture, Save or epoch
change. Original Unknown and known publication take precedence. Real construction
still enters the existing Commit driver; no placeholder producer or second driver
has been added. Native attachment and captured namespace normalization are R2/R4/R5.

The current executable has no complete application stop/fence/join operation.
It does not claim a graceful daemon drain when the process/container is stopped.
Unexpected fatal accept/synchronization failure may leave worker owners live
until process termination. Complete lifecycle/force/drain custody remains R6;
retained malformed/product faults may exhaust the explicitly finite connection
capacity until an operator transfers custody or ends that daemon lifecycle.
Functional binary tests explicitly stop their owned process after proving logical
Workspace close and label that stop as process termination, not native drain.

R1d now qualifies actual macOS controller startup/installation against the Linux
application and corrects EndSession socket ordering: final reply send completes,
then the daemon waits once for the caller's EOF and fences once before releasing
its connection slot. Original Served and terminal byte/work/phase/fence-attempt
receipts survive failure; a failed connection retains receipts while its local
socket owner drops. This establishes no application force-close/join or native
filesystem drain. [Owned Sandbox lifecycle](72-owned-sandbox-lifecycle.md) records
the current source, actual failed/final proof identities and precise scope.
