//! Read-only compatibility checks for development homes made before host naming.
use super::{layout::foreign_fix, paths, read_record, records::unreadable, with_field};
use crate::outcome::{refused, with_fix};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use std::path::Path;

pub(super) fn check(home: &Path) -> Result<(), SdkRefusal> {
    let mut records = Vec::new();
    for kind in ["machine", "sandbox", "store"] {
        let directory = home.join(kind);
        for name in paths::names(&directory).map_err(|error| unreadable(&directory, &error))? {
            let relative = Path::new(kind).join(&name);
            if kind == "machine" && name == "native" {
                records.push(relative.display().to_string());
                if directory
                    .join(&name)
                    .join("run")
                    .try_exists()
                    .map_err(|error| unreadable(&directory, &error))?
                {
                    records.push(relative.join("run").display().to_string());
                }
            }
            let record = relative.join("record.json");
            let Some(value) = read_record(&home.join(&record))? else {
                continue;
            };
            let native = |key| value.get_text(key) == Some("native");
            let legacy = match kind {
                "machine" => {
                    native("platform")
                        || matches!(name.as_str(), "macos-host" | "linux-host")
                            && value.get_text("platform") != Some("host")
                }
                "sandbox" => native("machine") || native("runtime") || native("platform"),
                _ => {
                    native("machine")
                        || matches!(value.get("attached"), Some(SdkValue::List(names)) if names.iter().any(|name| matches!(name, SdkValue::Text(name) if name == "native")))
                }
            };
            if legacy {
                records.push(record.display().to_string());
            }
        }
    }
    records.sort();
    if records.is_empty() {
        return Ok(());
    }
    let mut refusal = with_fix(refused("Unsupported", format!(
        "This development home uses the previous host naming in {}. Use the build that created the home to stop its workers and Sandboxes and export any data you need before parking it and initializing a new home.",
        records.join(", ")
    )), foreign_fix(home));
    refusal.detail = with_field(
        refusal.detail,
        "legacy_host_records",
        SdkValue::List(records.into_iter().map(SdkValue::Text).collect()),
    );
    Err(refusal)
}
