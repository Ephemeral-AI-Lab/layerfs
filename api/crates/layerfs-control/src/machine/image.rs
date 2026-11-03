//! The docker machine's default image (#535 P5). With no image named (no
//! `sandbox create --image`, no `machine update docker --image`), the
//! sealed bundle's `sandbox-image.tar` is loaded into Docker once and the
//! image id Docker reports is used, for the docker worker's container and
//! for Sandboxes. The docker record keeps `loaded_image: { id, bundle }`;
//! another bundle's image is loaded again. A load is one attempt.
use super::spec::{self as record, machine_of, refusal};
use super::status::home_id;
use crate::common::home::{lock, with_field};
use crate::common::outcome::refused;
use crate::sandbox::deploy::IMAGE_ARCHIVE;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine as adapters;
use std::path::Path;

/// The image a docker Sandbox, or the docker machine's worker, runs when
/// none is named: the bundle's Sandbox image, loaded into Docker the first
/// time and recorded.
pub(crate) fn docker_default(home: &Path) -> Result<String, SdkRefusal> {
    let bundle = super::bundle::bundle()?;
    let record = machine_of(home, "docker")?;
    if let Some(loaded) = record.get("loaded_image") {
        if let (Some(id), Some(of)) = (loaded.get_text("id"), loaded.get_text("bundle")) {
            if of == bundle.hash {
                return Ok(id.to_owned());
            }
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
    let spec = record::spec(home, &home_id(home)?, "docker", &record)?;
    let id = adapters::load_image(&spec, &archive).map_err(refusal)?;
    let _lock = lock::take(home)?;
    record::update(home, "docker", |record| {
        let loaded = SdkValue::map([
            ("id", SdkValue::text(&id)),
            ("bundle", SdkValue::text(&bundle.hash)),
        ]);
        with_field(record, "loaded_image", loaded)
    })?;
    Ok(id)
}
