//! Which Docker sockets the host names and which one a never-started docker
//! machine takes, from `DOCKER_HOST`, the Docker CLI's context files in a
//! scratch HOME and the engines' own sockets; what a socket does when
//! pinged; and the fallback folders searched after `PATH`.
#![cfg(unix)]
#[path = "../support/engine.rs"]
mod support;
use layerfs_machine::platform::{docker_automatic, docker_candidates, find_tool, search_path, socket_state, tool_fallbacks, DockerCandidate, DockerEnv, SocketState, Tools};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A short scratch folder (Unix socket paths are short), removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let dir = PathBuf::from(format!(
            "/tmp/lfs-ds-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A context `name` on `host`, as `docker context create` writes it (its
/// folder is the name's digest; any folder name is read).
fn context(home: &Path, folder: &str, name: &str, host: &str) {
    write(
        &home
            .join(".docker/contexts/meta")
            .join(folder)
            .join("meta.json"),
        &format!(
            r#"{{"Name":"{name}","Metadata":{{"Description":"x"}},"Endpoints":{{"docker":{{"Host":"{host}","SkipTLSVerify":false}}}}}}"#
        ),
    );
}

fn env(home: &Path) -> DockerEnv {
    DockerEnv {
        home: Some(home.to_owned()),
        ..DockerEnv::default()
    }
}

fn sources(found: &[DockerCandidate]) -> Vec<(&str, Option<String>, bool)> {
    found
        .iter()
        .map(|c| {
            (
                c.source.as_str(),
                c.path.as_ref().map(|p| p.display().to_string()),
                c.usable(),
            )
        })
        .collect()
}

#[test]
fn candidates_are_docker_host_then_contexts_then_engine_sockets_then_the_default() {
    let dir = Dir::new();
    let home = &dir.0;
    let desktop = home.join(".docker/run/docker.sock");
    let colima = home.join(".colima/default/docker.sock");
    let orbstack = home.join(".orbstack/run/docker.sock");
    write(
        &home.join(".docker/config.json"),
        r#"{"auths":{},"currentContext":"colima"}"#,
    );
    context(
        home,
        "aa",
        "desktop-linux",
        &format!("unix://{}", desktop.display()),
    );
    context(
        home,
        "bb",
        "colima",
        &format!("unix://{}", colima.display()),
    );
    context(home, "cc", "remote", "tcp://10.0.0.1:2376");
    // Docker Desktop's and OrbStack's sockets exist; colima's does not
    write(&desktop, "");
    write(&orbstack, "");
    let mut env = env(home);
    env.host = Some("unix:///tmp/lfs-ds-elsewhere.sock".into());
    let s = |p: &Path| Some(p.display().to_string());
    assert_eq!(
        sources(&docker_candidates(&env)),
        [
            (
                "env:DOCKER_HOST",
                Some("/tmp/lfs-ds-elsewhere.sock".into()),
                true
            ),
            ("context:colima", s(&colima), true),
            ("context:desktop-linux", s(&desktop), true),
            ("context:remote", None, false),
            ("orbstack", s(&orbstack), true),
            ("default", Some("/var/run/docker.sock".into()), true),
        ],
        "the current context first; Docker Desktop's socket once, as its context"
    );
    let remote = docker_candidates(&env)
        .into_iter()
        .find(|c| c.source == "context:remote")
        .unwrap();
    assert_eq!(remote.host, "tcp://10.0.0.1:2376");
    // DOCKER_HOST names the socket a never-started machine takes
    let chosen = docker_automatic(&env);
    assert_eq!(chosen.source, "env:DOCKER_HOST");
    assert_eq!(
        chosen.path.as_deref(),
        Some(Path::new("/tmp/lfs-ds-elsewhere.sock"))
    );
}

#[test]
fn the_automatic_socket_skips_endpoints_it_cannot_use() {
    let dir = Dir::new();
    let home = &dir.0;
    let colima = home.join(".colima/default/docker.sock");
    write(
        &home.join(".docker/config.json"),
        r#"{"currentContext":"colima"}"#,
    );
    context(
        home,
        "bb",
        "colima",
        &format!("unix://{}", colima.display()),
    );
    context(home, "cc", "remote", "tcp://10.0.0.1:2376");
    let mut env = env(home);
    // a tcp DOCKER_HOST is listed, not followed: the current context is next
    env.host = Some("tcp://10.0.0.2:2375".into());
    let found = docker_candidates(&env);
    assert_eq!(found[0].source, "env:DOCKER_HOST");
    assert!(!found[0].usable());
    let chosen = docker_automatic(&env);
    assert_eq!(chosen.source, "context:colima");
    assert_eq!(chosen.path.as_deref(), Some(colima.as_path()));
    // DOCKER_CONTEXT overrides config.json; a tcp context is not followed
    env.context = Some("remote".into());
    assert_eq!(docker_candidates(&env)[1].source, "context:remote");
    assert_eq!(docker_automatic(&env).source, "default");
    // `default` is Docker's own context: no file, the default socket
    env.context = Some("default".into());
    assert_eq!(docker_automatic(&env).source, "default");
}

#[test]
fn without_docker_host_or_a_context_an_engine_socket_that_exists_comes_before_the_default() {
    let dir = Dir::new();
    let home = &dir.0;
    let env = env(home);
    assert_eq!(docker_automatic(&env).source, "default");
    assert_eq!(
        docker_automatic(&env).path.as_deref(),
        Some(Path::new("/var/run/docker.sock"))
    );
    let orbstack = home.join(".orbstack/run/docker.sock");
    write(&orbstack, "");
    assert_eq!(docker_automatic(&env).source, "orbstack");
    let colima = home.join(".colima/default/docker.sock");
    write(&colima, "");
    let chosen = docker_automatic(&env);
    assert_eq!(
        (chosen.source.as_str(), chosen.path),
        ("colima", Some(colima))
    );
    // DOCKER_CONFIG moves the config folder, not the engines' sockets
    let other = home.join("elsewhere");
    write(&other.join("config.json"), r#"{"currentContext":"mine"}"#);
    let mine = home.join("mine.sock");
    write(
        &other.join("contexts/meta/x/meta.json"),
        &format!(
            r#"{{"Name":"mine","Endpoints":{{"docker":{{"Host":"unix://{}"}}}}}}"#,
            mine.display()
        ),
    );
    let moved = DockerEnv {
        config: Some(other),
        ..env.clone()
    };
    assert_eq!(docker_automatic(&moved).source, "context:mine");
    assert_eq!(docker_automatic(&moved).path, Some(mine));
}

#[test]
fn a_socket_answers_is_absent_is_denied_or_is_silent() {
    let dir = Dir::new();
    let short = Duration::from_millis(300);
    assert_eq!(
        socket_state(&dir.0.join("none.sock"), short),
        SocketState::Absent
    );
    assert_eq!(
        socket_state(&dir.0.join("no/such/dir.sock"), short),
        SocketState::Absent
    );
    let answering = dir.0.join("a.sock");
    let _engine = support::engine(&answering, "HTTP/1.1 200 OK\r\nApi-Version: 1.47\r\n\r\nOK");
    assert_eq!(socket_state(&answering, short), SocketState::Answering);
    // a listener that answers something else, and a socket file nobody listens on
    let other = dir.0.join("o.sock");
    let _other = support::engine(&other, "SSH-2.0-not-docker\r\n");
    assert_eq!(socket_state(&other, short), SocketState::Silent);
    let stale = dir.0.join("s.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    assert_eq!(socket_state(&stale, short), SocketState::Silent);
    // EACCES: the socket's file refuses this user (root is never refused)
    let denied = dir.0.join("d.sock");
    let _denied = support::engine(&denied, "HTTP/1.1 200 OK\r\n\r\nOK");
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
    let expected = if nix::unistd::geteuid().is_root() {
        SocketState::Answering
    } else {
        SocketState::Denied
    };
    assert_eq!(socket_state(&denied, short), expected);
    for state in [
        SocketState::Answering,
        SocketState::Absent,
        SocketState::Denied,
        SocketState::Silent,
    ] {
        assert!(["answering", "absent", "denied", "silent"].contains(&state.as_str()));
    }
}

#[test]
fn tools_are_found_in_the_fallback_folders_after_path_and_tests_can_empty_them() {
    let dir = Dir::new();
    let first = dir.0.join("first");
    let fallback = dir.0.join("fallback");
    for (folder, body) in [(&first, "echo first"), (&fallback, "echo fallback")] {
        fs::create_dir_all(folder).unwrap();
        let tool = folder.join("limactl");
        fs::write(&tool, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(fallback.join("container"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(
        fallback.join("container"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let path = search_path(
        &first.clone().into_os_string(),
        &[fallback.clone(), first.clone()],
    );
    assert_eq!(
        std::env::split_paths(&path).collect::<Vec<_>>(),
        [first.clone(), fallback.clone()],
        "after PATH, each folder once"
    );
    let tools = Tools {
        path,
        docker_socket: PathBuf::from("/var/run/docker.sock"),
    };
    // PATH first; a tool only in a fallback folder is found there
    assert_eq!(find_tool(&tools, "limactl"), Some(first.join("limactl")));
    assert_eq!(
        find_tool(&tools, "container"),
        Some(fallback.join("container"))
    );
    assert_eq!(find_tool(&tools, "docker"), None);

    // the defaults, then LAYERFS_TOOL_FALLBACKS replacing them (empty: none)
    let before = std::env::var_os("LAYERFS_TOOL_FALLBACKS");
    std::env::remove_var("LAYERFS_TOOL_FALLBACKS");
    let defaults = tool_fallbacks();
    if cfg!(target_os = "macos") {
        assert_eq!(
            defaults,
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/Applications/Docker.app/Contents/Resources/bin"),
            ]
        );
    } else {
        assert!(defaults.is_empty());
    }
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    assert!(tool_fallbacks().is_empty());
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", &fallback);
    assert_eq!(tool_fallbacks(), [fallback.clone()]);
    let from_env = Tools::from_env();
    assert!(std::env::split_paths(&from_env.path).any(|d| d == fallback));
    match before {
        Some(value) => std::env::set_var("LAYERFS_TOOL_FALLBACKS", value),
        None => std::env::remove_var("LAYERFS_TOOL_FALLBACKS"),
    }
}
