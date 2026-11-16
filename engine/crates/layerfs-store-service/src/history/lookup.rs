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
    error::{HolderKind, HolderState, Missing, NameSource, NameTakenState, RemovedState},
    BranchRecord, CommitId, CommitRecord, HistoryError, HistoryName, LayerId, LayerRecord,
    LayerStackRecord, NodeId,
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
        HistoryError::UnknownOutcome => "Unknown",
        _ => "Failed",
    };
    let detail = match error.cause() {
        HistoryError::NameTaken(state) => name_taken_detail(state),
        HistoryError::Removed(state) => removed_detail(state),
        HistoryError::MountsActive(state) => mounts_detail(state),
        HistoryError::BusyOp(state) | HistoryError::Unresolved(state) => op_detail(state),
        // A code with no spec'd detail carries an empty detail (spec gaps
        // G1), never a null a client would have to special-case.
        _ => SdkValue::map::<0>([]),
    };
    refuse(code, message, detail)
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
                (
                    "workspace",
                    holder
                        .workspace
                        .as_ref()
                        .map_or(SdkValue::Null, SdkValue::text),
                ),
                (
                    "sandbox",
                    holder
                        .sandbox
                        .as_ref()
                        .map_or(SdkValue::Null, SdkValue::text),
                ),
                (
                    "machine",
                    holder
                        .machine
                        .as_ref()
                        .map_or(SdkValue::Null, SdkValue::text),
                ),
                ("op", SdkValue::text(&holder.op)),
            ])
        })
        .collect();
    SdkValue::map([("holders", SdkValue::List(holders))])
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
        (
            "phase",
            state.phase.as_ref().map_or(SdkValue::Null, SdkValue::text),
        ),
        ("op", SdkValue::text(&state.op)),
        ("uncertain", SdkValue::Bool(state.uncertain)),
        // The PostgreSQL holder, when the refusal names one (spec 10);
        // null on SQLite and when the holder could not be read.
        ("pid", state.pid.map_or(SdkValue::Null, SdkValue::Int)),
        (
            "client_addr",
            state
                .client_addr
                .as_ref()
                .map_or(SdkValue::Null, SdkValue::text),
        ),
        (
            "backend_start",
            state
                .backend_start
                .as_ref()
                .map_or(SdkValue::Null, SdkValue::text),
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
pub(super) fn published(error: Arc<PortError>) -> SdkRefusal {
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
        Ok(name) if wanted != super::MAIN => find(&name)?,
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


#[cfg(test)]
mod tests {
    use super::history;
    use layerfs_bridge::control::SdkValue;
    use layerfs_history::error::{
        HolderKind, HolderState, Missing, MountHolders, MovedState, NameHolder, NameSource,
        NameTakenState, OpConflict, RemovedPath, RemovedState,
    };
    use layerfs_history::records::{GcIntentKind, GcScopeKind};
    use layerfs_history::{CommitId, HistoryError, LayerId};

    fn layer(tag: u8) -> LayerId {
        let mut bytes = [0x32u8; 33];
        bytes[32] = tag;
        LayerId::from_slice(&bytes).expect("a tagged Layer identity")
    }

    fn commit(tag: u8) -> CommitId {
        let mut bytes = [0x12u8; 33];
        bytes[32] = tag;
        CommitId::from_slice(&bytes).expect("a tagged Commit identity")
    }

    fn empty(detail: &SdkValue) -> bool {
        matches!(detail, SdkValue::Map(fields) if fields.is_empty())
    }

    /// Spec 10: every HistoryError has one explicit wire code, built before
    /// the service's `_ => "Failed"` wildcard for the codes that once fell
    /// through it (StaleStore, Removed, Unsupported).
    #[test]
    fn every_history_error_maps_to_its_spec_10_wire_code() {
        let cases = [
            (HistoryError::InvalidInput("bad"), "InvalidInput"),
            (HistoryError::Missing(Missing::Branch), "NoSuchBranch"),
            (HistoryError::Missing(Missing::Commit), "NoSuchCommit"),
            (HistoryError::Missing(Missing::LayerStack), "NotFound"),
            (HistoryError::Missing(Missing::Layer), "NotFound"),
            (HistoryError::Unsupported("no"), "Unsupported"),
            (HistoryError::Exists("there"), "Exists"),
            (HistoryError::Conflict("moved"), "Conflict"),
            (HistoryError::StaleStore, "StaleStore"),
            (HistoryError::Busy, "Busy"),
            (HistoryError::Capacity("full"), "Capacity"),
            (HistoryError::Integrity("broken"), "Failed"),
            (HistoryError::NotInHistory("other stack"), "NotInHistory"),
            (HistoryError::OwnershipUnavailable, "Failed"),
            (HistoryError::ContinuityUnavailable, "Failed"),
            (HistoryError::UnknownOutcome, "Unknown"),
        ];
        for (error, code) in cases {
            let refused = history(error);
            assert_eq!(refused.code, code, "{refused:?}");
            assert!(empty(&refused.detail), "{} carries no detail", refused.code);
        }
    }

    /// The moving-head families map to the one HeadMoved code, whatever their
    /// shape; their detail stays empty because the code names the outcome.
    #[test]
    fn the_moved_head_family_maps_to_head_moved() {
        let moved = HistoryError::HeadMoved(Box::new(MovedState {
            expected_head: Some(commit(0x12)),
            actual_head: None,
            expected_base: layer(0x32),
            actual_base: layer(0x33),
        }));
        let refused = history(moved);
        assert_eq!(refused.code, "HeadMoved");
        assert!(empty(&refused.detail));

        let stack = HistoryError::StackMoved {
            expected: layer(0x32),
            actual: layer(0x33),
        };
        assert_eq!(history(stack).code, "HeadMoved");

        let base = HistoryError::BaseMismatch {
            commit_base: layer(0x32),
            branch_base: layer(0x33),
        };
        assert_eq!(history(base).code, "Failed");
    }

    #[test]
    fn name_taken_reports_the_holder_and_source() {
        let state = NameTakenState::new(
            NameHolder::new(HolderKind::Project, "shop", HolderState::Deleted),
            NameSource::Home,
        );
        let refused = history(HistoryError::NameTaken(Box::new(state)));
        assert_eq!(refused.code, "NameTaken");
        let holder = refused.detail.get("holder").expect("holder");
        assert_eq!(holder.get_text("kind"), Some("project"));
        assert_eq!(holder.get_text("name"), Some("shop"));
        assert_eq!(holder.get_text("state"), Some("deleted"));
        assert_eq!(refused.detail.get_text("source"), Some("home"));
    }

    #[test]
    fn removed_reports_every_path_and_frontier() {
        let state = RemovedState::new(vec![
            RemovedPath::new("/b", 9),
            RemovedPath::new("/a", 4),
        ]);
        let refused = history(HistoryError::Removed(Box::new(state)));
        assert_eq!(refused.code, "Removed");
        let SdkValue::List(paths) = refused.detail.get("paths").expect("paths") else {
            panic!("paths are a list: {:?}", refused.detail);
        };
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1].get_text("path"), Some("/a"));
        assert_eq!(paths[1].get_int("frontier_serial"), Some(4));
    }

    #[test]
    fn mounts_active_reports_every_holder() {
        let refused = history(HistoryError::MountsActive(Box::new(MountHolders::new(
            Vec::new(),
        ))));
        assert_eq!(refused.code, "MountsActive");
        let SdkValue::List(holders) = refused.detail.get("holders").expect("holders") else {
            panic!("holders are a list: {:?}", refused.detail);
        };
        assert!(holders.is_empty());
    }

    /// Busy and Unresolved share one entity/phase/op shape, and the resource
    /// scope kind names what is held (store, project or branch), not the
    /// operation kind. The PostgreSQL holder is null unless a lock named one.
    #[test]
    fn busy_and_unresolved_report_the_resource_scope_and_holder() {
        let busy = OpConflict::new(
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            "store",
            Some("swap".to_owned()),
            "00ff".to_owned(),
            false,
        )
        .with_backend(
            Some(4242),
            Some("10.0.0.7".to_owned()),
            Some("2026-01-02 03:04:05+00".to_owned()),
        );
        let refused = history(HistoryError::BusyOp(Box::new(busy)));
        assert_eq!(refused.code, "Busy");
        let entity = refused.detail.get("entity").expect("entity");
        assert_eq!(entity.get_text("kind"), Some("store"));
        assert_eq!(entity.get_text("name"), Some("store"));
        assert_eq!(refused.detail.get_text("phase"), Some("swap"));
        assert_eq!(refused.detail.get_text("op"), Some("00ff"));
        assert!(!refused.detail.get_bool("uncertain"));
        assert_eq!(refused.detail.get_int("pid"), Some(4242));
        assert_eq!(refused.detail.get_text("client_addr"), Some("10.0.0.7"));
        assert_eq!(
            refused.detail.get_text("backend_start"),
            Some("2026-01-02 03:04:05+00")
        );

        // Unresolved reports where the dead owner's op sits; on SQLite no
        // backend is named, so the holder fields are null.
        let unresolved = OpConflict::new(
            GcIntentKind::Removal,
            GcScopeKind::Project,
            "shop",
            None,
            "00aa".to_owned(),
            true,
        );
        let refused = history(HistoryError::Unresolved(Box::new(unresolved)));
        assert_eq!(refused.code, "Unresolved");
        let entity = refused.detail.get("entity").expect("entity");
        assert_eq!(entity.get_text("kind"), Some("project"));
        assert_eq!(entity.get_text("name"), Some("shop"));
        assert!(refused.detail.get("phase").expect("phase") == &SdkValue::Null);
        assert!(refused.detail.get_bool("uncertain"));
        assert!(refused.detail.get("pid").expect("pid") == &SdkValue::Null);
        assert!(refused.detail.get("client_addr").expect("addr") == &SdkValue::Null);
        assert!(
            refused.detail.get("backend_start").expect("start") == &SdkValue::Null
        );
    }
}
