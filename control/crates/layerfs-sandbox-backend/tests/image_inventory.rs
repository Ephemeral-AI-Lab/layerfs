//! Public archive discovery works on Unix hosts without Docker or root.
#![cfg(unix)]
#[allow(dead_code)]
mod support {
    pub mod image;
}
use layerfs_sandbox_backend::rootfs;
use sha2::{Digest, Sha256};
use support::image::{Scratch, Tar};

fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn archive(
    manifest: &str,
    configs: &[(&str, &[u8])],
    layers: &[(&str, &[u8])],
) -> (Scratch, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let scratch = Scratch::new(&format!(
        "inventory-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let path = scratch.base.join("images.tar");
    let mut tar = Tar::new().file("manifest.json", 0o644, manifest.as_bytes());
    for (id, bytes) in configs {
        tar = tar.file(&format!("{}.json", &id[7..]), 0o644, bytes);
    }
    for (name, bytes) in layers {
        tar = tar.file(name, 0o644, bytes);
    }
    std::fs::write(&path, tar.finish()).unwrap();
    (scratch, path)
}

#[test]
fn multiple_images_expose_config_ids_and_tags_without_default_guessing() {
    let config = br#"{"os":"linux","architecture":"arm64"}"#;
    let first = digest(config);
    let second = digest(b"{}");
    let manifest = format!(
        r#"[{{"Config":"{}.json","Layers":[],"RepoTags":["python:3.12"]}},{{"Config":"{}.json","Layers":[],"RepoTags":null}}]"#,
        &first[7..],
        &second[7..]
    );
    let (_scratch, path) = archive(&manifest, &[(&first, config), (&second, b"{}")], &[]);
    let entries = rootfs::images(&path).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].digest.as_str(), first);
    assert_eq!(entries[0].tags, ["python:3.12"]);
    assert_eq!(entries[0].architecture.as_deref(), Some("arm64"));
    assert_eq!(entries[0].os.as_deref(), Some("linux"));
    assert_eq!(entries[1].digest.as_str(), second);
    assert!(entries[1].tags.is_empty());
    assert!(rootfs::image_of(&path).is_err());
    rootfs::holds(&path, &entries[1].digest).unwrap();
}

#[test]
fn discovery_refuses_missing_blobs_malformed_digest_and_duplicate_fields() {
    let id = digest(b"{}");
    for manifest in [
        format!(r#"[{{"Config":"{}.json","Layers":[]}}]"#, &id[7..]),
        r#"[{"Config":"latest.json","Layers":[]}]"#.to_owned(),
        format!(
            r#"[{{"Config":"{}.json","Config":"{}.json","Layers":[]}}]"#,
            &id[7..],
            &id[7..]
        ),
        format!(
            r#"[{{"Config":"{}.json","Layers":["missing/layer.tar"]}}]"#,
            &id[7..]
        ),
    ] {
        let (_scratch, path) = archive(&manifest, &[], &[]);
        assert!(rootfs::images(&path).is_err(), "{manifest}");
    }
}

#[test]
fn single_image_default_matches_the_listed_digest() {
    let id = digest(b"{}");
    let manifest = format!(
        r#"[{{"Config":"{}.json","Layers":[],"RepoTags":[]}}]"#,
        &id[7..]
    );
    let (_scratch, path) = archive(&manifest, &[(&id, b"{}")], &[]);
    assert_eq!(
        rootfs::image_of(&path).unwrap().as_str(),
        rootfs::images(&path).unwrap()[0].digest.as_str()
    );
}

#[test]
fn import_hashes_layers_and_listing_only_checks_config_and_compatibility() {
    let layer = Tar::new().file("hello", 0o644, b"hello").finish();
    let config = format!(
        r#"{{"rootfs":{{"type":"layers","diff_ids":["{}"]}}}}"#,
        digest(&layer)
    );
    let id = digest(config.as_bytes());
    let manifest = format!(
        r#"[{{"Config":"{}.json","Layers":["layer.tar"]}}]"#,
        &id[7..]
    );
    let (_scratch, path) = archive(
        &manifest,
        &[(&id, config.as_bytes())],
        &[("layer.tar", &layer)],
    );
    rootfs::validate_import(&path).unwrap();
    let wrong_layer = Tar::new().file("hello", 0o644, b"changed").finish();
    let (_scratch2, wrong) = archive(
        &manifest,
        &[(&id, config.as_bytes())],
        &[("layer.tar", &wrong_layer)],
    );
    rootfs::images(&wrong).unwrap();
    assert!(rootfs::validate_import(&wrong).is_err());
    let (_scratch3, spoofed) = archive(&manifest, &[(&id, b"{}")], &[("layer.tar", &layer)]);
    assert!(rootfs::images(&spoofed).is_err());
    for compressed in [b"\x1f\x8bxx".as_slice(), b"\x28\xb5\x2f\xfd".as_slice()] {
        let (_scratch4, path) = archive(
            &manifest,
            &[(&id, config.as_bytes())],
            &[("layer.tar", compressed)],
        );
        assert!(rootfs::images(&path).is_err());
    }
}
