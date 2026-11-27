//! `backend.daemon.workspace.get_storage`: the overlay's own row counts.
use super::list_changes::selected;
use crate::workspace::Service;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::lookup::{self as args, Answer};

/// The overlay's own row counts, read from its database alone (#508). Path
/// change counts are `null`: naming them walks the base tree, which costs
/// minutes on a large tree, and state reads call this for every Workspace.
/// `list_changes` pages the changed paths on request.
pub(crate) fn overlay(service: &Service, args: &SdkValue) -> Answer {
    let workspace = selected(service, args)?;
    let op = workspace.operation().map_err(args::failed)?;
    let resources = op.overlay().resources().map_err(args::failed)?;
    let c = resources.counts;
    Ok(SdkValue::map([
        ("inodes", SdkValue::Int(c.inode_rows as i64)),
        ("entries", SdkValue::Int(c.directory_entry_rows as i64)),
        ("payload", SdkValue::Int(c.payload_bytes as i64)),
        ("metadata", SdkValue::Null),
        (
            "metadataRows",
            SdkValue::Int((c.inode_rows + c.directory_entry_rows + c.operation_record_rows) as i64),
        ),
        ("changes", SdkValue::Null),
    ]))
}
