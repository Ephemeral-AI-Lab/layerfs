//! Exploratory userspace FUSE passthrough control, outside product source.
#[cfg(target_os = "linux")]
mod native;
#[cfg(target_os = "linux")]
mod state;
#[cfg(target_os = "linux")]
mod callbacks;

fn main() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    return native::run();
    #[cfg(not(target_os = "linux"))]
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported,
                            "R7 passthrough requires Linux /dev/fuse"))
}
