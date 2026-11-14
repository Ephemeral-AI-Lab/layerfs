//! One docker-archive: its `manifest.json`, the digest it names its config by,
//! and its layers, unpacked into the tree bottom first.
use super::{
    layers::{apply, directory_times},
    refuse,
    tar::{path_of, Kind, Tape, Time},
    PATH_MAX,
};
use crate::{json::Json, Image, RuntimeError};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// The largest `manifest.json` accepted: it names a config and a few layers.
const MANIFEST_MAX: usize = 1 << 20;
/// The largest number of layers one archive may name.
const LAYERS_MAX: usize = 1024;

/// One image archive's decided layout: the config blob and the layers, in the
/// order they are applied.
#[derive(Debug)]
struct Manifest {
    config: PathBuf,
    layers: Vec<PathBuf>,
    tags: Vec<String>,
}

/// Where one blob of the archive is: the offset its bytes start at, and how
/// many there are.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Blob {
    offset: u64,
    size: u64,
}

/// What a layer's first bytes say it is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Magic {
    Tar,
    Gzip,
    Zstd,
}

/// Unpacks `archive`'s layers, bottom first, into `root`, then gives each
/// directory the time its last entry carries.
pub(super) fn unpack(archive: &Path, image: &Image, root: &Path) -> Result<(), RuntimeError> {
    let manifest = manifest(archive, Some(image))?;
    let mut wanted = Vec::with_capacity(manifest.layers.len() + 1);
    wanted.push(manifest.config.clone());
    wanted.extend(manifest.layers.iter().cloned());
    let blobs = locate(archive, &wanted)?;
    // the config digest is the name the archive gave that blob; it was checked
    // before this point, so only its presence is still open here
    if blobs.first().copied().flatten().is_none() {
        return Err(refuse(format!(
            "the image archive has no config blob {}",
            manifest.config.display()
        )));
    }
    let mut times = BTreeMap::new();
    for (index, layer) in manifest.layers.iter().enumerate() {
        let blob = blobs.get(index + 1).copied().flatten().ok_or_else(|| {
            refuse(format!(
                "the image archive has no layer {}",
                layer.display()
            ))
        })?;
        extract_layer(
            archive,
            blob,
            &layer.display().to_string(),
            root,
            &mut times,
        )?;
    }
    directory_times(root, &times)
}

/// Unpacks one flat root filesystem tar (`docker export`'s output) into
/// `root` with the same entry rules as a layer, then gives each directory
/// the time its entry carries.
pub(super) fn unpack_flat(tar: &Path, root: &Path) -> Result<(), RuntimeError> {
    let size = open(tar)?.metadata()?.len();
    let mut times = BTreeMap::new();
    extract_layer(
        tar,
        Blob { offset: 0, size },
        "the flat image tar",
        root,
        &mut times,
    )?;
    directory_times(root, &times)
}

/// The image `archive` holds, by its config digest (#529): the one image its
/// `manifest.json` names, which is the default of a namespaces Sandbox given
/// no image. An archive naming none or several is refused, never guessed from.
pub fn image_of(archive: &Path) -> Result<Image, RuntimeError> {
    let manifest = manifest(archive, None)?;
    let name = (manifest.config.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| refuse("the image archive's config name"))?;
    Image::parse(&format!(
        "sha256:{}",
        name.strip_suffix(".json").unwrap_or(name)
    ))
}

/// Whether `archive` holds `image`: its `manifest.json` names an entry by
/// that config digest. Read when a deployment is built (#529), so a digest
/// the archive lacks is refused before any effect.
pub fn holds(archive: &Path, image: &Image) -> Result<(), RuntimeError> {
    manifest(archive, Some(image)).map(drop)
}

/// The archive's `manifest.json`, as the entry whose config digest is
/// `image`, or with no `image` its one entry.
fn manifest_bytes(archive: &Path) -> Result<Vec<u8>, RuntimeError> {
    let located = locate(archive, &[PathBuf::from("manifest.json")])?;
    let blob = located
        .first()
        .copied()
        .flatten()
        .ok_or_else(|| refuse("the image archive has no manifest.json"))?;
    if blob.size > MANIFEST_MAX as u64 {
        return Err(refuse("the image archive's manifest.json is too long"));
    }
    read_blob(archive, blob, MANIFEST_MAX)
}

/// One selectable image in a Docker-save archive.
#[derive(Debug)]
pub struct ImageEntry {
    /// Docker image config digest, accepted by Sandbox creation.
    pub digest: Image,
    /// Repository tags carried in the archive, used only for display.
    pub tags: Vec<String>,
    /// Operating system from the verified config blob.
    pub os: Option<String>,
    /// CPU architecture from the verified config blob.
    pub architecture: Option<String>,
}

/// Enumerates an archive without extracting its layers. Every advertised
/// image has a valid config identity and all referenced blobs present.
pub fn images(archive: &Path) -> Result<Vec<ImageEntry>, RuntimeError> {
    let manifests = parse_manifests(&manifest_bytes(archive)?)?;
    let mut images = Vec::new();
    for manifest in manifests {
        let digest = config_image(&manifest.config)?;
        let mut wanted = vec![manifest.config];
        wanted.extend(manifest.layers);
        let blobs = locate(archive, &wanted)?;
        let config_blob = blobs[0].expect("located config");
        let metadata = config_metadata(archive, config_blob, &digest)?;
        for blob in blobs.iter().skip(1).flatten() {
            compatible_layer(archive, *blob)?;
        }
        images.push(ImageEntry {
            digest,
            tags: manifest.tags,
            os: metadata.os,
            architecture: metadata.architecture,
        });
    }
    Ok(images)
}

/// Validates a user-provided archive before publication: config identity,
/// uncompressed layer identities and the config's rootfs reference closure.
/// This scans layer bytes once on import, never during read-only inventory.
pub fn validate_import(archive: &Path) -> Result<(), RuntimeError> {
    let manifests = parse_manifests(&manifest_bytes(archive)?)?;
    if manifests.is_empty() {
        return Err(refuse("image archive contains no images"));
    }
    for manifest in manifests {
        let digest = config_image(&manifest.config)?;
        let mut wanted = vec![manifest.config];
        wanted.extend(manifest.layers);
        let blobs = locate(archive, &wanted)?;
        let metadata = config_metadata(archive, blobs[0].expect("located config"), &digest)?;
        if !metadata.has_rootfs || metadata.rootfs_type.as_deref() != Some("layers") {
            return Err(refuse(
                "image config requires rootfs type layers and diff_ids",
            ));
        }
        if metadata.diff_ids.len() != blobs.len() - 1 {
            return Err(refuse(
                "image config rootfs.diff_ids does not match its layer count",
            ));
        }
        for (blob, expected) in blobs.into_iter().skip(1).zip(metadata.diff_ids) {
            let blob = blob.expect("located layer");
            compatible_layer(archive, blob)?;
            if hash_blob(archive, blob)? != expected {
                return Err(refuse(
                    "image layer SHA256 does not match config rootfs.diff_ids",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct ConfigMetadata {
    os: Option<String>,
    architecture: Option<String>,
    diff_ids: Vec<String>,
    has_rootfs: bool,
    rootfs_type: Option<String>,
}

fn config_metadata(
    archive: &Path,
    blob: Blob,
    expected: &Image,
) -> Result<ConfigMetadata, RuntimeError> {
    let bytes = read_blob(archive, blob, MANIFEST_MAX)?;
    let actual = digest_text(Sha256::digest(&bytes).as_ref());
    if actual != expected.as_str() {
        return Err(refuse(
            "image config SHA256 does not match its advertised digest",
        ));
    }
    let mut metadata = ConfigMetadata::default();
    let mut json = Json::new(bytes.as_slice());
    let mut seen = 0;
    json.object(|reader, key| {
        match key.text() {
            "os" | "architecture" => {
                let bit = if key.text() == "os" { 1 } else { 2 };
                crate::json::once(&mut seen, bit)?;
                let mut text = Vec::new();
                reader.text_into(&mut text, PATH_MAX)?;
                let text = String::from_utf8(text)
                    .map_err(|_| refuse("image config text is not UTF-8"))?;
                if bit == 1 {
                    metadata.os = Some(text);
                } else {
                    metadata.architecture = Some(text);
                }
            }
            "rootfs" => {
                crate::json::once(&mut seen, 4)?;
                let mut fields = 0;
                reader.object(|reader, key| {
                    if key.text() == "type" {
                        crate::json::once(&mut fields, 2)?;
                        let mut text = Vec::new();
                        reader.text_into(&mut text, PATH_MAX)?;
                        metadata.rootfs_type = Some(
                            String::from_utf8(text)
                                .map_err(|_| refuse("image rootfs type is not UTF-8"))?,
                        );
                        Ok(())
                    } else if key.text() == "diff_ids" {
                        metadata.has_rootfs = true;
                        crate::json::once(&mut fields, 1)?;
                        reader.array(|reader| {
                            if metadata.diff_ids.len() == LAYERS_MAX {
                                return Err(refuse("too many image diff_ids"));
                            }
                            let mut text = Vec::new();
                            reader.text_into(&mut text, PATH_MAX)?;
                            let text = String::from_utf8(text)
                                .map_err(|_| refuse("image diff_id is not UTF-8"))?;
                            Image::parse(&text)?;
                            metadata.diff_ids.push(text);
                            Ok(())
                        })
                    } else {
                        reader.skip(0)
                    }
                })?;
            }
            _ => reader.skip(0)?,
        }
        Ok(())
    })?;
    json.finish()?;
    Ok(metadata)
}

fn hash_blob(archive: &Path, blob: Blob) -> Result<String, RuntimeError> {
    let mut file = open(archive)?;
    file.seek(SeekFrom::Start(blob.offset))?;
    let mut input = file.take(blob.size);
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        hash.update(&buffer[..count]);
    }
    if total != blob.size {
        return Err(refuse("truncated image blob"));
    }
    Ok(digest_text(hash.finalize().as_ref()))
}

fn digest_text(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(71);
    text.push_str("sha256:");
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut text, "{byte:02x}").expect("writing a String");
    }
    text
}

fn compatible_layer(archive: &Path, blob: Blob) -> Result<(), RuntimeError> {
    let mut file = open(archive)?;
    file.seek(SeekFrom::Start(blob.offset))?;
    match magic(&mut file)? {
        Magic::Tar => Ok(()),
        Magic::Gzip | Magic::Zstd => {
            Err(refuse("image layers must be uncompressed docker-save tars"))
        }
    }
}

fn config_image(config: &Path) -> Result<Image, RuntimeError> {
    let name = config
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| refuse("the image archive's config name"))?;
    Image::parse(&format!(
        "sha256:{}",
        name.strip_suffix(".json").unwrap_or(name)
    ))
}

fn manifest(archive: &Path, image: Option<&Image>) -> Result<Manifest, RuntimeError> {
    let manifests = parse_manifests(&manifest_bytes(archive)?)?;
    let named = manifests
        .iter()
        .take(8)
        .map(|entry| entry.config.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let count = manifests.len();
    match image {
        None if count != 1 => Err(refuse(format!(
            "the image archive names {count} images ({named}); give the Sandbox's image digest"
        ))),
        None => Ok(manifests.into_iter().next().expect("one image")),
        Some(image) => manifests.into_iter()
            .find(|entry| check_config(image, &entry.config).is_ok())
            .ok_or_else(|| refuse(format!(
                "the image archive's config digest is {named}, not the requested {}: the bundle does not hold that image",
                image.as_str()
            ))),
    }
}

/// Reads the shared creation/inventory manifest shape, preserving tags only
/// as labels. Unknown fields are streamed past without retaining them.
fn parse_manifests(bytes: &[u8]) -> Result<Vec<Manifest>, RuntimeError> {
    let mut manifests = Vec::new();
    let mut json = Json::new(bytes);
    json.array(|element| {
        let mut config: Option<Vec<u8>> = None;
        let mut layers: Option<Vec<Vec<u8>>> = None;
        let mut tags = Vec::new();
        let mut seen: u8 = 0;
        element.object(|reader, key| {
            if key.overflow {
                return reader.skip(0);
            }
            match key.text() {
                "Config" => {
                    crate::json::once(&mut seen, 1)?;
                    let mut text = Vec::new();
                    reader.text_into(&mut text, PATH_MAX)?;
                    config = Some(text);
                }
                "Layers" => {
                    crate::json::once(&mut seen, 2)?;
                    let mut items = Vec::new();
                    reader.array(|entry| {
                        if items.len() == LAYERS_MAX {
                            return Err(refuse("the image archive names too many layers"));
                        }
                        let mut text = Vec::new();
                        entry.text_into(&mut text, PATH_MAX)?;
                        items.push(text);
                        Ok(())
                    })?;
                    layers = Some(items);
                }
                "RepoTags" => {
                    crate::json::once(&mut seen, 4)?;
                    if !reader.null()? {
                        reader.array(|entry| {
                            let mut text = Vec::new();
                            entry.text_into(&mut text, PATH_MAX)?;
                            tags.push(
                                String::from_utf8(text)
                                    .map_err(|_| refuse("image tag is not UTF-8"))?,
                            );
                            Ok(())
                        })?;
                    }
                }
                _ => reader.skip(0)?,
            }
            Ok(())
        })?;
        let (Some(config), Some(layers)) = (config, layers) else {
            return Err(refuse("the image archive entry needs Config and Layers"));
        };
        let config = path_of(&config)?;
        config_image(&config)?;
        manifests.push(Manifest {
            config,
            layers: layers
                .iter()
                .map(|layer| path_of(layer))
                .collect::<Result<Vec<_>, _>>()?,
            tags,
        });
        Ok(())
    })?;
    json.finish()?;
    Ok(manifests)
}

/// The config digest check: the archive names its config blob by that digest
/// (`<64 hex>.json` or `blobs/sha256/<64 hex>`), and it must be `image`'s.
fn check_config(image: &Image, config: &Path) -> Result<(), RuntimeError> {
    let name = config
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| refuse("the image archive's config name"))?;
    let digest = name.strip_suffix(".json").unwrap_or(name);
    if digest != image.hex() {
        return Err(refuse(format!(
            "the image archive's config digest is {digest}, not the requested {}",
            image.as_str()
        )));
    }
    Ok(())
}

/// One scan of the archive for the entries named by `wanted`, in that order;
/// everything else is skipped by seeking, never read.
fn locate(archive: &Path, wanted: &[PathBuf]) -> Result<Vec<Option<Blob>>, RuntimeError> {
    let mut found = vec![None; wanted.len()];
    let mut remaining = wanted.len();
    let mut tape = Tape::new(BufReader::with_capacity(64 * 1024, open(archive)?));
    while remaining > 0 {
        let Some(entry) = tape.next()? else { break };
        if entry.kind != Kind::Regular {
            continue;
        }
        for (index, name) in wanted.iter().enumerate() {
            if found[index].is_none() && *name == entry.path {
                found[index] = Some(Blob {
                    offset: tape.data_offset(),
                    size: entry.size,
                });
                remaining -= 1;
                break;
            }
        }
    }
    if remaining > 0 {
        let missing: Vec<String> = (wanted.iter().enumerate())
            .filter(|(index, _)| found[*index].is_none())
            .map(|(_, name)| name.display().to_string())
            .collect();
        return Err(refuse(format!(
            "the image archive is missing {}",
            missing.join(", ")
        )));
    }
    Ok(found)
}

/// The first bytes of `blob`, up to `max`.
fn read_blob(archive: &Path, blob: Blob, max: usize) -> Result<Vec<u8>, RuntimeError> {
    let mut file = open(archive)?;
    file.seek(SeekFrom::Start(blob.offset))?;
    let mut bytes = Vec::new();
    let limit = blob.size.min(max as u64 + 1);
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(refuse("the image archive's blob is longer than allowed"));
    }
    Ok(bytes)
}

/// Applies one layer, whose bytes are the `size` at `blob`, keeping each
/// directory entry's time in `times` (a later entry's replaces an earlier's).
fn extract_layer(
    archive: &Path,
    blob: Blob,
    name: &str,
    root: &Path,
    times: &mut BTreeMap<PathBuf, Time>,
) -> Result<(), RuntimeError> {
    let mut file = open(archive)?;
    file.seek(SeekFrom::Start(blob.offset))?;
    match magic(&mut file)? {
        Magic::Tar => {}
        Magic::Gzip => {
            return Err(refuse(format!(
                "the image archive's layer {name} is gzip-compressed; a docker-archive must hold \
                 uncompressed layers (docker save writes them so)"
            )))
        }
        Magic::Zstd => {
            return Err(refuse(format!(
                "the image archive's layer {name} is zstd-compressed; a docker-archive must hold \
                 uncompressed layers (docker save writes them so)"
            )))
        }
    }
    file.seek(SeekFrom::Start(blob.offset))?;
    let mut tape = Tape::new(Window {
        input: file,
        left: blob.size,
    });
    while let Some(entry) = tape.next()? {
        let mut data = (&mut tape).take(entry.size);
        apply(root, &entry, &mut data)?;
        if entry.kind == Kind::Directory {
            times.insert(entry.path, entry.mtime);
        }
    }
    Ok(())
}

/// What the first bytes at the current position say the layer is.
fn magic(file: &mut File) -> Result<Magic, RuntimeError> {
    let mut header = [0u8; 4];
    let mut read = 0;
    while read < header.len() {
        let count = file.read(&mut header[read..])?;
        if count == 0 {
            break;
        }
        read += count;
    }
    Ok(match &header[..read] {
        [0x1f, 0x8b, ..] => Magic::Gzip,
        [0x28, 0xb5, 0x2f, 0xfd] => Magic::Zstd,
        _ => Magic::Tar,
    })
}

/// A bounded window of an archive: one layer's bytes. Seeking forward is what
/// skipping an entry needs; anything else is refused.
struct Window<R> {
    input: R,
    left: u64,
}

impl<R: Read> Read for Window<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let wanted = (buffer.len() as u64).min(self.left) as usize;
        let read = self.input.read(&mut buffer[..wanted])?;
        self.left -= read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for Window<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let SeekFrom::Current(skip) = position else {
            return Err(io::Error::from(io::ErrorKind::Unsupported));
        };
        if skip < 0 {
            return Err(io::Error::from(io::ErrorKind::Unsupported));
        }
        let skip = (skip as u64).min(self.left);
        self.input.seek(SeekFrom::Current(skip as i64))?;
        self.left -= skip;
        Ok(self.left)
    }
}

/// An archive opened for reading, refused when it is not a regular file (a
/// symbolic link would let the archive be swapped after the check).
fn open(archive: &Path) -> Result<File, RuntimeError> {
    let metadata = fs::symlink_metadata(archive).map_err(|error| {
        refuse(format!(
            "the image archive {} cannot be read: {error}",
            archive.display()
        ))
    })?;
    if !metadata.file_type().is_file() {
        return Err(refuse(format!(
            "the image archive {} is not a regular file",
            archive.display()
        )));
    }
    OpenOptions::new()
        .read(true)
        .custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits())
        .open(archive)
        .map_err(|error| {
            refuse(format!(
                "the image archive {} cannot be opened: {error}",
                archive.display()
            ))
        })
}
