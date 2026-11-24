//! A Linux `native` or `docker` Sandbox dials the Store URL exactly as the
//! caller and `store doctor` dial it: `localhost`, IP literals, `hostaddr`,
//! `verify-full`, host lists and Unix sockets all pass through unchanged.
#![cfg(target_os = "linux")]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Platform};
use standin::Scratch;

const SECRET: &str = "s3cr3t-pw";

fn urls() -> Vec<String> {
    [
        "postgresql://lfs:S@localhost/layerfs",
        "postgres://lfs:S@localhost:5432/layerfs?sslmode=disable",
        "postgres://lfs:S@LocalHost:5432/layerfs",
        "postgres://lfs:S@127.0.0.1:5432/layerfs?sslmode=disable",
        "postgres://lfs:S@[::1]:5432/layerfs?sslmode=disable",
        "postgres://lfs:S@localhost:5432/layerfs?hostaddr=::1",
        "postgres://lfs:S@localhost:5432/layerfs?hostaddr=127.0.0.1",
        "postgres://lfs:S@localhost:5432/layerfs?sslmode=verify-full",
        "postgres://lfs:S@127.0.0.1:5432/layerfs?sslmode=verify-full",
        "postgres://lfs:S@[::1]:5432/layerfs?sslmode=verify-full",
        "postgres://lfs:S@localhost:5432,[::1]:5433/layerfs",
        "postgres://lfs:S@/layerfs?host=/var/run/postgresql",
        "postgres://lfs:S@db.example.com:5432/layerfs?sslmode=verify-full",
    ]
    .iter()
    .map(|u| u.replace(":S@", &format!(":{SECRET}@")))
    .collect()
}

#[test]
fn native_and_docker_dial_every_url_as_given() {
    let scratch = Scratch::new("reach-localhost");
    let mut changed = Vec::new();
    for kind in [Platform::Native, Platform::Docker] {
        let spec = scratch.spec(kind);
        for url in urls() {
            let outcome = machine::reach(&spec, "pg", &url);
            if outcome.as_deref() != Ok(url.as_str()) {
                changed.push(format!("{kind:?} {url} -> {outcome:?}"));
            }
        }
    }
    assert!(changed.is_empty(), "{}", changed.join("\n"));
    assert_eq!(scratch.log(), "", "no tool is asked for a shared loopback");
}
