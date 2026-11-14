//! Live native machine force stop and Sandbox force close with mounted, executing Workspaces.
#![cfg(all(target_os = "macos", feature = "machine"))]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use std::{
    path::Path,
    process::{Child, Command},
    time::{Duration, Instant},
};
fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}
fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(
        Some(home),
        name,
        SdkValue::Map(
            args.iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        ),
        &mut |_| {},
    )
    .unwrap_or_else(|error| panic!("{name}: {error:?}"))
}
struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
#[test]
fn exec_helper() {
    let Ok(home) = std::env::var("LAYERFS_FORCE_NATIVE_HELPER") else {
        return;
    };
    let home = Path::new(&home);
    let name = std::env::var("LAYERFS_FORCE_NATIVE_NAME").unwrap();
    let _ = layerfs_api::call(
        Some(home),
        "workspace.exec",
        SdkValue::map([
            ("ws", t(&format!("{name}:work"))),
            (
                "command",
                t(&format!(
                    "printf dirty > pending.txt; echo $$ > '{}'; sleep 60",
                    home.join(format!("{name}.pid")).display()
                )),
            ),
        ]),
        &mut |_| {},
    );
}
fn live_case(home: &Path, name: &str, machine: bool) {
    call(
        home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t(name)),
            ("machine", t("native")),
        ],
    );
    let mounted = call(
        home,
        "workspace.mount",
        &[
            ("sandbox", t(name)),
            ("branch", t("work")),
            ("name", t("work")),
        ],
    );
    let mount = mounted.get_text("path").unwrap().to_owned();
    let record = read_record(&paths::sandbox_record(home, name))
        .unwrap()
        .unwrap();
    let daemon = record.get("daemon").unwrap();
    let process = LocalProcess::observe(daemon.get_int("pid").unwrap() as u32).unwrap();
    let mut helper = Helper(
        Command::new(std::env::current_exe().unwrap())
            .args(["exec_helper", "--exact", "--nocapture"])
            .env("LAYERFS_FORCE_NATIVE_HELPER", home)
            .env("LAYERFS_FORCE_NATIVE_NAME", name)
            .spawn()
            .unwrap(),
    );
    let marker = home.join(format!("{name}.pid"));
    let until = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(Instant::now() < until, "exec never became ready");
        std::thread::sleep(Duration::from_millis(20));
    }
    let command = LocalProcess::observe(
        std::fs::read_to_string(marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    )
    .unwrap();
    assert!(process.alive() && command.alive());
    if machine {
        let stopped = call(
            home,
            "machine.stop",
            &[("name", t("native")), ("force", SdkValue::Bool(true))],
        );
        assert_eq!(stopped.get_text("state"), Some("ready"));
        assert_eq!(
            call(home, "sandbox.get", &[("name", t(name))]).get_text("state"),
            Some("gone")
        );
        // An explicit second stop of retained terminal records is an idempotent no-op.
        let again = call(
            home,
            "machine.stop",
            &[("name", t("native")), ("force", SdkValue::Bool(true))],
        );
        assert_eq!(again.get_text("state"), Some("ready"));
    } else {
        let closed = call(
            home,
            "sandbox.close",
            &[("name", t(name)), ("force", SdkValue::Bool(true))],
        );
        assert!(!paths::sandbox_record(home, name).exists());
        assert!(
            closed.get("unknown").is_some(),
            "active Workspace frontier is explicitly unknown"
        );
    }
    assert!(
        !process.alive() && !command.alive(),
        "exact daemon and command processes ended"
    );
    let until = Instant::now() + Duration::from_secs(5);
    while helper.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < until, "exec helper remained live");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mounts = Command::new("/sbin/mount").output().unwrap();
    assert!(
        !String::from_utf8_lossy(&mounts.stdout).contains(&format!(" on {mount} ")),
        "native mount remains attached"
    );
    assert!(paths::store_file(home, "shop").exists());
    assert!(paths::machine_record(home, "native").exists());
    let saved = call(
        home,
        "file.read",
        &[
            ("project", t("shop/app")),
            ("branch", t("work")),
            ("path", t("/README.md")),
        ],
    );
    assert_eq!(saved.get_text("text"), Some("saved\n"));
}
/// A recorded pid that now names an unrelated live process: the forced stop
/// refuses that Sandbox before any signal, leaves no op on `native`, and
/// signals neither process.
fn identity_case(home: &Path) {
    call(
        home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t("moved")),
            ("machine", t("native")),
        ],
    );
    let path = paths::sandbox_record(home, "moved");
    let original = read_record(&path).unwrap().unwrap();
    let daemon = original.get("daemon").unwrap().clone();
    let real = LocalProcess::observe(daemon.get_int("pid").unwrap() as u32).unwrap();
    let mut other = Helper(Command::new("/bin/sleep").arg("30").spawn().unwrap());
    let moved = home::with_field(daemon, "pid", SdkValue::Int(i64::from(other.0.id())));
    write_record(&path, &home::with_field(original.clone(), "daemon", moved)).unwrap();
    let refused = layerfs_api::call(
        Some(home),
        "machine.stop",
        SdkValue::map([("name", t("native")), ("force", SdkValue::Bool(true))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(
        format!("{:?}", refused.detail).contains("moved"),
        "refused_sandboxes names it: {refused:?}"
    );
    assert!(
        other.0.try_wait().unwrap().is_none(),
        "the unrelated process was signalled"
    );
    assert!(real.alive(), "the real daemon was signalled");
    let op = read_record(&paths::machine_record(home, "native"))
        .unwrap()
        .and_then(|record| record.get("op").cloned());
    assert!(
        op.is_none(),
        "a refusal before any signal left {op:?} on native"
    );
    write_record(&path, &original).unwrap();
    call(
        home,
        "sandbox.close",
        &[("name", t("moved")), ("force", SdkValue::Bool(true))],
    );
    assert!(!real.alive(), "the restored record's daemon ended");
}
#[test]
fn native_force_stops_live_daemon_and_command_and_detaches_mount_without_opening_store() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: explicit LAYERFS_DAEMON required for live native force proof");
        return;
    };
    let home = std::env::temp_dir().join(format!("lfs-native-force-{}", std::process::id()));
    home::open(&home).unwrap();
    let source = home.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("README.md"), b"saved\n").unwrap();
    call(&home, "store.create", &[("name", t("shop"))]);
    assert_eq!(
        read_record(&paths::store_record(&home, "shop"))
            .unwrap()
            .unwrap()
            .get_text("profile"),
        Some("disposable")
    );
    call(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&source.display().to_string())),
        ],
    );
    call(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/app")),
            ("name", t("work")),
            ("from_branch", t("main")),
        ],
    );
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    live_case(&home, "live", true);
    live_case(&home, "close", false);
    identity_case(&home);
    std::fs::remove_dir_all(&home).unwrap();
}
