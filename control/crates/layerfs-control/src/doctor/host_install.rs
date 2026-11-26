//! The install's own checks, without deployment: this program
//! (`home.build`), what a Sandbox on this computer needs
//! (`home.prerequisites`), the Linux bundle where a recorded machine needs
//! it (`home.bundle`), the native daemon (`home.native_daemon`) and each
//! machine's configured daemon file (`home.artifacts`).
use super::super::checks::daemon_fix;
use super::super::scan::{shell, Scanned};
use super::{executable, Checks};
use crate::home::{Build, NativeDaemon};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::read_record;
use layerfs_home::names::HOST_MACHINE;
use layerfs_machine::{bundle_dir, IMAGE_ARCHIVE};
use std::path::Path;

/// `home.build`: the calling program exists and can run.
pub(super) fn build(home: &Path, checks: &mut Checks) {
    const ID: &str = "home.build";
    match std::env::current_exe() {
        Ok(path) if executable(&path) => checks.pass(ID, "this program exists and can run"),
        _ => {
            checks.unknown(ID, "this program's own file could not be inspected");
            return;
        }
    }
    checks.evidence(
        ID,
        SdkValue::map([
            ("home", SdkValue::text(home.display().to_string())),
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("os", SdkValue::text(std::env::consts::OS)),
            ("arch", SdkValue::text(std::env::consts::ARCH)),
        ]),
    );
}

/// `home.prerequisites`: what a Sandbox on this computer needs is
/// installed. A build without machine adapters has nothing to say: the
/// check does not apply.
pub(super) fn prerequisites(checks: &mut Checks) {
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::platform::{native_missing, HostOs};
        const ID: &str = "home.prerequisites";
        let missing = native_missing(HostOs::current());
        if missing.is_empty() {
            checks.pass(ID, "what a Sandbox on this computer needs is installed");
        } else {
            checks.warning(
                ID,
                format!(
                    "a Sandbox on this computer needs {}, which is not installed",
                    missing.join(", ")
                ),
            );
            // no fix: the host's setup is not a layerfs call
            let missing = missing.iter().map(SdkValue::text).collect();
            checks.evidence(ID, SdkValue::map([("missing", SdkValue::List(missing))]));
        }
    }
    #[cfg(not(feature = "machine"))]
    let _ = checks;
}

/// `home.bundle`: the Linux bundle is present and complete, made only
/// when a recorded machine needs it: one whose calls run in its worker
/// (its platform's executor on this host), or one whose record cannot say.
pub(super) fn bundle(home: &Path, records: &Scanned, checks: &mut Checks) {
    const ID: &str = "home.bundle";
    let worker = |machine: &str| !layerfs_machine::runs_here(home, machine).unwrap_or(false);
    let remote = records.machines.iter().any(|m| worker(m.name()))
        || records.sandboxes.iter().any(|s| worker(s.machine()))
        || records
            .stores
            .iter()
            .any(|s| s.machine().is_ok_and(|m| worker(&m)));
    if !remote {
        return;
    }
    let Ok(dir) = bundle_dir("linux") else {
        checks.unknown(ID, "the Linux bundle's folder could not be found");
        return;
    };
    let manifest = read_record(&dir.join("bundle.json"));
    let platform = format!("linux-{}", std::env::consts::ARCH);
    let word = match &manifest {
        Ok(None) => "missing",
        Err(_) => "unreadable",
        Ok(Some(_)) => "invalid",
    };
    let found = (manifest.as_ref().ok().cloned().flatten())
        .and_then(|m| m.get_text("version").map(str::to_owned));
    let valid = manifest.ok().flatten().is_some_and(|m| {
        m.get_text("version") == Some(env!("CARGO_PKG_VERSION"))
            && m.get_text("platform") == Some(platform.as_str())
            && m.get_text("hash")
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
    });
    let shown = dir.display();
    if !valid {
        checks.fail(
            ID,
            format!("the Linux bundle at {shown} is missing, malformed or of another build; install this build's bundle there"),
        );
        // no fix: no call installs a bundle, it ships with the install
        checks.evidence(
            ID,
            SdkValue::map([
                ("path", SdkValue::text(dir.display().to_string())),
                ("platform", SdkValue::text(&platform)),
                (
                    "expected_version",
                    SdkValue::text(env!("CARGO_PKG_VERSION")),
                ),
                ("found_version", SdkValue::opt_text(found)),
                ("manifest", SdkValue::text(word)),
            ]),
        );
        return;
    }
    let binaries = ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"];
    let lacking: Vec<SdkValue> = (binaries.iter().copied())
        .chain([IMAGE_ARCHIVE])
        .filter(|name| match *name == IMAGE_ARCHIVE {
            true => !dir.join(name).is_file(),
            false => !executable(&dir.join(name)),
        })
        .map(SdkValue::text)
        .collect();
    if lacking.is_empty() {
        checks.pass(ID, format!("the Linux bundle at {shown} is complete"));
    } else {
        checks.fail(
            ID,
            format!("the Linux bundle at {shown} lacks a program or its image; install this build's bundle there"),
        );
        // no fix: no call installs a bundle
        checks.evidence(
            ID,
            SdkValue::map([
                ("path", SdkValue::text(dir.display().to_string())),
                ("missing", SdkValue::List(lacking)),
            ]),
        );
    }
}

/// `home.artifacts`: each machine's configured daemon file (other than
/// `native`'s, which is `home.native_daemon`'s) is present and can run. A
/// home whose machines configure none has nothing to check.
pub(super) fn artifacts(records: &Scanned, budget: &super::Budget, checks: &mut Checks) {
    const ID: &str = "home.artifacts";
    for machine in records
        .machines
        .iter()
        .filter(|m| m.name() != layerfs_home::names::HOST_MACHINE)
    {
        if budget.expired() {
            checks.timeout(ID, "its time ran out while the daemon files were read");
            return;
        }
        let Some(path) = machine.daemon() else {
            continue;
        };
        let name = machine.name();
        let file = Path::new(path);
        if executable(file) {
            checks.pass(ID, format!("machine {name}'s daemon {path} can run"));
        } else if file.is_file() {
            checks.fail(
                ID,
                format!("machine {name}'s daemon {path} is not executable"),
            );
            checks.fix(ID, format!("chmod +x {}", shell(file)));
            artifact(checks, name, path, true);
        } else {
            checks.fail(
                ID,
                format!("machine {name}'s daemon {path} is not there; record another with `layerfs machine update {name} --daemon <path>`"),
            );
            // no fix: the daemon of a machine that is not `native` is the
            // one the user keeps for it; no file of this build is known
            artifact(checks, name, path, false);
        }
    }
}

/// The evidence of a configured daemon file that cannot run: `present` is
/// whether the file is there.
fn artifact(checks: &mut Checks, machine: &str, path: &str, present: bool) {
    let mut fields = vec![
        ("machine", SdkValue::text(machine)),
        ("path", SdkValue::text(path)),
    ];
    match present {
        true => fields.push(("executable", SdkValue::Bool(false))),
        false => fields.push(("present", SdkValue::Bool(false))),
    }
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    checks.evidence("home.artifacts", SdkValue::Map(fields));
}

/// `home.native_daemon`: the daemon the native machine runs (its recorded
/// one, else the default beside this host's bundle) is present and this
/// build's, in `home.get`'s words: missing is an error when recorded and a
/// warning for the default (remote machines may still be usable); another
/// commit is a warning (`DaemonMismatch`); a commit it cannot compare
/// passes, saying so.
pub(super) fn native_daemon(home: &Path, build: Build<'_>, checks: &mut Checks) {
    const ID: &str = "home.native_daemon";
    let Some(daemon) = NativeDaemon::of(home) else {
        checks.unknown(
            ID,
            "the host machine's daemon location could not be resolved",
        );
        return;
    };
    let shown = daemon.path.display().to_string();
    let matched = daemon.matches(build);
    let (commit, dirty) = match &daemon.stamp {
        Some((commit, dirty)) => (SdkValue::text(commit), SdkValue::Bool(*dirty)),
        None => (SdkValue::Null, SdkValue::Null),
    };
    let evidence = SdkValue::map([
        ("path", SdkValue::text(&shown)),
        ("source", SdkValue::text(daemon.source)),
        ("commit", commit),
        ("dirty", dirty),
        ("matches", matched.clone()),
    ]);
    if daemon.present && !executable(&daemon.path) {
        let message = format!(
            "the host machine's daemon at {shown} is not executable; `chmod +x` it or record another: `layerfs machine update {HOST_MACHINE} --daemon <path>`"
        );
        match daemon.source {
            "recorded" => checks.fail(ID, message),
            _ => checks.warning(ID, message),
        }
        checks.fix(ID, format!("chmod +x {}", shell(&daemon.path)));
    } else if !daemon.present {
        match daemon.source {
            "recorded" => checks.fail(
                ID,
                layerfs_sandbox::deploy::missing_daemon(layerfs_home::names::HOST_MACHINE, &shown),
            ),
            _ => checks.warning(ID, daemon.missing()),
        }
        if let Some(fix) = daemon_fix(layerfs_home::names::HOST_MACHINE, build) {
            checks.fix(ID, fix);
        }
    } else if let Some(message) = daemon.mismatch(build) {
        checks.warning(ID, message);
        if let Some(fix) = daemon_fix(layerfs_home::names::HOST_MACHINE, build) {
            checks.fix(ID, fix);
        }
    } else if matched == SdkValue::Bool(true) {
        checks.pass(
            ID,
            format!("the host machine's daemon at {shown} is this build's"),
        );
    } else {
        checks.pass(
            ID,
            format!("the host machine's daemon at {shown} is executable; its build cannot be compared with this one (a commit unknown or a tree dirty)"),
        );
    }
    checks.evidence(ID, evidence);
}
