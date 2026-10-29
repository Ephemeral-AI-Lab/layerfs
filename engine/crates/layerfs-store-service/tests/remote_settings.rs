//! A project's named remotes (#506 R): the settings file round-trips, keeps
//! a credential reference and refuses any other key, so no secret is stored.
use layerfs_store_service::remote::state::{self, Direction, Transfer};
use layerfs_store_service::remote::storage::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_store_service::remote::storage::CredentialRef;
use layerfs_store_service::remote::{store_object, Provider, Remotes};
use std::path::PathBuf;

fn minio(prefix: &str) -> Provider {
    Provider::Minio(MinioSettings {
        endpoint: "http://127.0.0.1:9000".to_owned(),
        bucket: "layerfs".to_owned(),
        prefix: prefix.to_owned(),
        region: "us-east-1".to_owned(),
        credentials: CredentialRef::parse("env:LAYERFS_REMOTE").unwrap(),
        part_size: 8 * 1024 * 1024,
    })
}

#[test]
fn remotes_round_trip_through_their_file() {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("remote-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("remotes");
    assert_eq!(Remotes::load(&path).unwrap(), Remotes::default());
    let mut remotes = Remotes::default();
    remotes.set("origin", minio("projects/demo")).unwrap();
    remotes.set("empty-prefix", minio("")).unwrap();
    remotes
        .set(
            "backup",
            Provider::LocalFolder(PathBuf::from("/srv/layerfs")),
        )
        .unwrap();
    remotes.save(&path).unwrap();
    let loaded = Remotes::load(&path).unwrap();
    assert_eq!(loaded, remotes);
    assert_eq!(
        loaded.names().collect::<Vec<_>>(),
        ["backup", "empty-prefix", "origin"]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // a second save replaces the file
    remotes.remove("backup").unwrap();
    remotes.save(&path).unwrap();
    assert_eq!(Remotes::load(&path).unwrap(), remotes);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn settings_refuse_secrets_and_unknown_input() {
    let base = "remote origin\nprovider minio\nendpoint http://h:9000\nbucket b-1\nprefix \nregion r\ncredentials env:LAYERFS_X\n";
    let parsed = Remotes::decode(base).unwrap();
    let Some(Provider::Minio(settings)) = parsed.get("origin") else {
        panic!("{parsed:?}")
    };
    assert_eq!(settings.part_size, DEFAULT_PART_SIZE);
    assert_eq!(settings.prefix, "");
    for bad in [
        format!("{base}secret_key abc\n"),
        format!("{base}access-key abc\n"),
        base.replace("credentials env:LAYERFS_X", "credentials plain-secret"),
        base.replace("credentials env:LAYERFS_X", "credentials env:AWS_SECRET"),
        base.replace("remote origin", "remote Origin"),
        base.replace("provider minio", "provider gdrive"),
        format!("{base}{base}"),
        "provider minio\n".to_owned(),
    ] {
        assert!(Remotes::decode(&bad).is_err(), "{bad}");
    }
    let mut remotes = Remotes::default();
    assert!(remotes.set("no spaces", minio("p")).is_err());
    assert!(remotes
        .set("origin", Provider::LocalFolder(PathBuf::from("/a\nb")))
        .is_err());
}

#[test]
fn stores_are_keyed_by_name_and_their_state_kept_in_the_home() {
    assert_eq!(store_object("alpha").unwrap(), "alpha/store.sqlite");
    assert_eq!(store_object("a.b-c_1").unwrap(), "a.b-c_1/store.sqlite");
    let long = "x".repeat(129);
    for bad in ["", ".hidden", "a/b", "../x", "with space", long.as_str()] {
        assert!(store_object(bad).is_err(), "{bad:?}");
        assert!(state::state_path(std::path::Path::new("/home"), bad).is_err());
    }
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("remote-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    // the remotes are defined once per home
    assert_eq!(Remotes::home_path(&home), home.join("remotes"));
    let path = state::state_path(&home, "alpha").unwrap();
    assert_eq!(path, home.join("remote-state/alpha"));
    assert_eq!(state::load(&path).unwrap(), None);
    let pushed = Transfer::now("origin", Direction::Push, [9; 32], 4096);
    state::save(&path, &pushed).unwrap();
    assert_eq!(state::load(&path).unwrap(), Some(pushed.clone()));
    let pulled = Transfer {
        direction: Direction::Pull,
        ..pushed
    };
    state::save(&path, &pulled).unwrap();
    assert_eq!(state::load(&path).unwrap(), Some(pulled));
    std::fs::write(&path, "remote origin\ndirection sideways\n").unwrap();
    assert!(state::load(&path).is_err());
    std::fs::remove_dir_all(home).unwrap();
}
