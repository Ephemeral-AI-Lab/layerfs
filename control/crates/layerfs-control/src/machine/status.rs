//! `machine.status` and the home id the machine specs use.
use super::record::MachineRecord;
use super::spec::{self as record, refusal};
use crate::common::context::Context;
use crate::inventory::stores_on;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::Outcome;
use layerfs_platform as adapters;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(super) fn home_id(home: &Path) -> Result<String> {
    Ok(crate::migrate::open(home)?.id)
}

/// `machine.status(name)`: live state, free disk, the tool's version and
/// why it is down (O34). A stopped VM answers without being started. A
/// read: while an `op` holds the machine it probes nothing, answers the
/// state the record derives (`creating`, `removing`, `unresolved`) and
/// shows the `op` (#536 H6), which it never clears.
pub fn status(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = &context.home;
    let name = args.get_text("name").unwrap_or("");
    let record = MachineRecord::of(home, name)?;
    let kind = record.platform()?;
    let disabled = layerfs_isolation::platform_policy::disabled_reason(kind.name());
    let (state, free_disk, tool_version, down_reason) = match (disabled, record.in_flight()) {
        (Some(reason), _) => (
            record.state(None)?.to_owned(),
            None,
            None,
            Some(reason.to_owned()),
        ),
        (None, true) => (record.state(None)?.to_owned(), None, None, None),
        (None, false) => {
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
        ("name", SdkValue::text(name)),
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
                stores_on(home, name)?
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
