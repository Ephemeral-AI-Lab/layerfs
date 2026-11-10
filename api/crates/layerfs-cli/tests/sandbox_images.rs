//! The existing digest flag and caller-local archive input share the SDK call.
use layerfs_api::{find, json};
use layerfs_bridge::control::SdkValue;
use layerfs_cli::parse::{parse, Parsed};

fn sandbox(words: &[&str]) -> Parsed {
    let spec = find("sandbox.create").expect("sandbox.create");
    let words: Vec<String> = ["store/project", "dev", "lima1"]
        .into_iter()
        .chain(words.iter().copied())
        .map(str::to_owned)
        .collect();
    parse(spec, &words, &mut std::io::empty()).unwrap_or_else(|why| panic!("{}", why.0))
}

#[test]
fn existing_bare_digest_and_json_string_select_the_same_image() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let bare = sandbox(&["--image", &digest]);
    let quoted = sandbox(&["--image", &format!("\"{digest}\"")]);
    assert_eq!(bare.args, quoted.args);
    assert_eq!(bare.args.get_text("image"), Some(digest.as_str()));
}

#[test]
fn archive_object_is_passed_whole_to_the_api() {
    let input = r#"{"archive":"/tmp/my environment.tar","digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let parsed = sandbox(&["--image", input, "--progress"]);
    assert_eq!(parsed.args.get("image"), Some(&json::parse(input).unwrap()));
    assert!(parsed.switches.progress);
}

#[test]
fn archive_flag_preserves_paths_and_optional_digest() {
    let digest = format!("sha256:{}", "b".repeat(64));
    let parsed = sandbox(&[
        "--image-archive",
        "/tmp/my environment.tar",
        "--image",
        &digest,
    ]);
    assert_eq!(
        parsed.args.get_text("image_archive"),
        Some("/tmp/my environment.tar")
    );
    assert_eq!(parsed.args.get_text("image"), Some(digest.as_str()));
}

#[test]
fn registry_tags_are_not_interpreted_as_a_default_image() {
    let spec = find("sandbox.create").unwrap();
    let words = ["store/project", "dev", "--image", "ubuntu:latest"].map(str::to_owned);
    assert!(parse(spec, &words, &mut std::io::empty()).is_err());
}

#[test]
fn image_inventory_has_only_a_machine_positional() {
    let spec = find("machine.images").expect("machine.images");
    let parsed = parse(spec, &["lima1".to_owned()], &mut std::io::empty())
        .unwrap_or_else(|why| panic!("{}", why.0));
    assert_eq!(
        parsed.args,
        SdkValue::map([("name", SdkValue::text("lima1"))])
    );
}
