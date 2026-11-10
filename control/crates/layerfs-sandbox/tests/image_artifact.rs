//! Caller image contracts and bounded worker artifact publication, without root.
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
use fixture::{Scratch, Tar, DIGEST};
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_sandbox::{family::CreateRequest, image_artifact};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn strict_image_union_preserves_digest_and_archive_alias() {
    let text = SdkValue::text(DIGEST);
    assert_eq!(
        CreateRequest::image_choice(Some(&text), Some("/tmp/image.tar")).unwrap(),
        (Some(DIGEST.to_owned()), Some("/tmp/image.tar".to_owned()))
    );
    let object = SdkValue::map([
        ("archive", SdkValue::text("/tmp/custom.tar")),
        ("digest", text),
    ]);
    assert_eq!(
        CreateRequest::image_choice(Some(&object), None).unwrap(),
        (Some(DIGEST.to_owned()), Some("/tmp/custom.tar".to_owned()))
    );
    assert!(CreateRequest::image_choice(Some(&object), Some("/another.tar")).is_err());
    for value in [
        SdkValue::text("ubuntu:latest"),
        SdkValue::Int(7),
        SdkValue::map([("archive", SdkValue::text("relative.tar"))]),
        SdkValue::map([("digest", SdkValue::text(DIGEST))]),
        SdkValue::map([
            ("archive", SdkValue::text("/x.tar")),
            ("extra", SdkValue::Bool(true)),
        ]),
    ] {
        assert!(
            CreateRequest::image_choice(Some(&value), None).is_err(),
            "{value:?}"
        );
    }
}

#[test]
fn publication_checks_content_and_bounds_and_cache_corruption() {
    let scratch = Scratch::new("artifact-publish");
    let source = archive(&scratch.base, "input", "a");
    let bytes = fs::read(source).unwrap();
    let key = layerfs_bridge::sdk_values::hex(&Sha256::digest(&bytes));
    let frame = |step| {
        SdkValue::map([
            ("step", SdkValue::text(step)),
            ("key", SdkValue::text(&key)),
            ("token", SdkValue::text("0123456789abcdef")),
            ("size", SdkValue::Int(bytes.len() as i64)),
        ])
    };
    assert!(!image_artifact::in_worker(&scratch.base, &frame("probe"))
        .unwrap()
        .get_bool("present"));
    let write = |offset: i64, body: &[u8]| {
        let value = layerfs_home::home::with_field(frame("write"), "offset", SdkValue::Int(offset));
        layerfs_home::home::with_field(value, "bytes", SdkValue::Bytes(body.to_vec()))
    };
    assert!(image_artifact::in_worker(&scratch.base, &write(1, &bytes)).is_err());
    assert!(image_artifact::in_worker(&scratch.base, &write(0, &vec![0; (1 << 20) + 1])).is_err());
    image_artifact::in_worker(&scratch.base, &write(0, &bytes)).unwrap();
    image_artifact::in_worker(&scratch.base, &frame("publish")).unwrap();
    assert!(image_artifact::in_worker(&scratch.base, &frame("probe"))
        .unwrap()
        .get_bool("present"));
    fs::write(
        image_artifact::directory(&scratch.base).join(format!("{key}.tar")),
        b"changed",
    )
    .unwrap();
    let error = image_artifact::in_worker(&scratch.base, &frame("probe")).unwrap_err();
    assert_eq!(error.code, "InvalidInput");
    assert!(error.message.contains(&key));
    assert_eq!(
        fs::read(image_artifact::directory(&scratch.base).join(format!("{key}.tar"))).unwrap(),
        b"changed"
    );

    let mut corrupt = bytes.clone();
    corrupt[600] ^= 1;
    image_artifact::in_worker(&scratch.base, &write(0, &corrupt)).unwrap();
    assert!(image_artifact::in_worker(&scratch.base, &frame("publish")).is_err());
    image_artifact::in_worker(&scratch.base, &frame("discard")).unwrap();
}

#[test]
fn imports_deduplicate_content_and_keep_same_basename_images_distinct() {
    let scratch = Scratch::new("artifact-imports");
    let a = archive(&scratch.base, "same", "a");
    let mut updates = vec![];
    let mut sink = |event| updates.push(event);
    let mut events = Events::new(&mut sink);
    let first = image_artifact::local(&scratch.base, &mut events, &a).unwrap();
    assert_eq!(
        first,
        image_artifact::local(&scratch.base, &mut events, &a).unwrap()
    );
    let b = archive(&scratch.base, "same", "b");
    let second = image_artifact::local(&scratch.base, &mut events, &b).unwrap();
    assert_ne!(first, second);
    fs::remove_file(&b).unwrap();
    let published = image_artifact::directory(&scratch.base)
        .join(format!("{}.tar", second.strip_prefix("cache:").unwrap()));
    image_artifact::validate(published.to_str().unwrap(), None).unwrap();
    assert_eq!(
        fs::read_dir(image_artifact::directory(&scratch.base))
            .unwrap()
            .count(),
        2
    );
    assert!(updates.iter().any(
        |event| event.get("progress").and_then(|p| p.get_text("phase")) == Some("image-transfer")
    ));
}

#[test]
fn multi_image_archives_are_transferred_and_explicit_digest_is_validated() {
    let scratch = Scratch::new("artifact-multiple");
    let path = scratch.base.join("multiple.tar");
    let (a, digest_a) = config("a");
    let (b, digest_b) = config("b");
    let manifest = format!(
        r#"[{{"Config":"{digest_a}.json","Layers":[]}},{{"Config":"{digest_b}.json","Layers":[]}}]"#
    );
    let bytes = Tar::new()
        .file("manifest.json", 0o644, manifest.as_bytes())
        .file(&format!("{digest_a}.json"), 0o644, a.as_bytes())
        .file(&format!("{digest_b}.json"), 0o644, b.as_bytes())
        .finish();
    fs::write(&path, bytes).unwrap();
    image_artifact::validate(path.to_str().unwrap(), Some(&format!("sha256:{digest_b}"))).unwrap();
    assert!(image_artifact::validate(
        path.to_str().unwrap(),
        Some(&format!("sha256:{}", "d".repeat(64)))
    )
    .is_err());
    let cached =
        image_artifact::local(&scratch.base, &mut Events::new(&mut |_| {}), &path).unwrap();
    assert!(cached.starts_with("cache:"));
}

fn config(title: &str) -> (String, String) {
    let bytes = format!(
        r#"{{"architecture":"arm64","os":"linux","title":"{title}","rootfs":{{"type":"layers","diff_ids":[]}}}}"#
    );
    let digest = layerfs_bridge::sdk_values::hex(&Sha256::digest(bytes.as_bytes()));
    (bytes, digest)
}
fn archive(directory: &std::path::Path, name: &str, title: &str) -> std::path::PathBuf {
    let (config, digest) = config(title);
    let manifest = format!(r#"[{{"Config":"{digest}.json","Layers":[]}}]"#);
    let bytes = Tar::new()
        .file("manifest.json", 0o644, manifest.as_bytes())
        .file(&format!("{digest}.json"), 0o644, config.as_bytes())
        .finish();
    let path = directory.join(format!("{name}.tar"));
    fs::write(&path, bytes).unwrap();
    path
}
