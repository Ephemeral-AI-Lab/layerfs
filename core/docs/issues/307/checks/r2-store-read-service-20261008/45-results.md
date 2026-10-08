# R2 Store reader admission checkpoint

> Status: Verified Store component. Full R2 Goal remains ACTIVE.

The fixed Store reader set now has owned fair admission rather than blind
round-robin blocking mutex selection. ReadTicket is an event-driven Future;
ReadLease owns one idle session. Fixed per-Workspace FIFO lanes rotate grants,
with explicit bounded admission and original registration/release wakeups.
Cancellation and stop affect only unstarted admissions. Already leased consumers
retain their original session; no extra reader workers or session reopens exist.

Unknown outcomes quarantine the exact reader, retaining its opened provider and
same original Arc error. Healthy readers continue; all-readers-quarantined wakes
waiters with NoReaders. A new request has its own StorePorts failure scope;
short failure locks do not span provider I/O or admission waiting. Explicit
port-on-lease methods reject a different Store/Workspace before effect. Bind uses
the paired read-only History provider and succeeds while the original writer's
session returns real in-process Busy. Application/install startup selects the
fixed queue windows before ControlReady.

| Verification | Verdict and exact scope |
| --- | --- |
| Host final public tests | PASS30: read service9, installed Store3, Store Commit8, native control7, native install3 |
| Linux final public tests | PASS31: same30 plus actual daemon application1 |
| Actual application bound |192441958ns complete test command within9s; functional only, native_fuse_ready=false |
| Locked all-target Daemon build and Clippy | PASS macOS ARM64 and Linux ARM64 |
| Formatting, product boundary, tooling | PASS;766 product files and48 tooling tests |
| Runtime/profile | Existing pinned Linux image; explicit Disposable/WAL/OFF, separate Overlay MEMORY/OFF/EXCLUSIVE; root ARM build flags retained |
| Resource cleanup | Both acknowledged test containers removed by exact IDs19/33; only owned temporary Store fixtures used |
| Numerical/native filesystem acceptance | NOT_RUN; no new speed/storage/cold/RSS or mounted-R2 claim |
| Durable | NOT_RUN — disabled by owner until explicit reauthorization |

Source34 and binary31/42 identify the final selection; exact commands are in
28/29/31/32/41 and configuration43. Original failed build04/05 and Clippy16/18/24/28
are retained with causes/corrections20. All runtime selections passed; no test
reached an outer timeout. The quarantine test drives a real session into quarantine
through its public read-planner uncertain result; it is not a disk failure test.

This component supplies read ownership/admission and source APIs. Native per-demand
orchestration, consistent lookup/read decisions, backed native ownership, actual
Fuse service/handlers, Attach/Ready, security and complete normal drain remain
required by the active Goal. [Next source work](44-next-r2-work.md) records the
current seams and preserves the full completion boundary.

Production LOC: 171080 -> 171783 (delta +703). Core105663→106366;
active62789→63492; reference65417, excluded predecessors37431 and excluded
integration5443 unchanged. [Exact comparison](46-exact-production-loc.json)
uses the pinned production-only counter over first-parent and staged trees.
Growth implements read scheduling, original leases/failure custody and startup
composition; no relocation, reclassification or retirement occurred.
