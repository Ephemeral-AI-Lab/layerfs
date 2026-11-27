//! Incompatible manifests are rejected before any provider start or image load.
#![cfg(feature = "machine")]
use layerfs_machine::setup::bundle;
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
    // a bundle sealed from another commit than this build's is refused;
    // a build that names no commit cannot tell, and takes it
    let other = "0".repeat(40);
    write(&format!(r#","source":{{"head":"{other}","dirty":false}}"#));
    match option_env!("LAYERFS_BUILD_COMMIT").filter(|c| c.len() == 40) {
        Some(ours) => {
            // no bundle of this build where LayerFS looks: no command fixes
            // it, and the message says so (RT5 note 4)
            let cache = dir.join("cache");
            std::env::set_var("XDG_CACHE_HOME", &cache);
            let refusal = bundle::bundle().unwrap_err();
            assert_eq!(refusal.code, "Unavailable");
            assert!(refusal.message.contains("was sealed from 0000000000"));
            assert!(
                refusal.message.contains("no bundle of this build is where"),
                "{}",
                refusal.message
            );
            assert_eq!(refusal.detail.get("fix"), None, "{refusal:?}");
            // this build's bundle in the cache: unsetting LAYERFS_BUNDLE
            // reaches it
            let own = (cache.join("layerfs/bundle"))
                .join(env!("CARGO_PKG_VERSION"))
                .join(format!("linux-{}", std::env::consts::ARCH));
            fs::create_dir_all(&own).unwrap();
            let sealed = |head: &str| {
                let manifest = format!(r#"{{"hash":"own","source":{{"head":"{head}"}}}}"#);
                fs::write(own.join("bundle.json"), manifest).unwrap();
            };
            sealed(ours);
            let refusal = bundle::bundle().unwrap_err();
            assert_eq!(refusal.code, "Unavailable");
            assert_eq!(refusal.detail.get_text("fix"), Some("unset LAYERFS_BUNDLE"));
            let at = own.display().to_string();
            assert!(refusal.message.contains(&at), "{}", refusal.message);
            // unset, the cache's own is taken
            std::env::remove_var("LAYERFS_BUNDLE");
            assert_eq!(bundle::bundle().unwrap().hash, "own");
            // the cache's of another build, found with LAYERFS_BUNDLE unset:
            // no other place holds this build's, so no fix
            sealed(&other);
            let refusal = bundle::bundle().unwrap_err();
            assert_eq!(refusal.code, "Unavailable");
            assert!(refusal.message.contains(&at), "{}", refusal.message);
            assert_eq!(refusal.detail.get("fix"), None, "{refusal:?}");
            std::env::set_var("LAYERFS_BUNDLE", &dir);
            std::env::remove_var("XDG_CACHE_HOME");
            write(&format!(r#","source":{{"head":"{ours}","dirty":false}}"#));
            assert_eq!(bundle::bundle().unwrap().hash, "abcd");
        }
        None => assert_eq!(bundle::bundle().unwrap().hash, "abcd"),
    }
    assert_eq!(
        layerfs_machine::bundle_place("linux").unwrap(),
        (dir.clone(), "env:LAYERFS_BUNDLE")
    );
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
