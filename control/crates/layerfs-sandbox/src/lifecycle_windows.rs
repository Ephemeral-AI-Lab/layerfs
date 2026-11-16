//! The Windows sandbox.create: the A1 directory Sandbox with its
//! daemon record, no daemon process (#454). The mount lifecycle runs
//! through the in-process daemon assembly the daemon crate opens.
use crate::host::Host;
use crate::record::DaemonRecord;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// Starts the Sandbox the call names: its directory under the deploy
/// root, its daemon record naming the Windows native runtime. No
/// process starts - the in-process control face owns the lifecycle.
pub(super) fn start(host: &Host, directory: &Path, id: &str) -> Result<std::path::PathBuf, layerfs_bridge::control::SdkRefusal> {
    let Some(deploy) = host.deploy() else {
        return Err(layerfs_home::outcome::refused("InvalidInput", "missing deploy"));
    };
    let Some(root) = deploy.local_dir.clone() else {
        return Err(layerfs_home::outcome::refused("InvalidInput", "deploy.localDir is not configured"));
    };
    let prefix: String = host.project.as_str().chars().take(8).collect();
    let sandbox_dir = std::path::PathBuf::from(&root).join(format!("lfs-{prefix}-{id}"));
    if let Err(error) = std::fs::create_dir_all(&sandbox_dir) {
        return Err(layerfs_home::outcome::failed(error));
    }
    // The daemon record: the Windows native runtime, no process face.
    let _record = DaemonRecord::from_record(&SdkValue::Map(vec![
        ("runtime".to_owned(), SdkValue::text("native")),
        ("platform".to_owned(), SdkValue::text("windows")),
    ]));
    // The record's on-disk encoding lands with the record-face wiring;
    // the directory Sandbox itself is created and answers its name.
    let _ = directory;
    let _ = SdkValue::Map(vec![
        ("sandbox".to_owned(), SdkValue::text(id)),
        ("name".to_owned(), SdkValue::text(sandbox_dir.to_string_lossy().into_owned())),
    ]);
    Ok(sandbox_dir)
}
