//! Machine inventory merges labels and keeps corrupt archives visible as errors.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
use fixture::{Scratch, Tar};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::{
    deploy::{self, Choice, Side},
    image_artifact, images,
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn archive(path: &Path, config: &[u8], tags: &[&str]) -> String {
    let hex = layerfs_bridge::sdk_values::hex(&Sha256::digest(config));
    let tags = tags
        .iter()
        .map(|tag| format!("\"{tag}\""))
        .collect::<Vec<_>>()
        .join(",");
    let manifest = format!(r#"[{{"Config":"{hex}.json","Layers":[],"RepoTags":[{tags}]}}]"#);
    fs::write(
        path,
        Tar::new()
            .file("manifest.json", 0o644, manifest.as_bytes())
            .file(&format!("{hex}.json"), 0o644, config)
            .finish(),
    )
    .unwrap();
    format!("sha256:{hex}")
}
fn record(home: &Path, archive: &Path) {
    let daemon = home.join("daemon");
    fs::write(&daemon, b"standin").unwrap();
    MachineRecord::from_record(
        "native",
        SdkValue::map([
            ("name", SdkValue::text("native")),
            ("platform", SdkValue::text("native")),
            (
                "image_archive",
                SdkValue::text(archive.display().to_string()),
            ),
            ("daemon", SdkValue::text(daemon.display().to_string())),
        ]),
    )
    .write(home)
    .unwrap();
}

#[test]
fn duplicate_digests_merge_all_tags_and_keep_primary_default() {
    let scratch = Scratch::new("inventory-tag-merge");
    let primary = scratch.base.join("primary.tar");
    let digest = archive(&primary, b"{}", &["example:base"]);
    record(&scratch.base, &primary);
    let cache = image_artifact::directory(&scratch.base);
    fs::create_dir_all(&cache).unwrap();
    archive(
        &cache.join("import.tar"),
        b"{}",
        &["example:alias", "example:base"],
    );
    let result = images::list(&scratch.base, &MachineName::parse("native").unwrap()).unwrap();
    assert_eq!(result.get_text("default_digest"), Some(digest.as_str()));
    let Some(SdkValue::List(entries)) = result.get("images") else {
        panic!("images missing")
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].get("tags"),
        Some(&SdkValue::List(vec![
            SdkValue::text("example:alias"),
            SdkValue::text("example:base")
        ]))
    );
}

#[test]
fn digest_selection_propagates_corrupt_primary_and_imports_without_substitution() {
    let scratch = Scratch::new("inventory-corrupt-resolution");
    let primary = scratch.base.join("primary.tar");
    let first = archive(&primary, b"{}", &[]);
    record(&scratch.base, &primary);
    let cache = image_artifact::directory(&scratch.base);
    fs::create_dir_all(&cache).unwrap();
    let imported = cache.join("a.tar");
    let second = archive(&imported, br#"{"os":"linux"}"#, &[]);
    let choice = Choice {
        image: Some(second),
        archive: Some(primary.display().to_string()),
        platform: None,
    };
    let selected = deploy::resolve(&Side::Home(&scratch.base), &choice).unwrap();
    assert_eq!(selected.image_archive.as_deref(), imported.to_str());
    fs::write(&primary, b"corrupt primary").unwrap();
    let error = deploy::resolve(&Side::Home(&scratch.base), &choice).unwrap_err();
    assert_eq!(error.code, "InvalidInput");
    assert!(error.message.contains("primary.tar"));
    archive(&primary, b"{}", &[]);
    fs::write(&imported, b"corrupt import").unwrap();
    let error = deploy::resolve(&Side::Home(&scratch.base), &choice).unwrap_err();
    assert!(error.message.contains("a.tar"));
    let default = Choice {
        image: Some(first),
        ..choice
    };
    deploy::resolve(&Side::Home(&scratch.base), &default).unwrap();
}
