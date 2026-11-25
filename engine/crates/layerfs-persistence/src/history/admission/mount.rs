//! A Workspace mount's registration on PostgreSQL as one pipelined message
//! (#768 step B, `backend/postgresql/history/mount.rs`): the session keys,
//! the Branch probe, the conflict scan, the intent insert, the `COMMIT` and
//! the bound node's markers, one round trip.
//!
//! The message admits only what the statement-by-statement registration
//! would admit with nothing to settle: it refuses whenever a live operation
//! row conflicts, dead or not. Every refusal it cannot report exactly is
//! classified by that registration, once: a refusal after the keys were
//! granted first gives back exactly those keys (one statement), and a
//! refused key leaves none held. That registration then settles provably
//! dead rows and reports `MountsActive`, `Busy` or `Unresolved` with the
//! holder, or registers. A stale generation and a transaction-pooled
//! connection are reported as they are; an unknown outcome quarantines the
//! session and is never sent again.
use super::*;
use crate::backend::postgresql::history::mount::{admit, Admission, IntentRow};
use crate::backend::postgresql::session::connection::SessionKey;
use layerfs_history::identity::NodeId;
use layerfs_history::MountMarkers;

/// Registers `intent` and, with `markers` (`(stack, node)`), reads the
/// node's removals after it committed. On PostgreSQL a Branch mount on a
/// session with a generation is one message; anything else, and every
/// refusal of that message it cannot report exactly, registers statement by
/// statement and reads nothing.
pub(crate) fn register_mount(
    provider: &HistoryProvider,
    intent: &GcIntent,
    markers: Option<(LayerStackId, NodeId)>,
) -> HistoryResult<MountMarkers> {
    match one_message(provider, intent, markers)? {
        Some(read) => Ok(read),
        None => register_separately(provider, intent).map(|()| MountMarkers::NotRead),
    }
}

/// The one-message registration alone: `Some` when it admitted (the keys
/// held with the op), `None` when it does not apply or refused with
/// nothing written and no key held, for [`register_separately`] to decide;
/// `Err` for a refusal it reports as it is or an unknown outcome.
pub(crate) fn one_message(
    provider: &HistoryProvider,
    intent: &GcIntent,
    markers: Option<(LayerStackId, NodeId)>,
) -> HistoryResult<Option<MountMarkers>> {
    let session = match &provider.engine {
        Engine::Postgres(session)
            if intent.kind == GcIntentKind::Mount
                && intent.scope_kind == GcScopeKind::Branch
                && intent.project_id.is_some()
                && session
                    .incarnation
                    .load(std::sync::atomic::Ordering::SeqCst)
                    != 0 =>
        {
            session
        }
        _ => return Ok(None),
    };
    let catalog = provider.catalog_id;
    let keys: Vec<SessionKey> = (advisory_keys(catalog, intent).iter())
        .map(|(kind, id, shared)| (advisory_key(catalog, *kind, id), *shared))
        .collect();
    let row = IntentRow {
        op_id: intent.op_id.to_vec(),
        project_id: intent.project_id.clone().unwrap_or_default(),
        branch_id: intent.scope_id.clone(),
        kind: intent.kind.as_str(),
        scope_kind: intent.scope_kind.as_str(),
        workspace: intent.workspace.map(|bytes| hex(&bytes)),
        sandbox: intent.sandbox.clone(),
        machine: intent.machine.clone(),
        project: intent.project.clone(),
        branch: intent.branch.clone(),
        owner_runtime: intent.owner_runtime.clone(),
        owner_boot: intent.owner_boot.clone(),
        state: if intent.uncertain {
            "uncertain"
        } else {
            "live"
        },
        uncertain: i64::from(intent.uncertain),
        phase: intent.phase.clone(),
        evidence: place_evidence(intent),
        updated_ms: now_ms(),
    };
    let asked = markers.map(|(stack, node)| (node.as_slice().to_vec(), stack.as_slice().to_vec()));
    let asked = asked
        .as_ref()
        .map(|(node, stack)| (node.as_slice(), stack.as_slice()));
    match admit(session, &keys, &row, asked)? {
        Admission::Admitted(found) => {
            // A second registration of a held op keeps one level per key.
            if let Some(earlier) = session.hold_op(intent.op_id, keys) {
                let _ = session_unlock(session, &earlier);
            }
            Ok(Some(match found.map(paths::node_markers_of) {
                Some(Ok(found)) => MountMarkers::Read(found),
                _ => MountMarkers::NotRead,
            }))
        }
        Admission::Holding => {
            session_unlock(session, &keys)?;
            Ok(None)
        }
        Admission::Released => Ok(None),
        Admission::Refused(error) => Err(error),
        Admission::Unknown => Err(BackendError::Unknown.into()),
    }
}

/// Whether the statement-by-statement registration's own checks refuse
/// `intent` before it settles anything: its Branch is missing or retired,
/// or a live operation conflicts in its scope (`validate_mount`,
/// `conflicts_for`), read in one transaction.
pub(crate) fn separately_refuses(
    provider: &HistoryProvider,
    intent: &GcIntent,
) -> HistoryResult<bool> {
    provider.read(|tx| {
        if validate_mount(tx, intent).is_err() {
            return Ok(true);
        }
        Ok(!conflicts_for(tx, intent)?.is_empty())
    })
}
