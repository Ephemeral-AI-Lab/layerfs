//! Read-only machine image discovery, resolved on the machine that owns
//! the archive. Host-process Sandboxes do not select an image.
use crate::deploy::{self, Choice, Side};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::place::{self, Executor};
use std::path::Path;

/// Lists images from the selected machine, without starting it or a Sandbox.
pub fn list(home: &Path, name: &MachineName) -> Outcome {
    let machine = name.as_str();
    let choice = deploy::choose(home, machine, None)?;
    match place::on_machine(home, machine)? {
        Executor::Local => inspect(machine, &Side::Home(home), &choice, home),
        Executor::Worker(mut worker) => worker.call(
            "machine.images",
            SdkValue::map([
                ("machine", SdkValue::text(machine)),
                (
                    "archive",
                    choice.archive.map_or(SdkValue::Null, SdkValue::Text),
                ),
                ("image", choice.image.map_or(SdkValue::Null, SdkValue::Text)),
            ]),
            &mut |_| {},
        ),
    }
}

/// Reads this worker's current archives for its caller, without home records.
pub fn in_worker(root: &Path, args: SdkValue) -> Outcome {
    let machine = args.get_text("machine").unwrap_or("");
    MachineName::parse(machine)?;
    let choice = Choice {
        archive: args.get_text("archive").map(str::to_owned),
        image: args.get_text("image").map(str::to_owned),
        platform: None,
    };
    inspect(machine, &Side::Worker(root), &choice, root)
}

fn inspect(machine: &str, side: &Side<'_>, choice: &Choice, root: &Path) -> Outcome {
    #[cfg(target_os = "linux")]
    {
        inspect_linux(machine, side, choice, root)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (side, choice, root);
        if !cfg!(target_os = "macos") {
            return Err(refused(
                "Unsupported",
                "Sandbox image discovery requires Linux or macOS",
            ));
        }
        Ok(SdkValue::map([
            ("machine", SdkValue::text(machine)),
            ("mode", SdkValue::text("host")),
            ("default_digest", SdkValue::Null),
            ("images", SdkValue::List(Vec::new())),
        ]))
    }
}

#[cfg(target_os = "linux")]
fn inspect_linux(machine: &str, side: &Side<'_>, choice: &Choice, root: &Path) -> Outcome {
    use layerfs_sandbox_backend::namespaces::rootfs;
    let archive = deploy::archive(side, choice.archive.as_deref())?;
    let entries = rootfs::images(&archive).map_err(|error| {
        refused(
            "InvalidInput",
            format!(
                "machine {machine}'s image archive {}: {error}",
                archive.display()
            ),
        )
    })?;
    if entries.is_empty() {
        return Err(refused(
            "InvalidInput",
            "the machine's image archive contains no images",
        ));
    }
    let default = match &choice.image {
        Some(digest) => Some(digest.clone()),
        None if entries.len() == 1 => Some(entries[0].digest.as_str().to_owned()),
        None => None,
    };
    let mut all = entries;
    let imported = crate::image_artifact::directory(root);
    match std::fs::read_dir(&imported) {
        Ok(files) => {
            let mut paths = Vec::new();
            for file in files {
                let path = file.map_err(layerfs_home::outcome::failed)?.path();
                if path.extension().is_some_and(|ext| ext == "tar") {
                    paths.push(path);
                }
            }
            paths.sort();
            for path in paths {
                all.extend(rootfs::images(&path).map_err(|error| {
                    refused(
                        "InvalidInput",
                        format!(
                            "machine {machine}'s imported image archive {}: {error}",
                            path.display()
                        ),
                    )
                })?);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(layerfs_home::outcome::failed(error)),
    }
    if let Some(digest) = &default {
        if !all.iter().any(|entry| entry.digest.as_str() == digest) {
            return Err(refused(
                "InvalidInput",
                "the machine's configured image digest is absent from its archives",
            ));
        }
    }
    let mut merged = std::collections::BTreeMap::<String, rootfs::ImageEntry>::new();
    for mut entry in all {
        entry.tags.sort();
        entry.tags.dedup();
        match merged.entry(entry.digest.as_str().to_owned()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(entry);
            }
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                let tags = &mut slot.get_mut().tags;
                tags.extend(entry.tags);
                tags.sort();
                tags.dedup();
            }
        }
    }
    let images = merged
        .into_values()
        .map(|entry| {
            SdkValue::map([
                ("digest", SdkValue::text(entry.digest.as_str())),
                (
                    "tags",
                    SdkValue::List(entry.tags.into_iter().map(SdkValue::Text).collect()),
                ),
                ("os", entry.os.map_or(SdkValue::Null, SdkValue::Text)),
                (
                    "architecture",
                    entry.architecture.map_or(SdkValue::Null, SdkValue::Text),
                ),
            ])
        })
        .collect();
    Ok(SdkValue::map([
        ("machine", SdkValue::text(machine)),
        ("mode", SdkValue::text("archive")),
        ("archive", SdkValue::text(archive.display().to_string())),
        (
            "default_digest",
            default.map_or(SdkValue::Null, SdkValue::Text),
        ),
        ("images", SdkValue::List(images)),
    ]))
}

/// Validates an archive and optional exact config digest before transport.
/// This reader is shared with the namespaces runtime on every Unix host.
pub fn validate_archive(
    path: &Path,
    digest: Option<&str>,
) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    let entries = layerfs_sandbox_backend::rootfs::images(path).map_err(|error| {
        refused(
            "InvalidInput",
            format!("image archive {}: {error}", path.display()),
        )
    })?;
    if entries.is_empty() {
        return Err(refused(
            "InvalidInput",
            "the image archive contains no images",
        ));
    }
    if let Some(digest) = digest {
        layerfs_sandbox_backend::Image::parse(digest)
            .map_err(|error| refused("InvalidInput", error.to_string()))?;
        if !entries.iter().any(|entry| entry.digest.as_str() == digest) {
            return Err(refused(
                "InvalidInput",
                "the image archive does not contain the requested config digest",
            ));
        }
    }
    Ok(())
}

/// Verifies user-import layer identities once before publishing an archive.
pub fn validate_import(path: &Path) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    layerfs_sandbox_backend::rootfs::validate_import(path).map_err(|error| {
        refused(
            "InvalidInput",
            format!("image archive {}: {error}", path.display()),
        )
    })
}
