//! Built-in machine identity, platform and runtime have distinct roles.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn host_record_uses_the_current_os_name_and_host_platform() {
    use layerfs_home::names::{BUILT_IN, HOST_MACHINE};
    use layerfs_machine::record::MachineRecord;
    let expected = if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "linux-host"
    };
    assert_eq!(HOST_MACHINE, expected);
    assert_eq!(BUILT_IN, [expected, "docker"]);
    let record = MachineRecord::built_in(expected, "0123456789abcdef");
    assert_eq!(record.name(), expected);
    assert_eq!(record.platform_name(), Some("host"));
    assert_eq!(layerfs_machine::in_process(), Some(expected));
    assert!(!record.provided());
}
