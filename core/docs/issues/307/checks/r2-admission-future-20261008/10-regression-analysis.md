# Admission component failure analysis

The original oversized-input reproduction04/05 fails as selected: total ordinary
capacity cannot admit the original8MiB name input. Admission now compares its
charge with the same absolute capacity function used by try_submit before it
reserves a notification slot. No SQL attempt or input replacement occurs.

The next selection08/09 passes oversized, before-registration and racing cases,
but three saturation assertions fail before waiting. Source commands.rs maps
State to Lifecycle and Inode to Read. The fixture held State, leaving the one
ordinary slot free. Change the fixture to hold an Inode result; do not change
production class/reserve behavior. Later fixture setup uses event-driven admission
to handle the original publisher retaining its credit briefly after wakeup.
No command reached its100s limit. These failures remain in their original files.


Worker-loss reproduction16/17 exercises a public caller Waker that panics after
Capture publication. The source run loop had no exit fence. Its queue stayed
open after the worker unwound; a later original admitted read never completed,
failing the test's3s bounded wait (outer100s did not expire). An exit guard now
fences admission and returns queued commands unattempted; the published Capture
remains successful. The join preserves its exact panic payload.

Build19 fails because the initial Box<dyn Any + Send> payload would remove Sync
from OwnerError and consequently all shared completion/WorkspaceError custody.
Retaining that same Box inside a Mutex satisfies the existing public error
contract without casting, unsafe code, payload conversion or error-string loss.
The next source identity22 and build23 reflect this correction.
