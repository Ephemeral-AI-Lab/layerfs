//! Ordered NDJSON output. A failed sink never repeats the operation or its effects.
use crate::run::REFUSED;
use layerfs_api::json;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::Write;
use std::process::ExitCode;

pub fn event(value: &SdkValue) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", json::print(value))?;
    out.flush()
}

pub fn finish(outcome: Result<SdkValue, SdkRefusal>, failed: bool) -> ExitCode {
    if failed {
        return ExitCode::from(REFUSED);
    }
    let (key, value, code) = match outcome {
        Ok(value) => ("result", value, ExitCode::SUCCESS),
        Err(refusal) => (
            "refusal",
            SdkValue::map([
                ("code", SdkValue::text(refusal.code)),
                ("message", SdkValue::text(refusal.message)),
                ("detail", refusal.detail),
            ]),
            ExitCode::from(REFUSED),
        ),
    };
    match event(&SdkValue::map([(key, value)])) {
        Ok(()) => code,
        Err(_) => ExitCode::from(REFUSED),
    }
}
