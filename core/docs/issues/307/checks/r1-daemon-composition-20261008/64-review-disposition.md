# R1b independent source review disposition

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Main owns edits/builds/tests/proofs. Native reviewer was read-only throughout.

The reviewer found actual startup-slot/custody defects while main was implementing.
Final source resolves them as follows; stale intermediate review messages are not
final-source proof. Source identity is [44](44-session-fence-source-identity.json).

| Finding | Final source disposition | Verification/scope |
| --- | --- | --- |
| Installer failure continued outer receive / possible repeated fence | `install` returns terminal disposition after original installer result; outer loop stops. Original failure is held in startup; publication-lock failure carries entire original result | Source review; traditional installer interrupted/existing/complete-root tests [61](61-native_install-tests.txt); actual original install path [49](49-final-native-application-proof.txt) |
| Synthesized Protocol error around delivered unknown | Actual `Served` held by RetainedControl; one separately retained fence attempt, no fabricated delivery failure | Source review; control-only unknown precedence [26](26-control-unknown-tests.txt); no fault injection/new product hook |
| Reserved installer slot monopolized by Hello or ordinary Capacity, including stale Hello bypass | Pending-slot guard precedes incarnation validation; any reserved Capacity refusal closes once. Ordinary stale incarnation remains Invalid | Final main source review; actual ordinary stale Hello exercised [49](49-final-native-application-proof.txt); saturation/adversarial native admission not yet tested |
| Silent authentication/initial discriminator occupies startup admission | Explicit blocking I/O waits remain through authentication/first owned metadata record, then clear. These are per-I/O waits, not a whole-conversation wall timer | Source; healthy actual binary path [49](49-final-native-application-proof.txt); malicious slow-drip/overload qualification not claimed |
| No-product disconnect/authentication failure exhausts slots | Exact bounded no-decoded-input Io/Channel diagnostic transfers once to stderr before capacity release. Failed transfer retains original+output error. Malformed/decoded/product custody stays retained | Source; fault-transfer/retained saturation native tests remain open, no automatic operation replay |
| Lost Overlay startup receipts / later failure drops successful owner | OwnerStart retained on failed create; AfterOverlay carries successful owner on later failure. Original request and result carriers survive before-effect/publication errors | Source; no synthetic failure injection or test-only API |
| Debug exposes raw malformed authority | Raw/decoded inputs retained separately; Debug emits length/kind, never input bytes. Private DaemonSetup key/cursor debugging is redacted | Source plus configuration redaction test [14](14-daemon-records-tests.txt) |
| SDK channel transfer resets ended session | Original SessionEnded is validated; sole local fence quarantines underlying owner. Failure retains original Answer at Fence, no repeat close | Actual binary/raw-owner refusal proof [49](49-final-native-application-proof.txt) |
| Application stop/fence/join absent | Explicitly incomplete R6 lifecycle scope; no graceful daemon/native drain claim. No daemon caller-process supervisor added | Architecture states gap; proof process termination is crash scope, distinct from acknowledged logical Workspace unmount |

No owner permission/gate was inferred from these findings. Preserve original
partial/known/unknown receipts. Source review is not native behavior qualification.
