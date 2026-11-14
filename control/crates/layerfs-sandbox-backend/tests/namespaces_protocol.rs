//! The namespace backend's formats and records: the init stage's spec, the
//! recorded facts a reopen reads, and the refusals that keep one runtime's
//! records from another. Nothing here starts a process.
use layerfs_sandbox_backend::{
    Bind, HelperSpec, Image, LocalRecord, NamespaceRecord, RuntimeError, SandboxRecord,
};
use std::path::{Path, PathBuf};

/// The image digest these records name.
fn image() -> Image {
    Image::parse(&format!("sha256:{}", "a".repeat(64))).unwrap()
}

fn spec() -> HelperSpec {
    HelperSpec {
        root: PathBuf::from("/var/lib/layerfs/local/sandbox-1.root"),
        sandbox: PathBuf::from("/var/lib/layerfs/local/sandbox-1"),
        daemon: PathBuf::from("/var/lib/layerfs/local/sandbox-1/bin/layerfs-daemon"),
        config: PathBuf::from("/var/lib/layerfs/local/sandbox-1/config/daemon.setup"),
        cgroup: PathBuf::from("/sys/fs/cgroup/layerfs/0000000000000000000000000000000000000000000000000000000000000001"),
        lower: PathBuf::from(
            "/var/lib/layerfs/local/images/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        upper: PathBuf::from("/var/lib/layerfs/local/sandbox-1/overlay/upper"),
        work: PathBuf::from("/var/lib/layerfs/local/sandbox-1/overlay/work"),
        binds: vec![Bind::store(Path::new("/layerfs-store/project/store.sqlite")).unwrap()],
        runtime: None,
    }
}

#[test]
fn the_init_spec_roundtrips() {
    let spec = spec();
    let encoded = spec.encode().unwrap();
    assert_eq!(HelperSpec::parse(&encoded).unwrap(), spec);
    // one line per field, key first, then one per bind
    let text = String::from_utf8(encoded).unwrap();
    assert_eq!(text.lines().count(), 9);
    assert!(text.starts_with("root=/var/lib/layerfs/local/sandbox-1.root\n"));
    assert!(text.ends_with("\nbind=rw\t/layerfs-store/project\t/layerfs-store/project\n"));
}

/// R5: a spec may name the Sandbox's runtime layer once; it roundtrips, and
/// a spec written before it (the eight fields and binds, no runtime line)
/// still parses, with no runtime.
#[test]
fn the_runtime_is_optional_and_roundtrips() {
    let with = HelperSpec {
        runtime: Some(PathBuf::from("/var/lib/layerfs/local/sandbox-1/runtime")),
        ..spec()
    };
    let encoded = with.encode().unwrap();
    assert_eq!(HelperSpec::parse(&encoded).unwrap(), with);
    let text = String::from_utf8(encoded).unwrap();
    assert!(text.contains("\nruntime=/var/lib/layerfs/local/sandbox-1/runtime\n"));
    assert_eq!(text.lines().count(), 10);
    // an eight-field spec, as a Sandbox made before the runtime layer has it
    let old = [
        "root=/var/lib/layerfs/local/sandbox-1.root",
        "sandbox=/var/lib/layerfs/local/sandbox-1",
        "daemon=/var/lib/layerfs/local/sandbox-1/bin/layerfs-daemon",
        "config=/var/lib/layerfs/local/sandbox-1/config/daemon.setup",
        "cgroup=/sys/fs/cgroup/layerfs/0000000000000000000000000000000000000000000000000000000000000001",
        "lower=/var/lib/layerfs/local/images/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "upper=/var/lib/layerfs/local/sandbox-1/overlay/upper",
        "work=/var/lib/layerfs/local/sandbox-1/overlay/work",
        "bind=rw\t/layerfs-store/project\t/layerfs-store/project",
    ]
    .join("\n");
    assert_eq!(HelperSpec::parse(old.as_bytes()).unwrap(), spec());
    // twice, or not absolute, is refused
    let twice = format!(
        "{}runtime=/a\nruntime=/b\n",
        String::from_utf8(spec().encode().unwrap()).unwrap()
    );
    assert!(HelperSpec::parse(twice.as_bytes()).is_err());
    let relative = format!(
        "{}runtime=runtime\n",
        String::from_utf8(spec().encode().unwrap()).unwrap()
    );
    assert!(HelperSpec::parse(relative.as_bytes()).is_err());
    let bad = HelperSpec {
        runtime: Some(PathBuf::from("/a/../b")),
        ..spec()
    };
    assert!(bad.encode().is_err());
}

fn line(command: &std::process::Command) -> Vec<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|word| word.to_string_lossy().into_owned())
        .collect()
}

/// R5: with a runtime the daemon starts through the runtime's loader, on its
/// libraries only, and is told the runtime; without one, as before.
#[test]
fn the_daemon_starts_through_the_runtime_loader_when_the_spec_has_one() {
    let with = HelperSpec {
        runtime: Some(PathBuf::from("/var/lib/layerfs/local/sandbox-1/runtime")),
        ..spec()
    };
    let command = with.daemon_command().unwrap();
    let loader = layerfs_bridge::sandbox_runtime::LOADER.unwrap();
    assert_eq!(
        line(&command),
        [
            format!("/.layerfs/runtime/lib/{loader}").as_str(),
            "--inhibit-cache",
            "--library-path",
            "/.layerfs/runtime/lib",
            "/var/lib/layerfs/local/sandbox-1/bin/layerfs-daemon",
            "--config",
            "/var/lib/layerfs/local/sandbox-1/config/daemon.setup",
        ]
    );
    let told = command
        .get_envs()
        .find(|(name, _)| *name == "LAYERFS_SANDBOX_RUNTIME")
        .and_then(|(_, value)| value);
    assert_eq!(told, Some(std::ffi::OsStr::new("/.layerfs/runtime")));
    let without = spec().daemon_command().unwrap();
    assert_eq!(
        line(&without),
        [
            "/var/lib/layerfs/local/sandbox-1/bin/layerfs-daemon",
            "--config",
            "/var/lib/layerfs/local/sandbox-1/config/daemon.setup",
        ]
    );
    assert_eq!(without.get_envs().count(), 0);
}

/// #535 §6.3: a sqlite Store's Sandbox binds its Store's directory
/// read-write at its own path; a PostgreSQL Store's binds nothing; other
/// binds keep their mode, guest path and order.
#[test]
fn binds_roundtrip_in_order_and_may_be_none() {
    let store = Bind::store(Path::new("/layerfs/data/stores/shop/store.sqlite")).unwrap();
    assert_eq!(
        store,
        Bind {
            host: PathBuf::from("/layerfs/data/stores/shop"),
            guest: PathBuf::from("/layerfs/data/stores/shop"),
            read_write: true,
        }
    );
    let none = HelperSpec {
        binds: Vec::new(),
        ..spec()
    };
    let encoded = none.encode().unwrap();
    assert!(!String::from_utf8_lossy(&encoded).contains("bind="));
    assert_eq!(HelperSpec::parse(&encoded).unwrap(), none);
    let several = HelperSpec {
        binds: vec![
            store,
            Bind {
                host: PathBuf::from("/srv/reference data"),
                guest: PathBuf::from("/reference"),
                read_write: false,
            },
        ],
        ..spec()
    };
    let encoded = several.encode().unwrap();
    assert!(
        String::from_utf8_lossy(&encoded).contains("\nbind=ro\t/srv/reference data\t/reference\n")
    );
    assert_eq!(HelperSpec::parse(&encoded).unwrap(), several);
}

#[test]
fn a_store_file_that_names_no_directory_is_refused() {
    for bad in ["store.sqlite", "/store.sqlite/..", "./store.sqlite", "/"] {
        assert!(
            matches!(
                Bind::store(Path::new(bad)),
                Err(RuntimeError::Protocol("namespaces Store file"))
            ),
            "{bad}"
        );
    }
}

#[test]
fn a_malformed_bind_or_the_former_store_line_is_refused() {
    let text = String::from_utf8(
        HelperSpec {
            binds: Vec::new(),
            ..spec()
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    for line in [
        "bind=rw\t/a",
        "bind=rw\t/a\t/b\t/c",
        "bind=rx\t/a\t/b",
        "bind=rw\trelative\t/b",
        "bind=ro\t/a\t/b/../c",
        "bind=rw /a /b",
        // the format before binds: one Store directory
        "store=/layerfs-store/project",
    ] {
        let spec = format!("{text}{line}\n");
        assert!(HelperSpec::parse(spec.as_bytes()).is_err(), "{line}");
    }
    let guest = HelperSpec {
        binds: vec![Bind {
            host: PathBuf::from("/a"),
            guest: PathBuf::from("b"),
            read_write: true,
        }],
        ..spec()
    };
    assert!(guest.encode().is_err());
}

#[test]
fn a_value_may_hold_an_equals_sign() {
    let mut spec = spec();
    spec.binds[0].host = PathBuf::from("/layerfs-store/a=b");
    let encoded = spec.encode().unwrap();
    assert_eq!(HelperSpec::parse(&encoded).unwrap().binds, spec.binds);
}

#[test]
fn a_path_the_init_stage_cannot_mount_is_refused() {
    let relative = HelperSpec {
        root: PathBuf::from("relative/root"),
        ..spec()
    };
    assert!(relative.encode().is_err());
    let parent = HelperSpec {
        sandbox: PathBuf::from("/var/lib/../sandbox"),
        ..spec()
    };
    assert!(parent.encode().is_err());
    let newline = HelperSpec {
        config: PathBuf::from("/var/lib/a\nb"),
        ..spec()
    };
    assert!(newline.encode().is_err());
    let bytes = newline.encode().unwrap_err();
    assert_eq!(format!("{bytes:?}"), r#"Protocol("helper spec path")"#);
    // the three overlay paths are one mount option string: neither a comma nor
    // a colon may appear in them
    let comma = HelperSpec {
        upper: PathBuf::from("/var/lib/a,b/upper"),
        ..spec()
    };
    assert!(comma.encode().is_err());
    let colon = HelperSpec {
        lower: PathBuf::from("/var/lib/a:b"),
        ..spec()
    };
    assert_eq!(
        format!("{:?}", colon.encode().unwrap_err()),
        r#"Protocol("an overlay path holds a `,` or `:`")"#
    );
}

#[test]
fn a_spec_is_refused_unless_every_field_appears_once() {
    let encoded = spec().encode().unwrap();
    let text = String::from_utf8(encoded).unwrap();
    // one field missing
    let missing = text
        .lines()
        .filter(|line| !line.starts_with("cgroup="))
        .collect::<Vec<_>>();
    assert!(HelperSpec::parse(missing.join("\n").as_bytes()).is_err());
    // one field twice
    let twice = format!("{text}cgroup=/sys/fs/cgroup/layerfs/2\n");
    assert!(HelperSpec::parse(twice.as_bytes()).is_err());
    // an unknown field
    let unknown = format!("{text}kernel=nfs\n");
    assert!(HelperSpec::parse(unknown.as_bytes()).is_err());
    // a line without a key
    assert!(HelperSpec::parse(format!("{text}/\n").as_bytes()).is_err());
    // the whole file is bounded
    let huge = vec![b'a'; 8193];
    assert!(HelperSpec::parse(&huge).is_err());
}

#[test]
fn a_namespaces_record_is_its_own_variant_and_keeps_the_local_facts() {
    let local = LocalRecord {
        name: "layerfs-project-1".to_owned(),
        pid: 4242,
        start_time: 7,
        boot: Some(1),
        port: 41000,
        labels: vec![("layerfs.role".to_owned(), "sandbox".to_owned())],
    };
    let namespace = NamespaceRecord {
        local: local.clone(),
        image: image(),
    };
    let record = SandboxRecord::Namespaces(namespace.clone());
    assert_eq!(record.namespaces(), Some(&namespace));
    // the image digest the record keeps is the one its root is an overlay of
    assert_eq!(
        record.namespaces().map(|record| record.image().as_str()),
        Some(image().as_str())
    );
    assert!(record.local().is_none());
    // the native record of the same facts stays a native record
    let native = SandboxRecord::Local(local);
    assert!(native.namespaces().is_none());
}
