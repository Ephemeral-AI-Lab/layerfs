//! Routing netlink: what a network namespace holds (links, IPv4 addresses
//! and routes, as the planner needs them) and the few changes a Sandbox's
//! network makes: one veth pair, two addresses, links up, one default
//! route. The attribute numbers are stable kernel ABI (`linux/if_link.h`,
//! `linux/if_addr.h`, `linux/rtnetlink.h`, `linux/veth.h`), defined here
//! because `libc` lacks several of them.
use super::netlink::{
    attributes, find, text, u32_of, Message, Socket, NLM_F_ACK, NLM_F_CREATE, NLM_F_DUMP,
    NLM_F_EXCL,
};
use crate::network::pool::{is_link, Facts, Ipv4Net};
use nix::sys::socket::SockProtocol;
use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, BorrowedFd};

const RTM_NEWLINK: u16 = 16;
const RTM_GETLINK: u16 = 18;
const RTM_NEWADDR: u16 = 20;
const RTM_GETADDR: u16 = 22;
const RTM_NEWROUTE: u16 = 24;
const RTM_GETROUTE: u16 = 26;

const IFLA_IFNAME: u16 = 3;
const IFLA_MTU: u16 = 4;
const IFLA_LINKINFO: u16 = 18;
const IFLA_INFO_KIND: u16 = 1;
const IFLA_INFO_DATA: u16 = 2;
const VETH_INFO_PEER: u16 = 1;
const IFLA_AF_SPEC: u16 = 26;
const IFLA_NET_NS_FD: u16 = 28;
const IFLA_INET6_ADDR_GEN_MODE: u16 = 8;
const IN6_ADDR_GEN_MODE_NONE: u8 = 1;
const IFA_ADDRESS: u16 = 1;
const IFA_LOCAL: u16 = 2;
const RTA_DST: u16 = 1;
const RTA_OIF: u16 = 4;
const RTA_GATEWAY: u16 = 5;

const AF_UNSPEC: u8 = 0;
const AF_INET: u8 = 2;
const AF_INET6: u16 = 10;
const IFF_UP: u32 = 1;
const RT_TABLE_MAIN: u8 = 254;
const RTPROT_BOOT: u8 = 3;
const RT_SCOPE_UNIVERSE: u8 = 0;
const RTN_UNICAST: u8 = 1;
/// The MTU a pair gets when the namespace has no default route to copy.
const DEFAULT_MTU: u32 = 1500;

/// `struct ifinfomsg` for link `index` with `flags` changed by `change`.
fn ifinfo(index: u32, flags: u32, change: u32) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0] = AF_UNSPEC;
    bytes[4..8].copy_from_slice(&index.to_ne_bytes());
    bytes[8..12].copy_from_slice(&flags.to_ne_bytes());
    bytes[12..16].copy_from_slice(&change.to_ne_bytes());
    bytes
}

/// One link of a namespace.
#[derive(Clone, Debug)]
pub(crate) struct Link {
    pub(crate) index: u32,
    pub(crate) name: String,
    pub(crate) mtu: u32,
}

/// A routing netlink socket of the namespace it was opened in.
pub struct Routes(Socket);

impl Routes {
    /// A socket in the calling thread's network namespace.
    pub fn open() -> io::Result<Self> {
        Socket::open(SockProtocol::NetlinkRoute).map(Self)
    }

    pub(crate) fn links(&mut self) -> io::Result<Vec<Link>> {
        let request = Message::new(RTM_GETLINK, NLM_F_DUMP).raw(&ifinfo(0, 0, 0));
        let mut links = Vec::new();
        for reply in self.0.dump(request)? {
            if reply.kind != RTM_NEWLINK || reply.payload.len() < 16 {
                continue;
            }
            let index = u32::from_ne_bytes(reply.payload[4..8].try_into().unwrap_or_default());
            let found = attributes(&reply.payload[16..]);
            let name = find(&found, IFLA_IFNAME).map(text).unwrap_or_default();
            let mtu = find(&found, IFLA_MTU).and_then(u32_of).unwrap_or(0);
            links.push(Link { index, name, mtu });
        }
        Ok(links)
    }

    /// Every IPv4 address: its link and its address with prefix.
    pub(crate) fn addresses(&mut self) -> io::Result<Vec<(u32, Ipv4Net)>> {
        let request = Message::new(RTM_GETADDR, NLM_F_DUMP).raw(&[AF_INET, 0, 0, 0, 0, 0, 0, 0]);
        let mut addresses = Vec::new();
        for reply in self.0.dump(request)? {
            let payload = &reply.payload;
            if reply.kind != RTM_NEWADDR || payload.len() < 8 || payload[0] != AF_INET {
                continue;
            }
            let prefix = payload[1];
            let index = u32::from_ne_bytes(payload[4..8].try_into().unwrap_or_default());
            let found = attributes(&payload[8..]);
            let address = find(&found, IFA_LOCAL).or_else(|| find(&found, IFA_ADDRESS));
            if let Some(Ok(octets)) = address.map(<[u8; 4]>::try_from) {
                if let Some(net) = Ipv4Net::new(Ipv4Addr::from(octets), prefix) {
                    addresses.push((index, net));
                }
            }
        }
        Ok(addresses)
    }

    /// Every IPv4 route of every table: its destination and output link.
    pub(crate) fn routes(&mut self) -> io::Result<Vec<(Ipv4Net, Option<u32>, u8)>> {
        let mut header = [0u8; 12];
        header[0] = AF_INET;
        let request = Message::new(RTM_GETROUTE, NLM_F_DUMP).raw(&header);
        let mut routes = Vec::new();
        for reply in self.0.dump(request)? {
            let payload = &reply.payload;
            if reply.kind != RTM_NEWROUTE || payload.len() < 12 || payload[0] != AF_INET {
                continue;
            }
            let (prefix, table) = (payload[1], payload[4]);
            let found = attributes(&payload[12..]);
            let destination = match find(&found, RTA_DST).map(<[u8; 4]>::try_from) {
                Some(Ok(octets)) => Ipv4Addr::from(octets),
                _ => Ipv4Addr::UNSPECIFIED,
            };
            let oif = find(&found, RTA_OIF).and_then(u32_of);
            if let Some(net) = Ipv4Net::new(destination, prefix) {
                routes.push((net, oif, table));
            }
        }
        Ok(routes)
    }

    /// The planner's facts and the MTU of the default route's link.
    pub fn facts(&mut self) -> io::Result<(Facts, u32)> {
        let links = self.links()?;
        let sandbox = |index: u32| {
            links
                .iter()
                .find(|link| link.index == index)
                .filter(|link| is_link(&link.name))
        };
        let mut facts = Facts::default();
        for link in links.iter().filter(|link| is_link(&link.name)) {
            facts.links.push((link.name.clone(), Vec::new()));
        }
        for (index, net) in self.addresses()? {
            match sandbox(index) {
                Some(link) => {
                    if let Some((_, held)) = facts.links.iter_mut().find(|(n, _)| *n == link.name) {
                        held.push(net);
                    }
                }
                None => facts.addresses.push(net.base()),
            }
        }
        let mut mtu = None;
        for (net, oif, table) in self.routes()? {
            if oif.and_then(sandbox).is_some() {
                continue;
            }
            if net.prefix == 0 {
                if table == RT_TABLE_MAIN && mtu.is_none() {
                    mtu = oif
                        .and_then(|oif| links.iter().find(|l| l.index == oif))
                        .map(|l| l.mtu);
                }
                continue;
            }
            facts.routes.push(net.base());
        }
        Ok((facts, mtu.filter(|mtu| *mtu > 0).unwrap_or(DEFAULT_MTU)))
    }

    pub(crate) fn index(&mut self, name: &str) -> io::Result<u32> {
        (self.links()?.into_iter())
            .find(|link| link.name == name)
            .map(|link| link.index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no link {name}")))
    }

    /// A veth pair: `host` in this socket's namespace, `guest` created
    /// directly in the namespace `netns` refers to, both of MTU `mtu`.
    pub(crate) fn veth(
        &mut self,
        host: &str,
        guest: &str,
        netns: BorrowedFd<'_>,
        mtu: u32,
    ) -> io::Result<()> {
        let fd = u32::try_from(netns.as_raw_fd())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "namespace handle"))?;
        let request = Message::new(RTM_NEWLINK, NLM_F_CREATE | NLM_F_EXCL | NLM_F_ACK)
            .raw(&ifinfo(0, 0, 0))
            .text(IFLA_IFNAME, host)
            .u32(IFLA_MTU, mtu)
            .begin(IFLA_LINKINFO)
            .text(IFLA_INFO_KIND, "veth")
            .begin(IFLA_INFO_DATA)
            .begin(VETH_INFO_PEER)
            .raw(&ifinfo(0, 0, 0))
            .text(IFLA_IFNAME, guest)
            .u32(IFLA_MTU, mtu)
            .u32(IFLA_NET_NS_FD, fd)
            .end()
            .end()
            .end();
        self.0.ack(request)
    }

    /// No IPv6 link-local address on link `index` (set before it is up). A
    /// kernel without IPv6 has none to make.
    pub(crate) fn no_link_local(&mut self, index: u32) -> io::Result<()> {
        let request = Message::new(RTM_NEWLINK, NLM_F_ACK)
            .raw(&ifinfo(index, 0, 0))
            .begin(IFLA_AF_SPEC)
            .begin(AF_INET6)
            .attr(IFLA_INET6_ADDR_GEN_MODE, &[IN6_ADDR_GEN_MODE_NONE])
            .end()
            .end();
        match self.0.ack(request) {
            Err(error) if error.raw_os_error() == Some(nix::libc::EAFNOSUPPORT) => Ok(()),
            done => done,
        }
    }

    pub(crate) fn up(&mut self, index: u32) -> io::Result<()> {
        let request = Message::new(RTM_NEWLINK, NLM_F_ACK).raw(&ifinfo(index, IFF_UP, IFF_UP));
        self.0.ack(request)
    }

    pub(crate) fn address(&mut self, index: u32, address: Ipv4Addr, prefix: u8) -> io::Result<()> {
        let mut header = [0u8; 8];
        header[0] = AF_INET;
        header[1] = prefix;
        header[4..8].copy_from_slice(&index.to_ne_bytes());
        let request = Message::new(RTM_NEWADDR, NLM_F_CREATE | NLM_F_EXCL | NLM_F_ACK)
            .raw(&header)
            .attr(IFA_LOCAL, &address.octets())
            .attr(IFA_ADDRESS, &address.octets());
        self.0.ack(request)
    }

    pub(crate) fn default_route(&mut self, gateway: Ipv4Addr, oif: u32) -> io::Result<()> {
        let header = [
            AF_INET,
            0,
            0,
            0,
            RT_TABLE_MAIN,
            RTPROT_BOOT,
            RT_SCOPE_UNIVERSE,
            RTN_UNICAST,
            0,
            0,
            0,
            0,
        ];
        let request = Message::new(RTM_NEWROUTE, NLM_F_CREATE | NLM_F_EXCL | NLM_F_ACK)
            .raw(&header)
            .attr(RTA_GATEWAY, &gateway.octets())
            .u32(RTA_OIF, oif);
        self.0.ack(request)
    }
}
