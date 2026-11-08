//! `machine.create`: create a VM machine.
use crate::calls::table::Kind::{Bool, Int, Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

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
        opt("nested", Bool),
        opt("host", Name),
    ],
    "create an enabled VM machine (apple-container, lima)",
    run,
);

/// Runs `machine.create`.
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
        nested: match args.get("nested") {
            Some(SdkValue::Bool(nested)) => Some(*nested),
            _ => None,
        },
        host: text("host"),
    };
    let (home, events) = context.parts();
    layerfs_control::machine::create(home, events, &request)
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.create is not built yet (#533 step 6)",
    ))
}
