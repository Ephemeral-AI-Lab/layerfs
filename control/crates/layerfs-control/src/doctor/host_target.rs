//! The home's own checks (call surface 2.0, `calls.json` `doctor.checks.home`),
//! in the order they run: `home.build`, `home.prerequisites`, `home.layout`,
//! `home.records`, `home.bundle`, `home.native_daemon`, `home.artifacts`,
//! `home.capacity`. Each observes; none opens or changes the
//! home. A check that does not apply is left out: a home not made yet has
//! no layout or records; the bundle is checked only where a recorded
//! machine needs it; a check that waits on an unreadable layout or record
//! set is not made. Once the records scan gave a stable verdict,
//! `home.layout` also cross-checks the id `layerfs.json` holds against the
//! provider names the records keep (`super::host_identity`).
use super::records::Findings;
use super::target::{Budget, Checks};
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
        checks.timeout(LAYOUT, "its time ran out before the home was read");
        return None;
    }
    // `detect` refuses only a read the system refused, which may be a real
    // 2.0 home: the OS error, and no fix (RC-8)
    let layout = match detect(home) {
        Ok(layout) => layout,
        Err(refusal) => {
            checks.fail(LAYOUT, refusal.message);
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
            None
        }
        // a machine's worker root is not a home, and moving a data disk
        // would empty its volume: name the real home, run no records check
        // (layerfs-project#752, c5)
        Layout::WorkerRoot => {
            checks.fail(
                LAYOUT,
                "a machine's worker root, not a LayerFS 2.0 home: this build writes nothing in it; point LAYERFS_HOME at your home",
            );
            checks.fix(LAYOUT, layerfs_home::home::layout::worker_root_fix());
            None
        }
        // a home that lost its `layerfs.json`: its data is there, so this is
        // not a foreign folder and is never moved aside; `home init --adopt`
        // writes the marker without touching the data (layerfs-project#752, c6)
        Layout::DataWithoutMarker => {
            checks.fail(
                LAYOUT,
                "a LayerFS home's data with no layerfs.json: this build writes nothing in it; adopt it and keep the data",
            );
            checks.fix(
                LAYOUT,
                layerfs_home::home::layout::data_without_marker_fix(),
            );
            None
        }
        Layout::Current(file) => {
            checks.pass(LAYOUT, "a current-layout home this build reads");
            let scanned = observed(home, budget, checks);
            if let Some(records) = &scanned {
                identity(home, &file.id, records, checks);
            }
            scanned
        }
    }
}

/// `home.layout`'s second part: the id `layerfs.json` holds against the
/// provider names the home's records keep, once the records scan gave a
/// stable verdict. A mismatch fails `home.layout` with no fix: the record or
/// the id belongs to another home, and moving this one aside is not a
/// repair.
fn identity(home: &Path, home_id: &str, records: &super::scan::Scanned, checks: &mut Checks) {
    let Some(conflict) = super::host_identity::mismatch(home, home_id, records) else {
        return;
    };
    checks.fail(
        LAYOUT,
        format!(
            "{} records the provider name {:?}, but this home's id gives {:?}: the record or layerfs.json comes from another home, so use the home it belongs to or remove the record by hand",
            conflict.path.display(),
            conflict.recorded,
            conflict.expected
        ),
    );
}

/// `home.records`: every record, with the home's record set fingerprinted
/// before and after so a home that changed meanwhile gives no verdict.
fn observed(home: &Path, budget: &Budget, checks: &mut Checks) -> Option<super::scan::Scanned> {
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
