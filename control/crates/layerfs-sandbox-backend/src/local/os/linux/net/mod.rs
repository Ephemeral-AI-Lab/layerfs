//! A namespaces Sandbox's own network on Linux, over hand-encoded netlink
//! (no netlink crate is in the lockfile): the codec and socket
//! (`netlink`), routing facts and changes (`link`), LayerFS's nftables
//! table (`nft`), the namespace-wide lock around slot choice (`lock`) and
//! the steps the init stage, the launcher and doctor take
//! (`wire`). The pure pool and resolver rules are `crate::network`.
pub(crate) mod link;
pub(crate) mod lock;
pub(crate) mod netlink;
pub(crate) mod nft;
pub(crate) mod wire;
