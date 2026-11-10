//! Incompatible manifests are rejected before any provider start or image load.
#![cfg(feature = "machine")]
use layerfs_machine::bundle;
use std::fs;

#[test]
fn manifest_protocol_and_architecture_are_checked_while_legacy_is_readable() {
    let dir = std::env::temp_dir().join(format!("lfs-bundle-preflight-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    std::env::set_var("LAYERFS_BUNDLE", &dir);
    let write = |extra: &str| {
        fs::write(
            dir.join("bundle.json"),
            format!(r#"{{"hash":"abcd"{extra}}}"#),
        )
        .unwrap()
    };
    write(",\"protocol\":2");
    assert_eq!(bundle::bundle().unwrap_err().code, "Unavailable");
    write(",\"protocol\":\"3\"");
    assert_eq!(bundle::bundle().unwrap_err().code, "Unavailable");
    write(",\"protocol\":3,\"platform\":\"linux-other\"");
    assert_eq!(bundle::bundle().unwrap_err().code, "Unsupported");
    write(&format!(
        ",\"protocol\":3,\"platform\":\"linux-{}\"",
        std::env::consts::ARCH
    ));
    assert_eq!(bundle::bundle().unwrap().hash, "abcd");
    write("");
    assert_eq!(bundle::bundle().unwrap().hash, "abcd");
    let cache = dir.join("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    let content = "e".repeat(64);
    write(&format!(",\"protocol\":3,\"image\":\"sha256:{content}\""));
    let b = bundle::bundle().unwrap();
    let kind = layerfs_machine::platform::Platform::Lima;
    assert_eq!(bundle::prepared_os_image(kind, &b).unwrap(), None);
    let platform = format!("linux-{}", std::env::consts::ARCH);
    let folder = cache
        .join("layerfs/machine-images/lima")
        .join(&platform)
        .join(&content);
    fs::create_dir_all(&folder).unwrap();
    let image = dir.join("root.img");
    let mapping = |protocol, sanitized| {
        format!(
            r#"{{"version":1,"provider":"lima","platform":"{platform}","protocol":{protocol},"image":"sha256:{content}","sanitized":{sanitized},"os_image":"{}"}}"#,
            image.display()
        )
    };
    fs::write(folder.join("current.json"), mapping(2, true)).unwrap();
    assert_eq!(
        bundle::prepared_os_image(kind, &b).unwrap_err().code,
        "Unavailable"
    );
    fs::write(folder.join("current.json"), mapping(3, false)).unwrap();
    assert_eq!(
        bundle::prepared_os_image(kind, &b).unwrap_err().code,
        "Unavailable"
    );
    fs::write(folder.join("current.json"), mapping(3, true)).unwrap();
    assert_eq!(
        bundle::prepared_os_image(kind, &b).unwrap_err().code,
        "Unsupported"
    );
    fs::write(&image, b"prepared OS").unwrap();
    assert_eq!(
        bundle::prepared_os_image(kind, &b).unwrap(),
        Some(image.to_str().unwrap().to_owned())
    );
    std::env::remove_var("XDG_CACHE_HOME");
    std::env::remove_var("LAYERFS_BUNDLE");
    fs::remove_dir_all(dir).unwrap();
}
