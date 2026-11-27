#[cfg(target_os = "linux")]
pub(crate) mod detach_retry;
pub(crate) mod forced;
#[cfg(target_os = "linux")]
pub(crate) mod kernel_detach;
pub(crate) mod normal;
