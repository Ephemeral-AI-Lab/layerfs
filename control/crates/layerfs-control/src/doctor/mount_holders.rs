//! Compare Store admissions with local placement records. Missing records are
//! evidence of a blocker, never authority to release an admission.
use super::target::Checks;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{home::paths, names::SandboxName, outcome::refused};
use layerfs_sandbox::record::{SandboxRecord, WorkspaceRecord};
use std::path::Path;

pub(super) const AUDIT: &str = "store.gc_audit";

/// The worker's forwarded `store.gc_audit` event, or the local result: a
/// read that answers is evidence, never a warning; a read that fails is
/// `unknown`, so it cannot turn into a release.
pub(super) fn audit_result(result: Result<SdkValue, SdkRefusal>, checks: &mut Checks) {
    match result {
        Ok(page) => audit_report(&page, checks),
        Err(error) => checks.unknown(AUDIT, error.message),
    }
}

fn audit_report(page: &SdkValue, checks: &mut Checks) {
    let Some(SdkValue::List(items)) = page.get("items") else {
        checks.unknown(AUDIT, "the diagnostic did not report the done audit");
        return;
    };
    let retained = items.len();
    let message = if retained == 0 {
        "no retained done audit rows".to_owned()
    } else {
        format!(
            "{retained} retained done operator-audit row(s); preview and admission filter them, and the doctor changes nothing"
        )
    };
    checks.pass(AUDIT, message);
    if retained > 0 {
        checks.evidence(AUDIT, page.clone());
    }
}

fn recorded(home: &Path, store: &str, row: &SdkValue) -> Result<bool, SdkRefusal> {
    let (Some(sandbox), Some(machine), Some(project), Some(id)) = (
        row.get_text("sandbox"),
        row.get_text("machine"),
        row.get_text("project"),
        row.get_text("workspace"),
    ) else {
        return Ok(false);
    };
    SandboxName::parse(sandbox)?;
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(refused(
            "InvalidInput",
            "invalid workspace id in mount holder",
        ));
    }
    let Some(record) = SandboxRecord::read(home, sandbox)? else {
        return Ok(false);
    };
    if record.project() != Some(format!("{store}/{project}").as_str())
        || record.machine() != machine
    {
        return Ok(false);
    }
    let path = paths::workspace_dir(home, sandbox).join(format!("{id}.json"));
    Ok(WorkspaceRecord::read(&path)?.is_some_and(|workspace| workspace.id() == Some(id)))
}

pub(super) fn report(
    home: &Path,
    store: &str,
    shared: bool,
    check: &str,
    rows: Option<&SdkValue>,
    checks: &mut Checks,
) {
    let Some(SdkValue::List(rows)) = rows else {
        checks.unknown(check, "the diagnostic did not report mount holders");
        return;
    };
    let mut matched = true;
    for row in rows.iter().take(64) {
        let reason = match recorded(home, store, row) {
            Ok(true) => continue,
            Ok(false) => "no matching sandbox and workspace record in this home".to_owned(),
            Err(error) => format!("could not verify the local records: {}", error.message),
        };
        matched = false;
        let hint = |key| row.get_text(key).unwrap_or("unknown");
        checks.warning(check, format!("mount holder {} blocks removal and compaction: {reason} (sandbox {}, machine {}, workspace {})", hint("operation"), hint("sandbox"), hint("machine"), hint("workspace")));
        checks.evidence(check, row.clone());
        checks.fix(check, if shared { "Locate the owning home and machine. Unmount the matching workspace there. If its records are gone, coordinate offline recovery with every Store owner using the operation and scope IDs." } else { "On the Store's owning machine, unmount the matching workspace or remove its stopped sandbox. If its records are gone, follow the doctor guide's Historical mount holders procedure before retrying compact." });
    }
    if rows.len() > 64 {
        checks.unknown(
            check,
            "more than 64 mount holders; remaining ownership was not checked",
        );
    } else if matched {
        checks.pass(
            check,
            "all mount holders have matching local workspace records",
        );
    }
}
