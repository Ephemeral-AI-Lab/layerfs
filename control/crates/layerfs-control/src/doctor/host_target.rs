//! The home's own checks (call surface 2.0, `calls.json` `doctor.checks.home`),
//! in the order they run: `home.build`, `home.prerequisites`, `home.layout`,
//! `home.records`, `home.bundle`, `home.host_daemon`, `home.artifacts`,
//! `home.capacity`. Each observes; none opens or changes the
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
    let scanned = layout(home, build, budget, checks);
    if let Some(records) = &scanned {
        install::bundle(home, records, checks);
    }
    install::host_daemon(home, build, checks);
    if let Some(records) = &scanned {
        install::artifacts(records, budget, checks);
    }
    disk::check(home, budget, checks);
}

/// `home.layout`, then `home.records` on a home whose layout reads: what
/// the records scan read, when it gave a stable verdict.
fn layout(
    home: &Path,
    build: crate::home::Build<'_>,
    budget: &Budget,
    checks: &mut Checks,
) -> Option<super::scan::Scanned> {
    if budget.expired() {
        checks.timeout(LAYOUT, "its time ran out before the home was read");
        return None;
    }
    // `detect` refuses only a read the system refused, which may be a real
    // 2.0 home: the OS error, and no fix (RC-8)
    let layout = match detect(home) {
        Ok(layout) => layout,
        Err(refusal) => {
            checks.fail(LAYOUT, refusal.message);
            if let Some(records) = refusal.detail.get("legacy_host_records") {
                if let Some(fix) = refusal.detail.get_text("fix") {
                    checks.fix(LAYOUT, fix);
                }
                checks.evidence(LAYOUT, SdkValue::map([("records", records.clone())]));
                return None;
            }
            // no fix: the OS error is the finding, and the cause is the
            // home folder's permissions or mount
            checks.evidence(
                LAYOUT,
                SdkValue::map([
                    ("path", SdkValue::text(home.display().to_string())),
                    ("code", SdkValue::text(&refusal.code)),
                ]),
            );
            return None;
        }
    };
    match layout {
        // a home not made yet: nothing to read
        Layout::Empty => None,
        Layout::Foreign => {
            checks.fail(
                LAYOUT,
                "not a LayerFS 2.0 home: this build writes nothing in it; move it aside and make a new home",
            );
            checks.fix(LAYOUT, layerfs_home::home::layout::foreign_fix(home));
            checks.evidence(
                LAYOUT,
                SdkValue::map([
                    ("path", SdkValue::text(home.display().to_string())),
                    ("layout", SdkValue::text("foreign")),
                ]),
            );
            None
        }
        Layout::Current(_) => {
            checks.pass(LAYOUT, "a current-layout home this build reads");
            let scanned = observed(home, build, budget, checks);
            #[cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
            super::host_worker::directory(home, checks);
            scanned
        }
    }
}

/// `home.records`: every record, with the home's record set fingerprinted
/// before and after so a home that changed meanwhile gives no verdict.
fn observed(
    home: &Path,
    build: crate::home::Build<'_>,
    budget: &Budget,
    checks: &mut Checks,
) -> Option<super::scan::Scanned> {
    let before = inventory::fingerprint(home, budget);
    if before.is_err() {
        // a home folder the inventory will not walk (a symlinked `store/`)
        // is the records scan's own finding: report it when the scan names it
        let mut findings = Findings::default();
        let named = super::scan::scan(home, &mut findings).is_ok();
        if named && !findings.is_empty() {
            findings.publish(RECORDS, checks);
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
    } else {
        super::scan::scan(home, &mut findings)
            .map_err(|_| ())
            .and_then(|records| {
                let result = super::checks::check_local(home, &records, build, &mut findings)
                    .map_err(|_| ());
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
    if findings.is_empty() {
        checks.pass(
            RECORDS,
            "every record reads: references, file modes, leases and operations",
        );
    } else {
        findings.publish(RECORDS, checks);
    }
    scanned
}

pub(super) fn executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
