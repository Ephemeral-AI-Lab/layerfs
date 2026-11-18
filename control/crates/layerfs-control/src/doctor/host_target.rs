//! The home's own checks (call surface 2.0, `calls.json` `doctor.checks.home`),
//! in the order they run: `home.build`, `home.prerequisites`, `home.layout`,
//! `home.records`, `home.bundle`, `home.native_daemon`, `home.artifacts`,
//! `home.capacity`. Each observes; none opens, converts or changes the
//! home. A check that does not apply is left out: a home not made yet has
//! no layout or records; the bundle is checked only where a recorded
//! machine needs it; a check that waits on an unreadable layout or record
//! set is not made.
use super::records::Findings;
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{detect, Layout};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[path = "host_capacity.rs"]
mod disk;
#[path = "host_install.rs"]
mod install;
#[path = "host_inventory.rs"]
mod inventory;

const LAYOUT: &str = "home.layout";
const RECORDS: &str = "home.records";

pub(super) fn diagnose(
    home: &Path,
    build: crate::home::Build<'_>,
    budget: &Budget,
    checks: &mut Checks,
) {
    install::build(home, checks);
    install::prerequisites(checks);
    let scanned = layout(home, budget, checks);
    if let Some(records) = &scanned {
        install::bundle(records, checks);
    }
    install::native_daemon(home, build, checks);
    if let Some(records) = &scanned {
        install::artifacts(records, budget, checks);
    }
    disk::check(home, budget, checks);
}

/// `home.layout`, then `home.records` on a home whose layout reads: what
/// the records scan read, when it gave a stable verdict.
fn layout(home: &Path, budget: &Budget, checks: &mut Checks) -> Option<super::scan::Scanned> {
    if budget.expired() {
        checks.unknown(LAYOUT, "its time ran out before the home was read");
        return None;
    }
    let Ok(layout) = detect(home) else {
        checks.fail(
            LAYOUT,
            format!(
                "{}'s layerfs.json is unreadable, malformed or of a layout this build does not know; repair it by hand",
                home.display()
            ),
        );
        return None;
    };
    match layout {
        // a home not made yet: nothing to read
        Layout::Empty => None,
        Layout::Old => {
            checks.warning(
                LAYOUT,
                "a LayerFS 1.1.0 home: it is converted on its first 2.0 call that writes; nothing was converted",
            );
            checks.fix(LAYOUT, "layerfs home init");
            None
        }
        Layout::Previous(_) => {
            checks.warning(
                LAYOUT,
                "a layout 2 home: it is converted on its first call; nothing was converted",
            );
            checks.fix(LAYOUT, "layerfs home init");
            observed(home, budget, checks, true)
        }
        Layout::Current(_) => {
            checks.pass(LAYOUT, "a current-layout home this build reads");
            observed(home, budget, checks, false)
        }
    }
}

/// `home.records`: every record, with the home's record set fingerprinted
/// before and after so a home that changed meanwhile gives no verdict.
fn observed(
    home: &Path,
    budget: &Budget,
    checks: &mut Checks,
    previous: bool,
) -> Option<super::scan::Scanned> {
    let before = inventory::fingerprint(home, budget);
    if before.is_err() {
        // a home folder the inventory will not walk (a symlinked `store/`)
        // is the records scan's own finding: report it when the scan names it
        let mut findings = Findings::default();
        let named = !previous && super::scan::scan(home, &mut findings).is_ok();
        let said = findings.to_sdk();
        if named && matches!(said.get("findings"), Some(SdkValue::List(rows)) if !rows.is_empty()) {
            publish(findings, checks);
        } else {
            checks.unknown(
                RECORDS,
                "the home's records could not all be read; no verdict",
            );
        }
        return None;
    }
    let mut findings = Findings::default();
    let mut scanned = None;
    let result = if budget.expired() {
        Err(())
    } else if previous {
        super::convert::check(home, &mut findings);
        Ok(())
    } else {
        super::scan::scan(home, &mut findings)
            .map_err(|_| ())
            .and_then(|records| {
                let result =
                    super::checks::check_local(home, &records, &mut findings).map_err(|_| ());
                scanned = Some(records);
                result
            })
    };
    let after = inventory::fingerprint(home, budget);
    if budget.expired() || after.is_err() || before.ok() != after.ok() {
        checks.unknown(
            RECORDS,
            "the home's records changed while they were read, or its time ran out; no verdict",
        );
        return None;
    }
    if result.is_err() {
        checks.unknown(RECORDS, "a record or lease could not be read; no verdict");
        return None;
    }
    publish(findings, checks);
    scanned
}

/// The records findings, each on the thing it is about.
fn publish(findings: Findings, checks: &mut Checks) {
    let report = findings.to_sdk();
    let Some(SdkValue::List(rows)) = report.get("findings") else {
        return;
    };
    if rows.is_empty() {
        checks.pass(
            RECORDS,
            "every record reads: references, file modes, leases and operations",
        );
        return;
    }
    for row in rows {
        let message = row.get_text("message").unwrap_or("a record is wrong");
        if row.get_text("severity") == Some("error") {
            checks.fail(RECORDS, message);
        } else {
            checks.warning(RECORDS, message);
        }
        if let Some(fix) = row.get_text("fix").filter(|fix| !fix.is_empty()) {
            checks.fix(RECORDS, fix);
        }
        if let Some(entity) = row.get("entity") {
            let text = |key: &str| entity.get_text(key).unwrap_or("").to_owned();
            checks.about(RECORDS, &text("kind"), &text("name"));
        }
    }
}

pub(super) fn executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
