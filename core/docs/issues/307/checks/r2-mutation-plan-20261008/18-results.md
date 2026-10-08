# R2 resumable Workspace mutation component

> Status: Verified component; full R2–R5 remains unfinished.

Workspace now exposes the existing mutation decision rounds through an owned
MutationPlan. Preparation performs no SQL, immutable demand or serial allocation;
creating requests receive one separately reserved serial. Owner/Base/Finished
stages let a native executor schedule the original SQL job and immutable demand
independently. Only a nonempty Needs result permits continuation. A publication,
unchanged result, refusal or original error is terminal. Failed provider/source
work retains the original operation, needs and accumulated facts in the plan.
The synchronous mutate API drives that same plan; namespace decision algorithms
and publication semantics are shared, not copied into a native engine.

Host and Linux each pass30 public tests: mutation plan5, namespace5, actual
daemon-owner namespace8, namespace profile1, namespace view5, payload3 and nonfile3.
The new cases prove zero provider work during preparation/owner rounds, a current
namespace decision after a competing mutation during a park, one publication,
terminal original error identity, preserved original input on invalid serials,
exact-source refusal before demand and retained input/needs after missing base.
Each test invocation has an explicit100s wall limit; no timeout or failure occurred
in this checkpoint. Locked all-target Workspace/Daemon builds and warning-denying
Clippy pass on both platforms; workspace fmt,763-file boundary and48 tooling tests
pass. Source/config hashes02, commands04/06/07/10/16 and binary identities09/17
preserve the scope. ARM flags remain from the root Cargo config.

Only in-memory Content fixtures and the real separate MEMORY/OFF/EXCLUSIVE
Overlay execute here. Global Store/Durable execution is NOT_RUN; Durable remains
disabled by owner until explicit reauthorization. This is functional component
verification, not native, cold-cache, speed/storage or residency qualification.
The one acknowledged Linux container is removed by exact ID11.

The caller still owns its exact BaseSource, pending job/completion, native reply,
provider capacity, serial reservation and error custody. Native bounded ingress,
fair healthy Store admission and Fuse dispatch are not supplied by MutationPlan.
Existing ancestry/fact-cache behavior is unchanged; arbitrary-depth topology and
R4 Content validation remain separate work. The component closes no full native
R2/R3/R5 proof row and is not permission to stop the dispatched R2–R5 assignment.

Production LOC: 170986 -> 171080 (delta +94). Core105569→105663;
active62695→62789; reference65417, excluded predecessors37431 and excluded
integration5443 unchanged. [Exact parent/staged count](19-exact-production-loc.json)
uses the pinned production-only counter. No relocation or retirement.
