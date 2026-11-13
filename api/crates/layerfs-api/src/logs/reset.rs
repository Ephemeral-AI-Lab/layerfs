//! `logs.reset`: clears this home's call log.
use crate::calls::table::{call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call("logs", "reset", &[], &[], "clear this home's call log", run);

/// Runs `logs.reset`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let removed = layerfs_home::logs::reset(&context.home)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Reset")),
        (
            "removed",
            SdkValue::Int(i64::try_from(removed).unwrap_or(i64::MAX)),
        ),
    ]))
}
