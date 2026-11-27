//! `store.compact`: reclaim dead bytes from one Store.
//!
//! A sqlite Store on another machine is compacted there, by that machine's
//! worker (spec S6, `core/docs/issues/project-41/gc-42-decisions.md` D5);
//! this entry parses the name and hands the call over.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call, PROGRESS};
use crate::dispatch::{Context, Form};
use layerfs_home::names::StoreName;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// Its answer: the bytes freed, and in the long form the flattened report.
const FORM: Form = Form::of(
    &["freed_bytes"],
    &[],
    &[
        "freed_bytes",
        "outcome",
        "store",
        "generation_before",
        "generation_after",
        "dead_bytes",
        "discarded_bytes",
        "reusable_bytes",
        "kept_waste",
        "delta_base_references",
        "delta_base_references_count",
        "peak_temp_bytes",
        "residue_bytes",
        "residue_artifacts",
        "residue_reason",
        "estimate",
        "zero_reasons",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "compact",
    &[a("store", Name)],
    &[PROGRESS],
    "reclaim dead bytes from one Store",
    run,
)
.form(&FORM);

/// Runs `store.compact`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_store::admin::with_users::compact(home, events, &store, &layerfs_sandbox::users::STORE)
}
