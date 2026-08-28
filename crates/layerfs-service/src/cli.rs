use crate::serve_loopback;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = arguments()?;
    let root = PathBuf::from(arguments.get("root").ok_or("missing --root")?);
    let bearer = read_bearer(PathBuf::from(
        arguments
            .get("bearer-file")
            .ok_or("missing --bearer-file")?,
    ))?;
    let listener = TcpListener::bind(
        arguments
            .get("listen")
            .map(String::as_str)
            .unwrap_or("127.0.0.1:0"),
    )?;
    if !listener.local_addr()?.ip().is_loopback() {
        return Err("--listen must be loopback".into());
    }
    println!("{}", listener.local_addr()?);
    std::io::stdout().flush()?;
    serve_loopback(&root, &bearer, listener)?;
    Ok(())
}

fn arguments() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut output = HashMap::new();
    let mut input = std::env::args().skip(1);
    while let Some(name) = input.next() {
        let Some(name) = name.strip_prefix("--") else {
            return Err(format!("unexpected argument {name}").into());
        };
        let value = input
            .next()
            .ok_or_else(|| format!("missing value for --{name}"))?;
        if output.insert(name.to_owned(), value).is_some() {
            return Err(format!("duplicate --{name}").into());
        }
    }
    Ok(output)
}

fn read_bearer(path: PathBuf) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || !(32..=4096).contains(&metadata.len()) {
        return Err("bearer file must be a 32..4096 byte regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("bearer file must not be accessible by group/other".into());
        }
    }
    let mut bearer = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut bearer)?;
    if bearer.len() != metadata.len() as usize {
        return Err("bearer file changed while reading".into());
    }
    Ok(bearer)
}
