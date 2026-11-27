//! A Workspace mount's registration on PostgreSQL as one pipelined message
//! (#768 step B, `backend/postgresql/history/mount.rs`): the session keys,
//! the Branch probe, the conflict scan, the intent insert, the `COMMIT` and
//! the bound node's markers, one round trip.
//!
//! The message admits only what the statement-by-statement registration
//! would admit with nothing to settle: it refuses whenever a live operation
//! row conflicts, dead or not. Every definite refusal it does not report
//! itself (`StaleStore` and a pooled connection, which that registration
//! would report too, and a lock timeout before any key, reported `Busy`
//! because that registration's key statement would wait without one) is
//! decided by that registration, once,
//! whatever its cause: a refusal after the keys were
//! granted first gives back exactly those keys (one statement), and a
//! refused key leaves none held. That registration then settles provably
//! dead rows and reports `MountsActive`, `Busy` or `Unresolved` with the
//! holder, or registers. A stale generation and a transaction-pooled
//! connection are reported as they are; an unknown outcome quarantines the
//! session and is never sent again.
use super::intents::*;
use crate::backend::postgresql::history::mount::{admit, Admission, IntentRow};
use crate::backend::postgresql::session::connection::SessionKey;
use layerfs_history::identity::NodeId;
use layerfs_history::MountMarkers;

/// Registers `intent` and, with `markers` (`(stack, node)`), reads the
/// node's removals after it committed. On PostgreSQL a Branch mount on a
/// session with a generation is one message; anything else, and every
/// refusal of that message it cannot report exactly, registers statement by
/// statement and reads nothing.
///
/// A mount whose outcome is unknown may have committed its row, and never
/// reached its caller, which so holds no op to release: a writer that
/// reopens records the op, inside the turn that lost the reply, and
/// releases it when it reopens (`Session::note_orphan`).
pub(crate) fn register_mount(
    provider: &HistoryProvider,
    intent: &GcIntent,
    markers: Option<(LayerStackId, NodeId)>,
) -> HistoryResult<MountMarkers> {
    match one_message(provider, intent, markers) {
        Ok(Some(read)) => Ok(read),
        Ok(None) => register_separately(provider, intent).map(|()| MountMarkers::NotRead),
        Err(error) => Err(error),
    }
}

/// The one-message registration alone: `Some` when it admitted (the keys
/// held with the op), `None` when it does not apply or met any definite
/// refusal, with nothing written and no key held (keys it held are given
/// back first), for [`register_separately`] to decide once; `Err` only for
/// `StaleStore`, a pooled connection (the answers that registration would
/// give itself), `Busy` for a lock timeout before any key was taken (that
/// registration's key statement has no lock timeout), a refusal on a
/// session that lost a reply, and an unknown
/// outcome, which quarantined the session and is never sent again.
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
    // Inside the admission's turn: an admitted op is held (its keys are on
    // that connection), and an unknown one recorded as an orphan, before
    // any reopen can read them.
    let mut earlier = None;
    let settle = |admission: &Admission| match admission {
        Admission::Admitted(_) => earlier = session.hold_op(intent.op_id, keys.clone()),
        Admission::Unknown => session.note_orphan(intent.op_id),
        _ => {}
    };
    let (admission, epoch) = match admit(session, &keys, &row, asked, settle) {
        Ok(admitted) => admitted,
        // An unknown outcome quarantined the session: never sent again.
        Err(BackendError::Unknown) => return Err(BackendError::Unknown.into()),
        // A definite refusal before anything was sent (a statement that did
        // not prepare, among others): nothing written, no key held.
        Err(_) => return Ok(None),
    };
    match admission {
        Admission::Admitted(found) => {
            // A second registration of a held op keeps one level per key.
            if let Some(earlier) = earlier {
                let _ = session_unlock(session, &earlier, Some(epoch));
            }
            Ok(Some(match found.map(paths::node_markers_of) {
                Some(Ok(found)) => MountMarkers::Read(found),
                _ => MountMarkers::NotRead,
            }))
        }
        Admission::Holding => {
            session_unlock(session, &keys, Some(epoch))?;
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
