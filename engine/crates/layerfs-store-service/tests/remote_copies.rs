//! #506 E3/E9: one stat tells a Store copy's size, sha256 and heads; one
//! listing finds every Store copy under a remote, pieced ones at their
//! whole size, and skips other files.
use layerfs_store_service::remote::storage::local_folder::LocalFolder;
use layerfs_store_service::remote::storage::{pieces, Limits, Metadata};
use layerfs_store_service::remote::{check, list_stores, Checked, HEADS};
use std::fs;
use std::path::PathBuf;

#[test]
fn check_and_list_store_copies() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("remote-copies-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("remote")).unwrap();
    let remote = LocalFolder::new(
        &root.join("remote"),
        Limits {
            max_file: 1000,
            max_request: 1000,
            piece_size: 400,
        },
    )
    .unwrap();
    fs::write(root.join("small"), vec![1u8; 600]).unwrap();
    fs::write(root.join("big"), vec![2u8; 1500]).unwrap();
    let heads = Metadata::from([(HEADS.to_owned(), "ab".repeat(32))]);
    let team = pieces::upload(&remote, "team/store.sqlite", &root.join("small"), &heads).unwrap();
    let big = pieces::upload(
        &remote,
        "big/store.sqlite",
        &root.join("big"),
        &Metadata::new(),
    )
    .unwrap();
    assert_eq!(big.pieces, 4);
    pieces::upload(&remote, "team/notes", &root.join("small"), &Metadata::new()).unwrap();
    pieces::upload(&remote, "loose", &root.join("small"), &Metadata::new()).unwrap();

    assert_eq!(check(&remote, "absent").unwrap(), Checked::Absent);
    let Checked::Present(copy) = check(&remote, "team").unwrap() else {
        panic!("team is on the remote");
    };
    assert_eq!(
        (copy.bytes, copy.sha256, copy.heads),
        (600, Some(team.sha256), Some([0xab; 32]))
    );
    assert!(copy.modified.is_some());
    // a pieced copy: one stat of its manifest still tells the whole size
    let Checked::Present(copy) = check(&remote, "big").unwrap() else {
        panic!("big is on the remote");
    };
    assert_eq!(
        (copy.bytes, copy.sha256, copy.heads),
        (1500, Some(big.sha256), None)
    );
    assert!(check(&remote, ".bad").is_err());

    let listed = list_stores(&remote).unwrap();
    let shown: Vec<_> = listed
        .iter()
        .map(|c| (c.store.as_str(), c.bytes, c.sha256, c.heads))
        .collect();
    assert_eq!(
        shown,
        [
            ("big", 1500, Some(big.sha256), None),
            ("team", 600, Some(team.sha256), Some([0xab; 32]))
        ]
    );
    fs::remove_dir_all(root).unwrap();
}
