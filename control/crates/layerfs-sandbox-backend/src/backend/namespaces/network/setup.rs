//! A Sandbox's network as its parts do it: the init stage plans a slot and
//! wires the pair (`plan`, `connect`, `configure`), the launcher checks the
//! machine and writes the table (`forwarding`, `forward_policy`, `pool`,
//! `table`, `prepare`), and doctor reads what is there (`inspect`,
//! `private`). Every call acts once.
use super::links::Routes;
use super::nftables as nft;
use crate::network::pool::{self as plan, Ipv4Net};
use crate::RuntimeError;
use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::os::fd::BorrowedFd;
use std::os::unix::fs::MetadataExt;

/// The guest end's name inside the Sandbox's namespace.
pub const GUEST: &str = "eth0";
/// Each Sandbox's prefix: one /31, host end and guest.
pub const PREFIX: u8 = 31;
const IP_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";

/// One Sandbox's pair: its host end's name, the two addresses and the MTU.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Wiring {
    pub link: String,
    pub host: Ipv4Addr,
    pub guest: Ipv4Addr,
    pub mtu: u32,
}

impl Wiring {
    /// The line the init stage writes to `<sandbox>/network` and the record
    /// keeps: `<link> <guest>/31`.
    pub fn line(&self) -> String {
        format!("{} {}/{PREFIX}", self.link, self.guest)
    }
}

/// Under the lock, in the machine's namespace (`machine`): the pool must
/// still be `pool` and the link `link` must not exist; the lowest free slot.
pub fn plan(machine: &mut Routes, pool: Ipv4Net, link: &str) -> Result<Wiring, RuntimeError> {
    let (facts, mtu) = machine.facts()?;
    let now = plan::pool(&facts).map_err(RuntimeError::Unavailable)?;
    if now != pool {
        return Err(RuntimeError::Unavailable(format!(
            "this machine's Sandbox pool is {now}, not {pool} as when this start was checked"
        )));
    }
    if facts.links.iter().any(|(name, _)| name == link) {
        return Err(RuntimeError::Unavailable(format!(
            "the Sandbox link {link} already exists on this machine"
        )));
    }
    let (host, guest) = plan::slot(pool, &facts).map_err(RuntimeError::Unavailable)?;
    Ok(Wiring {
        link: link.to_owned(),
        host,
        guest,
        mtu,
    })
}

/// After the caller entered the Sandbox's new namespace (`sandbox`): the
/// pair, its guest end made directly in that namespace, and the host end's
/// address, with no IPv6 link-local address, up. `machine` was opened in
/// the machine's namespace.
pub fn connect(machine: &mut Routes, wiring: &Wiring, sandbox: BorrowedFd<'_>) -> io::Result<()> {
    machine.veth(&wiring.link, GUEST, sandbox, wiring.mtu)?;
    let index = machine.index(&wiring.link)?;
    machine.no_link_local(index)?;
    machine.address(index, wiring.host, PREFIX)?;
    machine.up(index)
}

/// Inside the Sandbox's namespace: `lo` up, the guest end's address with
/// IPv6 off on it, up, the default route via the host end, and ICMP echo
/// sockets for every group (a new namespace allows them to none).
pub fn configure(wiring: &Wiring) -> io::Result<()> {
    let mut routes = Routes::open()?;
    let lo = routes.index("lo")?;
    routes.up(lo)?;
    let guest = routes.index(GUEST)?;
    match fs::write(format!("/proc/sys/net/ipv6/conf/{GUEST}/disable_ipv6"), "1") {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        done => done?,
    }
    routes.address(guest, wiring.guest, PREFIX)?;
    routes.up(guest)?;
    routes.default_route(wiring.host, guest)?;
    fs::write("/proc/sys/net/ipv4/ping_group_range", "0 2147483647")
}

/// Refused unless this namespace forwards IPv4: Sandboxes route through
/// it, and LayerFS never writes the setting where it does not own the
/// namespace (a Linux host).
pub fn forwarding() -> Result<(), RuntimeError> {
    let value = fs::read_to_string(IP_FORWARD)?;
    match value.trim() {
        "1" => Ok(()),
        _ => Err(RuntimeError::Unavailable(
            "this machine forwards no IPv4 (net.ipv4.ip_forward=0), and every Sandbox's commands \
             reach the network through it; LayerFS does not change this setting on a Linux \
             host: set net.ipv4.ip_forward=1 (Docker hosts normally have it on). On a lima or \
             apple-container machine, `layerfs machine stop` and `layerfs machine start` set it"
                .to_owned(),
        )),
    }
}

/// Refused when another table's forward chain would drop every Sandbox's
/// traffic (nft::dropping_forward), naming it and the rules that admit
/// the host ends.
pub fn forward_policy() -> Result<(), RuntimeError> {
    let dropping = nft::dropping_forward()?;
    let Some(first) = dropping.first() else {
        return Ok(());
    };
    Err(RuntimeError::Unavailable(format!(
        "the forward chain {} of table {} {} drops what it does not accept and accepts nothing \
         from or to LayerFS's Sandbox links (lfs*), so every Sandbox's traffic would be dropped; \
         LayerFS does not change another table: admit them, for example on a Docker host with \
         `iptables -I DOCKER-USER -i lfs+ -j ACCEPT` and `iptables -I DOCKER-USER -o lfs+ -m \
         conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT` (or the same in chain FORWARD)",
        first.chain, first.family, first.table
    )))
}

/// This namespace's Sandbox pool (`plan::pool`), refused on a clash.
pub fn pool() -> Result<Ipv4Net, RuntimeError> {
    let (facts, _) = Routes::open()?.facts()?;
    plan::pool(&facts).map_err(RuntimeError::Unavailable)
}

/// Writes the table for `pool` with `closed` ports, in one transaction.
pub fn table(pool: Ipv4Net, closed: &[u16]) -> Result<(), RuntimeError> {
    nft::write(pool, closed).map_err(|error| {
        RuntimeError::Unavailable(format!(
            "the Sandboxes' firewall table inet {} could not be written on this machine \
             (its kernel needs nf_tables with inet NAT and masquerade): {error}",
            nft::TABLE
        ))
    })
}

/// The pool, and the table written for it with `closed` ports: what a
/// worker does when it starts.
pub fn prepare(closed: &[u16]) -> Result<Ipv4Net, RuntimeError> {
    let pool = pool()?;
    table(pool, closed)?;
    Ok(pool)
}

/// What doctor reads of this namespace's Sandbox network.
#[derive(Clone, Debug, Default)]
pub struct Inspection {
    pub table: bool,
    pub links: Vec<(String, Vec<Ipv4Net>)>,
    pub forwarding: Option<bool>,
}

/// This namespace's table, `lfs*` links and forwarding setting.
pub fn inspect() -> io::Result<Inspection> {
    let (facts, _) = Routes::open()?.facts()?;
    let forwarding = fs::read_to_string(IP_FORWARD)
        .ok()
        .map(|value| value.trim() == "1");
    Ok(Inspection {
        table: nft::exists()?,
        links: facts.links,
        forwarding,
    })
}

/// Whether process `pid` is in another network namespace than this
/// process: a Sandbox's init stage that holds its own network.
pub fn private(pid: u32) -> io::Result<bool> {
    let theirs = fs::metadata(format!("/proc/{pid}/ns/net"))?;
    let ours = fs::metadata("/proc/self/ns/net")?;
    Ok((theirs.dev(), theirs.ino()) != (ours.dev(), ours.ino()))
}
