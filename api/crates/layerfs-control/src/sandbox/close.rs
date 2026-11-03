//! `sandbox.close`, and the close of a Sandbox whose create never recorded an id.
use super::family::sandbox_of;
use super::record::SandboxRecord;
use crate::common::context::Context;
use crate::common::home;
use crate::common::outcome::{failed, refused, Outcome};
use crate::sandbox::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `sandbox.close(name, { force })`.
pub(super) fn close(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let record = sandbox_of(&home, name)?;
    if record.id().is_none() {
        if let Some(closed) = close_unstarted(&home, record.machine(), name)? {
            return Ok(closed);
        }
    }
    let record = sandbox_of(&home, name)?;
    let force = matches!(args.get("force"), Some(SdkValue::Bool(true)));
    let closed = on_machine(
        context,
        &record,
        "sandbox.close",
        vec![("force", SdkValue::Bool(force))],
    );
    match closed {
        Ok(value) => {
            let _lock = home::lock::take(&home)?;
            std::fs::remove_dir_all(record.dir(&home)).map_err(failed)?;
            Ok(home::with_field(value, "closed", SdkValue::text(name)))
        }
        // a Sandbox already gone loses its record too
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            let _lock = home::lock::take(&home)?;
            std::fs::remove_dir_all(record.dir(&home)).map_err(failed)?;
            Ok(SdkValue::map([
                ("closed", SdkValue::text(name)),
                ("was_gone", SdkValue::Bool(true)),
            ]))
        }
        Err(refusal) => Err(refusal),
    }
}

/// Closes Sandbox `name` whose record has no id: its create is in flight,
/// its outcome was unknown, or its process ended before recording one.
/// Refused `InUse` while the creating process lives; otherwise the record
/// goes and the name is free. `None` when the record gained an id
/// meanwhile, for the ordinary close.
fn close_unstarted(home: &Path, machine: &str, name: &str) -> Result<Option<SdkValue>> {
    let _lock = home::lock::take(home)?;
    let Some(record) = SandboxRecord::read(home, machine, name)? else {
        return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
    };
    if record.id().is_some() {
        return Ok(None);
    }
    let dir = record.dir(home);
    let state = record.state().to_owned();
    if let Some(pid) = record.creator() {
        return Err(refused(
            "InUse",
            format!("Sandbox {name} is being created by process {pid}"),
        ));
    }
    std::fs::remove_dir_all(&dir).map_err(failed)?;
    let mut fields = vec![
        ("closed", SdkValue::text(name)),
        ("was", SdkValue::text(&state)),
    ];
    if state == "unknown" {
        fields.push((
            "left",
            SdkValue::text("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here"),
        ));
    }
    Ok(Some(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    )))
}
