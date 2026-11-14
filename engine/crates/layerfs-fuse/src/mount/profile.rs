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
impl Negotiation {
    /// Plain protocol version for receipts that carry no adapter type.
    pub const fn abi_version(&self) -> (u32, u32) {
        (self.abi.0, self.abi.1)
    }
    pub const fn offered_bits(&self) -> u64 {
        self.offered.bits()
    }
    pub const fn selected_bits(&self) -> u64 {
        self.selected.bits()
    }
}
/// Prototype (FUSE profile probe, not a product contract): the request
/// window and background requests come from `LAYERFS_PROBE_FUSE_WINDOW`
/// (131072, 524288 or 1048576 bytes) and `LAYERFS_PROBE_FUSE_BACKGROUND`
/// (1 to 64); unset, they are the selected 128 KiB and 1.
pub fn window() -> u32 {
    static WINDOW: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *WINDOW.get_or_init(|| {
        std::env::var("LAYERFS_PROBE_FUSE_WINDOW")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| [131_072, 524_288, 1_048_576].contains(value))
            .unwrap_or(layerfs_overlay::READ_WINDOW as u32)
    })
}
/// See [`window`].
pub fn background() -> u16 {
    static BACKGROUND: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *BACKGROUND.get_or_init(|| {
        std::env::var("LAYERFS_PROBE_FUSE_BACKGROUND")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| (1..=64).contains(value))
            .unwrap_or(1)
    })
}
pub(crate) fn negotiate(config: &mut KernelConfig) -> io::Result<Negotiation> {
    let offered = config.capabilities();
    let defaults = InitFlags::FUSE_ASYNC_READ | InitFlags::FUSE_BIG_WRITES;
    if !offered.contains(defaults) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "required kernel capability absent",
        ));
    }
    let selected = defaults | (offered & InitFlags::FUSE_MAX_PAGES);
    let refused = |_| io::Error::new(io::ErrorKind::Unsupported, "kernel profile limit refused");
    let window = window();
    let background = background();
    config.set_max_write(window).map_err(refused)?;
    // The kernel offers its bdi readahead (128 KiB) as the most it accepts;
    // a larger window reads ahead only once the bdi's read_ahead_kb is raised.
    let readahead = match config.set_max_readahead(window) {
        Ok(_) => window,
        Err(most) => {
            config.set_max_readahead(most).map_err(refused)?;
            most
        }
    };
    config.set_max_background(background).map_err(|_| refused(0))?;
    config
        .set_congestion_threshold(background)
        .map_err(|_| refused(0))?;
    config
        .set_time_granularity(Duration::from_nanos(1))
        .map_err(|_| refused(0))?;
    let page_size = sysconf(SysconfVar::PAGE_SIZE)
        .map_err(io::Error::from)?
        .and_then(|size| u32::try_from(size).ok())
        .filter(|size| *size != 0)
        .ok_or_else(|| io::Error::other("kernel page size unavailable"))?;
    Ok(Negotiation {
        abi: config.kernel_abi(),
        offered,
        selected,
        max_write: window,
        max_readahead: readahead,
        max_background: background,
        congestion_threshold: background,
        page_size,
    })
}
