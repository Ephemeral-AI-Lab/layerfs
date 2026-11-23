//! The one nftables table LayerFS owns, `table inet layerfs`, written as one
//! nfnetlink transaction, and the reads that judge another table's forward
//! policy. The table holds no per-Sandbox rule: it matches the host ends by
//! their name prefix `lfs`:
//!
//! ```nft
//! table inet layerfs {
//!   chain forward { type filter hook forward priority filter; policy accept;
//!     iifname "lfs*" oifname "lfs*" drop
//!     iifname "lfs*" accept
//!     oifname "lfs*" ct state established,related accept
//!     oifname "lfs*" drop }
//!   chain input { type filter hook input priority filter; policy accept;
//!     iifname "lfs*" tcp dport <each closed port> drop }
//!   chain postrouting { type nat hook postrouting priority srcnat; policy accept;
//!     ip saddr <pool> oifname != "lfs*" masquerade }
//! }
//! ```
//!
//! The write adds the table if it is missing, adds or keeps the three
//! chains, flushes their rules and adds the current ones, in one batch: the
//! kernel applies all of it or none. No chain has a drop policy, so other
//! tables (Docker's, lima's) keep working. No `fib` expression: Docker
//! Desktop's kernel has no `nft_fib_inet`. The numbers are stable kernel
//! ABI (`linux/netfilter/nf_tables.h`, `nfnetlink.h`).
use super::netlink::{
    attributes, find, text, Message, Socket, NLM_F_ACK, NLM_F_APPEND, NLM_F_CREATE, NLM_F_DUMP,
};
use crate::network::plan::{Ipv4Net, LINK_PREFIX};
use nix::sys::socket::SockProtocol;
use std::io;

/// The table's name, in family `inet`.
pub const TABLE: &str = "layerfs";

const NFNL_SUBSYS_NFTABLES: u16 = 10;
const NFNL_MSG_BATCH_BEGIN: u16 = 0x10;
const NFNL_MSG_BATCH_END: u16 = 0x11;
const NFT_MSG_NEWTABLE: u16 = 0;
const NFT_MSG_GETTABLE: u16 = 1;
const NFT_MSG_NEWCHAIN: u16 = 3;
const NFT_MSG_GETCHAIN: u16 = 4;
const NFT_MSG_NEWRULE: u16 = 6;
const NFT_MSG_GETRULE: u16 = 7;
const NFT_MSG_DELRULE: u16 = 8;

const NFPROTO_UNSPEC: u8 = 0;
const NFPROTO_INET: u8 = 1;
const NFPROTO_IPV4: u8 = 2;

const NFTA_TABLE_NAME: u16 = 1;
const NFTA_CHAIN_TABLE: u16 = 1;
const NFTA_CHAIN_NAME: u16 = 3;
const NFTA_CHAIN_HOOK: u16 = 4;
const NFTA_CHAIN_POLICY: u16 = 5;
const NFTA_CHAIN_TYPE: u16 = 7;
const NFTA_HOOK_HOOKNUM: u16 = 1;
const NFTA_HOOK_PRIORITY: u16 = 2;
const NFTA_RULE_TABLE: u16 = 1;
const NFTA_RULE_CHAIN: u16 = 2;
const NFTA_RULE_EXPRESSIONS: u16 = 4;
const NFTA_LIST_ELEM: u16 = 1;
const NFTA_EXPR_NAME: u16 = 1;
const NFTA_EXPR_DATA: u16 = 2;
const NFTA_DATA_VALUE: u16 = 1;
const NFTA_DATA_VERDICT: u16 = 2;
const NFTA_VERDICT_CODE: u16 = 1;

const NF_INET_LOCAL_IN: u32 = 1;
const NF_INET_FORWARD: u32 = 2;
const NF_INET_POST_ROUTING: u32 = 4;
const NF_DROP: u32 = 0;
const NF_ACCEPT: u32 = 1;
const PRIORITY_FILTER: i32 = 0;
const PRIORITY_SRCNAT: i32 = 100;

const NFT_REG_VERDICT: u32 = 0;
const NFT_REG_1: u32 = 1;
const NFT_META_NFPROTO: u32 = 15;
const NFT_META_L4PROTO: u32 = 16;
const NFT_META_IIFNAME: u32 = 6;
const NFT_META_OIFNAME: u32 = 7;
const NFT_CT_STATE: u32 = 0;
const NFT_CMP_EQ: u32 = 0;
const NFT_CMP_NEQ: u32 = 1;
const NFT_PAYLOAD_NETWORK_HEADER: u32 = 1;
const NFT_PAYLOAD_TRANSPORT_HEADER: u32 = 2;
/// `ct state established,related`: bits of IP_CT_ESTABLISHED and RELATED.
const CT_ESTABLISHED_RELATED: u32 = (1 << 1) | (1 << 2);
const IPPROTO_TCP: u8 = 6;

fn kind(message: u16) -> u16 {
    (NFNL_SUBSYS_NFTABLES << 8) | message
}

/// `struct nfgenmsg` of `family`.
fn genmsg(family: u8) -> [u8; 4] {
    [family, 0, 0, 0]
}

/// One rule's expressions, built in order.
struct Rule(Message);

impl Rule {
    fn new(chain: &str) -> Self {
        let message = Message::new(
            kind(NFT_MSG_NEWRULE),
            NLM_F_CREATE | NLM_F_APPEND | NLM_F_ACK,
        )
        .raw(&genmsg(NFPROTO_INET))
        .text(NFTA_RULE_TABLE, TABLE)
        .text(NFTA_RULE_CHAIN, chain)
        .begin(NFTA_RULE_EXPRESSIONS);
        Self(message)
    }

    fn expression(self, name: &str, data: impl FnOnce(Message) -> Message) -> Self {
        let message = self.0.begin(NFTA_LIST_ELEM).text(NFTA_EXPR_NAME, name);
        let message = data(message.begin(NFTA_EXPR_DATA)).end();
        Self(message.end())
    }

    fn meta(self, key: u32) -> Self {
        self.expression("meta", |m| m.be32(1, NFT_REG_1).be32(2, key))
    }

    fn cmp(self, op: u32, value: &[u8]) -> Self {
        self.expression("cmp", |m| {
            m.be32(1, NFT_REG_1)
                .be32(2, op)
                .begin(3)
                .attr(NFTA_DATA_VALUE, value)
                .end()
        })
    }

    fn payload(self, base: u32, offset: u32, length: u32) -> Self {
        self.expression("payload", |m| {
            m.be32(1, NFT_REG_1)
                .be32(2, base)
                .be32(3, offset)
                .be32(4, length)
        })
    }

    fn bitwise(self, mask: &[u8]) -> Self {
        let zero = vec![0u8; mask.len()];
        self.expression("bitwise", |m| {
            m.be32(1, NFT_REG_1)
                .be32(2, NFT_REG_1)
                .be32(3, mask.len() as u32)
                .begin(4)
                .attr(NFTA_DATA_VALUE, mask)
                .end()
                .begin(5)
                .attr(NFTA_DATA_VALUE, &zero)
                .end()
        })
    }

    fn ct_state(self) -> Self {
        self.expression("ct", |m| m.be32(1, NFT_REG_1).be32(2, NFT_CT_STATE))
    }

    fn verdict(self, code: u32) -> Message {
        let rule = self.expression("immediate", |m| {
            m.be32(1, NFT_REG_VERDICT)
                .begin(2)
                .begin(NFTA_DATA_VERDICT)
                .be32(NFTA_VERDICT_CODE, code)
                .end()
                .end()
        });
        rule.0.end()
    }

    fn masquerade(self) -> Message {
        let message = self
            .0
            .begin(NFTA_LIST_ELEM)
            .text(NFTA_EXPR_NAME, "masq")
            .end();
        message.end()
    }

    /// `iifname "lfs*"` or `oifname "lfs*"` (`op` equal or not).
    fn link(self, key: u32, op: u32) -> Self {
        self.meta(key).cmp(op, LINK_PREFIX.as_bytes())
    }
}

fn chain(name: &str, kind_name: &str, hook: u32, priority: i32) -> Message {
    Message::new(kind(NFT_MSG_NEWCHAIN), NLM_F_CREATE | NLM_F_ACK)
        .raw(&genmsg(NFPROTO_INET))
        .text(NFTA_CHAIN_TABLE, TABLE)
        .text(NFTA_CHAIN_NAME, name)
        .begin(NFTA_CHAIN_HOOK)
        .be32(NFTA_HOOK_HOOKNUM, hook)
        .be32(NFTA_HOOK_PRIORITY, priority as u32)
        .end()
        .be32(NFTA_CHAIN_POLICY, NF_ACCEPT)
        .text(NFTA_CHAIN_TYPE, kind_name)
}

/// The batch that writes the table for `pool` with `closed` ports.
pub(crate) fn transaction(pool: Ipv4Net, closed: &[u16]) -> Vec<Message> {
    let (iif, oif) = (NFT_META_IIFNAME, NFT_META_OIFNAME);
    let mut batch = vec![
        Message::new(NFNL_MSG_BATCH_BEGIN, 0).raw(&[0, 0, 0, NFNL_SUBSYS_NFTABLES as u8]),
        Message::new(kind(NFT_MSG_NEWTABLE), NLM_F_CREATE | NLM_F_ACK)
            .raw(&genmsg(NFPROTO_INET))
            .text(NFTA_TABLE_NAME, TABLE),
        chain("forward", "filter", NF_INET_FORWARD, PRIORITY_FILTER),
        chain("input", "filter", NF_INET_LOCAL_IN, PRIORITY_FILTER),
        chain("postrouting", "nat", NF_INET_POST_ROUTING, PRIORITY_SRCNAT),
        // the table's rules flushed, every chain kept
        Message::new(kind(NFT_MSG_DELRULE), NLM_F_ACK)
            .raw(&genmsg(NFPROTO_INET))
            .text(NFTA_RULE_TABLE, TABLE),
        Rule::new("forward")
            .link(iif, NFT_CMP_EQ)
            .link(oif, NFT_CMP_EQ)
            .verdict(NF_DROP),
        Rule::new("forward")
            .link(iif, NFT_CMP_EQ)
            .verdict(NF_ACCEPT),
        Rule::new("forward")
            .link(oif, NFT_CMP_EQ)
            .ct_state()
            .bitwise(&CT_ESTABLISHED_RELATED.to_ne_bytes())
            .cmp(NFT_CMP_NEQ, &0u32.to_ne_bytes())
            .verdict(NF_ACCEPT),
        Rule::new("forward").link(oif, NFT_CMP_EQ).verdict(NF_DROP),
    ];
    for port in closed {
        batch.push(
            Rule::new("input")
                .link(iif, NFT_CMP_EQ)
                .meta(NFT_META_L4PROTO)
                .cmp(NFT_CMP_EQ, &[IPPROTO_TCP])
                .payload(NFT_PAYLOAD_TRANSPORT_HEADER, 2, 2)
                .cmp(NFT_CMP_EQ, &port.to_be_bytes())
                .verdict(NF_DROP),
        );
    }
    let mask = u32::MAX
        .checked_shl(32 - u32::from(pool.prefix))
        .unwrap_or(0)
        .to_be_bytes();
    batch.push(
        Rule::new("postrouting")
            .meta(NFT_META_NFPROTO)
            .cmp(NFT_CMP_EQ, &[NFPROTO_IPV4])
            .payload(NFT_PAYLOAD_NETWORK_HEADER, 12, 4)
            .bitwise(&mask)
            .cmp(NFT_CMP_EQ, &pool.network().octets())
            .link(oif, NFT_CMP_NEQ)
            .masquerade(),
    );
    batch.push(Message::new(NFNL_MSG_BATCH_END, 0).raw(&[0, 0, 0, NFNL_SUBSYS_NFTABLES as u8]));
    batch
}

fn socket() -> io::Result<Socket> {
    Socket::open(SockProtocol::NetlinkNetFilter)
}

/// Writes the table for `pool`, closing `closed` to the Sandboxes, in one
/// transaction in the calling thread's network namespace.
pub fn write(pool: Ipv4Net, closed: &[u16]) -> io::Result<()> {
    socket()?.batch(transaction(pool, closed))
}

/// Whether the table exists in this network namespace.
pub fn exists() -> io::Result<bool> {
    let request = Message::new(kind(NFT_MSG_GETTABLE), NLM_F_DUMP).raw(&genmsg(NFPROTO_UNSPEC));
    let found = socket()?.dump(request)?.into_iter().any(|reply| {
        reply.payload.first() == Some(&NFPROTO_INET)
            && find(
                &attributes(reply.payload.get(4..).unwrap_or_default()),
                NFTA_TABLE_NAME,
            )
            .map(text)
            .as_deref()
                == Some(TABLE)
    });
    Ok(found)
}

/// A forward-hook base chain of another table whose policy drops what it
/// does not accept, and which accepts nothing from or to the host ends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dropping {
    pub family: &'static str,
    pub table: String,
    pub chain: String,
}

/// The forward chains (families `ip` and `inet`) whose drop policy would
/// drop every Sandbox's traffic: their table has no rule that matches
/// `iifname "lfs*"` with an accept verdict, or none that matches `oifname
/// "lfs*"` with one. Only nftables chains are seen (iptables-nft too);
/// legacy iptables rules are not.
pub fn dropping_forward() -> io::Result<Vec<Dropping>> {
    let mut socket = socket()?;
    let request = Message::new(kind(NFT_MSG_GETCHAIN), NLM_F_DUMP).raw(&genmsg(NFPROTO_UNSPEC));
    let mut chains = Vec::new();
    for reply in socket.dump(request)? {
        let family = match reply.payload.first() {
            Some(&NFPROTO_IPV4) => "ip",
            Some(&NFPROTO_INET) => "inet",
            _ => continue,
        };
        let found = attributes(reply.payload.get(4..).unwrap_or_default());
        let hook = find(&found, NFTA_CHAIN_HOOK).map(attributes);
        let hooknum = hook
            .as_deref()
            .and_then(|hook| find(hook, NFTA_HOOK_HOOKNUM))
            .and_then(be32);
        let policy = find(&found, NFTA_CHAIN_POLICY).and_then(be32);
        let table = find(&found, NFTA_CHAIN_TABLE).map(text).unwrap_or_default();
        if hooknum == Some(NF_INET_FORWARD) && policy == Some(NF_DROP) && table != TABLE {
            let chain = find(&found, NFTA_CHAIN_NAME).map(text).unwrap_or_default();
            chains.push((
                reply.payload[0],
                Dropping {
                    family,
                    table,
                    chain,
                },
            ));
        }
    }
    if chains.is_empty() {
        return Ok(Vec::new());
    }
    let request = Message::new(kind(NFT_MSG_GETRULE), NLM_F_DUMP).raw(&genmsg(NFPROTO_UNSPEC));
    let rules: Vec<(u8, String, Vec<Expression>)> = socket
        .dump(request)?
        .into_iter()
        .filter_map(|reply| {
            let family = *reply.payload.first()?;
            let found = attributes(reply.payload.get(4..)?);
            let table = find(&found, NFTA_RULE_TABLE).map(text)?;
            let expressions = find(&found, NFTA_RULE_EXPRESSIONS).map(expressions)?;
            Some((family, table, expressions))
        })
        .collect();
    let admits = |family: u8, table: &str, key: u32| {
        rules
            .iter()
            .any(|(f, t, rule)| *f == family && t == table && accepts_link(rule, key))
    };
    Ok(chains
        .into_iter()
        .filter(|(family, chain)| {
            !(admits(*family, &chain.table, NFT_META_IIFNAME)
                && admits(*family, &chain.table, NFT_META_OIFNAME))
        })
        .map(|(_, chain)| chain)
        .collect())
}

fn be32(data: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(..4)?.try_into().ok()?))
}

/// One decoded expression: its name and its data's attributes as bytes.
pub(crate) struct Expression {
    name: String,
    data: Vec<(u16, Vec<u8>)>,
}

impl Expression {
    fn get(&self, kind: u16) -> Option<&[u8]> {
        self.data
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| v.as_slice())
    }
}

fn expressions(list: &[u8]) -> Vec<Expression> {
    attributes(list)
        .into_iter()
        .filter(|(kind, _)| *kind == NFTA_LIST_ELEM)
        .map(|(_, element)| {
            let found = attributes(element);
            let name = find(&found, NFTA_EXPR_NAME).map(text).unwrap_or_default();
            let data = find(&found, NFTA_EXPR_DATA)
                .map(|data| {
                    (attributes(data).into_iter())
                        .map(|(k, v)| (k, v.to_vec()))
                        .collect()
                })
                .unwrap_or_default();
            Expression { name, data }
        })
        .collect()
}

/// Whether `rule` loads the interface name `key`, compares it equal to the
/// prefix `lfs` and ends in an accept verdict (iptables-nft's
/// `-i lfs+ -j ACCEPT` and `-o lfs+ … -j ACCEPT` are such rules).
fn accepts_link(rule: &[Expression], key: u32) -> bool {
    let loads = rule
        .iter()
        .position(|e| e.name == "meta" && e.get(2).and_then(be32) == Some(key));
    let Some(loads) = loads else {
        return false;
    };
    let compared = rule[loads + 1..].iter().any(|e| {
        e.name == "cmp"
            && e.get(2).and_then(be32) == Some(NFT_CMP_EQ)
            && e.get(3)
                .map(attributes)
                .as_deref()
                .and_then(|d| find(d, NFTA_DATA_VALUE))
                == Some(LINK_PREFIX.as_bytes())
    });
    let accepted = rule.iter().any(|e| {
        e.name == "immediate"
            && e.get(2)
                .map(attributes)
                .as_deref()
                .and_then(|d| find(d, NFTA_DATA_VERDICT))
                .map(attributes)
                .as_deref()
                .and_then(|v| find(v, NFTA_VERDICT_CODE))
                .and_then(be32)
                == Some(NF_ACCEPT)
    });
    compared && accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A message's rule expressions, as (name, the cmp data if any).
    fn rule_of(message: Message) -> Vec<Expression> {
        let bytes = message.bytes(1);
        let found = attributes(&bytes[20..]);
        expressions(find(&found, NFTA_RULE_EXPRESSIONS).unwrap())
    }

    #[test]
    fn the_table_is_one_batch_of_table_chains_flush_and_rules() {
        let pool = Ipv4Net::parse("10.213.0.0/16").unwrap();
        let batch = transaction(pool, &[7070]);
        // begin, table, 3 chains, flush, 4 forward, 1 input, 1 nat, end
        assert_eq!(batch.len(), 13);
        let kinds: Vec<u16> = batch
            .into_iter()
            .map(|m| {
                let b = m.bytes(1);
                u16::from_ne_bytes([b[4], b[5]])
            })
            .collect();
        assert_eq!(kinds[0], NFNL_MSG_BATCH_BEGIN);
        assert_eq!(kinds[1], kind(NFT_MSG_NEWTABLE));
        assert_eq!(&kinds[2..5], &[kind(NFT_MSG_NEWCHAIN); 3]);
        assert_eq!(kinds[5], kind(NFT_MSG_DELRULE));
        assert!(kinds[6..12].iter().all(|k| *k == kind(NFT_MSG_NEWRULE)));
        assert_eq!(kinds[12], NFNL_MSG_BATCH_END);
        // no closed port, no input rule
        assert_eq!(transaction(pool, &[]).len(), 12);
    }

    #[test]
    fn rules_match_the_host_ends_by_prefix_and_masquerade_the_pool() {
        let pool = Ipv4Net::parse("10.213.0.0/16").unwrap();
        let mut batch = transaction(pool, &[7070]);
        let nat = batch.remove(11);
        let input = batch.remove(10);
        let siblings = batch.remove(6);
        let names = |rule: &[Expression]| rule.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        let siblings = rule_of(siblings);
        assert_eq!(
            names(&siblings),
            ["meta", "cmp", "meta", "cmp", "immediate"]
        );
        let value = |e: &Expression| {
            find(&attributes(e.get(3).unwrap()), NFTA_DATA_VALUE)
                .unwrap()
                .to_vec()
        };
        assert_eq!(value(&siblings[1]), b"lfs");
        let input = rule_of(input);
        assert_eq!(value(&input[5]), 7070u16.to_be_bytes());
        let nat = rule_of(nat);
        assert_eq!(
            names(&nat),
            ["meta", "cmp", "payload", "bitwise", "cmp", "meta", "cmp", "masq"]
        );
        assert_eq!(value(&nat[4]), [10, 213, 0, 0]);
        assert_eq!(nat[6].get(2).and_then(be32), Some(NFT_CMP_NEQ));
    }

    #[test]
    fn a_rule_that_accepts_the_host_ends_is_recognised() {
        let accept = rule_of(
            Rule::new("x")
                .link(NFT_META_IIFNAME, NFT_CMP_EQ)
                .verdict(NF_ACCEPT),
        );
        assert!(accepts_link(&accept, NFT_META_IIFNAME));
        assert!(!accepts_link(&accept, NFT_META_OIFNAME));
        let drop = rule_of(
            Rule::new("x")
                .link(NFT_META_IIFNAME, NFT_CMP_EQ)
                .verdict(NF_DROP),
        );
        assert!(!accepts_link(&drop, NFT_META_IIFNAME));
    }
}
