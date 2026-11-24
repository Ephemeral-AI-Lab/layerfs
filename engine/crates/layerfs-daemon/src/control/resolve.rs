//! The one resolution of a Workspace's retained Commit custody, run when its
//! next Commit is requested (store-service `store/resolve.rs`). It is not a
//! retry: nothing is resent. A record whose publication outcome the attempt
//! could not determine stays for the next request, which only reads again.
use super::{registry::bound_mut, Failure, Service};
use crate::store::Resolution;
use layerfs_bridge::control::{Activity, ControlCode, ControlRefusal, WorkspaceToken};

impl Service {
    /// When `token`'s Workspace is `Uncertain` or `LocalFailure` with a
    /// retained Commit record, takes the entry for the attempt (activity
    /// `Committing`, phase `resolve`), runs it once and settles the entry:
    /// resolved, it is `Idle` and the Commit's own admission follows;
    /// unresolved, it keeps custody (`LocalFailure` once a publication is
    /// known) and the Commit is refused `Unknown` with the reason. Any other
    /// entry is left to that admission.
    pub(super) fn resolve_custody(&self, token: WorkspaceToken) -> Result<(), Failure> {
        let (workspace, before) = {
            let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound_mut(&mut entries, token)?;
            let before = value.activity;
            if !matches!(before, Activity::Uncertain | Activity::LocalFailure)
                || !value.workspace.has_retained()
            {
                return Ok(());
            }
            value.enter(Activity::Committing);
            value.phase = "resolve";
            value.epoch = value.epoch.saturating_add(1);
            (value.workspace.clone(), before)
        };
        let result = workspace.resolve_retained();
        // An install of a merged root replaced what the Workspace showed.
        let invalidated = match &result {
            Ok(Some(Resolution {
                replaced: Some((shown, installed)),
                ..
            })) => self.invalidate(token, &workspace, *shown, *installed),
            _ => Ok(()),
        };
        let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let value = bound_mut(&mut entries, token)?;
        value.epoch = value.epoch.saturating_add(1);
        match result {
            Ok(Some(_)) => {
                value.snapshot = workspace.snapshot().map_err(Failure::Workspace)?;
                value.published = None;
                value.enter(Activity::Idle);
                invalidated
            }
            Ok(None) => {
                value.enter(before);
                Ok(())
            }
            Err(unresolved) => {
                if unresolved.published.is_some() {
                    value.published = unresolved.published;
                }
                value.enter(if value.published.is_some() {
                    Activity::LocalFailure
                } else {
                    before
                });
                Err(Failure::Custody(Box::new(ControlRefusal {
                    code: ControlCode::Unknown,
                    phase: "admission".into(),
                    moved: None,
                    published: value.published.clone(),
                    detail: super::kernel::bounded(&format!(
                        "original Commit custody retained; its one resolution did not settle it: {}",
                        unresolved.reason
                    )),
                })))
            }
        }
    }
}
