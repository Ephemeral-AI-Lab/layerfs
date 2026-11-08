//! `store.stats`: one Store's publication and allocation counters.
use crate::calls::{
    table::{a, call, Call, Kind::Name},
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{names::StoreName, outcome::Outcome};

pub(crate) const CALL: Call = call(
    "store",
    "stats",
    &[a("name", Name)],
    &[],
    "Store-wide publication and allocation counters",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_store::stats::stats(home, events, &name)
}
