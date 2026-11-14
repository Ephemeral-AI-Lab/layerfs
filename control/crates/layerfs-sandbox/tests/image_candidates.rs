//! The host's Docker images a machine could copy (`machine.images`'
//! `docker` and `available`), read through a stand-in Docker.
#[allow(dead_code)]
#[path = "../../layerfs-sandbox-backend/tests/support/image.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/standin.rs"]
mod standin;
use fixture::Scratch;
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::docker::{candidates, need, Docker};
use standin::Standin;

fn id(digit: char) -> String {
    format!("sha256:{}", digit.to_string().repeat(64))
}

fn row<'a>(rows: &'a [SdkValue], id: &str) -> Option<&'a SdkValue> {
    rows.iter().find(|r| r.get_text("id") == Some(id))
}

#[test]
fn candidates_are_deduped_and_judged() {
    let scratch = Scratch::new("candidates");
    let standin = Standin::new(&scratch.base.join("standin"));
    let bundle = id('b');
    standin.image(
        &id('1'),
        &["postgres:17", "pg:latest"],
        ("linux", "arm64"),
        1000,
    );
    standin.image(&id('2'), &["x86/tool:1"], ("linux", "amd64"), 10);
    standin.image(&id('3'), &["win/app:1"], ("windows", "arm64"), 10);
    standin.image(&id('4'), &["held:1"], ("linux", "arm64"), 10);
    standin.image(&id('5'), &["also-held:1"], ("linux", "arm64"), 10);
    let tag = format!("layerfs-cache/sandbox:{}", &bundle[7..]);
    standin.image(&id('6'), &[tag.as_str()], ("linux", "arm64"), 10);
    standin.image(&id('7'), &["big:1"], ("linux", "arm64"), 1_000_000);
    let held = vec![
        (id('4'), vec![]),
        (id('e'), vec!["also-held:1".to_owned()]),
        (bundle.clone(), vec![]),
    ];
    let (docker, rows) = candidates(&standin.docker(), "arm64", Some(10_000), &held);
    assert_eq!(docker, SdkValue::map([("available", SdkValue::Bool(true))]));
    assert_eq!(rows.len(), 4, "{rows:?}");
    let pg = row(&rows, &id('1')).unwrap();
    assert_eq!(
        pg.get("refs"),
        Some(&SdkValue::List(vec![
            SdkValue::text("pg:latest"),
            SdkValue::text("postgres:17")
        ]))
    );
    assert!(pg.get_bool("usable") && pg.get_bool("fits"));
    assert_eq!(pg.get_int("need"), Some(need(1000) as i64));
    assert_eq!(need(1000), 2100);
    let x86 = row(&rows, &id('2')).unwrap();
    assert!(!x86.get_bool("usable"));
    assert_eq!(
        x86.get("reason").and_then(|r| r.get_text("kind")),
        Some("arch")
    );
    assert_eq!(
        x86.get("reason").and_then(|r| r.get_text("value")),
        Some("amd64")
    );
    let win = row(&rows, &id('3')).unwrap();
    assert_eq!(
        win.get("reason").and_then(|r| r.get_text("kind")),
        Some("os")
    );
    assert!(!row(&rows, &id('7')).unwrap().get_bool("fits"));
    for gone in ['4', '5', '6'] {
        assert!(row(&rows, &id(gone)).is_none(), "{gone}");
    }
    // free unknown: everything fits
    let (_, rows) = candidates(&standin.docker(), "arm64", None, &held);
    assert!(row(&rows, &id('7')).unwrap().get_bool("fits"));
}

#[test]
fn a_docker_failure_never_fails_the_listing() {
    let (docker, rows) = candidates(&Docker::at(None, None), "arm64", None, &[]);
    assert_eq!(docker.get("available"), Some(&SdkValue::Bool(false)));
    assert_eq!(docker.get_text("reason"), Some("not-installed"));
    assert!(rows.is_empty());
    let scratch = Scratch::new("candidates-down");
    let script = scratch.base.join("docker");
    std::fs::write(
        &script,
        "#!/bin/sh\necho 'Cannot connect to the Docker daemon at unix:///x. Is the docker daemon running?' >&2\nexit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (docker, _) = candidates(&Docker::at(Some(script), None), "arm64", None, &[]);
    assert_eq!(docker.get_text("reason"), Some("not-running"));
    assert!(docker
        .get_text("message")
        .unwrap()
        .contains("Cannot connect"));
}
