//! Prototype (FUSE profile probe, copies experiment): process-wide counts and
//! bytes of the userspace copies a cold READ makes, by site, and the
//! `LAYERFS_PROBE_COPY` switches that remove some of them. Diagnostic only; a
//! reader diffs [`snapshot`] around the phase it measures.
use std::sync::atomic::{AtomicU64, Ordering};

/// One place a payload byte is copied on the read path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Site {
    /// SQLite pages into a pack body or selected units (Store).
    Pack,
    /// A stored (raw) record's frame into its own `Vec` (Store `decode_payload`).
    Frame,
    /// Raw payload or record bytes into the canonical object (Store).
    Canonical,
    /// The object cache insert (`Cache::insert`, `to_vec`).
    Insert,
    /// The object cache read (`Cache::get`, `clone`).
    Get,
    /// The planned range written into the reply buffer (`Wave::flush`).
    Emit,
    /// A split READ's window appended to its one reply (`windows`).
    Join,
    /// The reply handed to the kernel (`writev` to `/dev/fuse`).
    Reply,
    /// A mapping page into or out of a read's page cache (`Wave::pages`).
    Page,
}
/// Number of [`Site`]s.
pub const SITES: usize = Site::Page as usize + 1;
/// Names of the [`Site`]s, in order.
pub const SITE_NAMES: [&str; SITES] = [
    "pack",
    "frame",
    "canonical",
    "insert",
    "get",
    "emit",
    "join",
    "reply",
    "page",
];

const ZERO: AtomicU64 = AtomicU64::new(0);
static COUNTS: [AtomicU64; SITES] = [ZERO; SITES];
static BYTES: [AtomicU64; SITES] = [ZERO; SITES];

/// Counts one copy of `bytes` bytes at `site`.
pub fn copied(site: Site, bytes: usize) {
    COUNTS[site as usize].fetch_add(1, Ordering::Relaxed);
    BYTES[site as usize].fetch_add(bytes as u64, Ordering::Relaxed);
}
/// Counts and bytes of every site so far.
pub fn snapshot() -> ([u64; SITES], [u64; SITES]) {
    (
        std::array::from_fn(|index| COUNTS[index].load(Ordering::Relaxed)),
        std::array::from_fn(|index| BYTES[index].load(Ordering::Relaxed)),
    )
}

/// Prototype switches from `LAYERFS_PROBE_COPY`, a comma list; unset, none
/// applies and every copy is made as before.
#[derive(Clone, Copy, Debug, Default)]
pub struct Switches {
    /// Store: a stored chunk record is encoded into its canonical object
    /// straight from the pack body, without the intermediate frame `Vec`.
    pub frame: bool,
    /// Object cache: a fetched object is moved in, not copied.
    pub moved: bool,
    /// Object cache: a hit is handed out as a shared buffer, not cloned.
    pub shared: bool,
    /// Split READ: every window is emitted into the one reply buffer.
    pub join: bool,
}
/// The switches of this process, read once.
pub fn switches() -> Switches {
    static SWITCHES: std::sync::OnceLock<Switches> = std::sync::OnceLock::new();
    *SWITCHES.get_or_init(|| {
        let value = std::env::var("LAYERFS_PROBE_COPY").unwrap_or_default();
        let has = |name: &str| {
            value
                .split(',')
                .any(|part| part.trim() == name || part.trim() == "all")
        };
        Switches {
            frame: has("frame"),
            moved: has("move"),
            shared: has("share"),
            join: has("join"),
        }
    })
}
