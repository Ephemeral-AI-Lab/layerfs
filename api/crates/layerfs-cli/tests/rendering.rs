//! Tests for terminal front-end rendering and suggestions (A1-A4).
use layerfs_bridge::control::SdkValue;

#[test]
fn format_time_utc() {
    assert_eq!(
        layerfs_cli::output::format_time(1794117080050),
        "2026-11-08 05:51:20"
    );
    assert_eq!(layerfs_cli::output::format_time(0), "1970-01-01 00:00:00");
    assert_eq!(
        layerfs_cli::output::format_time(1700000000000),
        "2023-11-14 22:13:20"
    );
}

#[test]
fn format_size_human_units() {
    assert_eq!(layerfs_cli::output::format_size(0), "0 B");
    assert_eq!(layerfs_cli::output::format_size(500), "500 B");
    assert_eq!(layerfs_cli::output::format_size(1023), "1023 B");
    assert_eq!(layerfs_cli::output::format_size(1024), "1 KiB");
    assert_eq!(layerfs_cli::output::format_size(1536), "1.5 KiB");
    assert_eq!(layerfs_cli::output::format_size(1048576), "1 MiB");
    assert_eq!(layerfs_cli::output::format_size(5242880), "5 MiB");
    assert_eq!(layerfs_cli::output::format_size(1073741824), "1 GiB");
    assert_eq!(layerfs_cli::output::format_size(2147483648), "2 GiB");
}

#[test]
fn format_field_renders_matching_keys() {
    let t = SdkValue::Int(1794117080050);
    assert_eq!(
        layerfs_cli::output::format_field("created", &t),
        "2026-11-08 05:51:20"
    );
    assert_eq!(
        layerfs_cli::output::format_field("modified", &t),
        "2026-11-08 05:51:20"
    );
    assert_eq!(
        layerfs_cli::output::format_field("at", &t),
        "2026-11-08 05:51:20"
    );

    let s = SdkValue::Int(1048576);
    assert_eq!(layerfs_cli::output::format_field("size", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("bytes", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("part_size", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("memory", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("disk", &s), "1 MiB");
    assert_eq!(layerfs_cli::output::format_field("free", &s), "1 MiB");

    // Other integer fields remain raw
    let other = SdkValue::Int(12345);
    assert_eq!(layerfs_cli::output::format_field("inode", &other), "12345");
    assert_eq!(layerfs_cli::output::format_field("step", &other), "12345");
}
