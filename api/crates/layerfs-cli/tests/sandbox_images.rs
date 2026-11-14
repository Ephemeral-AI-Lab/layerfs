//! A Sandbox image is one word on the command line: a digest, a Docker Id or
//! a Docker reference, passed to the call as text.
use layerfs_api::find;
use layerfs_bridge::control::SdkValue;
use layerfs_cli::parse::parse;

fn parsed(call: &str, words: &[&str]) -> layerfs_cli::parse::Parsed {
    let spec = find(call).expect(call);
    let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
    parse(spec, &words, &mut std::io::empty()).unwrap_or_else(|why| panic!("{}", why.0))
}

#[test]
fn a_digest_and_a_reference_are_both_text() {
    let digest = format!("sha256:{}", "a".repeat(64));
    for image in [
        digest.as_str(),
        "postgres:17-bookworm",
        "ghcr.io/org/app@sha256:abc",
    ] {
        let p = parsed(
            "sandbox.create",
            &[
                "store/project",
                "dev",
                "docker",
                "--image",
                image,
                "--progress",
            ],
        );
        assert_eq!(p.args.get_text("image"), Some(image));
        assert!(p.switches.progress);
    }
}

#[test]
fn load_and_prune_take_a_machine_and_an_image() {
    let p = parsed("machine.load", &["lima1", "alpine:3", "--progress"]);
    assert_eq!(
        p.args,
        SdkValue::map([
            ("name", SdkValue::text("lima1")),
            ("image", SdkValue::text("alpine:3"))
        ])
    );
    assert!(p.switches.progress);
    let p = parsed("machine.prune", &["lima1", "--unused"]);
    assert_eq!(p.args.get_text("name"), Some("lima1"));
    assert!(p.args.get_bool("unused"));
    let p = parsed("machine.images", &["lima1"]);
    assert_eq!(p.args, SdkValue::map([("name", SdkValue::text("lima1"))]));
}
