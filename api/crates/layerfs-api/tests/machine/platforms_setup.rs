//! What `home get --with platforms` (`machine.platforms` before the call
//! surface 2.0) tells a client setting LayerFS up (spec §2.3):
//! it needs no home and creates none; a platform that runs a worker names
//! the bundle it lacks (`bundle:<dir>`); options carry their defaults; the
//! docker row names its socket (where it comes from and what it does) and
//! the host's candidates; a recorded socket wins and `--docker ""` clears
//! it. And `machine.create --os-image stock` (§2.4) takes the provider's own
//! image over a prepared one. One test: it owns this process's environment.
#![cfg(feature = "machine")]
use layerfs_protocol::sdk::SdkValue;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn rows(home: &Path) -> Vec<SdkValue> {
    let asked = [
        ("detail", SdkValue::Bool(true)),
        ("with", SdkValue::List(vec![t("platforms")])),
    ];
    let answer = call(home, "home.get", &asked).unwrap();
    let platforms = answer.get("platforms").cloned().unwrap_or_default();
    let Some(SdkValue::List(rows)) = platforms.get("items") else {
        panic!("{answer:?}")
    };
    rows.clone()
}

fn row<'a>(rows: &'a [SdkValue], platform: &str) -> &'a SdkValue {
    rows.iter()
        .find(|r| r.get_text("platform") == Some(platform))
        .unwrap()
}

fn missing(row: &SdkValue) -> Vec<String> {
    match row.get("missing") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| match item {
                SdkValue::Text(text) => text.clone(),
                other => panic!("{other:?}"),
            })
            .collect(),
        other => panic!("{other:?}"),
    }
}

fn socket(row: &SdkValue) -> (String, String, String) {
    let socket = row.get("socket").unwrap();
    let text = |name| socket.get_text(name).unwrap().to_owned();
    (text("path"), text("source"), text("state"))
}

fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn platforms_describe_the_host_without_a_home_and_create_takes_the_stock_image() {
    let _env = crate::env_lock();
    let root = PathBuf::from(format!("/tmp/lfs-mps-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for dir in ["bin", "user", "bundle", "cache"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::env::set_var("PATH", root.join("bin"));
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    std::env::set_var("HOME", root.join("user"));
    std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
    std::env::remove_var("DOCKER_CONTEXT");
    std::env::remove_var("DOCKER_CONFIG");
    let engine = root.join("engine.sock");
    std::env::set_var("DOCKER_HOST", format!("unix://{}", engine.display()));
    let bundle = root.join("bundle");
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let need = format!("bundle:{}", bundle.display());

    // no home: none is made, and the host is described
    let home = root.join("home");
    let found = rows(&home);
    assert!(!home.exists(), "home get --with platforms made the home");
    let worker_platforms: Vec<&str> = found
        .iter()
        .filter(|r| r.get_text("sqlite") == Some("worker"))
        .map(|r| r.get_text("platform").unwrap())
        .collect();
    assert!(worker_platforms.contains(&"docker"), "{worker_platforms:?}");
    for r in &found {
        let platform = r.get_text("platform").unwrap();
        // a platform that runs a worker needs the bundle; none other does
        assert_eq!(
            missing(r).contains(&need),
            worker_platforms.contains(&platform),
            "{r:?}"
        );
        if worker_platforms.contains(&platform) {
            assert_eq!(r.get("ready"), Some(&SdkValue::Bool(false)));
        }
        // each option's default: the VMs' size, null for the rest
        let Some(SdkValue::List(options)) = r.get("options") else {
            panic!("{r:?}")
        };
        for option in options {
            let default = option.get("default").unwrap();
            let expected = match option.get_text("option").unwrap() {
                "cpus" => SdkValue::Int(2),
                "memory" => SdkValue::Int(4 << 30),
                "disk" => SdkValue::Int(20 << 30),
                _ => SdkValue::Null,
            };
            assert_eq!(default, &expected, "{platform}: {option:?}");
        }
    }
    let lima = row(&found, "lima");
    let Some(SdkValue::List(options)) = lima.get("options") else {
        panic!()
    };
    let names: Vec<_> = options
        .iter()
        .map(|o| o.get_text("option").unwrap())
        .collect();
    for size in ["cpus", "memory", "disk"] {
        assert!(names.contains(&size), "{names:?}");
    }
    // only the docker row has a socket and candidates
    assert_eq!(lima.get("socket"), Some(&SdkValue::Null));
    let docker = row(&found, "docker");
    assert_eq!(
        socket(docker),
        (
            engine.display().to_string(),
            "env:DOCKER_HOST".to_owned(),
            "absent".to_owned()
        )
    );
    let Some(SdkValue::List(candidates)) = docker.get("candidates") else {
        panic!("{docker:?}")
    };
    let listed: Vec<(Option<&str>, Option<&str>, Option<&SdkValue>)> = candidates
        .iter()
        .map(|c| (c.get_text("source"), c.get_text("path"), c.get("usable")))
        .collect();
    assert_eq!(
        listed,
        [
            (
                Some("env:DOCKER_HOST"),
                engine.to_str(),
                Some(&SdkValue::Bool(true))
            ),
            (
                Some("default"),
                Some("/var/run/docker.sock"),
                Some(&SdkValue::Bool(true))
            ),
        ]
    );

    // a sealed bundle of this protocol is no need; another protocol is
    fs::write(
        bundle.join("bundle.json"),
        r#"{"hash":"abcd","protocol":3}"#,
    )
    .unwrap();
    assert!(!missing(row(&rows(&home), "docker")).contains(&need));
    fs::write(
        bundle.join("bundle.json"),
        r#"{"hash":"abcd","protocol":2}"#,
    )
    .unwrap();
    assert!(missing(row(&rows(&home), "docker")).contains(&need));

    // with a home: a recorded socket wins, and `--docker ""` clears it
    call(&home, "home.init", &[]).unwrap();
    let recorded = root.join("recorded.sock");
    call(
        &home,
        "machine.update",
        &[
            ("machine", t("docker")),
            ("docker", t(recorded.to_str().unwrap())),
        ],
    )
    .unwrap();
    let (path, source, _) = socket(row(&rows(&home), "docker"));
    assert_eq!(
        (path.as_str(), source.as_str()),
        (recorded.to_str().unwrap(), "recorded")
    );
    call(
        &home,
        "machine.update",
        &[("machine", t("docker")), ("docker", t(""))],
    )
    .unwrap();
    let (path, source, _) = socket(row(&rows(&home), "docker"));
    assert_eq!(
        (path.as_str(), source.as_str()),
        (engine.to_str().unwrap(), "env:DOCKER_HOST")
    );
    let asked = [("machine", t("docker")), ("detail", SdkValue::Bool(true))];
    let shown = call(&home, "machine.get", &asked).unwrap();
    assert_eq!(shown.get("docker"), Some(&SdkValue::Null), "{shown:?}");

    if cfg!(target_os = "macos") {
        stock_image(&root, &home, &bundle);
    }
    for name in [
        "LAYERFS_TOOL_FALLBACKS",
        "XDG_CACHE_HOME",
        "DOCKER_HOST",
        "LAYERFS_BUNDLE",
    ] {
        std::env::remove_var(name);
    }
    fs::remove_dir_all(&root).unwrap();
}

/// A Lima create with a prepared image for the bundle's content: absent
/// `os_image` takes it; `stock` takes Lima's Ubuntu image. The stand-in
/// `limactl` keeps the template it is given and makes nothing.
fn stock_image(root: &Path, home: &Path, bundle: &Path) {
    let content = "e".repeat(64);
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        fs::write(bundle.join(name), b"").unwrap();
    }
    fs::write(
        bundle.join("bundle.json"),
        format!(r#"{{"hash":"abcd","protocol":3,"image":"sha256:{content}"}}"#),
    )
    .unwrap();
    let platform = format!("linux-{}", std::env::consts::ARCH);
    let mapped = root
        .join("cache/layerfs/machine-images/lima")
        .join(&platform)
        .join(&content);
    fs::create_dir_all(&mapped).unwrap();
    let prepared = root.join("prepared.img");
    fs::write(&prepared, b"prepared OS").unwrap();
    fs::write(
        mapped.join("current.json"),
        format!(
            r#"{{"version":1,"provider":"lima","platform":"{platform}","protocol":3,"image":"sha256:{content}","sanitized":true,"os_image":"{}"}}"#,
            prepared.display()
        ),
    )
    .unwrap();
    let template = root.join("template");
    executable(
        &root.join("bin/limactl"),
        &format!(
            r#"case "$1" in
  --version) echo 'limactl version 9.9.9' ;;
  create) /bin/cat > '{}'; exit 1 ;;
esac"#,
            template.display()
        ),
    );
    let create = |name: &str, os_image: Option<&str>| {
        let _ = fs::remove_file(&template);
        let mut args = vec![("name", t(name)), ("platform", t("lima"))];
        if let Some(image) = os_image {
            args.push(("os_image", t(image)));
        }
        let (code, message) = call(home, "machine.create", &args).unwrap_err();
        assert_eq!(code, "Failed", "{message}");
        fs::read_to_string(&template).unwrap()
    };
    let prepared = prepared.display().to_string();
    let made = create("prep", None);
    assert!(made.contains(&prepared), "{made}");
    let made = create("plain", Some("stock"));
    assert!(!made.contains(&prepared), "{made}");
    assert!(made.contains("cloud-images.ubuntu.com"), "{made}");
}
