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
    /// The owning runtime token, when the Store recorded one.
    pub owner_runtime: Option<String>,
    /// The owning boot token, when the Store recorded one.
    pub owner_boot: Option<String>,
    /// The machine or worker generation that placed the admission.
    pub machine_incarnation: Option<String>,
    /// The Sandbox generation that placed the admission.
    pub sandbox_generation: Option<String>,
    /// True when the owner is uncertain: the row may not be released by
    /// observation alone.
    pub uncertain: bool,
}

/// One retained `done` operator-audit row, as the read-only diagnostic sees
/// it. It names no live holder: admission and preview filter these rows.
pub struct DoneAuditRow {
    /// The 16-byte operation identity.
    pub operation: Vec<u8>,
    /// The operation kind, as the table records it.
    pub kind: String,
    /// The scope kind, as the table records it.
    pub scope_kind: String,
    /// The typed scope identity.
    pub scope: Vec<u8>,
    /// Project name, when recorded.
    pub project: Option<String>,
    /// Branch name, when recorded.
    pub branch: Option<String>,
    /// Machine name, when recorded.
    pub machine: Option<String>,
    /// Sandbox name, when recorded.
    pub sandbox: Option<String>,
    /// Operation phase the action left.
    pub phase: Option<String>,
    /// The action/operator/op evidence a release recorded.
    pub evidence: Option<String>,
    /// When the row was retained, in Unix milliseconds.
    pub updated_ms: i64,
}

/// One bounded page of retained `done` audit rows, oldest first.
pub struct DoneAuditPage {
    /// The retained rows on this page.
    pub items: Vec<DoneAuditRow>,
    /// The zero-based page that was read.
    pub page: u32,
    /// The page size that was read, clamped to the maximum.
    pub limit: u32,
    /// True when another page may follow.
    pub has_more: bool,
}

/// Reads one bounded page of retained `done` rows in the diagnostic
/// transaction. The live holder query above keeps filtering them out.
pub(super) fn done(view: &dyn View, page: u32, limit: u32) -> Result<DoneAuditPage, BackendError> {
    let limit = limit.clamp(1, layerfs_history::MAXIMUM_AUDIT_PAGE);
    let offset = i64::from(page) * i64::from(limit);
    let sql = crate::backend::sqlite::session::query::history("gc_intent_done")?;
    let rows = view.query(sql, vec![Param::I64(i64::from(limit)), Param::I64(offset)])?;
    let items = rows
        .iter()
        .map(|r| {
            Ok(DoneAuditRow {
                operation: r.get(0)?,
                kind: r.get(1)?,
                scope_kind: r.get(2)?,
                scope: r.get(3)?,
                project: r.get(4)?,
                branch: r.get(5)?,
                machine: r.get(6)?,
                sandbox: r.get(7)?,
                phase: r.get(8)?,
                evidence: r.get(9)?,
                updated_ms: r.get(10)?,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    let has_more = items.len() == usize::try_from(limit).unwrap_or(usize::MAX);
    Ok(DoneAuditPage {
        items,
        page,
        limit,
        has_more,
    })
}

pub(super) fn read(
    view: &dyn View,
    stack: Option<LayerStackId>,
) -> Result<Vec<MountHolder>, BackendError> {
    let mut sql = String::from("SELECT g.op_id,g.scope_id,g.workspace,g.sandbox,g.machine,p.name,b.name,g.owner_runtime,g.owner_boot,g.evidence,g.uncertain FROM gc_intent g LEFT JOIN branch b ON g.scope_kind='branch' AND b.branch_id=g.scope_id LEFT JOIN layer_stack p ON p.layer_stack_id=CASE WHEN g.scope_kind='project' THEN g.scope_id ELSE b.layer_stack_id END WHERE g.kind='mount' AND g.state<>'done'");
    let mut params = Vec::new();
    if let Some(stack) = stack {
        sql.push_str(" AND (g.scope_kind='store' OR p.layer_stack_id=?1)");
        params.push(Param::Bytes(stack.as_slice().to_vec()));
    }
    sql.push_str(" ORDER BY g.op_id LIMIT 65");
    view.query(&sql, params)?
        .iter()
        .map(|r| {
            let (machine_incarnation, sandbox_generation) = crate::history::place_tokens(r.get(9)?);
            Ok(MountHolder {
                operation: r.get(0)?,
                scope: r.get(1)?,
                workspace: r.get(2)?,
                sandbox: r.get(3)?,
                machine: r.get(4)?,
                project: r.get(5)?,
                branch: r.get(6)?,
                owner_runtime: r.get(7)?,
                owner_boot: r.get(8)?,
                machine_incarnation,
                sandbox_generation,
                uncertain: r.get::<i64>(10)? != 0,
            })
        })
        .collect()
}
