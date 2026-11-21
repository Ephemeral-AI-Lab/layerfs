//! Compare Store admissions with local placement records. Missing records are
//! evidence of a blocker, never authority to release an admission.
use super::target::Checks;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{home::paths, names::SandboxName, outcome::refused};
use layerfs_sandbox::record::{SandboxRecord, WorkspaceRecord};
use std::path::Path;

pub(super) const STORE: &str = "store.mount_holders";
pub(super) const PROJECT: &str = "project.mount_holders";

pub(super) fn result(
    home: &Path,
    store: &str,
    shared: bool,
    check: &str,
    result: Result<SdkValue, SdkRefusal>,
    checks: &mut Checks,
) {
    match result {
        Ok(rows) => report(home, store, shared, check, Some(&rows), checks),
        Err(error) => checks.unknown(check, error.message),
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
        checks.unknown(check, "the worker did not report mount holders");
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
        checks.warning(check, format!("mount holder {} blocks removal and compaction: {reason} (sandbox {}, machine {}, workspace {})", hint("operation"),hint("sandbox"),hint("machine"),hint("workspace")));
        checks.evidence(check, row.clone());
        checks.fix(check, if shared { "Locate the owning home and machine. Unmount the matching workspace there. If its records are gone, coordinate offline recovery with every Store owner using the operation and scope IDs (doctor guide, Historical mount holders)." } else { "On the Store's owning machine, unmount the matching workspace or remove its stopped sandbox. If its records are gone, follow the doctor guide's Historical mount holders procedure before retrying compact." });
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
