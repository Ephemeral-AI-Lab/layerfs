//! Observational host diagnosis: installation, local records and local capacity.
use super::report::Findings;
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{detect, Layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

#[path = "host_capacity.rs"]
mod capacity;
#[path = "host_install.rs"]
mod install;

pub(super) fn diagnose(home: &Path, budget: &Budget, checks: &mut Checks) {
    install::identity(checks);
    capacity::check(home, budget, checks);
    if budget.expired() {
        checks.unknown("host.layout", "Deadline expired before inspecting the home");
        return;
    }
    let layout = match detect(home) {
        Ok(layout) => layout,
        Err(_) => {
            checks.fail(
                "host.layout",
                "Home layout is unreadable, malformed or unsupported",
            );
            checks.blocked(
                "host.records",
                "Home layout could not be established",
                "host.layout",
            );
            install::without_inventory(checks, true);
            return;
        }
    };
    checks.evidence(
        "host.build",
        SdkValue::map([
            ("home", SdkValue::text(home.display().to_string())),
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("os", SdkValue::text(std::env::consts::OS)),
            ("arch", SdkValue::text(std::env::consts::ARCH)),
        ]),
    );
    match layout {
        Layout::Empty => {
            checks.skip(
                "host.layout",
                "Home is not initialized; no home was created",
            );
            checks.skip("host.records", "No initialized home to inspect");
            install::without_inventory(checks, false);
        }
        Layout::Old => {
            checks.warning(
                "host.layout",
                "LayerFS 1.1.0 home requires explicit import; no conversion performed",
            );
            checks.skip(
                "host.records",
                "Current-layout checks do not apply to this home",
            );
            install::without_inventory(checks, false);
        }
        Layout::Previous(_) => {
            checks.warning(
                "host.layout",
                "Layout 2 home requires conversion; no conversion performed",
            );
            observed(home, budget, checks, true);
        }
        Layout::Current(_) => {
            checks.pass("host.layout", "Current home layout is readable");
            observed(home, budget, checks, false);
        }
    }
}

fn observed(home: &Path, budget: &Budget, checks: &mut Checks, previous: bool) {
    let before = fingerprint(home, budget);
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
    let after = fingerprint(home, budget);
    if budget.expired() || before.is_err() || after.is_err() || before.ok() != after.ok() {
        checks.unknown("host.records", "Record inventory changed or could not be observed completely; no stable consistency verdict");
        install::without_inventory(checks, true);
        return;
    }
    if result.is_err() {
        checks.unknown(
            "host.records",
            "Local record or lease inspection could not complete",
        );
        install::without_inventory(checks, true);
        return;
    }
    publish(findings, checks);
    if let Some(records) = scanned {
        install::artifacts(&records, budget, checks);
    } else {
        install::without_inventory(checks, false);
    }
    checks.skip(
        "host.remote",
        "Worker connections and remote transfer files are outside host scope",
    );
}

fn publish(findings: Findings, checks: &mut Checks) {
    let report = findings.to_sdk();
    let Some(SdkValue::List(rows)) = report.get("findings") else {
        return;
    };
    if rows.is_empty() {
        checks.pass(
            "host.records",
            "Local record, permission, reference, intent and lease checks found no problems",
        );
        return;
    }
    for row in rows {
        let message = row.get_text("message").unwrap_or("Local record issue");
        if row.get_text("severity") == Some("error") {
            checks.fail("host.records", message);
        } else {
            checks.warning("host.records", message);
        }
        if let Some(fix) = row.get_text("fix") {
            checks.fix("host.records", fix);
        }
        if let Some(entity) = row.get("entity") {
            checks.evidence("host.records", entity.clone());
        }
    }
}

// Fingerprints metadata only, never reads database payloads or follows links.
// Comparing both observations prevents a changed inventory becoming a corruption verdict.
fn fingerprint(home: &Path, budget: &Budget) -> Result<u64, ()> {
    fn visit(path: &Path, budget: &Budget, hash: &mut DefaultHasher) -> Result<(), ()> {
        if budget.expired() {
            return Err(());
        }
        let meta = std::fs::symlink_metadata(path).map_err(|_| ())?;
        path.hash(hash);
        (
            meta.ino(),
            meta.dev(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            meta.mode(),
        )
            .hash(hash);
        if meta.is_dir() {
            let mut entries = std::fs::read_dir(path)
                .map_err(|_| ())?
                .map(|entry| entry.map(|e| e.path()).map_err(|_| ()))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort();
            for entry in entries {
                visit(&entry, budget, hash)?;
            }
        }
        Ok(())
    }
    let mut hash = DefaultHasher::new();
    visit(home, budget, &mut hash)?;
    Ok(hash.finish())
}

pub(super) fn executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
