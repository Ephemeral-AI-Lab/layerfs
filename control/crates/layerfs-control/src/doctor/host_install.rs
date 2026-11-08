//! Local executable and applicable bundle checks, without deployment.
use super::super::scan::Scanned;
use super::{executable, Budget, Checks};
use layerfs_home::home::read_record;
use layerfs_machine::{bundle_dir, IMAGE_ARCHIVE};
use std::path::Path;

pub(super) fn identity(checks: &mut Checks) {
    match std::env::current_exe() {
        Ok(path) if executable(&path) => {
            checks.pass("host.build", "Calling executable exists and is executable")
        }
        _ => checks.unknown("host.build", "Calling executable could not be inspected"),
    }
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::platform::{native_missing, HostOs};
        let missing = native_missing(HostOs::current());
        if missing.is_empty() {
            checks.pass(
                "host.prerequisites",
                "Observable native runtime prerequisites are present",
            );
        } else {
            checks.warning(
                "host.prerequisites",
                format!(
                    "Native runtime prerequisites missing: {}",
                    missing.join(", ")
                ),
            );
        }
    }
    #[cfg(not(feature = "machine"))]
    checks.skip(
        "host.prerequisites",
        "Build has no machine-adapter capability",
    );
}

pub(super) fn artifacts(records: &Scanned, budget: &Budget, checks: &mut Checks) {
    for machine in &records.machines {
        if budget.expired() {
            checks.unknown(
                "host.artifacts",
                "Deadline expired while inspecting configured artifacts",
            );
            return;
        }
        if let Some(path) = machine.daemon() {
            if executable(Path::new(path)) {
                checks.pass(
                    "host.artifacts",
                    format!("Machine {} configured daemon is executable", machine.name()),
                );
            } else {
                checks.fail(
                    "host.artifacts",
                    format!(
                        "Machine {} configured daemon is missing or not executable",
                        machine.name()
                    ),
                );
            }
        }
    }
    let native_override = records
        .machines
        .iter()
        .any(|m| m.name() == "native" && m.daemon().is_some());
    if !native_override {
        default_native(checks);
    }
    let remote = records.machines.iter().any(|m| m.name() != "native")
        || records.sandboxes.iter().any(|s| s.machine() != "native")
        || records
            .stores
            .iter()
            .any(|s| s.machine().is_ok_and(|m| m != "native"));
    if !remote {
        checks.skip(
            "host.bundle",
            "No recorded remote machine requires a Linux bundle",
        );
        return;
    }
    let Ok(dir) = bundle_dir("linux") else {
        checks.unknown("host.bundle", "Linux bundle location could not be resolved");
        return;
    };
    let manifest = read_record(&dir.join("bundle.json"));
    let platform = format!("linux-{}", std::env::consts::ARCH);
    let valid = manifest.ok().flatten().is_some_and(|m| {
        m.get_text("version") == Some(env!("CARGO_PKG_VERSION"))
            && m.get_text("platform") == Some(platform.as_str())
            && m.get_text("hash")
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
    });
    if !valid {
        checks.fail(
            "host.bundle",
            "Applicable Linux bundle manifest is missing, malformed or incompatible",
        );
        return;
    }
    let binaries = ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"];
    if binaries.iter().all(|name| executable(&dir.join(name))) && dir.join(IMAGE_ARCHIVE).is_file()
    {
        checks.pass(
            "host.bundle",
            "Applicable bundle manifest and required executable/image files are present",
        );
        checks.skip("host.bundle_hashes", "Artifact content hashes were not recomputed; presence does not authenticate bundle bytes");
    } else {
        checks.fail(
            "host.bundle",
            "Applicable Linux bundle is missing required executable/image files",
        );
    }
}

/// Installation observations independent of an initialized current-layout home.
pub(super) fn without_inventory(checks: &mut Checks, blocked: bool) {
    default_native(checks);
    if blocked {
        checks.blocked(
            "host.artifacts",
            "Configured artifact applicability requires a stable readable record inventory",
            "host.records",
        );
        checks.blocked(
            "host.bundle",
            "Configured remote bundle applicability could not be established",
            "host.records",
        );
    } else {
        checks.skip("host.artifacts", "Configured artifact checks require a current-layout home; default installation was inspected independently");
        checks.skip(
            "host.bundle",
            "Configured remote bundle checks require a current-layout home",
        );
    }
}

fn default_native(checks: &mut Checks) {
    match bundle_dir(std::env::consts::OS) {
        Ok(dir) if executable(&dir.join("layerfs-daemon")) => checks.pass("host.native_daemon", "Default native daemon is executable"),
        _ => checks.warning("host.native_daemon", "Default native daemon is unavailable; configured overrides or remote operations may still be usable"),
    }
}
