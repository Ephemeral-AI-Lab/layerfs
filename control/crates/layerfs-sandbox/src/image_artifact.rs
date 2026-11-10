//! Bounded image archive transfer over the existing authenticated worker frames.
//! Published files are keyed by whole-file SHA256, independent of basename.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_store::place::Worker;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const CHUNK: usize = 1 << 20;

/// Whether a value is a full lowercase config digest.
pub fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(hex)
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// Machine-local imported image archives.
pub fn directory(root: &Path) -> PathBuf {
    root.join("image")
}

/// Validates a caller-local archive before any transfer.
pub fn validate(path: &str, digest: Option<&str>) -> Result<(), SdkRefusal> {
    let path = Path::new(path);
    if !path.is_absolute()
        || !fs::symlink_metadata(path)
            .map_err(failed)?
            .file_type()
            .is_file()
    {
        return Err(refused(
            "InvalidInput",
            "image archive must be an absolute regular file",
        ));
    }
    crate::images::validate_archive(path, digest)?;
    crate::images::validate_import(path)
}

fn hash(path: &Path, emit: &mut dyn FnMut(u64, u64)) -> Result<(u64, String), SdkRefusal> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(failed)?;
    let metadata = file.metadata().map_err(failed)?;
    if !metadata.is_file() {
        return Err(refused(
            "InvalidInput",
            "image artifact is not a regular file",
        ));
    }
    let size = metadata.len();
    let mut hasher = Sha256::new();
    let mut bytes = [0u8; 64 << 10];
    let mut done = 0;
    emit(0, size);
    let mut last = std::time::Instant::now();
    loop {
        let count = file.read(&mut bytes).map_err(failed)?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
        done += count as u64;
        if last.elapsed() >= std::time::Duration::from_millis(100) {
            emit(done, size);
            last = std::time::Instant::now();
        }
    }
    emit(done, size);
    if done != size {
        return Err(refused(
            "InvalidInput",
            "image archive changed while hashing",
        ));
    }
    Ok((size, layerfs_bridge::sdk_values::hex(&hasher.finalize())))
}
fn progress(events: &mut Events<'_>, phase: &str, done: u64, total: u64) {
    events.progress(SdkValue::map([
        ("phase", SdkValue::text(phase)),
        ("done", SdkValue::Int(done as i64)),
        ("total", SdkValue::Int(total as i64)),
    ]));
}
/// Sends each part once; a lost response is returned without replay.
pub fn send(
    worker: &mut Worker,
    events: &mut Events<'_>,
    path: &Path,
) -> Result<String, SdkRefusal> {
    let (size, key) = hash(path, &mut |done, total| {
        progress(events, "image-hash", done, total)
    })?;
    let token = layerfs_bridge::sdk_values::hex(&layerfs_home::helpers::random()?);
    let fields = |step: &str| {
        SdkValue::map([
            ("step", SdkValue::text(step)),
            ("key", SdkValue::text(&key)),
            ("token", SdkValue::text(&token)),
            ("size", SdkValue::Int(size as i64)),
        ])
    };
    let probe = worker.call("image.artifact", fields("probe"), &mut |e| events.event(e))?;
    if probe.get_bool("present") {
        progress(events, "image-transfer", size, size);
        return Ok(format!("cache:{key}"));
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(failed)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut offset = 0u64;
        loop {
            let count = file.read(&mut buffer).map_err(failed)?;
            if count == 0 {
                break;
            }
            let frame = layerfs_home::home::with_field(
                fields("write"),
                "offset",
                SdkValue::Int(offset as i64),
            );
            let frame = layerfs_home::home::with_field(
                frame,
                "bytes",
                SdkValue::Bytes(buffer[..count].to_vec()),
            );
            worker.call("image.artifact", frame, &mut |e| events.event(e))?;
            offset += count as u64;
            progress(events, "image-transfer", offset, size);
        }
        worker.call("image.artifact", fields("publish"), &mut |e| {
            events.event(e)
        })?;
        Ok(format!("cache:{key}"))
    })();
    if result
        .as_ref()
        .err()
        .is_some_and(|e: &SdkRefusal| e.code != "Unknown")
    {
        let _ = worker.call("image.artifact", fields("discard"), &mut |_| {});
    }
    result
}

/// One bounded worker artifact step; no caller path is accepted.
pub fn in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let key = args
        .get_text("key")
        .filter(|s| hex(s))
        .ok_or_else(|| refused("InvalidInput", "image artifact key"))?;
    let token = args
        .get_text("token")
        .filter(|s| s.len() <= 64 && !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| refused("InvalidInput", "image artifact token"))?;
    let dir = directory(root);
    layerfs_home::home::private_dir(&dir)?;
    let published = dir.join(format!("{key}.tar"));
    let stage = dir.join(format!(".{key}.{token}.part"));
    if !fs::symlink_metadata(&dir)
        .map_err(failed)?
        .file_type()
        .is_dir()
    {
        return Err(refused(
            "InvalidInput",
            "image cache must be a real directory",
        ));
    }
    let size = args
        .get_int("size")
        .filter(|n| *n >= 0)
        .ok_or_else(|| refused("InvalidInput", "image artifact size"))? as u64;
    match args.get_text("step").unwrap_or("") {
        "probe" => {
            let present = match fs::symlink_metadata(&published) {
                Ok(meta) if meta.file_type().is_file() => {
                    if hash(&published, &mut |_, _| {})? != (size, key.to_owned()) {
                        return Err(refused("InvalidInput", format!(
                            "image artifact cache {} does not match content key {key}; cached bytes were preserved",
                            published.display()
                        )));
                    }
                    true
                }
                Ok(_) => {
                    return Err(refused(
                        "InvalidInput",
                        "image artifact cache is not a regular file",
                    ))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(failed(e)),
            };
            Ok(SdkValue::map([("present", SdkValue::Bool(present))]))
        }
        "write" => {
            let offset = args
                .get_int("offset")
                .filter(|n| *n >= 0)
                .ok_or_else(|| refused("InvalidInput", "image artifact offset"))?
                as u64;
            let bytes = match args.get("bytes") {
                Some(SdkValue::Bytes(bytes)) if bytes.len() <= CHUNK => bytes,
                _ => return Err(refused("InvalidInput", "image artifact chunk")),
            };
            if offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|end| end > size)
            {
                return Err(refused(
                    "InvalidInput",
                    "image artifact exceeds declared size",
                ));
            }
            let mut options = OpenOptions::new();
            options.write(true).custom_flags(libc::O_NOFOLLOW);
            if offset == 0 {
                options.create_new(true).mode(0o600);
            }
            let mut file = options.open(stage).map_err(failed)?;
            if file.metadata().map_err(failed)?.len() != offset {
                return Err(refused(
                    "InvalidInput",
                    "image artifact parts are out of order",
                ));
            }
            file.seek(SeekFrom::Start(offset)).map_err(failed)?;
            file.write_all(bytes).map_err(failed)?;
            Ok(SdkValue::Null)
        }
        "publish" => {
            if hash(&stage, &mut |_, _| {})? != (size, key.to_owned()) {
                return Err(refused(
                    "InvalidInput",
                    "image artifact content changed during transfer",
                ));
            }
            crate::images::validate_archive(&stage, None)?;
            crate::images::validate_import(&stage)?;
            fs::rename(stage, published).map_err(failed)?;
            Ok(SdkValue::Null)
        }
        "discard" => match fs::remove_file(stage) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
            _ => Ok(SdkValue::Null),
        },
        _ => Err(refused("InvalidInput", "unknown image artifact step")),
    }
}

/// Copies a local archive into the same content-addressed machine cache.
pub fn local(root: &Path, events: &mut Events<'_>, path: &Path) -> Result<String, SdkRefusal> {
    let (size, key) = hash(path, &mut |done, total| {
        progress(events, "image-hash", done, total)
    })?;
    let token = layerfs_bridge::sdk_values::hex(&layerfs_home::helpers::random()?);
    let frame = |step: &str| {
        SdkValue::map([
            ("step", SdkValue::text(step)),
            ("key", SdkValue::text(&key)),
            ("token", SdkValue::text(&token)),
            ("size", SdkValue::Int(size as i64)),
        ])
    };
    if !in_worker(root, &frame("probe"))?.get_bool("present") {
        let result = (|| {
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(failed)?;
            let mut bytes = vec![0u8; CHUNK];
            let mut offset = 0u64;
            loop {
                let count = file.read(&mut bytes).map_err(failed)?;
                if count == 0 {
                    break;
                }
                let args = layerfs_home::home::with_field(
                    frame("write"),
                    "offset",
                    SdkValue::Int(offset as i64),
                );
                let args = layerfs_home::home::with_field(
                    args,
                    "bytes",
                    SdkValue::Bytes(bytes[..count].to_vec()),
                );
                in_worker(root, &args)?;
                offset += count as u64;
                progress(events, "image-transfer", offset, size);
            }
            in_worker(root, &frame("publish"))
        })();
        if result.is_err() {
            let _ = in_worker(root, &frame("discard"));
        }
        result?;
    }
    progress(events, "image-transfer", size, size);
    Ok(format!("cache:{key}"))
}
