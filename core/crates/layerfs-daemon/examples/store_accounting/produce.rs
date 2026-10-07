//! One matching local byte edit and direct Content construction for the Store half.
use layerfs_content::{
    filesystem::{
        FilesystemInput, FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemRootId,
        InodeUpdate, PathName,
    },
    ConstructionPolicy,
};
use layerfs_daemon::{
    store::{BoundWorkspace, CommitError},
    Command, Response,
};
use layerfs_history::BranchSnapshot;
use layerfs_storage::Save;
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{Operation, Outcome, Position, Time};
pub fn changed_bytes() -> Vec<u8> {
    changed_bytes_with_tag(b'X')
}
pub fn changed_bytes_with_tag(tag: u8) -> Vec<u8> {
    let mut bytes = b"complete-native-entry:000000\0".to_vec();
    bytes[0] = tag;
    bytes
}
pub fn mutate(bound: &BoundWorkspace) {
    mutate_tag(bound, b'X');
}
pub fn mutate_tag(bound: &BoundWorkspace, tag: u8) {
    let op = bound.operation().unwrap();
    let route = bound.route();
    let done = op
        .overlay()
        .try_submit(Some(route), Command::AcquireBaseSource { owner: 1 })
        .unwrap()
        .wait()
        .unwrap();
    let source = match done.result() {
        Ok(Response::BaseSource(value)) => *value,
        other => panic!("{other:?}"),
    };
    drop(done);
    let view = op.workspace().view_for_source(source).unwrap();
    let stat = view
        .lookup(
            op.overlay(),
            view.root_serial(),
            &PathName::new("entry-000000").unwrap(),
        )
        .unwrap();
    let result = op
        .workspace()
        .mutate(
            op.overlay(),
            op.ports(),
            &view,
            Operation::Write {
                serial: stat.serial,
                position: Position::At(0),
                data: vec![tag].into(),
            },
            Time {
                seconds: stat.metadata.mtime_seconds,
                nanoseconds: stat.metadata.mtime_nanoseconds,
            },
        )
        .unwrap();
    let publication = match result {
        Outcome::Applied { publication, .. } => publication,
        other => panic!("{other:?}"),
    };
    assert!(op
        .overlay()
        .try_submit(Some(route), Command::ReplyAttempted(publication))
        .unwrap()
        .wait()
        .unwrap()
        .result()
        .is_ok());
    drop(view);
    assert!(op
        .overlay()
        .try_submit(Some(route), Command::ReleaseBaseSource(source))
        .unwrap()
        .wait()
        .unwrap()
        .result()
        .is_ok());
}
pub fn construct(
    save: &Save<'_>,
    snapshot: &BranchSnapshot,
    policy: ConstructionPolicy,
) -> Result<FilesystemRootId, CommitError> {
    construct_tag(save, snapshot, policy, b'X')
}
pub fn construct_tag(
    save: &Save<'_>,
    snapshot: &BranchSnapshot,
    policy: ConstructionPolicy,
    tag: u8,
) -> Result<FilesystemRootId, CommitError> {
    let mut reader = FilesystemRead::new(save, FilesystemRootId(snapshot.effective_root))?;
    let root = reader.root();
    let mut inode =
        reader.resolve_child(root.root_inode().serial(), &PathName::new("entry-000000")?)?;
    let mut sink = save.sink();
    let built = Timing::disabled("accounting.construct", |scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            &changed_bytes_with_tag(tag),
            &mut sink,
            scope.child("file"),
        )
    })
    .0?;
    inode.value.content_root = built.root;
    let input = FilesystemInput {
        base: Some(FilesystemRootId(snapshot.effective_root)),
        scope: root.scope(),
        root_serial: root.root_inode().serial(),
        directories: &[],
        inodes: &[InodeUpdate {
            serial: inode.serial,
            value: inode.value,
        }],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut objects = FilesystemObjects::new_with_accepted(save, &mut sink, save);
    Ok(layerfs_content::filesystem::update_filesystem(&mut objects, &input, None)?.root)
}
