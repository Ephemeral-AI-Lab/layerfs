//! The #551 isolation and scheduling values the machine and Sandbox read
//! models show, and the one legacy name map that reads a record written
//! before the contract landed. New records carry the fields themselves;
//! this map is a read fallback, not a router.
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::{ExecMode, IsolationClass, KernelScope};

/// The class a pre-#551 record's platform or backend name stands for.
///
/// `native` and `namespaces` are `process`, `mac` and `linux` name a host
/// process or namespaces Sandbox, `docker` is `container`, a VM kind is
/// `vm`, and `ssh` is `remote`. A name outside the table is unknown.
pub fn legacy_class(name: &str) -> Option<IsolationClass> {
    match name {
        "native" | "mac" | "macos" | "linux" | "namespaces" => Some(IsolationClass::Process),
        "docker" => Some(IsolationClass::Container),
        "apple-container" | "microsandbox" | "lima" | "firecracker" => Some(IsolationClass::Vm),
        "ssh" => Some(IsolationClass::Remote),
        _ => None,
    }
}

/// The `isolation` object a read model shows (#551 B8-164): the requested
/// and effective class, the effective `class`, its kernel scope and its
/// strength. A machine has no distinct request, so `requested` is `None`
/// there and equals the effective class.
pub fn isolation_sdk(requested: Option<IsolationClass>, effective: IsolationClass) -> SdkValue {
    let requested = requested.unwrap_or(effective);
    SdkValue::map([
        ("class", SdkValue::text(effective.as_str())),
        ("requested", SdkValue::text(requested.as_str())),
        ("effective", SdkValue::text(effective.as_str())),
        (
            "kernel",
            SdkValue::text(KernelScope::of_class(effective).as_str()),
        ),
        ("strength", SdkValue::text(effective.strength())),
    ])
}

/// The `scheduling` object a read model shows (#551 B8-165): whether work
/// runs through our worker and whether the Sandbox is long-lived. A
/// serverless/provider-exec mode is `remote-exec` and is not a worker.
pub fn scheduling_sdk(mode: ExecMode) -> SdkValue {
    SdkValue::map([
        ("worker", SdkValue::Bool(mode == ExecMode::Worker)),
        ("long_lived", SdkValue::Bool(true)),
        ("exec_mode", SdkValue::text(mode.as_str())),
    ])
}
