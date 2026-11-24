//! The pool and slot of a Sandbox's network, chosen from what the machine's
//! network namespace already holds. Pure: the Linux side reads the facts
//! (`links`, `setup`) and these functions decide, so every rule is
//! tested on any OS.
//!
//! - The pool is one private IPv4 /16. While `lfs*` links exist in the
//!   namespace, it is the /16 their addresses are in (all must agree); with
//!   none, it is the first of [`CANDIDATES`] that overlaps no route (the
//!   default route excepted) and no address of the namespace. A pool that
//!   now overlaps another route or address is refused, naming the clash:
//!   nothing is renumbered.
//! - A slot is one /31 of the pool: the host end `.2k`, the guest `.2k+1`,
//!   the lowest `k` whose host address no `lfs*` link holds. The address on
//!   the link is the allocation; the kernel frees it with the link.
use std::fmt;
use std::net::Ipv4Addr;

/// The host end's name prefix, which the firewall matches (`lfs*`).
pub const LINK_PREFIX: &str = "lfs";
/// How many hex digits of the Sandbox's id follow the prefix: 3 + 12 is
/// the kernel's 15-byte interface name limit.
pub const LINK_ID_DIGITS: usize = 12;

/// An IPv4 network: an address and a prefix length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Net {
    pub address: Ipv4Addr,
    pub prefix: u8,
}

impl Ipv4Net {
    /// `address/prefix`, refused when `prefix` is over 32.
    pub fn new(address: Ipv4Addr, prefix: u8) -> Option<Self> {
        (prefix <= 32).then_some(Self { address, prefix })
    }

    /// `a.b.c.d/n` text.
    pub fn parse(text: &str) -> Option<Self> {
        let (address, prefix) = text.split_once('/')?;
        Self::new(address.parse().ok()?, prefix.parse().ok()?)
    }

    fn mask(prefix: u8) -> u32 {
        match prefix {
            0 => 0,
            n => u32::MAX << (32 - u32::from(n)),
        }
    }

    /// The network's first address.
    pub fn network(&self) -> Ipv4Addr {
        Ipv4Addr::from(u32::from(self.address) & Self::mask(self.prefix))
    }

    /// The same network with its host bits cleared.
    pub fn base(&self) -> Self {
        Self {
            address: self.network(),
            prefix: self.prefix,
        }
    }

    /// Whether `address` is in this network.
    pub fn contains(&self, address: Ipv4Addr) -> bool {
        let mask = Self::mask(self.prefix);
        u32::from(address) & mask == u32::from(self.address) & mask
    }

    /// Whether the two networks share an address.
    pub fn overlaps(&self, other: &Self) -> bool {
        let prefix = self.prefix.min(other.prefix);
        let mask = Self::mask(prefix);
        u32::from(self.address) & mask == u32::from(other.address) & mask
    }
}

impl fmt::Display for Ipv4Net {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

/// The pools tried in order when no `lfs*` link exists yet.
pub const CANDIDATES: [(Ipv4Addr, u8); 5] = [
    (Ipv4Addr::new(10, 213, 0, 0), 16),
    (Ipv4Addr::new(10, 214, 0, 0), 16),
    (Ipv4Addr::new(10, 215, 0, 0), 16),
    (Ipv4Addr::new(172, 30, 0, 0), 16),
    (Ipv4Addr::new(172, 31, 0, 0), 16),
];

/// What a network namespace holds, as the planner needs it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Facts {
    /// Each `lfs*` link: its name and its IPv4 addresses.
    pub links: Vec<(String, Vec<Ipv4Net>)>,
    /// Every other IPv4 route but the default one, as networks.
    pub routes: Vec<Ipv4Net>,
    /// Every other IPv4 address, as the network it is in.
    pub addresses: Vec<Ipv4Net>,
}

/// The /16 the namespace's Sandboxes use (see the module docs).
pub fn pool(facts: &Facts) -> Result<Ipv4Net, String> {
    let mut used: Option<(Ipv4Net, &str)> = None;
    for (name, addresses) in &facts.links {
        for address in addresses {
            let pool = Ipv4Net {
                address: address.address,
                prefix: 16,
            }
            .base();
            match used {
                None => used = Some((pool, name)),
                Some((seen, _)) if seen == pool => {}
                Some((seen, other)) => {
                    return Err(format!(
                        "the Sandbox links {other} ({seen}) and {name} ({pool}) of this network \
                         namespace are in different pools, which LayerFS never makes; remove \
                         the one no Sandbox holds"
                    ))
                }
            }
        }
    }
    if let Some((pool, _)) = used {
        return match clash(&pool, facts) {
            Some(what) => Err(format!(
                "the Sandboxes' pool {pool} now overlaps {what} on this machine; LayerFS does \
                 not renumber running Sandboxes: remove that route or address, or remove the \
                 machine's Sandboxes first"
            )),
            None => Ok(pool),
        };
    }
    for (address, prefix) in CANDIDATES {
        let pool = Ipv4Net { address, prefix };
        if clash(&pool, facts).is_none() {
            return Ok(pool);
        }
    }
    let taken: Vec<String> = CANDIDATES
        .iter()
        .filter_map(|(address, prefix)| {
            let pool = Ipv4Net {
                address: *address,
                prefix: *prefix,
            };
            clash(&pool, facts).map(|what| format!("{pool} by {what}"))
        })
        .collect();
    Err(format!(
        "no private network is free for Sandboxes on this machine: {}",
        taken.join(", ")
    ))
}

/// The first route or address `pool` overlaps, in words.
fn clash(pool: &Ipv4Net, facts: &Facts) -> Option<String> {
    if let Some(route) = facts.routes.iter().find(|route| route.overlaps(pool)) {
        return Some(format!("the route {route}"));
    }
    (facts.addresses.iter())
        .find(|address| address.overlaps(pool))
        .map(|address| format!("the address {address}"))
}

/// The lowest free /31 of `pool`: its host end and guest addresses.
pub fn slot(pool: Ipv4Net, facts: &Facts) -> Result<(Ipv4Addr, Ipv4Addr), String> {
    let held: Vec<Ipv4Addr> = (facts.links.iter())
        .flat_map(|(_, addresses)| addresses.iter().map(|net| net.address))
        .collect();
    let base = u32::from(pool.network());
    let size = 1u64 << (32 - u32::from(pool.prefix));
    let mut offset = 0u64;
    while offset + 1 < size {
        let host = Ipv4Addr::from(base + offset as u32);
        if !held.contains(&host) {
            return Ok((host, Ipv4Addr::from(base + offset as u32 + 1)));
        }
        offset += 2;
    }
    Err(format!(
        "every address of the Sandboxes' pool {pool} is in use on this machine"
    ))
}

/// The host end's name for the Sandbox whose id is `id`: `lfs` and the
/// first [`LINK_ID_DIGITS`] characters of the id.
pub fn link_name(id: &str) -> String {
    let digits: String = id.chars().take(LINK_ID_DIGITS).collect();
    format!("{LINK_PREFIX}{digits}")
}

/// Whether `name` is a Sandbox host end's.
pub fn is_link(name: &str) -> bool {
    name.starts_with(LINK_PREFIX)
}

/// The line an init stage writes to its Sandbox's log when the machine
/// cannot give the Sandbox its network after the launcher checked it (the
/// pool changed or now clashes, or the link exists): the launcher then
/// answers the start `Unavailable` with the rest of the line (spec §3.2),
/// not as a stage that failed.
pub const UNAVAILABLE: &str = "LAYERFS_SANDBOX_UNAVAILABLE ";

/// The reason an init stage's log `log` gives on an [`UNAVAILABLE`] line.
pub fn unavailable_in(log: &str) -> Option<String> {
    (log.lines())
        .find_map(|line| line.strip_prefix(UNAVAILABLE))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(text: &str) -> Ipv4Net {
        Ipv4Net::parse(text).unwrap()
    }

    #[test]
    fn the_first_free_candidate_is_the_pool() {
        let facts = Facts {
            routes: vec![net("192.168.5.0/24")],
            addresses: vec![net("192.168.5.15/24")],
            ..Facts::default()
        };
        assert_eq!(pool(&facts).unwrap(), net("10.213.0.0/16"));
        // a route inside the first candidate skips it
        let facts = Facts {
            routes: vec![net("10.213.4.0/24")],
            addresses: vec![net("10.214.0.9/8")],
            ..Facts::default()
        };
        // 10.214.0.9/8 covers all of 10/8: both 10.x candidates clash
        assert_eq!(pool(&facts).unwrap(), net("172.30.0.0/16"));
    }

    #[test]
    fn no_free_candidate_is_refused_naming_what_is_in_the_way() {
        let facts = Facts {
            routes: vec![net("10.0.0.0/8"), net("172.16.0.0/12")],
            ..Facts::default()
        };
        let error = pool(&facts).unwrap_err();
        assert!(
            error.contains("10.213.0.0/16 by the route 10.0.0.0/8"),
            "{error}"
        );
        assert!(
            error.contains("172.31.0.0/16 by the route 172.16.0.0/12"),
            "{error}"
        );
    }

    #[test]
    fn the_pool_of_existing_links_wins_and_a_clash_is_refused() {
        let links = vec![("lfs0123456789ab".to_owned(), vec![net("172.30.0.2/31")])];
        let facts = Facts {
            links: links.clone(),
            ..Facts::default()
        };
        // 10.213/16 would be free, but the links say 172.30/16
        assert_eq!(pool(&facts).unwrap(), net("172.30.0.0/16"));
        let facts = Facts {
            links,
            routes: vec![net("172.30.128.0/17")],
            ..Facts::default()
        };
        let error = pool(&facts).unwrap_err();
        assert!(error.contains("172.30.0.0/16"), "{error}");
        assert!(error.contains("the route 172.30.128.0/17"), "{error}");
        assert!(error.contains("does not renumber"), "{error}");
    }

    #[test]
    fn links_in_two_pools_are_refused() {
        let facts = Facts {
            links: vec![
                ("lfsa".to_owned(), vec![net("10.213.0.0/31")]),
                ("lfsb".to_owned(), vec![net("10.214.0.0/31")]),
            ],
            ..Facts::default()
        };
        let error = pool(&facts).unwrap_err();
        assert!(error.contains("lfsa") && error.contains("lfsb"), "{error}");
    }

    #[test]
    fn the_lowest_free_slot_is_taken_and_reused_once_its_link_is_gone() {
        let pool = net("10.213.0.0/16");
        let empty = Facts::default();
        let first = slot(pool, &empty).unwrap();
        assert_eq!(
            first,
            (Ipv4Addr::new(10, 213, 0, 0), Ipv4Addr::new(10, 213, 0, 1))
        );
        let two = Facts {
            links: vec![
                ("lfsa".to_owned(), vec![net("10.213.0.0/31")]),
                ("lfsb".to_owned(), vec![net("10.213.0.2/31")]),
            ],
            ..Facts::default()
        };
        assert_eq!(slot(pool, &two).unwrap().0, Ipv4Addr::new(10, 213, 0, 4));
        // the first link is gone: its slot is the lowest free one again
        let one = Facts {
            links: vec![("lfsb".to_owned(), vec![net("10.213.0.2/31")])],
            ..Facts::default()
        };
        assert_eq!(slot(pool, &one).unwrap(), first);
        // a full pool is refused
        let tiny = net("10.213.0.0/30");
        let full = Facts {
            links: vec![(
                "lfsa".to_owned(),
                vec![net("10.213.0.0/31"), net("10.213.0.2/31")],
            )],
            ..Facts::default()
        };
        assert!(slot(tiny, &full).unwrap_err().contains("in use"));
    }

    #[test]
    fn link_names_fit_the_kernel_limit() {
        let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(link_name(id), "lfs0123456789ab");
        assert_eq!(link_name(id).len(), 15);
        assert!(is_link("lfs0123456789ab") && !is_link("eth0"));
    }

    #[test]
    fn networks_parse_and_compare() {
        assert!(net("10.213.0.0/16").contains(Ipv4Addr::new(10, 213, 255, 1)));
        assert!(!net("10.213.0.0/16").contains(Ipv4Addr::new(10, 214, 0, 1)));
        assert!(net("0.0.0.0/0").overlaps(&net("10.0.0.0/8")));
        assert_eq!(net("192.168.5.15/24").base(), net("192.168.5.0/24"));
        assert!(Ipv4Net::parse("1.2.3.4/33").is_none());
        assert_eq!(net("10.213.0.1/31").to_string(), "10.213.0.1/31");
    }

    #[test]
    fn an_init_stage_s_unavailable_line_is_its_reason() {
        let log = "layerfs-sandbox-helper: starting\n\
                   LAYERFS_SANDBOX_UNAVAILABLE this machine's Sandbox pool is 10.214.0.0/16, \
                   not 10.213.0.0/16 as when this start was checked\n\
                   layerfs-sandbox-helper: choosing the Sandbox's address: …\n";
        assert_eq!(
            unavailable_in(log).as_deref(),
            Some(
                "this machine's Sandbox pool is 10.214.0.0/16, not 10.213.0.0/16 as when this \
                 start was checked"
            )
        );
        // a stage that failed otherwise, or a line that only mentions it
        assert_eq!(
            unavailable_in("layerfs-sandbox-helper: pivoting: EPERM\n"),
            None
        );
        assert_eq!(unavailable_in("said LAYERFS_SANDBOX_UNAVAILABLE x\n"), None);
    }
}
