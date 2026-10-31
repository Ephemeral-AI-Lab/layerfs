//! The verbatim wide spelling the audited Win32 calls use: prefix forms,
//! the UNC separator, slash normalization and passthrough cases.
#![cfg(target_os = "windows")]
use layerfs_persistence::verbatim_wide;
use std::path::Path;

fn text(path: &str) -> String {
    let wide = verbatim_wide(Path::new(path));
    assert_eq!(*wide.last().unwrap(), 0, "NUL terminated");
    String::from_utf16(&wide[..wide.len() - 1]).unwrap()
}

#[test]
fn absolute_paths_gain_the_verbatim_prefix() {
    assert_eq!(text(r"D:\dir\store.sqlite"), r"\\?\D:\dir\store.sqlite");
}

#[test]
fn forward_slashes_normalize_under_the_prefix() {
    assert_eq!(text("D:/dir/store.sqlite"), r"\\?\D:\dir\store.sqlite");
}

#[test]
fn unc_paths_gain_the_unc_form_with_its_separator() {
    // The separator after UNC is part of the spelling: without it the
    // server name mis-joins (\\?\UNCserver instead of \\?\UNC\server).
    assert_eq!(
        text(r"\\server\share\store.sqlite"),
        r"\\?\UNC\server\share\store.sqlite"
    );
    assert_eq!(
        text("//server/share/store.sqlite"),
        r"\\?\UNC\server\share\store.sqlite"
    );
}

#[test]
fn already_verbatim_and_relative_paths_pass_through() {
    assert_eq!(text(r"\\?\D:\dir\store.sqlite"), r"\\?\D:\dir\store.sqlite");
    assert_eq!(text("relative/store.sqlite"), "relative/store.sqlite");
}
