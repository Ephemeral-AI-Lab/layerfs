# Sandbox doctor

> Status: implemented observational target; qualification is recorded separately.

```text
 doctor sandbox <name>
   record + references + operation owner (read only)
   |
   +-- native ----------------------> saved control endpoint
   +-- saved forward -> carrier ----> saved control endpoint
   +-- worker route -> doctor.sandbox -> worker's saved control endpoint
                                        |
                    authenticate -> observe instance + ControlReady
                                        |
                      registry-only bounded workspace pages
```

The target follows the saved route without normal `sandbox.get`, deployment,
Store open, leases, record refresh, forward creation or runtime lifecycle
changes. Native/worker-side daemon or init processes are observed by PID, start
time and boot. Forwarded routes require the recorded worker's presence and
validate their carrier process identity where one is recorded; they never
require a worker RPC merely to reach the daemon.

The authenticated readiness observation does not wait for startup. A different
instance is stale; non-ControlReady phases retain their own readiness result.
A socket closing during authentication cannot reliably distinguish saturated
admission from transport loss, so that observation is unknown. Typed registry
`Busy` is reported explicitly, without reconnect or retry. The original read
request is never replayed after a lost answer.

`sandbox.doctor_workspaces` is an internal daemon control method, not a normal
SDK call-table entry. It uses nonblocking registry admission and returns at
most 32 fixed-size fact rows per page, without history, Store or overlay reads.
Membership/binding-epoch generation and ordered identities detect changed,
repeated or incomplete pagination. Reports retain counts/activity/mount facts
and bounded examples of retained failed or uncertain activity, not an unbounded
copy of the workspace inventory. Dirty counts and pending merges remain outside
this registry-only observation; they are not treated as corruption.

An absolute socket fence bounds the original authenticated daemon observation,
including trickling records, and is cancelled/joined on return. No supplied
overall timeout uses a 10-second daemon/worker connection observation window.
Synchronous local record/kernel observations check the budget between probes.

Control readiness and registry enumeration do not qualify application FUSE/NFS
reads, namespace containment, mutable-overlay integrity or Store integrity.
Forwarded daemon identity establishes daemon presence, while guest runtime
containment is explicitly not separately inspected through a mandatory worker
RPC. Existing daemon builds without the diagnostic method report incomplete
workspace coverage.
