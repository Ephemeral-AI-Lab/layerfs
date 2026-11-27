//! `machine.create`: create a VM machine.
use crate::calls::table::Kind::{Int, Name, Size, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Policy};
#[cfg(feature = "machine")]
use layerfs_protocol::provision::install::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`; the long form adds what was made.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &[
        "ok",
        "outcome",
        "machine",
        "platform",
        "backend",
        "cpus",
        "memory_bytes",
        "disk_bytes",
        "image",
        "os_image",
        "created_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "create",
    &[a("name", Name), a("platform", Name)],
    &[
        opt("cpus", Int),
        opt("memory", Size),
        opt("disk", Size),
        opt("image", Text),
        opt("os_image", Text),
        PROGRESS,
    ],
    "create a VM machine (lima, apple-container) and start it",
    run,
)
.policy(Policy::Platform("platform"))
.form(&FORM);

/// Runs `machine.create`: `{ phase, done, total }` events, phases
/// `bundle`, `boot` and `worker`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = layerfs_machine::MachineCreate {
        name: text("name").unwrap_or_default(),
        platform: text("platform").unwrap_or_default(),
        cpus: args.get_int("cpus"),
        memory: args.get_int("memory"),
        disk: args.get_int("disk"),
        image: text("image"),
        os_image: text("os_image"),
    };
    let (home, events) = context.parts();
    layerfs_machine::with_users::create(home, events, &request)
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("create");
