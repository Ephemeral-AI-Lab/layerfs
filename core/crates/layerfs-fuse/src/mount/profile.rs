//! Exact selected S8 profile over the pinned library's immutable defaults.
use fuser::{InitFlags, KernelConfig, Version};
use nix::unistd::{sysconf, SysconfVar};
use std::{io, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Negotiation {
    pub abi: Version,
    pub offered: InitFlags,
    /// Derived from pinned fuser0.18.0 default_init_flags; offered flags are
    /// never reported as selected. This adapter adds no optional capability.
    pub selected: InitFlags,
    pub max_write: u32,
    pub max_readahead: u32,
    pub max_background: u16,
    pub congestion_threshold: u16,
    pub page_size: u32,
}
pub(crate) fn negotiate(config: &mut KernelConfig) -> io::Result<Negotiation> {
    let offered = config.capabilities();
    let defaults = InitFlags::FUSE_ASYNC_READ | InitFlags::FUSE_BIG_WRITES;
    if !offered.contains(defaults) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "required native capability absent",
        ));
    }
    let selected = defaults | (offered & InitFlags::FUSE_MAX_PAGES);
    let refused = |_| io::Error::new(io::ErrorKind::Unsupported, "native profile limit refused");
    let window = layerfs_overlay::READ_WINDOW as u32;
    config.set_max_write(window).map_err(refused)?;
    config.set_max_readahead(window).map_err(refused)?;
    config.set_max_background(1).map_err(|_| refused(0))?;
    config.set_congestion_threshold(1).map_err(|_| refused(0))?;
    config
        .set_time_granularity(Duration::from_nanos(1))
        .map_err(|_| refused(0))?;
    let page_size = sysconf(SysconfVar::PAGE_SIZE)
        .map_err(io::Error::from)?
        .and_then(|size| u32::try_from(size).ok())
        .filter(|size| *size != 0)
        .ok_or_else(|| io::Error::other("native page size unavailable"))?;
    Ok(Negotiation {
        abi: config.kernel_abi(),
        offered,
        selected,
        max_write: window,
        max_readahead: window,
        max_background: 1,
        congestion_threshold: 1,
        page_size,
    })
}
