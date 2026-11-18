//! `store.compact`: reclaim dead bytes from one Store.
//!
//! A sqlite Store on another machine is refused by the control function as
//! local-only (spec S6 deviation D5, `core/docs/issues/project-41/gc-42-decisions.md`);
//! this entry parses the name and hands the call over.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

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
    layerfs_control::store::compact(home, events, &store)
}
