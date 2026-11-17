//! `store.compact`: reclaim dead bytes from one Store.
//!
//! A sqlite Store on another machine is refused by the control function as
//! local-only (spec S6 deviation D5, `core/docs/issues/project-41/gc-42-decisions.md`);
//! this entry parses the name and hands the call over.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "compact",
    &[a("name", Name)],
    &[PROGRESS],
    "reclaim dead bytes from one Store",
    run,
);

/// Runs `store.compact`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::store::compact(home, events, &name)
}
