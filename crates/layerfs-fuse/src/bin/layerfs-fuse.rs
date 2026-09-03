fn main() {
    if version_requested() {
        return;
    }
    #[cfg(all(target_os = "linux", feature = "proxy"))]
    {
        if let Err(error) = run() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "proxy")))]
    {
        eprintln!("layerfs-fuse requires Linux and the proxy feature");
        std::process::exit(1);
    }
}

fn version_requested() -> bool {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() == 1 && arguments[0] == "--version" {
        println!("layerfs-fuse {}", env!("CARGO_PKG_VERSION"));
        true
    } else {
        false
    }
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::sync::Arc;

    let mut arguments = std::env::args_os().skip(1);
    let endpoint = arguments.next().ok_or("missing endpoint")?;
    let capability_text = arguments
        .next()
        .ok_or("missing capability")?
        .to_str()
        .ok_or("capability text")?
        .to_owned();
    let capability = capability(&capability_text)?;
    let mountpoint = std::path::PathBuf::from(arguments.next().ok_or("missing mountpoint")?);
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let endpoint = endpoint.to_str().ok_or("endpoint text")?;
    let client = Arc::new(layerfs_fuse::ProxyClient::connect(endpoint, capability)?);
    let control = layerfs_fuse::serve_remote_control(endpoint, capability, client.clone())?;
    let mut mount = layerfs_fuse::mount_host(client.clone(), &mountpoint, 0, 0)?;
    client.set_notifier(mount.notifier()?)?;
    println!("READY");
    let mountpoint_text = mountpoint.to_string_lossy();
    let mountinfo_text = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mountinfo = mountinfo_text
        .lines()
        .find(|line| line.split_whitespace().nth(4) == Some(mountpoint_text.as_ref()))
        .ok_or("mounted FUSE path missing from mountinfo")?;
    println!("MOUNTINFO\t{mountinfo}");
    std::io::stdout().flush()?;
    if let Err(error) = control.wait_for_shutdown() {
        let _ = mount.unmount();
        return Err(error.into());
    }
    let shutdown = mount
        .unmount()
        .and_then(|()| cleanup_owned(&mountpoint, &capability_text));
    let acknowledged = control.finish_shutdown(shutdown.is_ok());
    shutdown?;
    acknowledged.map_err(Into::into)
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn cleanup_owned(mountpoint: &std::path::Path, capability: &str) -> std::io::Result<()> {
    const FIXED_HELPER: &str = "/usr/local/bin/layerfs-fuse";
    let helper = std::env::current_exe()?;
    if std::env::var_os("LAYERFS_OWNED_HELPER").as_deref() != Some(helper.as_os_str())
        || std::env::var_os("LAYERFS_OWNED_ROOT").as_deref() != Some(mountpoint.as_os_str())
        || std::env::var("LAYERFS_OWNED_CAPABILITY").as_deref() != Ok(capability)
    {
        return Err(std::io::Error::other("LayerFS cleanup ownership"));
    }
    if std::env::var("LAYERFS_FIXED_HELPER").as_deref() == Ok("1") {
        return if helper == std::path::Path::new(FIXED_HELPER) {
            Ok(())
        } else {
            Err(std::io::Error::other("LayerFS fixed helper identity"))
        };
    }
    let mut identity = helper.as_os_str().to_os_string();
    identity.push(".identity");
    let identity = std::path::PathBuf::from(identity);
    let text = std::fs::read_to_string(&identity)?;
    let mut fields = text.split_whitespace();
    let pid = std::process::id().to_string();
    let start = process_start()?;
    if fields.next() != Some(pid.as_str()) || fields.next() != Some(start.as_str()) {
        return Err(std::io::Error::other("LayerFS cleanup identity"));
    }
    let created_root = match fields.next() {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(std::io::Error::other("LayerFS cleanup root identity")),
    };
    if fields.next().is_some() {
        return Err(std::io::Error::other("LayerFS cleanup identity"));
    }
    std::fs::remove_file(&helper)?;
    std::fs::remove_file(identity)?;
    if created_root {
        std::fs::remove_dir(mountpoint)?;
    }
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "proxy"))]
fn process_start() -> std::io::Result<String> {
    let stat = std::fs::read_to_string("/proc/self/stat")?;
    stat.rsplit_once(") ")
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::other("LayerFS process identity"))
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
