//! The docker machine's default image (#535 P5). With no image named (no
//! `sandbox create --image`, no `machine update docker --image`), the
//! sealed bundle's `sandbox-image.tar` is loaded into Docker once and the
//! image id Docker reports is used, for the docker worker's container and
//! for Sandboxes. The docker record keeps `loaded_image: { id, bundle }`;
//! another bundle's image is loaded again. A load is one attempt.
use super::spec::{self as record, machine_of, refusal};
use super::status::home_id;
use crate::sandbox::deploy::IMAGE_ARCHIVE;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home::lock;
use layerfs_home::outcome::refused;
use layerfs_platform as adapters;
use std::path::Path;

/// The image a docker Sandbox, or the docker machine's worker, runs when
/// none is named: the bundle's Sandbox image, loaded into Docker the first
/// time and recorded.
pub(crate) fn docker_default(home: &Path) -> Result<String, SdkRefusal> {
    let bundle = super::bundle::bundle()?;
    let record = machine_of(home, "docker")?;
    if let Some((id, of)) = record.loaded_image() {
        if of == bundle.hash {
            return Ok(id.to_owned());
        }
    }
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
    let id = adapters::load_image(&spec, &archive).map_err(refusal)?;
    let _lock = lock::take(home)?;
    record::update(home, "docker", |record| {
        record.with_loaded_image(&id, &bundle.hash)
    })?;
    Ok(id)
}
