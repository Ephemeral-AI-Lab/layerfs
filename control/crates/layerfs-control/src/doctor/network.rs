//! The Sandbox network checks: `machine.network` (the machine's firewall
//! table, IPv4 forwarding and `lfs*` links no Sandbox holds) and
//! `sandbox.network` (a running Sandbox's init stage holds a network of its
//! own and its host end is there with its recorded address). Report-only:
//! each finding names its fix, and nothing is removed, because on a Linux
//! host the network namespace is shared with the user's other homes and
//! machines, whose links this home cannot tell from leftovers. Read on the
//! side that runs the Sandboxes: a machine's worker (`network.inspect`), or
//! the caller for `native` on Linux.
use super::target::Checks;
use layerfs_bridge::control::SdkValue;
#[cfg(target_os = "linux")]
use layerfs_home::outcome::refused;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::record::SandboxRecord;
use std::path::Path;

/// The worker operation that answers [`report`].
pub(crate) const OP: &str = "network.inspect";
#[cfg(feature = "machine")]
const MACHINE: &str = "machine.network";
#[cfg(target_os = "linux")]
const SANDBOX: &str = "sandbox.network";

/// The worker's answer to [`OP`]: [`report`] of its own root.
pub(crate) fn inspect_in_worker(root: &Path) -> Outcome {
    report(root, None)
}

/// This kernel's Sandbox network as doctor reads it: `{ table, forwarding,
/// links: [{ link, addresses, sandbox }] }`, where `sandbox` names the
/// Sandbox recorded under `root` (on `machine` when given) whose runtime id
/// the link is named for, or is null.
#[cfg(target_os = "linux")]
pub(crate) fn report(root: &Path, machine: Option<&str>) -> Outcome {
    use layerfs_sandbox_backend::network::{inspect, plan::link_name};
    let seen = inspect().map_err(|error| {
        refused(
            "Failed",
            format!("this machine's Sandbox network could not be read: {error}"),
        )
    })?;
    let records = layerfs_sandbox::record::sandboxes(root).unwrap_or_default();
    let owners: Vec<(String, String)> = (records.iter())
        .filter(|record| machine.is_none_or(|machine| record.machine() == machine))
        .filter_map(|record| {
            let id = record.daemon()?.container()?;
            Some((link_name(id), record.name().to_owned()))
        })
        .collect();
    let links = (seen.links.iter())
        .map(|(link, addresses)| {
            let owner = owners.iter().find(|(name, _)| name == link);
            SdkValue::map([
                ("link", SdkValue::text(link)),
                (
                    "addresses",
                    SdkValue::List(
                        addresses
                            .iter()
                            .map(|a| SdkValue::text(a.to_string()))
                            .collect(),
                    ),
                ),
                ("sandbox", SdkValue::opt_text(owner.map(|(_, s)| s.clone()))),
            ])
        })
        .collect();
    Ok(SdkValue::map([
        ("table", SdkValue::Bool(seen.table)),
        (
            "forwarding",
            seen.forwarding.map_or(SdkValue::Null, SdkValue::Bool),
        ),
        ("links", SdkValue::List(links)),
    ]))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn report(_root: &Path, _machine: Option<&str>) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "Sandbox networks are Linux only",
    ))
}

/// `machine.network` from a [`report`] of machine `name`, on which
/// `sandboxes` are recorded; `host` when its network is a Linux host's,
/// shared with other homes and machines (`native`, a Linux Engine docker).
#[cfg(feature = "machine")]
pub(super) fn machine(
    report: &SdkValue,
    (name, sandboxes, host): (&str, usize, bool),
    checks: &mut Checks,
) {
    use layerfs_home::outcome::shell_word;
    let restart = format!(
        "layerfs machine stop {m} && layerfs machine start {m}",
        m = shell_word(name)
    );
    if report.get("forwarding") == Some(&SdkValue::Bool(false)) {
        checks.fail(
            MACHINE,
            "it forwards no IPv4 (net.ipv4.ip_forward=0), so no Sandbox can start on it",
        );
        checks.fix(
            MACHINE,
            match host {
                true => "sysctl -w net.ipv4.ip_forward=1 (a host setting LayerFS never changes)"
                    .to_owned(),
                false => restart,
            },
        );
        checks.evidence(MACHINE, report.clone());
        return;
    }
    if report.get("table") != Some(&SdkValue::Bool(true)) && sandboxes > 0 {
        checks.fail(
            MACHINE,
            "its firewall table inet layerfs is missing while Sandboxes are recorded on it: their \
             siblings are not kept apart and their traffic is not translated",
        );
        checks.fix(
            MACHINE,
            match host {
                true => "a Sandbox start writes the table again: `layerfs sandbox create` on it"
                    .to_owned(),
                false => restart,
            },
        );
        checks.evidence(MACHINE, report.clone());
        return;
    }
    let links = match report.get("links") {
        Some(SdkValue::List(links)) => links.as_slice(),
        _ => &[],
    };
    let left: Vec<&str> = (links.iter())
        .filter(|link| matches!(link.get("sandbox"), None | Some(SdkValue::Null)))
        .filter_map(|link| link.get_text("link"))
        .collect();
    if !left.is_empty() {
        let whose = match host {
            true => {
                "; on a Linux host they may be another home's or machine's, which this home \
                 cannot see"
            }
            false => "",
        };
        checks.warning(
            MACHINE,
            format!(
                "its Sandbox links {} belong to no Sandbox recorded here{whose}",
                left.join(", ")
            ),
        );
        let commands: Vec<String> = left
            .iter()
            .map(|link| format!("ip link delete {link}"))
            .collect();
        checks.fix(
            MACHINE,
            format!(
                "on the machine, as root, once nothing holds them: {}",
                commands.join(" && ")
            ),
        );
        checks.evidence(MACHINE, report.clone());
        return;
    }
    checks.pass(
        MACHINE,
        format!(
            "it forwards IPv4, {} its firewall table, and each of its {} Sandbox links belongs to a \
             recorded Sandbox",
            match report.get("table") == Some(&SdkValue::Bool(true)) {
                true => "has",
                false => "needs no",
            },
            links.len()
        ),
    );
    checks.evidence(MACHINE, report.clone());
}

/// `sandbox.network` of a running namespaces Sandbox on this kernel: its
/// init stage holds a network namespace of its own, and its host end is
/// there with the address its record keeps. Left out for a native one.
#[cfg(target_os = "linux")]
pub(super) fn sandbox(record: &SandboxRecord, checks: &mut Checks) {
    use layerfs_sandbox::state::remove_fix;
    use layerfs_sandbox_backend::network::{inspect, plan::link_name, private};
    let Some(daemon) = record.daemon() else {
        return;
    };
    if daemon.runtime() != Some("namespaces") {
        return;
    }
    let fix = remove_fix(record.name());
    let pid = daemon.pid().and_then(|pid| u32::try_from(pid).ok());
    let (Some(pid), Some(id)) = (pid, daemon.container()) else {
        checks.unknown(SANDBOX, "its record names no init process or runtime id");
        return;
    };
    match private(pid) {
        Ok(true) => {}
        Ok(false) => {
            checks.fail(
                SANDBOX,
                "its init stage holds no network of its own: an earlier build started it, and \
                 its commands are refused",
            );
            checks.fix(SANDBOX, fix);
            return;
        }
        Err(error) => {
            checks.unknown(
                SANDBOX,
                format!("its init stage's network could not be read: {error}"),
            );
            return;
        }
    }
    let link = link_name(id);
    let recorded = daemon.network().map(str::to_owned);
    let seen = match inspect() {
        Ok(seen) => seen,
        Err(error) => {
            checks.unknown(
                SANDBOX,
                format!("this machine's links could not be read: {error}"),
            );
            return;
        }
    };
    let Some((_, addresses)) = seen.links.iter().find(|(name, _)| *name == link) else {
        checks.fail(SANDBOX, format!("its host end {link} is missing"));
        checks.fix(SANDBOX, fix);
        return;
    };
    // the record keeps the guest's address; the host end holds the other
    // address of the same /31
    let matches = recorded
        .as_deref()
        .and_then(|line| line.split_once(' '))
        .is_some_and(|(name, guest)| {
            let guest = layerfs_sandbox_backend::network::plan::Ipv4Net::parse(guest);
            name == link
                && guest
                    .is_some_and(|guest| addresses.iter().any(|host| host.base() == guest.base()))
        });
    if !matches {
        checks.fail(
            SANDBOX,
            format!(
                "its host end {link} does not hold the network its record keeps ({})",
                recorded.as_deref().unwrap_or("none")
            ),
        );
        checks.fix(SANDBOX, fix);
        return;
    }
    checks.pass(
        SANDBOX,
        format!(
            "it holds a network of its own, joined by its host end {link} ({})",
            recorded.unwrap_or_default()
        ),
    );
}

#[cfg(not(target_os = "linux"))]
pub(super) fn sandbox(_record: &SandboxRecord, _checks: &mut Checks) {}
