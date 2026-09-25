//! Benchmark-only POSIX editor, invoked by the product's `WorkspaceApi::exec`.
//!
//! It performs exactly the declared edit algorithm of one registered #232 case
//! and nothing else: it never calls a LayerFS internal, never opens a host path
//! outside its arguments, and fails closed when the file it was given does not
//! match the declared pre-state. The bytes it writes pass through the mounted
//! FUSE projection because that is the only filesystem it can see.
//!
//! Five declared algorithms are implemented:
//!
//! * `pwrite` — one positional overwrite (`open`, `fstat`, `pwrite`, `close`);
//! * `truncate` / `extend` — one size change via `ftruncate`;
//! * `shift` — the authentic bounded-memory in-place window shift. A grow
//!   extends the file and moves the affected suffix backward, block by block
//!   from the old end; a shrink moves the suffix forward, block by block from
//!   the splice point, and then truncates. Either direction ends by writing the
//!   declared replacement bytes over the stale window. Memory stays bounded by
//!   one block for every input size: no full-file copy, no temporary file and
//!   no host-side path is used.
//! * `splice` — one bounded Linux FUSE range ioctl and caller confirmation.
//! * `splice-batch` — the frozen Phase 1C sequence of checked insert ioctls.
use std::{
    fmt::Write as _,
    fs::File,
    io::{Error, ErrorKind, Read},
    os::fd::AsRawFd,
    os::unix::fs::FileExt,
    path::PathBuf,
};

#[cfg(target_os = "linux")]
mod splice;

/// One shift block. FROZEN: this value is part of the declared algorithm and is
/// recorded in the case registry; it matches the projection's declared maximum
/// transfer (`MAX_READ_BYTES`) so one block is one projection request.
const SHIFT_BLOCK_BYTES: usize = 128 * 1024;

#[derive(Clone)]
struct Options {
    operation: String,
    output_version: u64,
    file: PathBuf,
    expect_size: u64,
    offset: u64,
    delete_length: u64,
    length: u64,
    size: u64,
    count: u64,
    direction: Option<String>,
    payload: Option<PathBuf>,
}

fn usage() -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        "usage: layerfs-edit-tool <pwrite|truncate|extend|shift|splice|splice-batch> --file P --expect-size N \
         [--offset N --length N --payload P | --offset N --delete-length N --length N \
         --direction grow|shrink [--payload P] | --size N] [--count 1|32|128 --output-version 5 for splice-batch]",
    )
}

fn parsed(args: &[String]) -> Result<Options, Error> {
    let operation = args.get(1).cloned().ok_or_else(usage)?;
    let mut values = std::collections::BTreeMap::new();
    let mut index = 2;
    while index < args.len() {
        let key = args[index].trim_start_matches("--").to_string();
        let value = args.get(index + 1).ok_or_else(usage)?.clone();
        values.insert(key, value);
        index += 2;
    }
    let number = |key: &str| -> Result<u64, Error> {
        values
            .get(key)
            .ok_or_else(usage)?
            .parse::<u64>()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, format!("{key} is not a number")))
    };
    let optional = |key: &str| -> Result<Option<u64>, Error> {
        values.get(key).map(|_| number(key)).transpose()
    };
    let output_version = optional("output-version")?.unwrap_or(3);
    if !matches!(output_version, 3 | 4 | 5)
        || (output_version == 4 && operation != "splice")
        || (output_version == 5 && operation != "splice-batch")
        || (operation == "splice-batch" && output_version != 5)
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "unsupported output version",
        ));
    }
    Ok(Options {
        operation,
        output_version,
        file: PathBuf::from(values.get("file").ok_or_else(usage)?),
        expect_size: number("expect-size")?,
        offset: optional("offset")?.unwrap_or(0),
        delete_length: optional("delete-length")?.unwrap_or(0),
        length: optional("length")?.unwrap_or(0),
        size: optional("size")?.unwrap_or(0),
        count: optional("count")?.unwrap_or(0),
        direction: values.get("direction").cloned(),
        payload: values.get("payload").map(PathBuf::from),
    })
}

fn opened(options: &Options) -> Result<File, Error> {
    let file = File::options().read(true).write(true).open(&options.file)?;
    let actual = file.metadata()?.len();
    if actual != options.expect_size {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "pre-state size {actual} != declared {}",
                options.expect_size
            ),
        ));
    }
    Ok(file)
}

/// One positional write, retried until every byte is accepted. A short write
/// through the projection is a partial completion, never a truncation.
fn write_all_at(file: &File, mut offset: u64, mut bytes: &[u8]) -> Result<(), Error> {
    while !bytes.is_empty() {
        let written = file.write_at(bytes, offset)?;
        if written == 0 {
            return Err(Error::new(ErrorKind::WriteZero, "short positional write"));
        }
        offset += written as u64;
        bytes = &bytes[written..];
    }
    Ok(())
}

/// One positional read, retried until the block is full or the file ends.
fn read_exact_at(file: &File, mut offset: u64, mut into: &mut [u8]) -> Result<(), Error> {
    while !into.is_empty() {
        let read = file.read_at(into, offset)?;
        if read == 0 {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                "short positional read",
            ));
        }
        offset += read as u64;
        into = &mut into[read..];
    }
    Ok(())
}

fn payload(options: &Options) -> Result<Vec<u8>, Error> {
    let path = options.payload.as_deref().ok_or_else(usage)?;
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() as u64 != options.length {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "payload length differs from the declared edit",
        ));
    }
    Ok(bytes)
}

/// The declared in-place window shift: replace `[offset, offset + delete_length)`
/// with `length` declared bytes, moving the affected suffix inside the same file.
fn shift(options: &Options, file: &File) -> Result<u64, Error> {
    let start = options.offset;
    let deleted = options.delete_length;
    let initial = options.expect_size;
    let inserted = options.length;
    let tail_start = start
        .checked_add(deleted)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "edit bounds overflow"))?;
    if tail_start > initial {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "the declared window is not inside the declared file",
        ));
    }
    let final_size = initial - deleted + inserted;
    let direction = options.direction.as_deref().ok_or_else(usage)?;
    let grow = match final_size.cmp(&initial) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "a shift changes the declared file size",
            ))
        }
    };
    if (direction == "grow") != grow {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("declared direction {direction} contradicts the declared sizes"),
        ));
    }
    let replacement = if inserted == 0 {
        Vec::new()
    } else {
        payload(options)?
    };
    let mut buffer = vec![0u8; SHIFT_BLOCK_BYTES];
    let shifted = initial - tail_start;
    if grow {
        // Extend first, then move the suffix backward from the old end so no
        // unread source byte is overwritten.
        let delta = final_size - initial;
        file.set_len(final_size)?;
        let mut end = initial;
        while end > tail_start {
            let take = std::cmp::min(buffer.len() as u64, end - tail_start) as usize;
            let from = end - take as u64;
            read_exact_at(file, from, &mut buffer[..take])?;
            write_all_at(file, from + delta, &buffer[..take])?;
            end = from;
        }
    } else {
        // Move the suffix forward from the splice point, then truncate. Every
        // destination lies below the region already read.
        let delta = initial - final_size;
        let mut cursor = tail_start;
        while cursor < initial {
            let take = std::cmp::min(buffer.len() as u64, initial - cursor) as usize;
            read_exact_at(file, cursor, &mut buffer[..take])?;
            write_all_at(file, cursor - delta, &buffer[..take])?;
            cursor += take as u64;
        }
        file.set_len(final_size)?;
    }
    // The stale window left by the move is exactly `inserted` bytes at `start`.
    if !replacement.is_empty() {
        write_all_at(file, start, &replacement)?;
    }
    Ok(shifted)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let options = parsed(&args)?;
    let file = opened(&options)?;
    let mut shifted = 0u64;
    let post_state_mtime = match options.operation.as_str() {
        "pwrite" => {
            let bytes = payload(&options)?;
            if options.offset < options.expect_size
                && options.offset + options.length > options.expect_size
            {
                return Err("positional write crosses the declared end of file".into());
            }
            write_all_at(&file, options.offset, &bytes)?;
            None
        }
        "truncate" | "extend" => {
            if options.operation == "truncate" && options.size >= options.expect_size {
                return Err("truncate does not shorten the declared file".into());
            }
            if options.operation == "extend" && options.size <= options.expect_size {
                return Err("extend does not lengthen the declared file".into());
            }
            file.set_len(options.size)?;
            None
        }
        "shift" => {
            shifted = shift(&options, &file)?;
            None
        }
        "splice" | "splice-batch" => {
            #[cfg(target_os = "linux")]
            {
                Some(if options.operation == "splice" {
                    splice::run(&options, &file)?
                } else {
                    splice::run_batch(&options, &file)?
                })
            }
            #[cfg(not(target_os = "linux"))]
            return Err(
                Error::new(ErrorKind::Unsupported, "splice requires Linux FUSE ioctl").into(),
            );
        }
        other => {
            return Err(format!("unknown operation {other}").into());
        }
    };
    let final_size = file.metadata()?.len();
    let line = result_line(&options, final_size, shifted, post_state_mtime)?;
    println!("{line}");
    // The descriptor is closed by drop; no fsync is issued, because the product
    // makes no durability promise about Workspace backing.
    let _ = file.as_raw_fd();
    Ok(())
}

fn result_line(
    options: &Options,
    final_size: u64,
    shifted: u64,
    post_state_mtime: Option<(i64, u32)>,
) -> Result<String, Error> {
    let mut line = String::new();
    write!(
        line,
        "{{\"status\":\"PASS\",\"operation\":\"{}\",\"direction\":\"{}\",\
\"final_bytes\":{final_size},\"shifted_bytes\":{shifted}}}",
        options.operation,
        options.direction.as_deref().unwrap_or("-")
    )
    .expect("string write");
    if options.output_version >= 4 {
        let (seconds, nanoseconds) = post_state_mtime
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "missing post-EDIT STATE mtime"))?;
        line.pop();
        write!(
            line,
            ",\"mtime_seconds\":{seconds},\"mtime_nanoseconds\":{nanoseconds}}}"
        )
        .expect("string write");
    }
    if options.output_version == 5 {
        line.pop();
        write!(
            line,
            ",\"edit_count\":{},\"accepted_bytes\":4096}}",
            options.count
        )
        .expect("string write");
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_output_adds_only_checked_state_mtime() {
        let mut options = Options {
            operation: "splice".into(),
            output_version: 3,
            file: PathBuf::new(),
            expect_size: 1,
            offset: 0,
            delete_length: 0,
            length: 0,
            size: 0,
            count: 0,
            direction: None,
            payload: None,
        };
        assert_eq!(
            result_line(&options, 5, 0, None).unwrap(),
            "{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":5,\"shifted_bytes\":0}"
        );
        options.output_version = 4;
        assert_eq!(
            result_line(&options, 5, 0, Some((-2, 750_000_000))).unwrap(),
            "{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":5,\"shifted_bytes\":0,\"mtime_seconds\":-2,\"mtime_nanoseconds\":750000000}"
        );
        assert!(result_line(&options, 5, 0, None).is_err());
        options.operation = "splice-batch".into();
        options.output_version = 5;
        options.count = 32;
        assert_eq!(
            result_line(&options, 4097, 0, Some((-2, 750_000_000))).unwrap(),
            "{\"status\":\"PASS\",\"operation\":\"splice-batch\",\"direction\":\"-\",\"final_bytes\":4097,\"shifted_bytes\":0,\"mtime_seconds\":-2,\"mtime_nanoseconds\":750000000,\"edit_count\":32,\"accepted_bytes\":4096}"
        );
    }
}
