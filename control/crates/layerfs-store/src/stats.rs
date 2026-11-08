//! Store-wide counters on the Store's existing in-process or worker route.
use crate::{call, calls::StoreOp, record::StoreRecord, resolve::Target};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{events::Events, names::StoreName, outcome::Outcome};
use std::path::Path;

/// Read one Store's statistics; start no worker and refuse a Store being moved.
pub fn stats(home: &Path, events: &mut Events<'_>, name: &StoreName) -> Outcome {
    let record = StoreRecord::of(home, name)?;
    let target = Target {
        store: name.to_string(),
        // Store-wide methods do not resolve or require any project.
        project: String::new(),
        store_ref: record.store_ref()?.clone(),
        record,
    };
    let kind = target.store_ref.kind().as_str();
    let value = call::store_call(home, events, &target, &StoreOp::Statistics)?;
    let mut fields = vec![
        ("name".to_owned(), SdkValue::text(name.to_string())),
        ("kind".to_owned(), SdkValue::text(kind)),
    ];
    if let SdkValue::Map(values) = value {
        fields.extend(values.into_iter().map(|(key, value)| {
            let value = if key == "free" && kind == "postgresql" {
                SdkValue::Null
            } else {
                value
            };
            (key, value)
        }));
    }
    Ok(SdkValue::Map(fields))
}
