//! A native Sandbox start that failed with the native machine's daemon of
//! another build says so in words, with a fix that records this build's
//! daemon when one is found, else the home doctor (UI pass 1 FAIL 1: an
//! old pinned daemon could not open a Store this build made). One test,
//! since it sets `XDG_CACHE_HOME` for the whole process.
use layerfs_home::home::{paths, write_entity};
use layerfs_home::names::HOST_MACHINE;
use layerfs_inspect::home::get::{native_start, Build};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

const OURS: &str = "1111111111111111111111111111111111111111";
const THEIRS: &str = "2222222222222222222222222222222222222222";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-native-start-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn daemon(path: &Path, commit: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!("bin layerfs-build-identity:{commit}:clean bin"),
    )
    .unwrap();
}

fn failed_start(daemon: &Path) -> SdkRefusal {
    SdkRefusal {
        code: "Failed".to_owned(),
        message: "Sandbox t1 did not start: it stopped before its daemon said it was listening: \
                  local daemon exited before its listener line"
            .to_owned(),
        detail: SdkValue::map([
            ("daemon", SdkValue::text(daemon.display().to_string())),
            ("fix", SdkValue::text("layerfs machine doctor native")),
        ]),
    }
}

#[test]
fn a_native_start_with_a_daemon_of_another_build_names_both_builds_and_the_fix() {
    let home = folder("home");
    let cache = folder("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    let build = Build {
        commit: Some(OURS),
        dirty: false,
    };
    let pinned = home.join("old").join("layerfs-daemon");
    daemon(&pinned, THEIRS);
    let record = paths::machine_record(&home, layerfs_home::names::HOST_MACHINE);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    write_entity(
        &record,
        &SdkValue::map([
            ("name", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
            ("platform", SdkValue::text("host")),
            ("daemon", SdkValue::text(pinned.display().to_string())),
        ]),
    )
    .unwrap();

    // no daemon of this build anywhere: the home doctor names the daemon
    let said = native_start(&home, build, failed_start(&pinned));
    assert_eq!(said.code, "Failed");
    assert!(
        said.message.starts_with("Sandbox t1 did not start"),
        "{said:?}"
    );
    assert!(
        said.message.contains(&format!(
            "daemon {} was built from 22222222, this build from 11111111",
            pinned.display()
        )),
        "{said:?}"
    );
    assert!(!said.message.contains('{'), "no Debug text: {said:?}");
    assert_eq!(said.detail.get_text("fix"), Some("layerfs home doctor"));

    // this build's daemon beside the bundle: the fix records it
    let bundle = layerfs_machine::bundle_dir(std::env::consts::OS).unwrap();
    assert!(bundle.starts_with(&cache), "{bundle:?}");
    let own = bundle.join("layerfs-daemon");
    daemon(&own, OURS);
    let said = native_start(&home, build, failed_start(&pinned));
    assert_eq!(
        said.detail.get_text("fix"),
        Some(
            format!(
                "layerfs machine update {HOST_MACHINE} --daemon {}",
                own.display()
            )
            .as_str()
        )
    );

    // a daemon that names no build is not this build
    std::fs::write(&pinned, "bin with no marker").unwrap();
    let said = native_start(&home, build, failed_start(&pinned));
    assert!(
        said.message.contains(&format!(
            "daemon {} names no build: it is not this build (11111111)",
            pinned.display()
        )),
        "{said:?}"
    );

    // a daemon of this build, or a start of another daemon, is left as it is
    daemon(&pinned, OURS);
    let same = native_start(&home, build, failed_start(&pinned));
    assert_eq!(same.message, failed_start(&pinned).message);
    assert_eq!(
        same.detail.get_text("fix"),
        Some("layerfs machine doctor native")
    );
    daemon(&pinned, THEIRS);
    let other = native_start(&home, build, failed_start(&home.join("elsewhere")));
    assert_eq!(other.message, failed_start(&pinned).message);
    // a refusal that names no daemon (no start failed) comes back as it is
    let plain = SdkRefusal {
        code: "InvalidInput".to_owned(),
        message: "a Sandbox name is a plain name".to_owned(),
        detail: SdkValue::Null,
    };
    let kept = native_start(&home, build, plain);
    assert_eq!(kept.code, "InvalidInput");
    assert_eq!(kept.message, "a Sandbox name is a plain name");
    assert_eq!(kept.detail, SdkValue::Null);

    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&cache);
}
