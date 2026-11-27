//! Each Workspace's own cgroup usage in `sandbox.get`: what the side that
//! runs a Sandbox reads of its `ws-<workspace>` groups
//! (`inspect::process::workspaces`, from a cgroup v2 folder given as an
//! argument) and how `run_folder::merged` puts it on each listed Workspace
//! as `usage` with `usage_reason`: a group that is there, a Workspace with
//! no group (no command runs in it), and a Sandbox with no cgroup at all
//! (a host-process Sandbox, another runtime, a cgroup v1 kernel, a group
//! that ended), and a side that did not answer.
//!
//! Nothing runs and no cgroup is touched: the hierarchy is a folder this
//! test writes, with the files the kernel would show.
#![cfg(unix)]
use layerfs_home::json::print;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::inspect::process;
use layerfs_sandbox::record::{DaemonRecord, Local};
use layerfs_sandbox::run_folder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A new folder under the build's own temporary folder.
fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "lfs-wsusage-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The runtime id the daemon records of this test name.
fn id() -> String {
    "ab".repeat(32)
}

/// A daemon record of `runtime`.
fn daemon(runtime: &'static str) -> DaemonRecord {
    DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: "layerfs-0a1b2c3d-s".to_owned(),
        container: id(),
        runtime,
        uid: 1000,
        gid: 1000,
        directory: "/nowhere".to_owned(),
        process: (1, 1, "0".repeat(32)),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    })
}

/// Daemon Workspace id `n`.
fn ws(n: u8) -> String {
    format!("{n:064x}")
}

/// What `process::workspaces` answered, as JSON text of its fields.
fn read(daemon: &DaemonRecord, root: &Path) -> SdkValue {
    SdkValue::Map(process::workspaces(daemon, root))
}

/// A listing of Workspaces `ids` as a daemon answers it.
fn listed(ids: &[String]) -> SdkValue {
    let items = ids.iter().enumerate().map(|(n, id)| {
        SdkValue::map([
            ("workspace", SdkValue::text(id.as_str())),
            ("namespace", SdkValue::Int(n as i64 + 1)),
        ])
    });
    SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("workspaces", SdkValue::List(items.collect())),
    ])
}

/// Each listed Workspace's `(usage, usage_reason)` after `merged` with `read`.
fn shown(ids: &[String], read: Option<&SdkValue>) -> Vec<(String, Option<String>)> {
    let merged = run_folder::merged(listed(ids), read);
    // the per-Workspace read stays on the Workspaces, not on the listing
    for field in ["workspace_usage", "workspace_usage_reason"] {
        assert!(merged.get(field).is_none(), "{field}: {merged:?}");
    }
    let Some(SdkValue::List(items)) = merged.get("workspaces") else {
        panic!("{merged:?}")
    };
    (items.iter())
        .map(|item| {
            // `overlay_bytes` is still there, beside `usage`
            assert!(item.get("overlay_bytes").is_some(), "{item:?}");
            let usage = item.get("usage").unwrap_or_else(|| panic!("{item:?}"));
            (
                print(usage),
                item.get_text("usage_reason").map(str::to_owned),
            )
        })
        .collect()
}

fn null(reason: &str) -> (String, Option<String>) {
    ("null".to_owned(), Some(reason.to_owned()))
}

#[test]
fn a_workspace_s_group_is_read_and_one_without_a_group_has_no_command() {
    let root = scratch("present");
    std::fs::write(root.join("cgroup.controllers"), "cpu memory pids\n").unwrap();
    let sandbox = root.join("layerfs").join(id());
    std::fs::create_dir_all(&sandbox).unwrap();
    // Workspace 1 runs a command: its group has processor time only, as
    // below a Sandbox group that gives its children no controller
    let one = sandbox.join(format!("ws-{}", ws(1)));
    std::fs::create_dir_all(one.join(format!("exec-{}", "e1"))).unwrap();
    std::fs::write(one.join("cgroup.procs"), "").unwrap();
    std::fs::write(
        one.join("cpu.stat"),
        "usage_usec 4321\nuser_usec 4000\nsystem_usec 321\n",
    )
    .unwrap();
    // Workspace 2's group has every file, as with the controllers given
    let two = sandbox.join(format!("ws-{}", ws(2)));
    std::fs::create_dir_all(&two).unwrap();
    std::fs::write(two.join("cgroup.procs"), "17\n").unwrap();
    std::fs::write(two.join("cpu.stat"), "usage_usec 99\n").unwrap();
    std::fs::write(two.join("memory.current"), "1048576\n").unwrap();
    std::fs::write(two.join("memory.peak"), "2097152\n").unwrap();
    std::fs::write(
        two.join("memory.stat"),
        "anon 1\nfile_mapped 2\nfile 4096\n",
    )
    .unwrap();
    std::fs::write(two.join("pids.current"), "3\n").unwrap();
    std::fs::write(two.join("memory.events"), "oom 0\noom_kill 0\n").unwrap();
    // a folder removed while the read ran (no `cgroup.procs`), a folder
    // that is not a Workspace group and a group with no Workspace name are
    // not listed
    std::fs::create_dir_all(sandbox.join(format!("ws-{}", ws(5)))).unwrap();
    std::fs::create_dir_all(sandbox.join("other")).unwrap();
    std::fs::create_dir_all(sandbox.join("ws-")).unwrap();
    std::fs::write(sandbox.join("ws-").join("cgroup.procs"), "").unwrap();

    let read = read(&daemon("namespaces"), &root);
    assert_eq!(
        print(&read),
        format!(
            concat!(
                r#"{{"workspace_usage":["#,
                r#"{{"workspace":"{}","usage":{{"cpu_usec":4321,"memory_current":null,"memory_peak":null,"memory_file":null,"pids":null,"oom_kills":null}},"usage_reason":"no_controller"}},"#,
                r#"{{"workspace":"{}","usage":{{"cpu_usec":99,"memory_current":1048576,"memory_peak":2097152,"memory_file":4096,"pids":3,"oom_kills":0}}}}"#,
                r#"]}}"#
            ),
            ws(1),
            ws(2)
        )
    );
    // Workspace 3 runs no command, so it has no group; Workspace 5's
    // folder is not a group
    let ids = [ws(1), ws(2), ws(3), ws(5)];
    let seen = shown(&ids, Some(&read));
    assert_eq!(
        seen[0],
        (
            r#"{"cpu_usec":4321,"memory_current":null,"memory_peak":null,"memory_file":null,"pids":null,"oom_kills":null}"#.to_owned(),
            Some("no_controller".to_owned())
        )
    );
    assert_eq!(
        seen[1],
        (
            r#"{"cpu_usec":99,"memory_current":1048576,"memory_peak":2097152,"memory_file":4096,"pids":3,"oom_kills":0}"#.to_owned(),
            None
        )
    );
    assert_eq!(seen[2], null("no_command"));
    assert_eq!(seen[3], null("no_command"));

    // the Sandbox's group with no Workspace group at all: every Workspace
    // has no command running
    for group in [one, two] {
        std::fs::remove_dir_all(group).unwrap();
    }
    let read = self::read(&daemon("namespaces"), &root);
    assert_eq!(
        shown(&ids[..2], Some(&read)),
        vec![null("no_command"), null("no_command")]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_sandbox_with_no_cgroup_says_why_for_each_workspace() {
    let root = scratch("none");
    let ids = [ws(1), ws(2)];
    let each = |daemon: &DaemonRecord, root: &Path, reason: &str| {
        let read = read(daemon, root);
        assert_eq!(
            print(&read),
            format!(r#"{{"workspace_usage":null,"workspace_usage_reason":"{reason}"}}"#)
        );
        assert_eq!(shown(&ids, Some(&read)), vec![null(reason), null(reason)]);
    };
    // a host-process Sandbox has no cgroup, whatever the kernel has
    each(&daemon("process"), &root, "host_process");
    // a runtime this build reads no usage of is not taken for a host process
    each(&daemon("firecracker"), &root, "not_supported");
    // a hierarchy that is not cgroup v2 (no `cgroup.controllers`)
    each(&daemon("namespaces"), &root, "cgroup_v1");
    // v2, and the Sandbox's group is not there: it ended
    std::fs::write(root.join("cgroup.controllers"), "cpu memory pids\n").unwrap();
    each(&daemon("namespaces"), &root, "no_cgroup");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_side_that_did_not_answer_leaves_each_workspace_not_observed() {
    let ids = [ws(1), ws(2)];
    let not = vec![null("not_observed"), null("not_observed")];
    // no answer from the side that runs the Sandbox
    assert_eq!(shown(&ids, None), not);
    assert_eq!(shown(&ids, Some(&run_folder::not_observed())), not);
    // a worker of an earlier build: overlay sizes, no Workspace usage
    let earlier = SdkValue::map([("overlay_sizes", SdkValue::List(Vec::new()))]);
    assert_eq!(shown(&ids, Some(&earlier)), not);
}
