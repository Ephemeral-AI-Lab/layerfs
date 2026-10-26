//! Names, identities, lookups and refusals shared by the Store calls.
//!
//! A project is named by its stack name and a Branch by its name; `main` is
//! the stack's line of Layers. Commits and Layers travel as the hex of their
//! identity. Every refusal has a stable code the JavaScript SDK maps to a
//! typed error.
use crate::store::{PortError, Store};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{ContentError, ObjectId};
use layerfs_history::{
    error::Missing, BranchRecord, CommitId, CommitRecord, HistoryError, HistoryName, LayerId,
    LayerRecord, LayerStackRecord, NodeId,
};
use std::sync::Arc;

pub use layerfs_bridge::sdk_values::{commit_value, hex, layer_value, node_text, short, unhex};

/// The result of one Store call.
pub type Answer = Result<SdkValue, SdkRefusal>;

/// An argument as the caller read it, refused or not. The engine raises the
/// refusal where it uses the value, after the steps that came before it, so
/// a call with several faults is refused by the first one, as always.
pub type Arg<T> = Result<T, SdkRefusal>;

/// A refusal.
pub fn refuse(code: &str, message: impl Into<String>, detail: SdkValue) -> SdkRefusal {
    SdkRefusal {
        code: code.to_owned(),
        message: message.into(),
        detail,
    }
}

/// An invalid argument.
pub fn invalid(message: impl Into<String>) -> SdkRefusal {
    refuse("InvalidInput", message, SdkValue::Null)
}

/// A history failure as a refusal.
pub fn history(error: HistoryError) -> SdkRefusal {
    let message = error.to_string();
    let code = match error.cause() {
        HistoryError::HeadMoved(_) => "HeadMoved",
        HistoryError::StackMoved { .. } => "HeadMoved",
        HistoryError::Missing(Missing::Branch) => "NoSuchBranch",
        HistoryError::Missing(Missing::Commit) => "NoSuchCommit",
        HistoryError::Missing(_) => "NotFound",
        HistoryError::InvalidInput(_) => "InvalidInput",
        HistoryError::NotInHistory(_) => "NotInHistory",
        HistoryError::Busy => "Busy",
        HistoryError::Capacity(_) => "Capacity",
        HistoryError::UnknownOutcome => "Unknown",
        _ => "Failed",
    };
    refuse(code, message, SdkValue::Null)
}

/// A content failure as a refusal.
pub fn content(error: ContentError) -> SdkRefusal {
    let code = match error {
        ContentError::BoundedCapacityExceeded { .. } => "Capacity",
        ContentError::ProviderFailure { .. } | ContentError::OutputRejected => "Unknown",
        _ => "Failed",
    };
    refuse(code, error.to_string(), SdkValue::Null)
}

/// A failed published-history lookup: its history error, else the reader's.
fn published(error: Arc<PortError>) -> SdkRefusal {
    match error.as_ref() {
        PortError::History(error) => history(error.clone()),
        other => refuse("Unknown", other.to_string(), SdkValue::Null),
    }
}

/// Any other failure as a refusal with its own description.
pub fn failed(error: impl std::fmt::Display) -> SdkRefusal {
    refuse("Failed", error.to_string(), SdkValue::Null)
}

/// A Commit or Layer named by its identity text.
pub fn node(text: &str) -> Result<NodeId, SdkRefusal> {
    unhex(text)
        .and_then(|bytes| NodeId::from_slice(&bytes).ok())
        .ok_or_else(|| invalid(format!("`{text}` is not a Commit or Layer identity")))
}

/// A name the history accepts.
pub fn name(text: &str) -> Result<HistoryName, SdkRefusal> {
    HistoryName::new(text).map_err(|_| invalid(format!("`{text}` is not a valid name")))
}

/// The project's stack, by name.
pub fn project(store: &Arc<Store>, wanted: &str) -> Result<LayerStackRecord, SdkRefusal> {
    let found = match HistoryName::new(wanted) {
        Ok(name) => store
            .published(|h| h.layer_stack_named(&name))
            .map_err(published)?,
        Err(_) => None,
    };
    found.ok_or_else(|| {
        refuse(
            "NoSuchProject",
            format!("project {wanted} does not exist"),
            SdkValue::Null,
        )
    })
}

/// One Branch by name, through the stack's name index.
pub fn branch(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    wanted: &str,
) -> Result<BranchRecord, SdkRefusal> {
    let found = match HistoryName::new(wanted) {
        Ok(name) if wanted != super::MAIN => store
            .published(|h| h.branch_named(stack.id, &name))
            .map_err(published)?,
        _ => None,
    };
    found.ok_or_else(|| {
        refuse(
            "NoSuchBranch",
            format!("Branch {wanted} does not exist"),
            SdkValue::Null,
        )
    })
}

/// A Branch's newest node: its head Commit, else its base Layer.
pub fn branch_node(branch: &BranchRecord) -> NodeId {
    branch
        .head_commit
        .map_or(NodeId::Layer(branch.base_layer), NodeId::Commit)
}

/// One Commit.
pub fn commit(store: &Arc<Store>, id: CommitId) -> Result<CommitRecord, SdkRefusal> {
    store
        .published(|h| h.commit(id))
        .map_err(published)?
        .ok_or_else(|| {
            refuse(
                "NoSuchCommit",
                format!("Commit {id} does not exist"),
                SdkValue::Null,
            )
        })
}

/// One Layer.
pub fn layer(store: &Arc<Store>, id: LayerId) -> Result<LayerRecord, SdkRefusal> {
    store
        .published(|h| h.layer(id))
        .map_err(published)?
        .ok_or_else(|| {
            refuse(
                "NotFound",
                format!("Layer {id} does not exist"),
                SdkValue::Null,
            )
        })
}

/// The root a node records.
pub fn root(store: &Arc<Store>, node: NodeId) -> Result<ObjectId, SdkRefusal> {
    Ok(match node {
        NodeId::Commit(id) => commit(store, id)?.root,
        NodeId::Layer(id) => layer(store, id)?.root,
    })
}

/// The next page position, or null after the last page.
pub fn next_page(pos: u64, count: usize, more: bool) -> SdkValue {
    if more {
        SdkValue::Int((pos + count as u64) as i64)
    } else {
        SdkValue::Null
    }
}
