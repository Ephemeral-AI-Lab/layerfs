//! The Sandbox network checks: `machine.network` (the machine's firewall
//! table, IPv4 forwarding and `lfs*` links no Sandbox holds) and
//! `sandbox.network` (a running Sandbox's init stage holds a network of its
//! own and its host end is there with its recorded address). Report-only:
//! each finding names its fix, and nothing is removed, because on a Linux
//! host the network namespace is shared with the user's other homes, whose
//! links this home cannot tell from leftovers. A link is this home's
//! leftover only when no Sandbox recorded in the home owns it, whichever
//! machine it is on: `native` and a Linux Engine `docker` share the host's
//! namespace, so each sees the other's links. Read on the side that runs the
//! Sandboxes: a machine's worker (`network.inspect`), or the caller for
//! `native` on Linux.
use super::target::Checks;
#[cfg(any(target_os = "linux", feature = "machine"))]
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
    report(root)
}

/// The host ends of the Sandboxes `records` keep, with each one's name:
/// the link its runtime id names and the one its recorded network names,
/// whichever machine it is on.
#[cfg(any(target_os = "linux", feature = "machine", test))]
pub(super) fn owned_links(records: &[SandboxRecord]) -> Vec<(String, String)> {
    use layerfs_sandbox_backend::network::plan::link_name;
    let mut owned = Vec::new();
    for record in records {
        let daemon = record.daemon();
        let named = (daemon.and_then(|d| d.container()).map(link_name))
            .into_iter()
            .chain(
                [record.network(), daemon.and_then(|d| d.network())]
                    .into_iter()
                    .flatten()
                    .filter_map(|line| line.split_whitespace().next().map(str::to_owned)),
            );
        for link in named {
            if !owned.iter().any(|(seen, _)| *seen == link) {
                owned.push((link, record.name().to_owned()));
            }
        }
    }
    owned
}

/// [`owned_links`] of every Sandbox recorded in `home`, on any machine. A
/// record that cannot be read refuses it, naming why: no link is then told
/// a leftover (its delete fix would cut a live Sandbox's network).
#[cfg(any(target_os = "linux", feature = "machine"))]
pub(super) fn owned_in(home: &Path) -> Result<Vec<(String, String)>, String> {
    match layerfs_sandbox::record::sandboxes(home) {
        Ok(records) => Ok(owned_links(&records)),
        Err(refusal) => Err(format!(
            "this home's Sandbox records could not be read, so no Sandbox link can be told a \
             leftover: {}",
            refusal.message
        )),
    }
}

/// This kernel's Sandbox network as doctor reads it: `{ table, forwarding,
/// links: [{ link, addresses, sandbox }] }`, where `sandbox` names the
/// Sandbox recorded under `root`, on any machine, whose host end the link
/// is, or is null.
#[cfg(target_os = "linux")]
pub(crate) fn report(root: &Path) -> Outcome {
    use layerfs_sandbox_backend::network::inspect;
    let seen = inspect().map_err(|error| {
        refused(
            "Failed",
            format!("this machine's Sandbox network could not be read: {error}"),
        )
    })?;
    let owners = owned_in(root).map_err(|why| refused("Failed", why))?;
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
pub(crate) fn report(_root: &Path) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "Sandbox networks are Linux only",
    ))
}

/// `machine.network` from a [`report`] of machine `name`, on which
/// `sandboxes` are recorded; `host` when its network is a Linux host's,
/// shared with other homes and machines (`native`, a Linux Engine docker).
/// A link the report names no owner for is still this home's when `owned`
/// ([`owned_links`] of every Sandbox the home records, on any machine)
/// holds it: a worker reads only its own machine's records.
#[cfg(feature = "machine")]
pub(super) fn machine(
    report: &SdkValue,
    (name, sandboxes, host): (&str, usize, bool),
    owned: &[(String, String)],
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
        .filter(|link| !owned.iter().any(|(name, _)| name == link))
        .collect();
    if !left.is_empty() {
        let whose = match host {
            true => "; on a Linux host they may be another home's, which this home cannot see",
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
             Sandbox recorded in this home",
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

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_bridge::control::SdkValue;

    /// A Sandbox record on `machine` whose recorded network is `link`'s.
    fn record(name: &str, machine: &str, link: &str) -> SandboxRecord {
        SandboxRecord::from_record(
            name,
            SdkValue::map([
                ("name", SdkValue::text(name)),
                ("machine", SdkValue::text(machine)),
                ("network", SdkValue::text(format!("{link} 10.213.0.1/31"))),
            ]),
        )
    }

    #[test]
    fn a_link_is_owned_by_a_sandbox_of_the_home_on_any_machine() {
        // on one Linux host, `native` and a Linux Engine `docker` share the
        // host's network namespace: each one's doctor sees the other's link
        let records = [
            record("on-native", "native", "lfs0123456789ab"),
            record("on-docker", "docker", "lfsba9876543210"),
        ];
        let owned = owned_links(&records);
        assert!(
            owned.contains(&("lfs0123456789ab".to_owned(), "on-native".to_owned())),
            "{owned:?}"
        );
        assert!(
            owned.contains(&("lfsba9876543210".to_owned(), "on-docker".to_owned())),
            "{owned:?}"
        );
    }

    /// One Sandbox record that cannot be read leaves no owner list: the
    /// home's owners are refused, naming the read, so no live link is
    /// reported a leftover with a delete fix.
    #[cfg(any(target_os = "linux", feature = "machine"))]
    #[test]
    fn an_unreadable_record_leaves_no_owner_list() {
        use layerfs_home::home::{self, paths, write_entity};
        let home = std::env::temp_dir().join(format!("lfs-owned-in-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        home::ensure(&home).unwrap();
        let good = record("good", "docker", "lfsba9876543210");
        write_entity(&paths::sandbox_record(&home, "good"), &good.to_record()).unwrap();
        assert_eq!(
            owned_in(&home).unwrap(),
            [("lfsba9876543210".to_owned(), "good".to_owned())]
        );
        let unreadable = paths::sandbox_record(&home, "unreadable");
        std::fs::create_dir_all(unreadable.parent().unwrap()).unwrap();
        std::fs::write(&unreadable, "{not json").unwrap();
        let why = owned_in(&home).unwrap_err();
        assert!(
            why.starts_with("this home's Sandbox records could not be read"),
            "{why}"
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[cfg(feature = "machine")]
    #[test]
    fn machine_network_names_only_links_no_sandbox_of_the_home_owns() {
        let link = |name: &str| {
            SdkValue::map([
                ("link", SdkValue::text(name)),
                ("addresses", SdkValue::List(Vec::new())),
                ("sandbox", SdkValue::Null),
            ])
        };
        // the native machine's report: docker's live link has no owner there
        let report = SdkValue::map([
            ("table", SdkValue::Bool(true)),
            ("forwarding", SdkValue::Bool(true)),
            (
                "links",
                SdkValue::List(vec![link("lfsba9876543210"), link("lfs00000000000f")]),
            ),
        ]);
        let owned = owned_links(&[record("on-docker", "docker", "lfsba9876543210")]);
        let mut checks = Checks::default();
        machine(&report, ("native", 0, true), &owned, &mut checks);
        let printed = layerfs_home::json::print(&checks.to_wire());
        assert_eq!(
            checks.outcome(MACHINE),
            Some(("fail", Some("warning"))),
            "{printed}"
        );
        assert!(printed.contains("lfs00000000000f"), "{printed}");
        assert!(
            !printed.contains("ip link delete lfsba9876543210"),
            "{printed}"
        );
        // with both owned, it passes
        let owned = [
            owned,
            owned_links(&[record("on-native", "native", "lfs00000000000f")]),
        ]
        .concat();
        let mut checks = Checks::default();
        machine(&report, ("native", 2, true), &owned, &mut checks);
        let printed = layerfs_home::json::print(&checks.to_wire());
        assert_eq!(
            checks.outcome(MACHINE).map(|(o, _)| o),
            Some("pass"),
            "{printed}"
        );
    }
}
