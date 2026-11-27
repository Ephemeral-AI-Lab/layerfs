//! Same as fresh (#804 §11 family C): the fixture scenario run by this
//! build (`tools/store_format.py capture`, with this build's `layerfs`)
//! gives, per Branch, the same files and the same number of Commits as the
//! old build's manifest, and so as the upgraded fixture (family A proves the
//! upgraded fixture equals that manifest). Commit ids differ by time.
//!
//! Ignored: the scenario starts the `docker` machine and mounts a
//! Workspace there, so it needs Docker, this build's Linux bundle and more
//! than the fast lane's time. Run at integration:
//!
//! ```text
//! LAYERFS_TEST_BUNDLE=<cache>/layerfs/bundle/<version>/linux-<arch> \
//!   cargo test -p layerfs-api --test store_upgrade_fresh -- --ignored
//! ```
//!
//! (`capture` refreshes `fixtures/store-formats/SHA256SUMS` from the
//! fixtures it finds, which leaves it as it is: the capture goes to a
//! folder outside them.)
#[path = "../support/upgrade.rs"]
mod upgrade_support;
use layerfs_home::json;
use std::path::Path;
use upgrade_support::*;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");

#[test]
#[ignore = "starts the docker machine; needs LAYERFS_TEST_BUNDLE (see the file's doc)"]
fn the_scenario_run_by_this_build_writes_what_the_fixture_holds() {
    let Ok(bundle) = std::env::var("LAYERFS_TEST_BUNDLE") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_BUNDLE unset");
        return;
    };
    let cli = Path::new(REPO).join("target/debug/layerfs");
    let out = tempdir("fresh").join("capture");
    let sums = Path::new(FIXTURES).join("SHA256SUMS");
    let frozen = std::fs::read(&sums).unwrap();
    let status = std::process::Command::new("python3")
        .arg(Path::new(REPO).join("tools/store_format.py"))
        .args(["capture", "--engine", "sqlite", "--cli"])
        .arg(&cli)
        .arg("--bundle")
        .arg(&bundle)
        .arg("--out")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "capture with this build");
    assert_eq!(std::fs::read(&sums).unwrap(), frozen, "SHA256SUMS changed");
    let fresh = json::parse(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    let old = manifest("sqlite/v2");
    let projects = |m: &layerfs_protocol::sdk::SdkValue| fields(m.get("projects").unwrap());
    for ((name, now), (old_name, then)) in projects(&fresh).iter().zip(projects(&old).iter()) {
        assert_eq!(name, old_name);
        let branches = |p: &layerfs_protocol::sdk::SdkValue| fields(p.get("branches").unwrap());
        let (now, then) = (branches(now), branches(then));
        let names = |b: &[(String, layerfs_protocol::sdk::SdkValue)]| {
            b.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names(&now), names(&then), "{name}: Branches");
        for ((branch, a), (_, b)) in now.iter().zip(then.iter()) {
            assert_eq!(a.get("files"), b.get("files"), "{name} {branch}: files");
            assert_eq!(
                list(a.get("commits")).len(),
                list(b.get("commits")).len(),
                "{name} {branch}: Commits"
            );
        }
    }
    let _ = std::fs::remove_dir_all(out.parent().unwrap());
}
