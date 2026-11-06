//! The build of each running machine's worker: a machine whose
//! `worker.json` is recorded is dialed for its hello alone (its secret
//! answered with its protocol and bundle); no request is sent, and nothing
//! is started. A worker of another build, or one that does not answer, is
//! a warning naming the calls that fix it.
use super::report::Findings;
use super::scan::Scanned;
use crate::machine::worker::{self, Endpoint};
use layerfs_home::home::{paths, read_record};
use std::path::Path;

/// Asks each recorded worker which build it is.
pub(super) fn check(home: &Path, scanned: &Scanned, findings: &mut Findings) {
    for machine in &scanned.machines {
        let name = machine.name();
        let entity = ("machine", name);
        // a worker.json that does not parse is the scan's finding
        let Ok(Some(recorded)) = read_record(&paths::worker_record(home, name)) else {
            continue;
        };
        let replace = worker::replace_with(name);
        let Some(address) = (recorded.get_text("address")).and_then(|a| a.parse().ok()) else {
            findings.error(
                entity,
                format!("machine {name}'s worker.json names no worker address"),
                replace,
            );
            continue;
        };
        let endpoint = Endpoint {
            address,
            secret: recorded.get_text("secret").unwrap_or("").to_owned(),
        };
        match worker::hello(&endpoint) {
            Ok(hello) => {
                if let Some(expected) = hello.expected() {
                    findings.warning(
                        entity,
                        format!(
                            "machine {name}'s worker is of another build ({}; this build: {}); calls to it are refused",
                            hello.describe(),
                            expected.describe()
                        ),
                        format!("{replace}; if the start says the Linux bundle was sealed before this build, rebuild it (`python3 tools/linux_bundle.py`) first"),
                    );
                }
            }
            Err(refusal) => findings.warning(
                entity,
                format!(
                    "machine {name}'s recorded worker does not answer ({})",
                    refusal.message
                ),
                format!("`layerfs machine start {name}` starts it again; `layerfs machine stop {name}` records it stopped"),
            ),
        }
    }
}
