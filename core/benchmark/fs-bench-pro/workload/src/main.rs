//! Benchmark-only POSIX editor, invoked by the product's `WorkspaceApi::exec`.
//!
//! It performs exactly the declared edit algorithm of one registered #232 case
//! and nothing else: it never calls a LayerFS internal, never opens a host path
//! outside its arguments, and fails closed when the file it was given does not
//! match the declared pre-state. The bytes it writes pass through the mounted
//! FUSE projection because that is the only filesystem it can see.
use std::{
    fmt::Write as _,
    fs::File,
    io::{Error, ErrorKind, Read, Seek, SeekFrom, Write},
    os::fd::AsRawFd,
    path::PathBuf,
};

struct Options {
    operation: String,
    file: PathBuf,
    expect_size: u64,
    offset: u64,
    length: u64,
    size: u64,
    payload: Option<PathBuf>,
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
    Ok(Options {
        operation,
        file: PathBuf::from(values.get("file").ok_or_else(usage)?),
        expect_size: number("expect-size")?,
        offset: values.get("offset").map(|_| number("offset")).transpose()?.unwrap_or(0),
        length: values
            .get("length")
            .map(|_| number("length"))
            .transpose()?
            .unwrap_or(0),
        size: values.get("size").map(|_| number("size")).transpose()?.unwrap_or(0),
        payload: values.get("payload").map(PathBuf::from),
    })
}

fn usage() -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        "usage: layerfs-edit-tool <pwrite|truncate|extend> --file P --expect-size N \
         [--offset N --length N --payload P | --size N]",
    )
}

fn opened(options: &Options) -> Result<File, Error> {
    let file = File::options().read(true).write(true).open(&options.file)?;
    let actual = file.metadata()?.len();
    if actual != options.expect_size {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("pre-state size {actual} != declared {}", options.expect_size),
        ));
    }
    Ok(file)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let options = parsed(&args)?;
    let mut file = opened(&options)?;
    match options.operation.as_str() {
        "pwrite" => {
            let payload_path = options.payload.as_deref().ok_or_else(usage)?;
            let mut payload = Vec::new();
            File::open(payload_path)?.read_to_end(&mut payload)?;
            if payload.len() as u64 != options.length {
                return Err("payload length differs from the declared edit".into());
            }
            if options.offset <= options.expect_size
                && options.offset + options.length > options.expect_size
                && options.offset != options.expect_size
            {
                return Err("positional write crosses the declared end of file".into());
            }
            file.seek(SeekFrom::Start(options.offset))?;
            file.write_all(&payload)?;
        }
        "truncate" | "extend" => {
            if options.operation == "truncate" && options.size >= options.expect_size {
                return Err("truncate does not shorten the declared file".into());
            }
            if options.operation == "extend" && options.size <= options.expect_size {
                return Err("extend does not lengthen the declared file".into());
            }
            file.set_len(options.size)?;
        }
        other => {
            return Err(format!("unknown operation {other}").into());
        }
    }
    let final_size = file.metadata()?.len();
    let mut line = String::new();
    write!(
        line,
        "{{\"status\":\"PASS\",\"operation\":\"{}\",\"final_bytes\":{final_size}}}",
        options.operation
    )
    .expect("string write");
    println!("{line}");
    // The descriptor is closed by drop; no fsync is issued, because the product
    // makes no durability promise about Workspace backing.
    let _ = file.as_raw_fd();
    Ok(())
}
