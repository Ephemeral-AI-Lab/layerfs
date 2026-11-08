//! `machine.status` and the home id the machine specs use.
use super::record::MachineRecord;
use super::spec::{self as record, refusal};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_platform as adapters;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(super) fn home_id(home: &Path) -> Result<String> {
    Ok(layerfs_home::home::opened(home)?.id)
}

/// `machine.status(name)`: live state, free disk, the tool's version and
/// why it is down (O34). A stopped VM answers without being started. A
/// read: while an `op` holds the machine it probes nothing, answers the
/// state the record derives (`creating`, `removing`, `unresolved`) and
/// shows the `op` (#536 H6), which it never clears.
pub fn status(home: &Path, name: &MachineName, related: Related) -> Outcome {
    let record = MachineRecord::of(home, name)?;
    let kind = record.platform()?;
    let (state, free_disk, tool_version, down_reason) = match record.in_flight() {
        true => (record.state(None)?.to_owned(), None, None, None),
        false => {
            let spec = record::spec(home, &home_id(home)?, &record)?;
            let status = adapters::status(&spec).map_err(refusal)?;
            let state = status.state.as_str().to_owned();
            (
                state,
                status.free_disk,
                status.tool_version,
                status.down_reason,
            )
        }
    };
    let text = |value: Option<String>| value.map(SdkValue::Text).unwrap_or_default();
    let mut fields = vec![
        ("name", SdkValue::text(name.as_str())),
        ("platform", SdkValue::text(kind.name())),
        ("state", SdkValue::Text(state)),
        (
            "free_disk",
            free_disk
                .map(|b| SdkValue::Int(b as i64))
                .unwrap_or_default(),
        ),
        ("tool_version", text(tool_version)),
        ("down_reason", text(down_reason)),
        (
            "stores",
            SdkValue::List(
                (related.stores_on)(home, name)?
                    .into_iter()
                    .map(SdkValue::Text)
                    .collect(),
            ),
        ),
    ];
    if let Some(op) = record.op() {
        fields.push(("op", op.to_sdk()));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}
