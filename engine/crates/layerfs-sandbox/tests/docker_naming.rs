//! A client that names its containers takes only names the Engine accepts
//! without escaping, and refuses every other before any request.
use layerfs_sandbox::backend::docker::Docker;
use std::time::Duration;

fn client() -> Docker {
    Docker::new("/var/run/docker.sock", Duration::from_secs(1)).unwrap()
}

#[test]
fn container_names_follow_the_engine_rule() {
    for good in [
        "s1",
        "layerfs-issue352-r1-service.demo",
        "A_b.c-d",
        &"a".repeat(128),
    ] {
        assert!(client().naming(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "-s1",
        "_service.demo",
        ".x",
        "a/b",
        "a b",
        "a?b",
        "a&b",
        &"a".repeat(129),
    ] {
        assert!(client().naming(bad).is_err(), "{bad}");
    }
}
