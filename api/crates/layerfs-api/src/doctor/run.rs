//! Diagnostic binding with no ordinary call dispatch or home initialization.
use super::doctor_request;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use std::path::{Path, PathBuf};

/// Makes one explicit diagnostic, or the unchanged legacy diagnostic for empty options.
pub fn doctor_with_options(home: &Path, options: &SdkValue) -> Outcome {
    match doctor_request(options)? {
        None => layerfs_control::doctor::doctor(home),
        Some(request) => layerfs_control::doctor::doctor_target(home, &request),
    }
}

/// JSON binding shared by Node and Python, outside the call table.
pub fn doctor_options_json(home: Result<PathBuf, SdkRefusal>, options: &str) -> String {
    let answer = home.and_then(|home| {
        let options = layerfs_home::json::parse(options)
            .map_err(|_| refused("InvalidInput", "doctor options are not valid JSON"))?;
        doctor_with_options(&home, &options)
    });
    layerfs_home::json::print(&match answer {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(r) => SdkValue::map([(
            "refused",
            SdkValue::map([
                ("code", SdkValue::Text(r.code)),
                ("message", SdkValue::Text(r.message)),
                ("detail", r.detail),
            ]),
        )]),
    })
}
