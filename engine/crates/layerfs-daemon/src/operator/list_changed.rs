//! `backend.daemon.workspace.list_changed`: what a Commit would capture now.
use super::list_changes::selected;
use crate::workspace::Service;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::{
    history::lookup::{self as args, Answer},
    operator::args::text,
};

/// `backend.daemon.workspace.list_changed { path, under? }`: what a Commit
/// would capture now, from one bounded read of the overlay
/// (`Service::changed`): `{ entries: [{ path, change, type }], partial,
/// activity, activityPhase, activitySinceMs }`, `type` `dir` or `file`.
/// The activity is observed just before the read: its word (as the Status
/// record's `activity` prints), the phase its operation is in (null when
/// none runs) and when it began. It rides here, not in the Status record,
/// so that record keeps the bytes other builds read.
pub(crate) fn changed(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    let activity = service.operator_activity(text(args, "path")?);
    let under = args.get_text("under");
    let found = service
        .changed(&workspace, under)
        .map_err(|e| args::failed(format!("{e:?}")))?;
    let entries = (found.entries.into_iter())
        .map(|(path, change, folder)| {
            SdkValue::map([
                ("path", SdkValue::Text(path)),
                ("change", SdkValue::text(change)),
                ("type", SdkValue::text(if folder { "dir" } else { "file" })),
            ])
        })
        .collect();
    let (word, phase, since) = match activity {
        Some((activity, phase, since)) => (
            SdkValue::text(format!("{activity:?}")),
            SdkValue::opt_text(Some(phase).filter(|phase| !phase.is_empty())),
            SdkValue::Int(since),
        ),
        None => (SdkValue::Null, SdkValue::Null, SdkValue::Null),
    };
    Ok(SdkValue::map([
        ("entries", SdkValue::List(entries)),
        ("partial", SdkValue::Bool(found.partial)),
        ("activity", word),
        ("activityPhase", phase),
        ("activitySinceMs", since),
    ]))
}
