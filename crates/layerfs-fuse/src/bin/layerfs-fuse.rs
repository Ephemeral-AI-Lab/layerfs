#[cfg(all(target_os = "linux", feature = "proxy"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(all(target_os = "linux", feature = "proxy")))]
fn main() {
    eprintln!("layerfs-fuse requires Linux and the proxy feature");
    std::process::exit(1);
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::sync::Arc;

    let mut arguments = std::env::args_os().skip(1);
    let endpoint = arguments.next().ok_or("missing endpoint")?;
    if endpoint == "--control" {
        let socket = arguments.next().ok_or("missing control socket")?;
        let command = arguments.next().ok_or("missing control command")?;
        if arguments.next().is_some() {
            return Err("unexpected control argument".into());
        }
        return layerfs_fuse::control_call(&socket, &command);
    }
    let capability = capability(
        arguments
            .next()
            .ok_or("missing capability")?
            .to_str()
            .ok_or("capability text")?,
    )?;
    let mountpoint = arguments.next().ok_or("missing mountpoint")?;
    let control = arguments.next().ok_or("missing control socket")?;
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let client = Arc::new(layerfs_fuse::ProxyClient::connect(
        endpoint.to_str().ok_or("endpoint text")?,
        capability,
    )?);
    layerfs_fuse::serve_control(control, client.clone())?;
    let mount = layerfs_fuse::mount_host(client, mountpoint, 0, 0)?;
    println!("READY");
    std::io::stdout().flush()?;
    mount.join()?;
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn capability(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 {
        return Err("capability length".into());
    }
    let mut output = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
    }
    Ok(output)
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn hex(value: u8) -> Result<u8, Box<dyn std::error::Error>> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("capability hex".into()),
    }
}
