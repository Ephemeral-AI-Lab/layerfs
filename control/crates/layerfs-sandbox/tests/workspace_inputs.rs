//! What `workspace.mount` and `workspace.exec` check and keep before any
//! daemon is asked (call surface 2.0): a Workspace's default name from its
//! Branch, the folder a command starts in, its environment, and a stream
//! past the 1 MiB inline window spilled to a named file with its tail kept.
use layerfs_sandbox::exec::{checked_env, start_folder};
use layerfs_sandbox::output::{self, Place, Spool, INLINE};
use layerfs_sandbox::workspace::calls::default_name;
use std::io::Write;

#[test]
fn a_default_name_is_the_branch_s_made_valid() {
    let name = |branch: &str| default_name("sb", branch).map(|n| n.as_str().to_owned());
    assert_eq!(name("dev").unwrap(), "dev");
    assert_eq!(name("Feature/Login").unwrap(), "feature-login");
    assert_eq!(name("-.v2_").unwrap(), "v2");
    assert_eq!(name(&"a".repeat(80)).unwrap().len(), 63);
    let refused = name("__").unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs workspace mount sb __ --name <name>")
    );
}

#[test]
fn a_command_starts_in_a_folder_of_the_workspace() {
    let root = "/w/1";
    assert_eq!(start_folder(root, None).unwrap(), "/w/1");
    assert_eq!(start_folder(root, Some("/")).unwrap(), "/w/1");
    assert_eq!(
        start_folder(root, Some("src/app/")).unwrap(),
        "/w/1/src/app"
    );
    assert_eq!(start_folder(root, Some("/src")).unwrap(), "/w/1/src");
    for bad in ["..", "src/../..", "a//b", "./a", "a\0b"] {
        let refused = start_folder(root, Some(bad)).unwrap_err();
        assert_eq!(refused.code, "InvalidInput", "{bad:?}");
    }
}

#[test]
fn env_is_k_equals_v() {
    let env = |entries: &[&str]| {
        let entries: Vec<String> = entries.iter().map(|e| (*e).to_owned()).collect();
        checked_env(&entries).map_err(|r| r.code)
    };
    assert_eq!(env(&["A=1", "_B=", "C_2=x=y"]), Ok(()));
    for bad in ["A", "=1", "1A=x", "A-B=1", "A=\0"] {
        assert_eq!(env(&[bad]), Err("InvalidInput".to_owned()), "{bad:?}");
    }
}

#[test]
fn a_stream_past_the_window_spills_to_its_file_and_keeps_its_tail() {
    let scratch = std::env::temp_dir().join(format!("lfs-spool-{}", std::process::id()));
    let tmp = scratch.join("tmp");
    let folder = output::folder(tmp.to_str().unwrap(), "ws1").unwrap();
    assert_eq!(folder, scratch.join("exec").join("ws1"));
    let place = Place {
        folder: folder.clone(),
        exec: "exec-0000000000000001".to_owned(),
    };
    let mut stdout = Spool::at(Some(&place), "stdout");
    let mut stderr = Spool::at(Some(&place), "stderr");
    let size = 3 * INLINE + 5;
    for at in 0..size / 4096 + 1 {
        let left = size - (at * 4096).min(size);
        let chunk = vec![b'a' + (at % 26) as u8; left.min(4096)];
        stdout.write_all(&chunk).unwrap();
    }
    stderr.write_all(b"small\n").unwrap();
    let fields = output::fields(stdout, stderr).unwrap();
    let get = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
    };
    use layerfs_bridge::control::SdkValue;
    assert_eq!(get("stdout"), Some(SdkValue::Null));
    let Some(SdkValue::Text(tail)) = get("stdoutTail") else {
        panic!("{fields:?}")
    };
    assert_eq!(tail.len(), INLINE);
    let file = folder.join("exec-0000000000000001.stdout");
    assert_eq!(
        get("stdoutFile"),
        Some(SdkValue::Text(file.display().to_string()))
    );
    let kept = std::fs::read(&file).unwrap();
    assert_eq!(kept.len(), size);
    assert!(kept.ends_with(tail.as_bytes()));
    assert_eq!(get("stdoutBytes"), Some(SdkValue::Int(size as i64)));
    assert_eq!(get("stderr"), Some(SdkValue::Text("small\n".to_owned())));
    assert_eq!(get("stderrFile"), None);
    let Some(SdkValue::Text(id)) = get("output") else {
        panic!("{fields:?}")
    };
    assert!(output::release(&id).unwrap());
    // the Workspace's unmount removes its output folder
    output::remove(tmp.to_str().unwrap(), "ws1").unwrap();
    assert!(!folder.exists());
    output::remove(tmp.to_str().unwrap(), "ws1").unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
}
