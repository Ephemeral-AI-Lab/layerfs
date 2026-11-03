//! `machine.get`.
use super::list::{live, summary};
use super::spec::machine_of;
use crate::common::context::Context;
use crate::common::outcome::Outcome;
use layerfs_bridge::control::SdkValue;

/// `machine.get(name)`: its record with its live state, Stores and Sandboxes.
pub fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(&context.home, name)?;
    let rows = [(name.to_owned(), record)];
    let state = live(&context.home, &rows)?.pop().flatten();
    summary(&context.home, name, &rows[0].1, state)
}
