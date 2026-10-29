//! A project's named remotes (#506 R): the settings file round-trips, keeps
//! a credential reference and refuses any other key, so no secret is stored.
use layerfs_store_service::remote::storage::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_store_service::remote::storage::CredentialRef;
use layerfs_store_service::remote::{Provider, Remotes};
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
