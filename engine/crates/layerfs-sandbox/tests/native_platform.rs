//! Platform refusal precedes filesystem access; a provider never selects another backend.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#[cfg(target_os = "linux")]
use layerfs_sandbox::backend::local::NativeMacosProvider;
use layerfs_sandbox::{backend::local::NativeLinuxProvider, RuntimeError};

#[test]
fn wrong_platform_is_refused_before_root_access() {
    #[cfg(target_os = "macos")]
    let result = NativeLinuxProvider::new("/does-not-exist/native-provider");
    #[cfg(target_os = "linux")]
    let result = NativeMacosProvider::new("/does-not-exist/native-provider");
    assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
}

#[test]
fn native_linux_requires_root_before_root_access() {
    if cfg!(target_os = "linux") && !nix::unistd::Uid::effective().is_root() {
        assert!(matches!(
            NativeLinuxProvider::new("/does-not-exist/native-provider"),
            Err(RuntimeError::Unsupported(_))
        ));
    }
}
