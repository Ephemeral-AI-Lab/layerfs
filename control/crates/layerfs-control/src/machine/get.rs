//! `machine.get`.
use super::list::{live, summary};
use super::record::MachineRecord;
use crate::common::context::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// `machine.get(name)`: its record with its live state, Stores and
/// Sandboxes. A read: answered while an `op` holds the machine too.
pub fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    let rows = [MachineRecord::of(&context.home, name)?];
    let state = live(&context.home, &rows)?.pop().flatten();
    summary(&context.home, &rows[0], state)
}
