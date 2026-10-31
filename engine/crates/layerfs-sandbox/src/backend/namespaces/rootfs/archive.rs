//! One docker-archive: its `manifest.json`, the digest it names its config by,
//! and its layers, unpacked into the tree bottom first.
use super::{
    layers::apply,
    refuse,
    tar::{path_of, Kind, Tape},
    PATH_MAX,
};
use crate::{backend::json::Json, Image, RuntimeError};
use std::{
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

/// Unpacks `archive`'s layers, bottom first, into `root`.
pub(super) fn unpack(archive: &Path, image: &Image, root: &Path) -> Result<(), RuntimeError> {
    let manifest = manifest(archive, image)?;
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
    for (index, layer) in manifest.layers.iter().enumerate() {
        let blob = blobs.get(index + 1).copied().flatten().ok_or_else(|| {
            refuse(format!(
                "the image archive has no layer {}",
                layer.display()
            ))
        })?;
        extract_layer(archive, blob, &layer.display().to_string(), root)?;
    }
    Ok(())
}

/// The archive's `manifest.json`, as the entry whose config digest is `image`.
fn manifest(archive: &Path, image: &Image) -> Result<Manifest, RuntimeError> {
    let located = locate(archive, &[PathBuf::from("manifest.json")])?;
    let blob = located
        .first()
        .copied()
        .flatten()
        .ok_or_else(|| refuse("the image archive has no manifest.json"))?;
    if blob.size > MANIFEST_MAX as u64 {
        return Err(refuse("the image archive's manifest.json is too long"));
    }
    read_blob(archive, blob, MANIFEST_MAX).and_then(|bytes| parse_manifest(&bytes, image))
}

/// Reads `manifest.json` and keeps the entry whose config digest is `image`.
fn parse_manifest(bytes: &[u8], image: &Image) -> Result<Manifest, RuntimeError> {
    let mut found: Option<Manifest> = None;
    let mut named: Vec<String> = Vec::new();
    let mut json = Json::new(bytes);
    json.array(|element| {
        let mut config: Option<Vec<u8>> = None;
        let mut layers: Option<Vec<Vec<u8>>> = None;
        let mut seen: u8 = 0;
        element.object(|reader, key| {
            if key.overflow {
                return Ok(());
            }
            match key.text() {
                "Config" => {
                    crate::backend::json::once(&mut seen, 1)?;
                    let mut text = Vec::new();
                    reader.text_into(&mut text, PATH_MAX)?;
                    config = Some(text);
                }
                "Layers" => {
                    crate::backend::json::once(&mut seen, 2)?;
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
                _ => reader.skip(0)?,
            }
            Ok(())
        })?;
        let (Some(config), Some(layers)) = (config, layers) else {
            return Ok(());
        };
        let config = path_of(&config)?;
        if named.len() < 8 {
            named.push(config.display().to_string());
        }
        if found.is_none() && check_config(image, &config).is_ok() {
            found = Some(Manifest {
                config,
                layers: (layers.iter())
                    .map(|layer| path_of(layer))
                    .collect::<Result<Vec<_>, _>>()?,
            });
        }
        Ok(())
    })?;
    json.finish()?;
    found.ok_or_else(|| {
        refuse(format!(
            "the image archive's config digest is {}, not the requested {}: the bundle does not \
             hold that image",
            if named.is_empty() {
                "missing".to_owned()
            } else {
                named.join(", ")
            },
            image.as_str()
        ))
    })
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

/// Applies one layer, whose bytes are the `size` at `blob`.
fn extract_layer(archive: &Path, blob: Blob, name: &str, root: &Path) -> Result<(), RuntimeError> {
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
