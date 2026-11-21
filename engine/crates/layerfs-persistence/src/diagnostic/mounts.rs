//! Durable mount admissions, including those whose runtime records are gone.
use super::transaction::View;
use crate::backend::records::{BackendError, Param};
use layerfs_history::LayerStackId;

/// Identity and placement as recorded by the Store. Display names are hints.
pub struct MountHolder {
    /// Unique operation identity for this admission.
    pub operation: Vec<u8>,
    /// Branch or project identity as recorded.
    pub scope: Vec<u8>,
    /// Workspace identity in its stored hex form.
    pub workspace: Option<String>,
    /// Sandbox placement hint.
    pub sandbox: Option<String>,
    /// Machine placement hint.
    pub machine: Option<String>,
    /// Current project name resolved through the authoritative scope.
    pub project: Option<String>,
    /// Current branch name resolved through the authoritative scope.
    pub branch: Option<String>,
}

pub(super) fn read(
    view: &dyn View,
    stack: Option<LayerStackId>,
) -> Result<Vec<MountHolder>, BackendError> {
    let mut sql = String::from("SELECT g.op_id,g.scope_id,g.workspace,g.sandbox,g.machine,p.name,b.name FROM gc_intent g LEFT JOIN branch b ON g.scope_kind='branch' AND b.branch_id=g.scope_id LEFT JOIN layer_stack p ON p.layer_stack_id=CASE WHEN g.scope_kind='project' THEN g.scope_id ELSE b.layer_stack_id END WHERE g.kind='mount' AND g.state<>'done'");
    let mut params = Vec::new();
    if let Some(stack) = stack {
        sql.push_str(" AND (g.scope_kind='store' OR p.layer_stack_id=?1)");
        params.push(Param::Bytes(stack.as_slice().to_vec()));
    }
    // One overflow row lets doctor explicitly report incomplete coverage.
    sql.push_str(" ORDER BY g.op_id LIMIT 65");
    view.query(&sql, params)?
        .iter()
        .map(|r| {
            Ok(MountHolder {
                operation: r.get(0)?,
                scope: r.get(1)?,
                workspace: r.get(2)?,
                sandbox: r.get(3)?,
                machine: r.get(4)?,
                project: r.get(5)?,
                branch: r.get(6)?,
            })
        })
        .collect()
}
