//! Names, identities, lookups and refusals shared by the Store calls.
//!
//! A project is named by its stack name and a Branch by its name; `main` is
//! the stack's line of Layers. Commits and Layers travel as the hex of their
//! identity. Every refusal has a stable code the JavaScript SDK maps to a
//! typed error.
use crate::store::{PortError, Store};
use layerfs_content::{ContentError, ObjectId};
use layerfs_history::{
    error::{HolderKind, HolderState, Missing, NameSource, NameTakenState, RemovedState},
    records::GcScopeKind,
    BranchRecord, CommitId, CommitRecord, HistoryError, HistoryName, LayerId, LayerRecord,
    LayerStackRecord, NodeId, UnknownOutcomeState,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

pub use layerfs_protocol::sdk::{commit_value, hex, layer_value, node_text, short, unhex};

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
        HistoryError::MountsActive(_) => "MountsActive",
        HistoryError::BusyOp(_) => "Busy",
        HistoryError::Unresolved(_) => "Unresolved",
        HistoryError::Capacity(_) => "Capacity",
        HistoryError::NameTaken(_) => "NameTaken",
        HistoryError::Removed(_) => "Removed",
        HistoryError::Conflict(_) => "Conflict",
        HistoryError::Exists(_) => "Exists",
        HistoryError::StaleStore => "StaleStore",
        HistoryError::Unsupported(_) => "Unsupported",
        HistoryError::UnknownOutcome | HistoryError::UnknownOutcomeOp(_) => "Unknown",
        _ => "Failed",
    };
    let detail = match error.cause() {
        HistoryError::NameTaken(state) => name_taken_detail(state),
        HistoryError::Removed(state) => removed_detail(state),
        HistoryError::MountsActive(state) => mounts_detail(state),
        HistoryError::BusyOp(state) | HistoryError::Unresolved(state) => op_detail(state),
        HistoryError::UnknownOutcomeOp(state) => unknown_detail(state),
        // A code with no spec'd detail carries an empty detail (spec gaps
        // G1), never a null a client would have to special-case.
        _ => SdkValue::map::<0>([]),
    };
    refuse(code, message, detail)
}

/// [`history`] with this call's entity, for a `UnknownOutcome` the call can
/// name: spec §10 A6 makes the `Unknown` detail `{entity: {kind, name},
/// phase, op}`, with `phase` and `op` null when no durable operation is
/// known. A definite refusal keeps its own wire detail.
pub fn history_in(
    error: HistoryError,
    scope_kind: GcScopeKind,
    entity: &str,
    phase: Option<&str>,
    op: Option<&[u8; 16]>,
) -> SdkRefusal {
    history(error.with_unknown(UnknownOutcomeState::new(
        scope_kind,
        entity,
        phase.map(str::to_owned),
        op.map(|op| hex(op)),
    )))
}

/// The `NameTaken` detail: the holder and where it is held.
pub fn name_taken_detail(state: &NameTakenState) -> SdkValue {
    SdkValue::map([
        (
            "holder",
            SdkValue::map([
                (
                    "kind",
                    SdkValue::text(match state.holder.kind {
                        HolderKind::Branch => "branch",
                        HolderKind::Project => "project",
                        // A newer history crate may add a holder kind; keep
                        // the old daemon readable instead of failing.
                        _ => "unknown",
                    }),
                ),
                ("name", SdkValue::text(&state.holder.name)),
                (
                    "state",
                    SdkValue::text(match state.holder.state {
                        HolderState::Live => "live",
                        HolderState::Retired => "retired",
                        HolderState::Deleted => "deleted",
                        // A newer history crate may add a holder state; keep
                        // the old daemon readable instead of failing.
                        _ => "unknown",
                    }),
                ),
            ]),
        ),
        (
            "source",
            SdkValue::text(match state.source {
                NameSource::Store => "store",
                NameSource::Home => "home",
                // A newer history crate may add a source; keep the old daemon
                // readable instead of failing.
                _ => "unknown",
            }),
        ),
    ])
}

fn mounts_detail(state: &layerfs_history::MountHolders) -> SdkValue {
    let holders = state
        .holders
        .iter()
        .map(|holder| {
            SdkValue::map([
                ("kind", SdkValue::text(holder.kind.as_str())),
                ("entity", SdkValue::text(&holder.entity)),
                ("workspace", SdkValue::opt_text(holder.workspace.as_ref())),
                ("sandbox", SdkValue::opt_text(holder.sandbox.as_ref())),
                ("machine", SdkValue::opt_text(holder.machine.as_ref())),
                ("op", SdkValue::text(&holder.op)),
            ])
        })
        .collect();
    SdkValue::map([("holders", SdkValue::List(holders))])
}

/// The §10 A6 `Unknown` detail: the entity the call names and, when a
/// durable operation is known, its phase and identity. Both are null when the
/// call recorded none.
fn unknown_detail(state: &layerfs_history::UnknownOutcomeState) -> SdkValue {
    SdkValue::map([
        (
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text(state.scope_kind.as_str())),
                ("name", SdkValue::text(&state.entity)),
            ]),
        ),
        ("phase", SdkValue::opt_text(state.phase.as_deref())),
        ("op", SdkValue::opt_text(state.op.as_deref())),
    ])
}

fn op_detail(state: &layerfs_history::OpConflict) -> SdkValue {
    SdkValue::map([
        (
            "entity",
            SdkValue::map([
                // The resource the operation holds, not the operation kind:
                // the entity names the Store, project or Branch.
                ("kind", SdkValue::text(state.scope_kind.as_str())),
                ("name", SdkValue::text(&state.entity)),
            ]),
        ),
        ("phase", SdkValue::opt_text(state.phase.as_ref())),
        ("op", SdkValue::text(&state.op)),
        ("uncertain", SdkValue::Bool(state.uncertain)),
        // The PostgreSQL holder, when the refusal names one (spec 10);
        // null on SQLite and when the holder could not be read.
        ("pid", state.pid.map_or(SdkValue::Null, SdkValue::Int)),
        (
            "client_addr",
            SdkValue::opt_text(state.client_addr.as_ref()),
        ),
        (
            "backend_start",
            SdkValue::opt_text(state.backend_start.as_ref()),
        ),
    ])
}

fn removed_detail(state: &RemovedState) -> SdkValue {
    SdkValue::map([(
        "paths",
        SdkValue::List(
            state
                .paths
                .iter()
                .map(|path| {
                    SdkValue::map([
                        ("path", SdkValue::text(&path.path)),
                        (
                            "frontier_serial",
                            SdkValue::Int(path.frontier_serial as i64),
                        ),
                    ])
                })
                .collect(),
        ),
    )])
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
/// Callers that read published history (every project listing) use this so a
/// `Busy`, `UnknownOutcome` or other history refusal keeps its own code
/// instead of being flattened to `Failed`.
pub fn published(error: Arc<PortError>) -> SdkRefusal {
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
    project_in(wanted, |name| {
        store
            .published(|h| h.layer_stack_named(name))
            .map_err(published)
    })
}

/// [`project`] through `find`, any reader of published history (a host's
/// PostgreSQL read session, #65), with the same refusals.
pub fn project_in(
    wanted: &str,
    find: impl FnOnce(&HistoryName) -> Result<Option<LayerStackRecord>, SdkRefusal>,
) -> Result<LayerStackRecord, SdkRefusal> {
    let found = match HistoryName::new(wanted) {
        Ok(name) => find(&name)?,
        Err(_) => None,
    };
    found.ok_or_else(|| no_project(wanted))
}

/// The refusal of project `wanted`, which does not exist.
pub fn no_project(wanted: &str) -> SdkRefusal {
    refuse(
        "NoSuchProject",
        format!("project {wanted} does not exist"),
        SdkValue::Null,
    )
}

/// The refusal of Branch `wanted`, which its project does not have.
pub fn no_branch(wanted: &str) -> SdkRefusal {
    refuse(
        "NoSuchBranch",
        format!("Branch {wanted} does not exist"),
        SdkValue::Null,
    )
}

/// One Branch by name, through the stack's name index.
pub fn branch(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    wanted: &str,
) -> Result<BranchRecord, SdkRefusal> {
    branch_in(wanted, |name| {
        store
            .published(|h| h.branch_named(stack.id, name))
            .map_err(published)
    })
}

/// [`branch`] through `find`, with the same refusals.
pub fn branch_in(
    wanted: &str,
    find: impl FnOnce(&HistoryName) -> Result<Option<BranchRecord>, SdkRefusal>,
) -> Result<BranchRecord, SdkRefusal> {
    let found = match HistoryName::new(wanted) {
        Ok(name) if wanted != super::history::MAIN => find(&name)?,
        _ => None,
    };
    found.ok_or_else(|| no_branch(wanted))
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
        .ok_or_else(|| absent(NodeId::Commit(id)))
}

/// One Layer.
pub fn layer(store: &Arc<Store>, id: LayerId) -> Result<LayerRecord, SdkRefusal> {
    store
        .published(|h| h.layer(id))
        .map_err(published)?
        .ok_or_else(|| absent(NodeId::Layer(id)))
}

/// The refusal of a Commit or Layer that has no row.
pub fn absent(node: NodeId) -> SdkRefusal {
    match node {
        NodeId::Commit(id) => refuse(
            "NoSuchCommit",
            format!("Commit {id} does not exist"),
            SdkValue::Null,
        ),
        NodeId::Layer(id) => refuse(
            "NotFound",
            format!("Layer {id} does not exist"),
            SdkValue::Null,
        ),
    }
}

/// The root a node records.
pub fn root(store: &Arc<Store>, node: NodeId) -> Result<ObjectId, SdkRefusal> {
    Ok(node_record(store, node)?.root())
}

/// A saved node's record, read once and kept for the call's later steps
/// (#818 R2): its project, its root and, for a Commit, its recorded Branch.
/// The history contract holds it since its context read returns it (R5).
pub use layerfs_history::NodeRecord;

/// One Commit or Layer, refused as [`commit`] and [`layer`] refuse.
pub fn node_record(store: &Arc<Store>, node: NodeId) -> Result<NodeRecord, SdkRefusal> {
    Ok(match node {
        NodeId::Commit(id) => NodeRecord::Commit(Box::new(commit(store, id)?)),
        NodeId::Layer(id) => NodeRecord::Layer(layer(store, id)?),
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
