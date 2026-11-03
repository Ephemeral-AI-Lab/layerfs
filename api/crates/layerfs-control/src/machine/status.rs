//! `machine.status` and the home id the machine specs use.
use super::spec::{self as record, machine_of, refusal};
use crate::common::context::Context;
use crate::common::home::{self};
use crate::common::outcome::Outcome;
use crate::inventory::stores_on;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine as adapters;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(super) fn home_id(home: &Path) -> Result<String> {
    Ok(home::open(home)?.id)
}

/// `machine.status(name)`: live state, free disk, the tool's version and
/// why it is down (O34). A stopped VM answers without being started.
pub fn status(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = &context.home;
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(home, name)?;
    let spec = record::spec(home, &home_id(home)?, &record)?;
    let status = adapters::status(&spec).map_err(refusal)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("platform", SdkValue::text(spec.kind.name())),
        ("state", SdkValue::text(status.state.as_str())),
        (
            "free_disk",
            status
                .free_disk
                .map(|b| SdkValue::Int(b as i64))
                .unwrap_or_default(),
        ),
        (
            "tool_version",
            status.tool_version.map(SdkValue::Text).unwrap_or_default(),
        ),
        (
            "down_reason",
            status.down_reason.map(SdkValue::Text).unwrap_or_default(),
        ),
        (
            "stores",
            SdkValue::List(
                stores_on(home, name)?
                    .into_iter()
                    .map(SdkValue::Text)
                    .collect(),
            ),
        ),
    ]))
}
