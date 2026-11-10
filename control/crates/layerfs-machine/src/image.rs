//! The docker machine's default image (#535 P5). With no image named (no
//! `sandbox create --image`, no `machine update docker --image`), the
//! sealed bundle's `sandbox-image.tar` is loaded into Docker once and the
//! image id Docker reports is used, for the docker worker's container and
//! for Sandboxes. The docker record keeps `loaded_image: { id, bundle }`;
//! content tags in the Engine reuse the image across homes and runtime builds.
//! A cached image is inspected before reuse; an import is one attempt.
use super::spec::{self as record, machine_of, refusal};
use super::status::home_id;
use crate::bundle::IMAGE_ARCHIVE;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home::lock;
use layerfs_home::outcome::refused;
use layerfs_platform as adapters;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// The image a docker Sandbox, or the docker machine's worker, runs when
/// none is named: the bundle's Sandbox image, loaded into Docker the first
/// time and recorded.
pub fn docker_default(home: &Path) -> Result<String, SdkRefusal> {
    let bundle = super::bundle::bundle()?;
    let record = machine_of(home, "docker")?;
    let archive = bundle.dir.join(IMAGE_ARCHIVE);
    if !archive.is_file() {
        return Err(refused(
            "Unsupported",
            format!(
                "the docker machine names no Sandbox image and the sealed bundle at {} has no \
                 {IMAGE_ARCHIVE} to load; `python3 tools/linux_bundle.py seal` places it, or \
                 `layerfs machine update docker --image sha256:<digest>` names one",
                bundle.dir.display()
            ),
        ));
    }
    let spec = record::spec(home, &home_id(home)?, &record)?;
    let manifest = layerfs_home::home::read_record(&bundle.dir.join("bundle.json"))?;
    let content = manifest.as_ref().and_then(|m| m.get_text("image"));
    let previous = record
        .loaded_image()
        .filter(|(_, of)| *of == bundle.hash)
        .map(|(id, _)| id);
    let id = match content {
        Some(content) => {
            // One import per content at a time across homes and bundle builds.
            // The Engine's tag is the mapping; this file only serializes imports.
            let socket = std::fs::canonicalize(&spec.tools.docker_socket)
                .unwrap_or_else(|_| spec.tools.docker_socket.clone());
            let engine = layerfs_bridge::sdk_values::hex(socket.as_os_str().as_bytes());
            let cache = super::bundle::cache_root()?
                .join("layerfs/docker-images")
                .join(engine);
            layerfs_home::home::private_dir(&cache)?;
            let hex = content
                .strip_prefix("sha256:")
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| refused("InvalidInput", "the bundle image ID is not sha256"))?;
            let _image = layerfs_home::home::entity_lock::take(
                &cache.join(format!("{hex}.lock")),
                std::time::Duration::from_secs(600),
                "another process is importing the Sandbox image",
            )?;
            adapters::cached_image(&spec, &archive, content, previous).map_err(refusal)?
        }
        // Legacy bundles without a content identity retain their old load path.
        None => match previous {
            Some(_) => adapters::cached_image(
                &spec,
                &archive,
                &format!("sha256:{}", bundle.hash),
                previous,
            )
            .map_err(refusal)?,
            None => adapters::load_image(&spec, &archive).map_err(refusal)?,
        },
    };
    let _lock = lock::take(home)?;
    record::update(home, "docker", |record| {
        record.with_loaded_image(&id, &bundle.hash)
    })?;
    Ok(id)
}
