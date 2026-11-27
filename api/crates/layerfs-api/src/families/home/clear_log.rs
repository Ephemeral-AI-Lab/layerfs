//! `home.clear_log`: clears this home's call log.
use crate::dispatch::table::{call, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// Its answer: nothing to say in the short form.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "line_count"]);

/// Its table entry: it writes no log line into the log it clears.
pub(crate) const CALL: Call = call(
    "home",
    "clear_log",
    &[],
    &[],
    "clear this home's call log",
    run,
)
.quiet()
.form(&FORM);

/// Runs `home.clear_log`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let lines = layerfs_home::logs::reset(&context.home)?;
    Ok(SdkValue::map([
        ("outcome", SdkValue::text("cleared")),
        (
            "line_count",
            SdkValue::Int(i64::try_from(lines).unwrap_or(i64::MAX)),
        ),
    ]))
}
