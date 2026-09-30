//! Fixed engine class and the actual linked provider identity.

/// Exact supported hard limit for the tracked SQLite native allocator.
pub const ENGINE_HEAP_LIMIT_BYTES: u64 = 32 * 1024 * 1024;
/// Fixed requested allocation of the exclusive startup accounting probe.
pub const ENGINE_PROBE_REQUEST_BYTES: u64 = 8192;

/// Established startup profile; actual native allocation sizes remain observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineProfile {
    /// Exact native hard limit, distinct from whole-process physical memory.
    pub hard_heap_limit_bytes: u64,
    /// Requested probe bytes; its actual `msize` is recorded separately.
    pub probe_request_bytes: u64,
    /// Runtime version of the linked SQLite provider.
    pub provider_version: &'static str,
    /// Exact source identity reported by that linked provider.
    pub provider_source_id: &'static str,
}
